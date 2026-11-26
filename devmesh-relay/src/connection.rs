use crate::{
    http::{AppState, Connection},
    Actor, Change,
};
use devmesh_protocol::{now, Error, Frame, Result};
use serde_json::{json, Value};
use std::sync::{Arc, LazyLock};
use tokio::io::{AsyncRead, AsyncWrite};
use uuid::Uuid;

static SCHEMA: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../devmesh-protocol/schemas/wire.schema.json"
    ))
    .unwrap()
});
fn check_payload(frame: &Frame) -> Result<()> {
    frame.check_version()?;
    let schema = SCHEMA["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["properties"]["type"]["const"] == frame.kind)
        .ok_or_else(|| Error::new("invalid_schema", "Unknown frame type"))?;
    let payload = &schema["properties"]["payload"];
    if let Some(properties) = payload["properties"].as_object() {
        let object = frame
            .payload
            .as_object()
            .ok_or_else(|| Error::new("invalid_schema", "Payload must be object"))?;
        if object.keys().any(|k| !properties.contains_key(k)) {
            return Err(Error::new("invalid_schema", "Unknown payload field"));
        }
        for required in payload["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !object.contains_key(required) {
                return Err(Error::new("invalid_schema", format!("Missing {required}")));
            }
        }
    }
    Ok(())
}
pub async fn run<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    state: AppState,
    credential: String,
    actor: Actor,
    socket: devmesh_protocol::socket::Socket<S>,
) -> Result<()> {
    let (output, mut input) = socket.into_channels();
    let first = tokio::time::timeout(std::time::Duration::from_secs(10), input.recv())
        .await
        .map_err(|_| Error::new("unavailable", "Registration timed out"))?
        .ok_or_else(|| Error::new("unavailable", "Registration stream ended"))??;
    let register: Frame = serde_json::from_str(&first)
        .map_err(|_| Error::new("invalid_schema", "Invalid registration"))?;
    check_payload(&register)?;
    if register.kind != "connection.register"
        || register.payload["device_id"].as_str() != actor.device_id.as_deref()
        || !register.payload["supported_versions"]
            .as_array()
            .is_some_and(|v| v.iter().any(|v| v == "1.1"))
    {
        return Err(Error::new(
            "incompatible_version",
            "Invalid device or supported protocol",
        ));
    }
    let device = actor.device_id.as_deref().unwrap().to_owned();
    let connection = Uuid::new_v4();
    let stop = Arc::new(tokio::sync::Notify::new());
    if let Some(old) = state.connections.lock().unwrap().insert(
        device.clone(),
        Connection {
            epoch: connection,
            output: output.clone(),
            stop: stop.clone(),
        },
    ) {
        old.stop.notify_one();
    }
    state.relay.set_device_online(&actor, true)?;
    let mut record = state.relay.store.get("devices", &device)?.unwrap();
    let epoch = record["connection_epoch"].as_u64().unwrap_or(0) + 1;
    record["connection_epoch"] = json!(epoch);
    state
        .relay
        .store
        .commit(&[Change::put("devices", &device, record)], &[])?;
    let mut ack = Frame::new(
        "connection.ack",
        json!({"connection_id":connection,"connection_epoch":epoch.to_string(),"selected_version":"1.1","server_time":now(),"heartbeat_seconds":10}),
    );
    ack.reply_to = Some(register.frame_id);
    output
        .send(serde_json::to_string(&ack).unwrap())
        .await
        .map_err(|_| Error::new("unavailable", "Connection closed"))?;
    for command in state.relay.pending_commands(&actor)? {
        state
            .relay
            .mark_dispatched(crate::events::field(&command, "command_id")?)?;
        output
            .send(serde_json::to_string(&crate::commands::dispatch_frame(&command)).unwrap())
            .await
            .map_err(|_| Error::new("unavailable", "Connection closed during command replay"))?;
    }
    let mut last_seen = tokio::time::Instant::now();
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(10));
    loop {
        tokio::select! {
            _=stop.notified()=>break,
            _=heartbeat.tick()=>{
                state.relay.expire_commands()?;
                if last_seen.elapsed().as_secs()>30||state.relay.authenticate(&credential).is_err(){break;}
                let frame=Frame::new("connection.heartbeat",json!({"connection_epoch":epoch.to_string(),"nonce":Uuid::new_v4(),"reply":false}));if output.send(serde_json::to_string(&frame).unwrap()).await.is_err(){break;}
            },
            item=input.recv()=>{
                let text=match item{Some(Ok(text))=>text,_=>break};
                let actor=match state.relay.authenticate(&credential){Ok(a)=>a,Err(_)=>break};
                let current=state.connections.lock().unwrap().get(&device).is_some_and(|c|c.epoch==connection);if !current{break;}
                let frame:Frame=match serde_json::from_str(&text){Ok(f)=>f,Err(_)=>break};last_seen=tokio::time::Instant::now();
                let result=check_payload(&frame).and_then(|_|handle(&state,&actor,&frame,epoch));
                let mut reply=match result{Ok(Some(frame))=>frame,Ok(None)=>continue,Err(error)=>Frame::new("protocol.error",json!({"request_id":frame.frame_id,"code":error.code,"message":error.message,"retryable":false}))};
                reply.reply_to=Some(frame.frame_id);if output.send(serde_json::to_string(&reply).unwrap()).await.is_err(){break;}
            }
        }
    }
    let current = state
        .connections
        .lock()
        .unwrap()
        .get(&device)
        .is_some_and(|c| c.epoch == connection);
    if current {
        state.connections.lock().unwrap().remove(&device);
        state.relay.set_device_online(&actor, false)?;
    }
    Ok(())
}
fn handle(state: &AppState, actor: &Actor, frame: &Frame, epoch: u64) -> Result<Option<Frame>> {
    if frame.kind == "connection.heartbeat" {
        if frame.payload["connection_epoch"]
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            != Some(epoch)
        {
            return Err(Error::new("stale_connection", "Connection epoch changed"));
        }
        state.relay.set_device_online(actor, true)?;
        if frame.payload["reply"] == true {
            return Ok(None);
        }
        let mut payload = frame.payload.clone();
        payload["reply"] = json!(true);
        return Ok(Some(Frame::new("connection.heartbeat", payload)));
    }
    let key = format!("frame:{}:{}", actor.user_id, frame.frame_id);
    if let Some(stored) = state.relay.store.get("settings", &key)? {
        if stored["request"] != serde_json::to_value(frame).unwrap() {
            return Err(Error::new(
                "idempotency_conflict",
                "Frame ID reused with different content",
            ));
        }
        return serde_json::from_value(stored["response"].clone())
            .map(Some)
            .map_err(|_| Error::new("storage_error", "Invalid prior response"));
    }
    let p = &frame.payload;
    let reply = match frame.kind.as_str() {
        "work.register" => {
            state.relay.register_work(actor, p.clone())?;
            record_ack("work", frame.frame_id)
        }
        "session.register" => {
            state.relay.register_session(actor, p.clone())?;
            record_ack("session", frame.frame_id)
        }
        "session.status" => {
            state.relay.update_session(actor, p.clone())?;
            record_ack("status", frame.frame_id)
        }
        "session.events" => {
            let mut ack = 0;
            for event in p["events"]
                .as_array()
                .ok_or_else(|| Error::new("invalid_schema", "events array required"))?
            {
                if event["session_id"] != p["session_id"] {
                    return Err(Error::new("invalid_target", "Event batch Session mismatch"));
                }
                ack = state.relay.ingest(actor, event.clone())?;
            }
            Frame::new(
                "events.ack",
                json!({"session_id":p["session_id"],"acked_sequence":ack.to_string(),"missing_sequences":[]}),
            )
        }
        "command.receipt" => {
            state.relay.receipt(actor, p.clone())?;
            record_ack("receipt", frame.frame_id)
        }
        "catalog.acquire" => {
            let lease = state
                .relay
                .acquire_catalog(actor, crate::events::field(p, "project_id")?)?;
            Frame::new(
                "catalog.lease",
                json!({"project_id":lease["project_id"],"epoch":lease["epoch"],"expires_at":lease["expires_at"],"renew_after_seconds":30}),
            )
        }
        "catalog.snapshot" | "catalog.delta" => {
            Frame::new("catalog.ack", state.relay.catalog_page(actor, p.clone())?)
        }
        "catalog.refresh" => return Ok(None),
        _ => {
            return Err(Error::new(
                "invalid_request",
                "Frame direction is not allowed",
            ))
        }
    };
    state.relay.store.commit(
        &[Change::put(
            "settings",
            key,
            json!({"request":frame,"response":reply}),
        )],
        &[],
    )?;
    Ok(Some(reply))
}
fn record_ack(kind: &str, id: Uuid) -> Frame {
    Frame::new(
        "record.ack",
        json!({"record_type":kind,"record_id":id,"result":"accepted"}),
    )
}
