use devmesh_agent::runtime::{
    pi::{normalize, request},
    NativeState,
};
use serde_json::json;

#[test]
fn pi_commands_keep_native_protocol_and_stale_epoch_is_rejected() {
    let state = NativeState {
        status: "running".into(),
        epoch: 4,
        ..Default::default()
    };
    let message = request("steer", &state, "add tests", Some("4")).unwrap();
    assert_eq!(message["type"], "steer");
    assert_eq!(message["message"], "add tests");
    assert!(request("steer", &state, "text", Some("3")).is_err());
    let idle = NativeState {
        status: "idle".into(),
        ..state
    };
    assert!(request("steer", &idle, "text", Some("4")).is_err());
    assert_eq!(request("stop", &idle, "", None).unwrap()["type"], "abort");
}
#[test]
fn pi_message_tool_and_agent_lifecycle_are_normalized() {
    let mut state = NativeState::default();
    normalize(&json!({"type":"agent_start"}), &mut state);
    let message = normalize(
        &json!({"type":"message_end","message":{"role":"assistant","content":[{"type":"thinking","thinking":"private"},{"type":"text","text":"done"}]}}),
        &mut state,
    );
    assert_eq!(message[0]["payload"]["text"], "done");
    assert!(!message[0].to_string().contains("private"));
    let tool = normalize(
        &json!({"type":"tool_execution_end","toolCallId":"t1","toolName":"bash","isError":false,"result":{"content":[{"type":"text","text":"ok"}]}}),
        &mut state,
    );
    assert_eq!(tool[0]["type"], "tool.completed");
    normalize(&json!({"type":"agent_end","messages":[]}), &mut state);
    assert_eq!(state.status, "idle");
}
