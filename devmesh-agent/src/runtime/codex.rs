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
    let native = state
        .native_id
        .as_deref()
        .ok_or_else(|| Error::new("invalid_state", "No native Thread"))?;
    let input = json!([{"type":"text","text":message,"text_elements":[]}]);
    match method {
        "steer" if state.status == "running" => Ok(
            json!({"jsonrpc":"2.0","method":"turn/steer","params":{"threadId":native,"expectedTurnId":state.turn_id,"input":input}}),
        ),
        "prompt" if state.status == "idle" => Ok(
            json!({"jsonrpc":"2.0","method":"turn/start","params":{"threadId":native,"input":input}}),
        ),
        "interrupt" | "stop" if state.status != "idle" => Ok(
            json!({"jsonrpc":"2.0","method":"turn/interrupt","params":{"threadId":native,"turnId":state.turn_id}}),
        ),
        _ => Err(Error::new(
            "invalid_state",
            format!("{method} is not valid while {}", state.status),
        )),
    }
}
pub fn normalize(value: &Value, state: &mut NativeState) -> Vec<Value> {
    let method = value["method"].as_str().unwrap_or("");
    let p = &value["params"];
    match method {
        "thread/started" => {
            state.native_id = p["thread"]["id"].as_str().map(str::to_owned);
            vec![]
        }
        "turn/started" => {
            let id = p["turn"]["id"].as_str().map(str::to_owned);
            if state.turn_id != id {
                state.epoch += 1;
            }
            state.turn_id = id;
            state.status = "running".into();
            if let Some(id) = p["threadId"].as_str() {
                state.native_id = Some(id.into());
            }
            vec![event(
                "turn.started",
                json!({"run_epoch":state.epoch.to_string(),"native_turn_id":state.turn_id}),
            )]
        }
        "turn/completed" => {
            state.status = "idle".into();
            vec![event(
                "turn.completed",
                json!({"run_epoch":state.epoch.to_string(),"native_turn_id":state.turn_id,"outcome":super::projection::outcome(p["turn"]["status"].as_str().unwrap_or("unknown"))}),
            )]
        }
        "turn/plan/updated" => visible_plan(p).into_iter().collect(),
        "item/started" | "item/completed" => item_event(&p["item"], method == "item/completed"),
        "item/commandExecution/requestApproval"
        | "item/fileChange/requestApproval"
        | "item/permissions/requestApproval"
        | "item/tool/requestUserInput" => {
            let id = value["id"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value["id"].to_string());
            vec![super::projection::approval_requested(
                state,
                &id,
                if method.ends_with("requestUserInput") {
                    "input"
                } else {
                    "approval"
                },
                value.clone(),
            )]
        }
        "serverRequest/resolved" => {
            let id = p["requestId"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| p["requestId"].to_string());
            super::projection::approval_resolved(state, &id, None)
                .into_iter()
                .collect()
        }
        _ => vec![],
    }
}

fn visible_plan(params: &Value) -> Option<Value> {
    let id = params["turnId"].as_str()?;
    let steps = params["plan"]
        .as_array()?
        .iter()
        .enumerate()
        .filter_map(|(index, step)| {
            let status = match step["status"].as_str()? {
                "pending" => "pending",
                "inProgress" => "running",
                "completed" => "done",
                _ => return None,
            };
            let shareable = crate::collector::sanitize(step["step"].as_str()?, "agent");
            Some(json!({"id":(index + 1).to_string(),"text":shareable["text"],"status":status}))
        })
        .collect::<Vec<_>>();
    Some(event("plan.updated", json!({"plan_id":id,"steps":steps})))
}
fn item_event(item: &Value, complete: bool) -> Vec<Value> {
    match item["type"].as_str().unwrap_or("") {
        "agentMessage" if complete => vec![event(
            "message.completed",
            crate::collector::sanitize(item["text"].as_str().unwrap_or(""), "agent"),
        )],
        "userMessage" if complete => vec![event(
            "message.completed",
            crate::collector::sanitize(&text_content(&item["content"]), "user"),
        )],
        "commandExecution" | "mcpToolCall" | "dynamicToolCall" => {
            let mut payload = json!({"tool_call_id":item["id"],"tool":item["tool"].as_str().or_else(||item["type"].as_str()).unwrap_or("tool"),"summary":if complete{"Tool finished"}else{"Tool started"}});
            if item["type"] == "commandExecution" {
                if let Some(detail) = super::codex_command::describe(item) {
                    payload["summary"] = json!(detail);
                }
            }
            if let Some(message) = item["error"]["message"].as_str() {
                payload["summary"] = crate::collector::sanitize(message, "agent")["text"].clone();
            }
            if complete {
                payload["outcome"] = json!(if item["status"] == "failed"
                    || item["exitCode"].as_i64().is_some_and(|code| code != 0)
                {
                    "failed"
                } else if item["status"] == "declined" {
                    "cancelled"
                } else {
                    "success"
                });
                payload["duration_ms"] = json!(item["durationMs"].as_u64().unwrap_or(0));
                payload["content_policy"] = super::projection::tool_policy();
                if !item["exitCode"].is_null() {
                    payload["exit_code"] = item["exitCode"].clone();
                }
                if item["type"] == "commandExecution" {
                    if let Some((output, policy)) = super::codex_command::output(item) {
                        payload["output"] = json!(output);
                        payload["content_policy"] = policy;
                    }
                }
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
        _ => vec![],
    }
}
pub fn text_content(content: &Value) -> String {
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter(|v| v["type"] == "text")
        .filter_map(|v| v["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Translate a local decision into the response required by the native request.
pub fn approval_result(original: &Value, decision: &str, input: Option<&str>) -> Result<Value> {
    if original["method"] == "item/permissions/requestApproval" {
        let permissions = if decision == "approve" {
            original["params"]["permissions"].clone()
        } else {
            json!({})
        };
        return Ok(json!({"permissions": permissions, "scope": "turn"}));
    }
    let Some(text) = input else {
        if original["method"] == "item/tool/requestUserInput" {
            return Err(Error::new("invalid_request", "User input requires answers"));
        }
        return Ok(json!({"decision": if decision == "approve" { "accept" } else { "decline" }}));
    };
    let questions = original["params"]["questions"]
        .as_array()
        .ok_or_else(|| Error::new("unsupported_capability", "Native input shape unavailable"))?;
    if questions.len() != 1 {
        return Err(Error::new(
            "unsupported_capability",
            "Multiple questions require native UI",
        ));
    }
    let id = questions[0]["id"]
        .as_str()
        .ok_or_else(|| Error::new("unsupported_capability", "Native question ID unavailable"))?;
    Ok(json!({"answers": {id: {"answers": [text]}}}))
}

pub fn approval_answers(original: &Value, answers: &Value) -> Result<Value> {
    let invalid = || {
        Error::new(
            "invalid_request",
            "Answer every native question by ID with a nonempty string array",
        )
    };
    let questions = original["params"]["questions"]
        .as_array()
        .ok_or_else(invalid)?;
    let provided = answers.as_object().ok_or_else(invalid)?;
    if questions.is_empty() || provided.len() != questions.len() {
        return Err(invalid());
    }
    let mut result = serde_json::Map::new();
    for question in questions {
        let id = question["id"].as_str().ok_or_else(invalid)?;
        let values = provided
            .get(id)
            .and_then(Value::as_array)
            .ok_or_else(invalid)?;
        if values.is_empty() || values.iter().any(|v| !v.is_string()) {
            return Err(invalid());
        }
        result.insert(id.into(), json!({"answers": values}));
    }
    Ok(json!({"answers":result}))
}
