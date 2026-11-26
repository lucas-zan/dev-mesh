use super::{event, NativeState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApprovalBinding {
    pub id: uuid::Uuid,
    pub kind: String,
    pub decision: Option<String>,
}
pub fn approval_requested(state: &mut NativeState, native: &str, kind: &str, raw: Value) -> Value {
    let binding = state
        .approval_bindings
        .entry(native.into())
        .or_insert_with(|| ApprovalBinding {
            id: uuid::Uuid::new_v4(),
            kind: kind.into(),
            decision: None,
        });
    let id = binding.id;
    state.approvals.insert(native.into(), raw);
    state.status = if kind == "input" {
        "waiting_input"
    } else {
        "waiting_approval"
    }
    .into();
    event(
        "approval.requested",
        json!({"approval_id":id,"kind":kind,"deadline":(chrono::Utc::now()+chrono::Duration::seconds(if kind=="input"{state.input_timeout_seconds as i64}else{state.approval_timeout_seconds as i64})).to_rfc3339_opts(chrono::SecondsFormat::Secs,true),"summary":"Runtime requests a local decision"}),
    )
}
pub fn approval_resolved(
    state: &mut NativeState,
    native: &str,
    outcome: Option<&str>,
) -> Option<Value> {
    let binding = state.approval_bindings.get(native)?;
    let payload = json!({"approval_id":binding.id,"kind":binding.kind,"outcome":outcome.or(binding.decision.as_deref()).unwrap_or("unknown")});
    state.approvals.remove(native);
    if state.approvals.is_empty()
        && matches!(state.status.as_str(), "waiting_input" | "waiting_approval")
    {
        state.status = "running".into();
    }
    Some(event("approval.resolved", payload))
}
pub fn outcome(native: &str) -> &'static str {
    match native {
        "completed" | "success" => "success",
        "failed" | "error" => "failed",
        "interrupted" | "cancelled" | "aborted" => "cancelled",
        _ => "unknown",
    }
}
pub fn tool_policy() -> Value {
    json!({"policy_id":"issue-share-v1","redacted":false,"truncated":false,"omitted":true,"original_bytes":0})
}
