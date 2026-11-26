use devmesh_agent::runtime::{codex, pi, NativeState};
use devmesh_protocol::{Event, Frame};
use serde_json::{json, Value};
use std::{
    io::Write,
    process::{Command, Stdio},
};
fn validate(events: Vec<Value>) {
    let session = uuid::Uuid::new_v4();
    let work = uuid::Uuid::new_v4();
    let events = events
        .into_iter()
        .enumerate()
        .map(|(index, value)| Event {
            event_id: uuid::Uuid::new_v4(),
            work_id: work,
            session_id: session,
            sequence: (index + 1).to_string(),
            occurred_at: devmesh_protocol::now(),
            run_epoch: "1".into(),
            kind: value["type"].as_str().unwrap().into(),
            payload: value["payload"].clone(),
        })
        .collect::<Vec<_>>();
    let frame = Frame::new(
        "session.events",
        json!({"session_id":session,"events":events}),
    );
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../devmesh-protocol/tests/validate-wire.mjs");
    let mut child = Command::new("node")
        .arg(script)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&frame).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn codex_projected_events_match_the_shared_schema() {
    let mut state = NativeState::default();
    let native = [
        json!({"method":"turn/started","params":{"threadId":"thread","turn":{"id":"turn"}}}),
        json!({"method":"turn/plan/updated","params":{"turnId":"turn","plan":[{"step":"Verify changes","status":"inProgress"}]}}),
        json!({"method":"item/completed","params":{"item":{"id":"tool","type":"commandExecution","status":"completed","exitCode":0}}}),
        json!({"id":1,"method":"item/tool/requestUserInput","params":{}}),
        json!({"method":"turn/completed","params":{"turn":{"id":"turn","status":"completed"}}}),
    ];
    validate(
        native
            .iter()
            .flat_map(|value| codex::normalize(value, &mut state))
            .collect(),
    );
}
#[test]
fn pi_projected_events_match_the_shared_schema() {
    let mut state = NativeState::default();
    let native = [
        json!({"type":"agent_start"}),
        json!({"type":"tool_execution_end","toolCallId":"tool","toolName":"bash","isError":false}),
        json!({"type":"extension_ui_request","id":"native-ui","method":"confirm"}),
        json!({"type":"agent_end"}),
    ];
    validate(
        native
            .iter()
            .flat_map(|value| pi::normalize(value, &mut state))
            .collect(),
    );
}

#[test]
fn durable_projected_events_match_the_shared_schema() {
    let mut state = NativeState::default();
    let native = [
        json!({"type":"event","run_epoch":"1","event_type":"turn.started","payload":{"run_epoch":"1"}}),
        json!({"type":"event","run_epoch":"1","event_type":"tool.completed","payload":{"tool_call_id":7,"tool":"bash","summary":"done","outcome":"completed","duration_ms":0}}),
        json!({"type":"event","run_epoch":"1","event_type":"approval.requested","payload":{"approval_id":"native-task","kind":"bash"}}),
        json!({"type":"event","run_epoch":"1","event_type":"approval.resolved","payload":{"approval_id":"native-task","outcome":"approve"}}),
        json!({"type":"event","run_epoch":"1","event_type":"turn.completed","payload":{"run_epoch":"1","outcome":"completed"}}),
    ];
    validate(
        native
            .iter()
            .flat_map(|value| devmesh_agent::runtime::normalize_durable(value, &mut state))
            .collect(),
    );
}
