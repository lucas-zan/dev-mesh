use crate::{
    config::{credential, Config},
    controller::{check_command, ControlTarget},
    daemon::WorkMap,
    gitea::Repo,
    storage::Store,
};
use devmesh_protocol::{now, Error, Frame, Result, WorkState};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    sync::{atomic::Ordering, Arc},
};
use tokio::io::{AsyncRead, AsyncWrite};
use uuid::Uuid;

pub struct AgentLink {
    pub device: Uuid,
    pub store: Arc<Store>,
}
impl AgentLink {
    pub fn new(device: Uuid, store: Arc<Store>) -> Self {
        Self { device, store }
    }
    pub fn accept(&self, frame: &Frame) -> Result<()> {
        frame.check_version()?;
        match frame.kind.as_str() {
            "events.ack" => {
                let session = frame.payload["session_id"]
                    .as_str()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| Error::new("invalid_response", "ACK Session missing"))?;
                let sequence = devmesh_protocol::sequence(
                    frame.payload["acked_sequence"].as_str().unwrap_or(""),
                )?;
                self.store.ack(session, sequence)?;
            }
            "record.ack" | "catalog.ack" => {
                if let Some(id) = frame.reply_to {
                    self.store.ack_frame(&id.to_string())?;
                }
            }
            "catalog.lease" => {
                self.store.set_setting(
                    &format!(
                        "lease:{}",
                        frame.payload["project_id"].as_str().unwrap_or("")
                    ),
                    &frame.payload,
                )?;
                if let Some(id) = frame.reply_to {
                    self.store.ack_frame(&id.to_string())?;
                }
            }
            "protocol.error" => {
                if let Some(id) = frame.reply_to {
                    let original = self
                        .store
                        .pending_frames()?
                        .into_iter()
                        .find(|pending| pending["frame_id"] == id.to_string());
                    if let Some(original) = original.filter(|pending| {
                        pending["type"]
                            .as_str()
                            .is_some_and(|kind| kind.starts_with("catalog."))
                    }) {
                        let project = original["payload"]["project_id"].as_str().unwrap_or("");
                        self.store.ack_frame(&id.to_string())?;
                        self.store
                            .set_setting(&format!("lease:{project}"), &Value::Null)?;
                        self.store.set_setting("catalog:last_status",&json!({"project_id":project,"code":frame.payload["code"],"state":if frame.payload["code"]=="lease_busy"{"standby"}else{"needs_refresh"}}))?;
                        return Ok(());
                    }
                }
                return Err(Error::new(
                    frame.payload["code"].as_str().unwrap_or("protocol_error"),
                    frame.payload["message"]
                        .as_str()
                        .unwrap_or("Relay rejected frame"),
                ));
            }
            _ => {}
        }
        Ok(())
    }
    pub async fn transfer<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
        &self,
        socket: devmesh_protocol::socket::Socket<S>,
        works: WorkMap,
        remote: crate::config::RemoteControl,
    ) -> Result<()> {
        self.store
            .set_setting("catalog:last_status", &Value::Null)?;
        for pending in self.store.pending_frames()? {
            if pending["type"]
                .as_str()
                .is_some_and(|kind| kind.starts_with("catalog."))
            {
                self.store
                    .ack_frame(pending["frame_id"].as_str().unwrap_or(""))?;
            }
        }
        let (output, mut input) = socket.into_channels();
        let registration = Frame::new(
            "connection.register",
            json!({"device_id":self.device,"supported_versions":["1.1"],"client_version":env!("CARGO_PKG_VERSION"),"resume_sessions":self.store.event_sessions()?}),
        );
        output
            .send(serde_json::to_string(&registration).unwrap())
            .await
            .map_err(|_| Error::new("unavailable", "Relay closed"))?;
        let mut sent = HashSet::new();
        let mut registered_sessions = HashSet::new();
        let mut registered_works = HashSet::new();
        let mut event_batches: HashSet<String> = HashSet::new();
        let mut handshake = false;
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(50));
        let mut pending_registrations = self.store.registrations()?;
        pending_registrations.sort_by_key(|f| if f["type"] == "work.register" { 0 } else { 1 });
        loop {
            tokio::select! {
                response=input.recv()=>{
                    let text=response.ok_or_else(||Error::new("unavailable","Relay disconnected"))??;
                    let frame:Frame=serde_json::from_str(&text).map_err(|_|Error::new("invalid_response","Relay sent invalid JSON"))?;
                    if frame.kind=="connection.ack" {handshake=true;}
                    if frame.kind=="connection.heartbeat"&&frame.payload["reply"]==false {let mut payload=frame.payload.clone();payload["reply"]=json!(true);output.send(serde_json::to_string(&Frame::new("connection.heartbeat",payload)).unwrap()).await.map_err(|_|Error::new("unavailable","Relay closed"))?;}
                    if frame.kind=="record.ack" {
                        if let Some(reply)=frame.reply_to {
                            if let Some(original)=pending_registrations.iter().chain(self.store.pending_frames()?.iter()).find(|f|f["frame_id"]==reply.to_string()){
                                if original["type"]=="work.register"{registered_works.insert(original["payload"]["work_id"].as_str().unwrap_or("").to_owned());}
                                if original["type"]=="session.register"{registered_sessions.insert(original["payload"]["session_id"].as_str().unwrap_or("").to_owned());}
                            }
                        }
                    }
                    if frame.kind=="events.ack"{event_batches.remove(frame.payload["session_id"].as_str().unwrap_or(""));}
                    if frame.kind=="command.dispatch" {
                        let store=self.store.clone();let works=works.clone();let remote=remote.clone();let command=frame.payload.clone();
                        tokio::spawn(async move{if let Err(error)=dispatch(&store,&works,&remote,&command).await{eprintln!("Command admission: {error}");}});
                    }
                    self.accept(&frame)?;
                },
                _=tick.tick(),if handshake=>{
                    let pending=self.store.pending_frames()?;let mut frames=pending_registrations.clone();frames.extend(pending);
                    for value in frames {
                        let frame:Frame=serde_json::from_value(value).map_err(|_|Error::new("storage_error","Invalid local frame"))?;
                        if sent.contains(&frame.frame_id){continue;}
                        if frame.kind=="session.register"&&!registered_works.contains(frame.payload["work_id"].as_str().unwrap_or("")){continue;}
                        if frame.kind=="session.status"&&!registered_sessions.contains(frame.payload["session_id"].as_str().unwrap_or("")){continue;}
                        output.send(serde_json::to_string(&frame).unwrap()).await.map_err(|_|Error::new("unavailable","Relay disconnected"))?;sent.insert(frame.frame_id);
                    }
                    for session in self.store.event_sessions()?{
                        let id=session["session_id"].as_str().unwrap_or("");if !registered_sessions.contains(id)||event_batches.contains(id){continue;}
                        let events=self.store.pending_events(id.parse().map_err(|_|Error::new("storage_error","Invalid Session UUID"))?)?;
                        if events.is_empty(){continue;}
                        let frame=Frame::new("session.events",json!({"session_id":id,"events":events}));output.send(serde_json::to_string(&frame).unwrap()).await.map_err(|_|Error::new("unavailable","Relay disconnected"))?;event_batches.insert(id.into());
                    }

                }
            }
        }
    }
}
pub fn catalog_item(repo: &Repo, issue: &Value) -> Result<Value> {
    let number = issue["number"]
        .as_u64()
        .ok_or_else(|| Error::new("invalid_response", "Gitea Issue number missing"))?;
    let meta = devmesh_protocol::metadata(issue["body"].as_str().unwrap_or("")).ok();
    let title =
        crate::collector::sanitize(issue["title"].as_str().unwrap_or(""), "system")["text"].clone();
    let mut value = json!({"issue_ref":repo.issue_ref(number),"source_updated_at":issue["updated_at"],"content_version":issue["content_version"].as_u64().map(|v|v.to_string()),"metadata_hash":"","source_state":issue["state"],"canonical_state":meta.as_ref().map(|m|m["state"].clone()),"role":meta.as_ref().map(|m|m["required_role"].clone()),"assignees":issue["assignees"].as_array().into_iter().flatten().filter_map(|u|u["id"].as_u64()).collect::<Vec<_>>(),"title":title,"url":issue["html_url"],"priority":meta.as_ref().map(|m|m["priority"].clone())});
    value["metadata_hash"] = json!(format!(
        "{:x}",
        Sha256::digest(value.to_string().as_bytes())
    ));
    Ok(value)
}
async fn dispatch(
    store: &Store,
    works: &WorkMap,
    remote: &crate::config::RemoteControl,
    command: &Value,
) -> Result<()> {
    let id = command["command_id"]
        .as_str()
        .ok_or_else(|| Error::new("invalid_request", "Command ID required"))?;
    if !store.journal(id, command)? {
        if let Some(previous) = store.command_record(id)? {
            if !previous["receipt"].is_null() {
                store.queue_frame(&Frame::new("command.receipt", previous["receipt"].clone()))?;
            }
        }
        return Ok(());
    }
    receipt(store, command, 1, "delivered", "none", None)?;
    let active = works.lock().await.values().cloned().collect::<Vec<_>>();
    let selected = active.iter().find(|w| {
        command["target"]["work_id"] == w.prepared.work_id.to_string()
            && command["target"]["session_id"] == w.prepared.session_id.to_string()
    });
    let _operation = if let Some(work) = selected {
        Some(work.operation.lock().await)
    } else {
        None
    };
    let outcome = async {
        let work = active
            .iter()
            .find(|w| {
                command["target"]["work_id"] == w.prepared.work_id.to_string()
                    && command["target"]["session_id"] == w.prepared.session_id.to_string()
            })
            .ok_or_else(|| Error::new("invalid_target", "Active Work/Session not found"))?;
        if work.closing.load(Ordering::SeqCst) {
            return Err(Error::new("invalid_state", "Work finalizing"));
        }
        let native = work.runtime.state.lock().await.clone();
        let state: WorkState =
            serde_json::from_value(store.work(work.prepared.work_id)?["state"].clone())
                .map_err(|_| Error::new("storage_error", "Invalid Work state"))?;
        let target = ControlTarget {
            work_state: state,
            session_state: native.status,
            instance: native.instance,
            epoch: native.epoch.to_string(),
            generation: work.prepared.claim["generation"].as_u64().unwrap_or(0),
            enabled: remote.enabled,
        };
        check_command(&target, command, &now())?;
        if command["type"] == "interrupt" && !remote.allow_interrupt {
            return Err(Error::new(
                "permission_denied",
                "Local interrupt policy disabled",
            ));
        }
        work.worker
            .service
            .verify_claim(work.prepared.issue_number, &work.prepared.claim)
            .await?;
        if work.closing.load(Ordering::SeqCst) {
            return Err(Error::new("invalid_state", "Work finalizing"));
        }
        let latest = work.runtime.state.lock().await.clone();
        let checked = ControlTarget {
            work_state: state,
            session_state: latest.status,
            instance: latest.instance,
            epoch: latest.epoch.to_string(),
            generation: target.generation,
            enabled: remote.enabled,
        };
        check_command(&checked, command, &now())?;
        if command["type"] == "follow_up" {
            return Ok(true);
        }
        let previous = store.command_record(id)?.unwrap();
        store.command_receipt(id, "dispatching", &previous["receipt"])?;
        work.runtime
            .control(
                command["type"].as_str().unwrap_or(""),
                command["message"].as_str().unwrap_or(""),
                command["target"]["expected_run_epoch"].as_str(),
            )
            .await?;
        Ok(false)
    }
    .await;
    match outcome {
        Ok(queued) => {
            let value = receipt(
                store,
                command,
                2,
                "accepted",
                if queued { "queued" } else { "native_accepted" },
                None,
            )?;
            store.command_receipt(id, if queued { "queued" } else { "accepted" }, &value)?;
        }
        Err(error) => {
            receipt(
                store,
                command,
                2,
                if error.code == "unknown_outcome" {
                    "unknown"
                } else if error.code == "command_expired" {
                    "expired"
                } else {
                    "rejected"
                },
                "none",
                Some(&error.code),
            )?;
        }
    }
    Ok(())
}
fn receipt(
    store: &Store,
    command: &Value,
    seq: u64,
    state: &str,
    disposition: &str,
    reason: Option<&str>,
) -> Result<Value> {
    let receipt = json!({"command_id":command["command_id"],"receipt_seq":seq.to_string(),"state":state,"disposition":disposition,"reason_code":reason,"evidence_ref":null,"at":now()});
    store.command_receipt(
        command["command_id"].as_str().unwrap_or(""),
        state,
        &receipt,
    )?;
    store.queue_frame(&Frame::new("command.receipt", receipt.clone()))?;
    Ok(receipt)
}
async fn advance_queue(store: &Store, works: &WorkMap) -> Result<()> {
    let active = works.lock().await.values().cloned().collect::<Vec<_>>();
    for command in store.queued_commands()? {
        let work = active
            .iter()
            .find(|w| command["target"]["work_id"] == w.prepared.work_id.to_string());
        if command["expires_at"]
            .as_str()
            .is_none_or(|s| s <= now().as_str())
        {
            receipt(
                store,
                &command,
                3,
                "expired",
                "queued",
                Some("command_expired"),
            )?;
            continue;
        }
        let Some(work) = work else {
            receipt(
                store,
                &command,
                3,
                "cancelled",
                "queued",
                Some("work_ended"),
            )?;
            continue;
        };
        if work.closing.load(Ordering::SeqCst) {
            receipt(
                store,
                &command,
                3,
                "cancelled",
                "queued",
                Some("work_finalizing"),
            )?;
            continue;
        }
        let _operation = work.operation.lock().await;
        let native = work.runtime.state.lock().await.clone();
        if native.status != "idle" {
            continue;
        }
        if native.instance.to_string()
            != command["target"]["runtime_instance_id"]
                .as_str()
                .unwrap_or("")
        {
            receipt(
                store,
                &command,
                3,
                "cancelled",
                "none",
                Some("stale_instance"),
            )?;
            continue;
        }
        let result = async {
            work.worker
                .service
                .verify_claim(work.prepared.issue_number, &work.prepared.claim)
                .await?;
            let previous = store
                .command_record(command["command_id"].as_str().unwrap())?
                .unwrap();
            store.command_receipt(
                command["command_id"].as_str().unwrap(),
                "dispatching",
                &previous["receipt"],
            )?;
            work.runtime
                .control(
                    "prompt",
                    command["message"].as_str().unwrap_or(""),
                    Some(&native.epoch.to_string()),
                )
                .await
        }
        .await;
        match result {
            Ok(_) => {
                receipt(store, &command, 3, "accepted", "native_accepted", None)?;
            }
            Err(e) => {
                receipt(
                    store,
                    &command,
                    3,
                    if e.code == "unknown_outcome" {
                        "unknown"
                    } else {
                        "cancelled"
                    },
                    "none",
                    Some(&e.code),
                )?;
            }
        }
    }
    Ok(())
}
pub async fn run(config: Config, store: Arc<Store>, works: WorkMap) {
    let Some(relay) = config.relay.clone() else {
        return;
    };
    loop {
        let result = async {
            let key = credential(&relay.credential_ref)?;
            let endpoint = format!(
                "{}/ws/v1/connect",
                relay
                    .endpoint
                    .trim_end_matches('/')
                    .replacen("https://", "wss://", 1)
                    .replacen("http://", "ws://", 1)
            );
            let socket = devmesh_protocol::socket::connect(&endpoint, &key).await?;
            AgentLink::new(config.device_id, store.clone())
                .transfer(socket, works.clone(), config.remote_control.clone())
                .await
        }
        .await;
        if let Err(error) = result {
            eprintln!("Relay: {error}");
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}
/// Native admission can take seconds; it must not block WSS heartbeats or event ACKs.
pub async fn run_queue(store: Arc<Store>, works: WorkMap) {
    loop {
        if let Err(error) = advance_queue(&store, &works).await {
            eprintln!("Follow-up queue: {error}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
