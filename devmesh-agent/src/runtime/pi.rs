use super::{check_epoch, event, NativeState};
use devmesh_protocol::{Error, Result};
use serde_json::{json, Value};

pub fn request(
    method: &str,
    state: &NativeState,
    message: &str,
    epoch: Option<&str>,
) -> Result<Value> {
    if matches!(method, "steer" | "prompt" | "interrupt") {
        check_epoch(state, epoch)?;
    }
    match method {
        "steer" if state.status == "running" => Ok(json!({"type":"steer","message":message})),
        "prompt" if state.status == "idle" => Ok(json!({"type":"prompt","message":message})),
        "interrupt" | "stop" => Ok(json!({"type":"abort"})),
        "status" => Ok(json!({"type":"get_state"})),
        _ => Err(Error::new(
            "invalid_state",
            format!("{method} while {}", state.status),
        )),
    }
}
pub fn normalize(value: &Value, state: &mut NativeState) -> Vec<Value> {
    match value["type"].as_str().unwrap_or("") {
        "agent_start" => {
            state.status = "running".into();
            state.epoch += 1;
            vec![event(
                "turn.started",
                json!({"run_epoch":state.epoch.to_string(),"native_turn_id":null}),
            )]
        }
        "agent_end" => {
            state.status = "idle".into();
            vec![event(
                "turn.completed",
                json!({"run_epoch":state.epoch.to_string(),"native_turn_id":null,"outcome":"success"}),
            )]
        }
        "message_end" => {
            let m = &value["message"];
            if m["customType"] == "devmesh.control.receipt" {
                if let Some(text) = m["content"].as_str() {
                    if let Ok(receipt) = serde_json::from_str::<Value>(text) {
                        return vec![event("command.native_receipt", receipt)];
                    }
                }
            }
            let role = m["role"].as_str().unwrap_or("");
            if !matches!(role, "assistant" | "user") {
                return vec![];
            }
            let text = if m["content"].is_string() {
                m["content"].as_str().unwrap().to_owned()
            } else {
                super::codex::text_content(&m["content"])
            };
            if text.is_empty() {
                return vec![];
            }
            vec![event(
                "message.completed",
                crate::collector::sanitize(
                    &text,
                    if role == "assistant" { "agent" } else { "user" },
                ),
            )]
        }
        "tool_execution_start" | "tool_execution_end" => {
            let complete = value["type"] == "tool_execution_end";
            let mut payload = json!({"tool_call_id":value["toolCallId"],"tool":value["toolName"],"summary":if complete{"Tool finished"}else{"Tool started"}});
            if complete {
                payload["outcome"] = json!(if value["isError"] == true {
                    "failed"
                } else {
                    "success"
                });
                payload["duration_ms"] = json!(0);
                payload["content_policy"] = super::projection::tool_policy();
            }
            vec![event(
                if complete {
                    "tool.completed"
                } else {
                    "tool.started"
                },
                payload,
            )]
        }
        "extension_ui_request" => {
            let method = value["method"].as_str().unwrap_or("");
            if !matches!(method, "confirm" | "input" | "editor" | "select") {
                return vec![];
            }
            let id = value["id"].as_str().unwrap_or("");
            vec![super::projection::approval_requested(
                state,
                id,
                if method == "confirm" {
                    "approval"
                } else {
                    "input"
                },
                value.clone(),
            )]
        }
        _ => vec![],
    }
}
