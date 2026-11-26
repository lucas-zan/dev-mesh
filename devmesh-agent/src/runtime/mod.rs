pub mod codex;
mod codex_command;
mod managed;
mod projection;
pub use projection::ApprovalBinding;
pub mod pi;
pub mod process;
pub use managed::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeState {
    pub instance: Uuid,
    pub native_id: Option<String>,
    #[serde(default)]
    pub resume_ref: Option<String>,
    pub turn_id: Option<String>,
    pub epoch: u64,
    #[serde(default)]
    pub runtime_version: String,
    #[serde(default)]
    pub capabilities: Value,
    pub status: String,
    #[serde(default = "default_approval_timeout")]
    pub approval_timeout_seconds: u64,
    #[serde(default = "default_input_timeout")]
    pub input_timeout_seconds: u64,
    pub approvals: BTreeMap<String, Value>,
    #[serde(default)]
    pub approval_bindings: BTreeMap<String, ApprovalBinding>,
}
impl Default for NativeState {
    fn default() -> Self {
        Self {
            runtime_version: "unknown".into(),
            capabilities: Value::Null,
            instance: Uuid::new_v4(),
            native_id: None,
            resume_ref: None,
            turn_id: None,
            epoch: 0,
            status: "starting".into(),
            approval_timeout_seconds: default_approval_timeout(),
            input_timeout_seconds: default_input_timeout(),
            approvals: BTreeMap::new(),
            approval_bindings: BTreeMap::new(),
        }
    }
}
pub fn check_epoch(state: &NativeState, epoch: Option<&str>) -> devmesh_protocol::Result<()> {
    if epoch != Some(state.epoch.to_string().as_str()) {
        return Err(devmesh_protocol::Error::new(
            "stale_turn",
            "Expected current top-level run epoch",
        ));
    }
    Ok(())
}
pub fn event(kind: &str, payload: Value) -> Value {
    serde_json::json!({"type":kind,"payload":payload})
}

fn default_approval_timeout() -> u64 {
    900
}
fn default_input_timeout() -> u64 {
    1800
}
