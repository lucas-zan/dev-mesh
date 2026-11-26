use super::{codex, pi, process::Peer, NativeState};
use devmesh_protocol::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::{broadcast, Mutex};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeKind {
    Codex,
    Pi,
    PiDurable,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Launch {
    pub kind: RuntimeKind,
    #[serde(default)]
    pub codex_unattended: bool,
    pub program: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub resume: Option<String>,
}
pub struct Start<'a> {
    pub work: uuid::Uuid,
    pub session: uuid::Uuid,
    pub cwd: &'a Path,
    pub issue: &'a Value,
    pub message: &'a str,
    pub model: Option<&'a str>,
    pub resume: Option<&'a str>,
}
pub struct Runtime {
    peer: Peer,
    pub state: Arc<Mutex<NativeState>>,
    pub kind: RuntimeKind,
    events: broadcast::Sender<Value>,
    binding: Mutex<Option<(uuid::Uuid, uuid::Uuid)>>,
}
impl Runtime {
    pub async fn launch(config: &Launch) -> Result<(Self, broadcast::Receiver<Value>)> {
        let approval_timeout_seconds =
            launch_timeout(&config.env, "DEVMESH_APPROVAL_TIMEOUT_SECONDS", 900)?;
        let input_timeout_seconds =
            launch_timeout(&config.env, "DEVMESH_INPUT_TIMEOUT_SECONDS", 1800)?;
        let (peer, mut native) =
            Peer::spawn(&config.program, &config.args, &config.cwd, &config.env)?;
        let state = Arc::new(Mutex::new(NativeState {
            approval_timeout_seconds,
            input_timeout_seconds,
            ..NativeState::default()
        }));
        if config.kind == RuntimeKind::PiDurable {
            let hello = tokio::time::timeout(std::time::Duration::from_secs(10), native.recv())
                .await
                .map_err(|_| Error::new("invalid_response", "Durable Host hello timed out"))?
                .map_err(|_| {
                    Error::new("invalid_response", "Durable Host disconnected before hello")
                })?;
            if hello["type"] != "hello" || hello["ipc_version"] != "1.1" {
                return Err(Error::new(
                    "incompatible_version",
                    "Expected Durable IPC 1.1 hello",
                ));
            }
            state.lock().await.instance = hello["runtime_instance_id"]
                .as_str()
                .and_then(|id| id.parse().ok())
                .ok_or_else(|| {
                    Error::new("invalid_response", "Durable Host instance ID missing")
                })?;
        }
        let (events, receive) = broadcast::channel(256);
        let kind = config.kind;
        let observed = state.clone();
        let send = events.clone();
        tokio::spawn(async move {
            loop {
                let raw = match native.recv().await {
                    Ok(v) => v,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        let _ = send.send(super::event(
                            "capture.gap",
                            json!({"reason":"native_buffer_overflow","dropped_count":n,"source_range":null}),
                        ));
                        continue;
                    }
                    Err(_) => break,
                };
                let mut state = observed.lock().await;
                let normalized = match kind {
                    RuntimeKind::Codex => codex::normalize(&raw, &mut state),
                    RuntimeKind::Pi => pi::normalize(&raw, &mut state),
                    RuntimeKind::PiDurable => normalize_durable(&raw, &mut state),
                };
                for event in normalized {
                    let _ = send.send(event);
                }
                if raw["type"] == "process_exited" || raw["type"] == "transport_error" {
                    state.status = "unknown".into();
                    let _ = send.send(super::event(
                        "runtime.disconnected",
                        json!({"reason":raw["type"]}),
                    ));
                    break;
                }
            }
        });
        let runtime = Self {
            peer,
            state,
            kind: config.kind,
            events,
            binding: Mutex::new(None),
        };
        if let Err(error) = runtime.initialize(config).await {
            runtime.peer.terminate().await?;
            return Err(error);
        }
        Ok((runtime, receive))
    }
    async fn initialize(&self, config: &Launch) -> Result<()> {
        if self.kind != RuntimeKind::PiDurable {
            let output = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                tokio::process::Command::new(&config.program)
                    .arg("--version")
                    .output(),
            )
            .await;
            let version = match output {
                Ok(Ok(output)) if output.status.success() => {
                    String::from_utf8_lossy(&output.stdout).trim().to_owned()
                }
                _ => "unknown".into(),
            };
            let mut state = self.state.lock().await;
            state.runtime_version = version;
            state.capabilities = json!({"message_events":true,"tool_events":true,"steer":true,"follow_up":true,"prompt":true,"interrupt":true,"local_approval":true,"local_input":true,"resume":true,"child_sessions":false});
        }
        match self.kind {
            RuntimeKind::Codex => {
                self.peer.call(json!({"jsonrpc":"2.0","method":"initialize","params":{"clientInfo":{"name":"devmesh","title":"DevMesh","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}}})).await?;
                self.peer
                    .send(&json!({"jsonrpc":"2.0","method":"initialized"}))
                    .await?;
                let method = if config.resume.is_some() {
                    "thread/resume"
                } else {
                    "thread/start"
                };
                let (sandbox, approval_policy) = if config.codex_unattended {
                    ("danger-full-access", "never")
                } else {
                    ("workspace-write", "on-request")
                };
                let mut params =
                    json!({"cwd":config.cwd,"sandbox":sandbox,"approvalPolicy":approval_policy});
                if let Some(id) = &config.resume {
                    params["threadId"] = json!(id);
                }
                if let Some(model) = &config.model {
                    params["model"] = json!(model);
                }
                let response = self
                    .peer
                    .call(json!({"jsonrpc":"2.0","method":method,"params":params}))
                    .await?;
                let id = response["thread"]["id"].as_str().ok_or_else(|| {
                    Error::new("invalid_response", "Codex did not return a Thread ID")
                })?;
                let mut state = self.state.lock().await;
                state.native_id = Some(id.into());
                state.resume_ref = Some(id.into());
                state.status = "idle".into();
            }
            RuntimeKind::Pi => {
                let response = self.peer.call(json!({"type":"get_state"})).await?;
                let mut state = self.state.lock().await;
                state.native_id = response["sessionId"].as_str().map(str::to_owned);
                state.resume_ref = response["sessionFile"].as_str().map(str::to_owned);
                state.capabilities["resume"] = json!(state.resume_ref.is_some());
                state.status = if response["isStreaming"] == true {
                    "running"
                } else {
                    "idle"
                }
                .into();
                if state.native_id.is_none() {
                    return Err(Error::new(
                        "invalid_response",
                        "Pi did not return sessionId",
                    ));
                }
            }
            RuntimeKind::PiDurable => {
                // The standard Host start/resume receives the work-specific IPC context on start().
                let probe=self.peer.call(json!({"type":"request","method":"probe","work_id":null,"session_id":null,"expected_epoch":null,"args":{}})).await?;
                let mut state = self.state.lock().await;
                state.runtime_version = probe["version"].as_str().unwrap_or("unknown").into();
                state.capabilities = probe["capabilities"].clone();
                state.capabilities["follow_up"] = json!(true);
            }
        }
        Ok(())
    }
    pub async fn start(&self, request: Start<'_>) -> Result<Value> {
        let Start {
            work,
            session,
            cwd,
            issue,
            message,
            model,
            resume,
        } = request;
        *self.binding.lock().await = Some((work, session));
        if self.kind != RuntimeKind::PiDurable {
            let epoch = self.state.lock().await.epoch.to_string();
            return self.control("prompt", message, Some(&epoch)).await;
        }
        let mut args = json!({"cwd":cwd,"issue_ref":issue,"initial_prompt":message,"runtime_config_ref":model.unwrap_or("default")});
        let method = if let Some(native) = resume {
            args = json!({"native_session_id":native,"workspace":cwd,"checkpoint_ref":null});
            "resume"
        } else {
            "start"
        };
        let result=self.peer.call(json!({"type":"request","method":method,"work_id":work,"session_id":session,"expected_epoch":null,"args":args})).await?;
        let mut state = self.state.lock().await;
        state.native_id = result["native_session_id"].as_str().map(str::to_owned);
        state.resume_ref = state.native_id.clone();
        state.epoch = result["run_epoch"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        state.status = result["state"].as_str().unwrap_or("unknown").into();
        if let Some(instance) = result["runtime_instance_id"]
            .as_str()
            .and_then(|s| s.parse().ok())
        {
            state.instance = instance;
        }
        Ok(result)
    }
    pub async fn process_identity(&self) -> Result<Value> {
        self.peer.identity().await
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }
    pub async fn control(&self, method: &str, message: &str, epoch: Option<&str>) -> Result<Value> {
        let state = self.state.lock().await.clone();
        if self.kind == RuntimeKind::Pi && method == "steer" {
            pi::request(method, &state, message, epoch)?;
            let command_id = uuid::Uuid::new_v4();
            let mut events = self.events.subscribe();
            let args =
                json!({"command_id":command_id,"expected_epoch":state.epoch,"message":message});
            self.peer
                .call(json!({"type":"prompt","message":format!("/devmesh-steer {args}")}))
                .await?;
            return tokio::time::timeout(std::time::Duration::from_secs(10), async {
                while let Ok(event) = events.recv().await {
                    if event["type"] == "command.native_receipt"
                        && event["payload"]["command_id"] == command_id.to_string()
                    {
                        if event["payload"]["state"] == "rejected" {
                            return Err(Error::new(
                                "runtime_rejected",
                                event["payload"]["reason"]
                                    .as_str()
                                    .unwrap_or("Steer rejected"),
                            ));
                        }
                        return Ok(event["payload"].clone());
                    }
                }
                Err(Error::new("unknown_outcome", "Pi receipt stream ended"))
            })
            .await
            .map_err(|_| {
                Error::new(
                    "unknown_outcome",
                    "Pi native extension receipt not observed",
                )
            })?;
        }
        let value = match self.kind {
            RuntimeKind::Codex => codex::request(method, &state, message, epoch)?,
            RuntimeKind::Pi => pi::request(method, &state, message, epoch)?,
            RuntimeKind::PiDurable => {
                super::check_epoch(&state, epoch)?;
                {
                    let binding = self
                        .binding
                        .lock()
                        .await
                        .ok_or_else(|| Error::new("invalid_state", "No bound Work"))?;
                    json!({"type":"request","method":method,"work_id":binding.0,"session_id":binding.1,"expected_epoch":epoch,"args":{"command_id":uuid::Uuid::new_v4(),"message":message}})
                }
            }
        };
        let response = self.peer.call(value).await?;
        if self.kind == RuntimeKind::Codex && method == "prompt" {
            let mut state = self.state.lock().await;
            let id = response["turn"]["id"].as_str().map(str::to_owned);
            if state.turn_id != id {
                state.epoch += 1;
                state.turn_id = id;
                state.status = if matches!(
                    response["turn"]["status"].as_str(),
                    Some("completed" | "failed" | "interrupted")
                ) {
                    "idle"
                } else {
                    "running"
                }
                .into();
            }
        }
        if self.kind == RuntimeKind::Pi
            && method == "prompt"
            && response["disposition"] == "started"
        {
            let mut current = self.state.lock().await;
            if current.epoch == state.epoch {
                current.status = "running".into();
            }
        }
        Ok(response)
    }
    pub async fn resolve(
        &self,
        id: &str,
        decision: &str,
        input: Option<&str>,
        answers: Option<&Value>,
    ) -> Result<()> {
        if answers.is_some() && self.kind != RuntimeKind::Codex {
            return Err(Error::new(
                "unsupported_capability",
                "Structured answers require Codex input",
            ));
        }
        let mut state = self.state.lock().await;
        let native_id = if state.approvals.contains_key(id) {
            id.to_owned()
        } else {
            state
                .approval_bindings
                .iter()
                .find(|(_, binding)| binding.id.to_string() == id)
                .map(|(native, _)| native.clone())
                .ok_or_else(|| Error::new("not_found", "Approval request no longer active"))?
        };
        let original = state
            .approvals
            .get(&native_id)
            .cloned()
            .ok_or_else(|| Error::new("not_found", "Approval request no longer active"))?;
        match self.kind {
            RuntimeKind::Codex => {
                let result = if let Some(answers) = answers {
                    codex::approval_answers(&original, answers)?
                } else {
                    codex::approval_result(&original, decision, input)?
                };
                self.peer
                    .send(&json!({"jsonrpc":"2.0","id":original["id"],"result":result}))
                    .await?;
            }
            RuntimeKind::Pi => {
                self.peer.send(&if let Some(text)=input{json!({"type":"extension_ui_response","id":original["id"],"value":text})}else{json!({"type":"extension_ui_response","id":original["id"],"confirmed":decision=="approve"})}).await?;
            }
            RuntimeKind::PiDurable => {
                let binding = self
                    .binding
                    .lock()
                    .await
                    .ok_or_else(|| Error::new("invalid_state", "No bound Work"))?;
                self.peer
                    .call(durable_approval(
                        binding.0,
                        binding.1,
                        state.epoch,
                        &native_id,
                        decision,
                        input,
                    )?)
                    .await?;
            }
        }
        let outcome = if input.is_some() || answers.is_some() {
            "answered"
        } else if decision == "approve" {
            "approved"
        } else {
            "denied"
        };
        if let Some(binding) = state.approval_bindings.get_mut(&native_id) {
            binding.decision = Some(outcome.into());
        }
        state.approvals.remove(&native_id);
        if self.kind == RuntimeKind::Pi {
            let observed = self.peer.call(json!({"type":"get_state"})).await?;
            state.status = if observed["isStreaming"] == true {
                "running"
            } else {
                "idle"
            }
            .into();
            if let Some(event) =
                super::projection::approval_resolved(&mut state, &native_id, Some("unknown"))
            {
                let _ = self.events.send(event);
            }
            let _=self.events.send(super::event("session.state",json!({"previous":null,"state":state.status,"connection":"unknown","runtime_instance_id":state.instance,"run_epoch":state.epoch.to_string()})));
        }
        Ok(())
    }
    pub async fn stop(&self) -> Result<bool> {
        let state = self.state.lock().await.clone();
        if state.status == "unknown" {
            return Ok(false);
        }
        match self.kind {
            RuntimeKind::Codex if state.status != "idle" => {
                self.peer
                    .call(codex::request("stop", &state, "", None)?)
                    .await?;
                for _ in 0..100 {
                    if self.state.lock().await.status == "idle" {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                if self.state.lock().await.status != "idle" {
                    return Ok(false);
                }
            }
            RuntimeKind::Pi => {
                self.peer.call(json!({"type":"clear_queue"})).await?;
                self.peer.call(json!({"type":"abort"})).await?;
            }
            RuntimeKind::PiDurable => {
                let binding = self
                    .binding
                    .lock()
                    .await
                    .ok_or_else(|| Error::new("invalid_state", "No bound Work"))?;
                let result=self.peer.call(json!({"type":"request","method":"stop","work_id":binding.0,"session_id":binding.1,"expected_epoch":state.epoch.to_string(),"args":{"reason":"work_finalizing"}})).await?;
                if result["quiesced"] != true {
                    return Ok(false);
                }
            }
            _ => {}
        }
        self.peer.terminate().await?;
        self.state.lock().await.status = "ended".into();
        Ok(true)
    }
}
pub fn normalize_durable(raw: &Value, state: &mut NativeState) -> Vec<Value> {
    if raw["type"] != "event" {
        return vec![];
    }
    if let Some(id) = raw["native_session_id"].as_str() {
        state.native_id = Some(id.into());
    }
    if let Some(epoch) = raw["run_epoch"].as_str().and_then(|s| s.parse().ok()) {
        state.epoch = epoch;
    }
    if raw["event_type"] == "turn.started" {
        state.status = "running".into();
    }
    if raw["event_type"] == "turn.completed" {
        state.status = "idle".into();
    }
    let kind = raw["event_type"].as_str().unwrap_or("x.durable");
    if kind == "approval.requested" {
        let id = raw["payload"]["approval_id"].as_str().unwrap_or("");
        return vec![super::projection::approval_requested(
            state,
            id,
            "approval",
            raw.clone(),
        )];
    }
    if kind == "approval.resolved" {
        let id = raw["payload"]["approval_id"].as_str().unwrap_or("");
        let outcome = match raw["payload"]["outcome"].as_str() {
            Some("approve" | "approved") => "approved",
            Some("deny" | "denied") => "denied",
            _ => "unknown",
        };
        return super::projection::approval_resolved(state, id, Some(outcome))
            .into_iter()
            .collect();
    }
    let payload = if kind == "message.completed" {
        crate::collector::sanitize(
            raw["payload"]["text"].as_str().unwrap_or(""),
            raw["payload"]["author_kind"].as_str().unwrap_or("agent"),
        )
    } else {
        raw["payload"].clone()
    };
    let mut payload = payload;
    if matches!(kind, "turn.started" | "turn.completed") {
        payload["native_turn_id"] = Value::Null;
    }
    if matches!(kind, "turn.completed" | "tool.completed") {
        payload["outcome"] = json!(super::projection::outcome(
            payload["outcome"].as_str().unwrap_or("unknown")
        ));
    }
    if matches!(kind, "tool.started" | "tool.completed") && !payload["tool_call_id"].is_string() {
        payload["tool_call_id"] = json!(payload["tool_call_id"].to_string());
    }
    if kind == "tool.completed" {
        payload["content_policy"] = super::projection::tool_policy();
    }
    vec![super::event(kind, payload)]
}

pub fn durable_approval(
    work: uuid::Uuid,
    session: uuid::Uuid,
    epoch: u64,
    id: &str,
    decision: &str,
    input: Option<&str>,
) -> Result<Value> {
    if input.is_some() {
        return Err(Error::new(
            "unsupported_capability",
            "Durable Host does not support local input",
        ));
    }
    if !matches!(decision, "approve" | "deny") {
        return Err(Error::new("invalid_request", "approve or deny required"));
    }
    Ok(
        json!({"type":"request","method":"resolve_approval","work_id":work,"session_id":session,"expected_epoch":epoch.to_string(),"args":{"approval_id":id,"decision":decision}}),
    )
}

fn launch_timeout(env: &[(String, String)], key: &str, default: u64) -> Result<u64> {
    let Some((_, value)) = env.iter().find(|(name, _)| name == key) else {
        return Ok(default);
    };
    value
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0 && *n <= i64::MAX as u64 / 1000)
        .ok_or_else(|| {
            Error::new(
                "config_error",
                format!("{key} must be a positive duration in seconds"),
            )
        })
}
