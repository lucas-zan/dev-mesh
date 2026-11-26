//! Newline-delimited JSON process transport; stdout is reserved for the native protocol.
use devmesh_protocol::{Error, Result};
use serde_json::{json, Value};
use std::{collections::HashMap, path::Path, process::Stdio, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{broadcast, oneshot, Mutex},
};
use uuid::Uuid;

type Pending = Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>;
pub struct Peer {
    input: Mutex<ChildStdin>,
    child: Mutex<Child>,
    pending: Pending,
    events: broadcast::Sender<Value>,
}
impl Peer {
    pub fn spawn(
        program: &str,
        args: &[String],
        cwd: &Path,
        env: &[(String, String)],
    ) -> Result<(Self, broadcast::Receiver<Value>)> {
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        for (key, value) in env {
            command.env(key, value);
        }
        let mut child = command
            .spawn()
            .map_err(|e| Error::new("runtime_unavailable", e.to_string()))?;
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (events, rx) = broadcast::channel(256);
        tokio::spawn(read_loop(
            BufReader::new(output).lines(),
            pending.clone(),
            events.clone(),
        ));
        Ok((
            Self {
                input: Mutex::new(input),
                child: Mutex::new(child),
                pending,
                events,
            },
            rx,
        ))
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }
    pub async fn send(&self, value: &Value) -> Result<()> {
        let text = format!("{value}\n");
        if text.len() > 1024 * 1024 {
            return Err(Error::new("payload_too_large", "IPC frame exceeds 1MiB"));
        }
        let mut input = self.input.lock().await;
        input
            .write_all(text.as_bytes())
            .await
            .map_err(|e| Error::new("ipc_error", e.to_string()))?;
        input
            .flush()
            .await
            .map_err(|e| Error::new("ipc_error", e.to_string()))
    }
    pub async fn call(&self, mut value: Value) -> Result<Value> {
        let id = Uuid::new_v4().to_string();
        if value["type"] == "request" {
            value["request_id"] = json!(id);
        } else {
            value["id"] = json!(id);
        }
        let (send, receive) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), send);
        if let Err(error) = self.send(&value).await {
            self.pending.lock().await.remove(&id);
            return Err(Error::new(
                "unknown_outcome",
                format!("Native admission could not be confirmed: {}", error.code),
            ));
        }
        let response = tokio::time::timeout(std::time::Duration::from_secs(30), receive).await;
        self.pending.lock().await.remove(&id);
        let value = response
            .map_err(|_| {
                Error::new(
                    "unknown_outcome",
                    "Native response timed out; do not replay",
                )
            })?
            .map_err(|_| Error::new("unknown_outcome", "Native process disconnected"))?;
        if value.get("error").is_some() || value["success"] == false || value["ok"] == false {
            return Err(Error::new(
                value["error"]["code"]
                    .as_str()
                    .unwrap_or("runtime_rejected"),
                value
                    .get("error")
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "Native request rejected".into()),
            ));
        }
        Ok(value
            .get("result")
            .or_else(|| value.get("data"))
            .cloned()
            .unwrap_or(value))
    }
    pub async fn identity(&self) -> Result<Value> {
        let pid = self
            .child
            .lock()
            .await
            .id()
            .ok_or_else(|| Error::new("runtime_unavailable", "Runtime process exited"))?;
        crate::recovery::process_identity(pid)
    }
    pub async fn exited(&self) -> Result<bool> {
        Ok(self
            .child
            .lock()
            .await
            .try_wait()
            .map_err(|e| Error::new("runtime_error", e.to_string()))?
            .is_some())
    }
    pub async fn terminate(&self) -> Result<()> {
        let mut child = self.child.lock().await;
        if child
            .try_wait()
            .map_err(|e| Error::new("runtime_error", e.to_string()))?
            .is_none()
        {
            child
                .kill()
                .await
                .map_err(|e| Error::new("runtime_error", e.to_string()))?;
        }
        child
            .wait()
            .await
            .map_err(|e| Error::new("runtime_error", e.to_string()))?;
        Ok(())
    }
}
async fn read_loop(
    mut lines: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    pending: Pending,
    events: broadcast::Sender<Value>,
) {
    while let Ok(Some(line)) = lines.next_line().await {
        if line.len() > 1024 * 1024 {
            let _ = events.send(json!({"type":"transport_error","reason":"frame_too_large"}));
            break;
        }
        let value: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                let _ = events.send(json!({"type":"transport_error","reason":"invalid_json"}));
                break;
            }
        };
        let response = value.get("result").is_some()
            || value.get("error").is_some()
            || value["type"] == "response";
        if response {
            if let Some(id) = value
                .get("request_id")
                .or_else(|| value.get("id"))
                .and_then(Value::as_str)
            {
                if let Some(sender) = pending.lock().await.remove(id) {
                    let _ = sender.send(value);
                    continue;
                }
            }
        }
        let _ = events.send(value);
    }
    pending.lock().await.clear();
    let _ = events.send(json!({"type":"process_exited"}));
}
