use devmesh_agent::{
    history::HistoryReader,
    mcp::{Context, McpServer},
};
use devmesh_protocol::Result;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct History(Mutex<Vec<(String, Value)>>);
#[async_trait::async_trait]
impl HistoryReader for History {
    async fn read(&self, tool: &str, args: &Value) -> Result<Value> {
        self.0.lock().unwrap().push((tool.into(), args.clone()));
        Ok(json!({"source":"relay","items":[],"next_cursor":"20","as_of_cursor":"99"}))
    }
}
#[tokio::test]
async fn all_session_tools_read_relay_history_and_preserve_pagination() {
    let history = Arc::new(History::default());
    let server = McpServer::new(Context {
        history: Some(history.clone()),
        ..Context::default()
    });
    let session = uuid::Uuid::new_v4();
    for tool in ["session_list", "session_get", "session_events"] {
        let args = json!({"session_id":session,"cursor":"10","limit":20,"after_sequence":"9007199254740993"});
        let response = server
            .handle(json!({"id":1,"method":"tools/call","params":{"name":tool,"arguments":args}}))
            .await
            .unwrap();
        assert_eq!(response["result"]["isError"], false, "{response}");
        let result: Value =
            serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(result["source"], "relay");
        assert_eq!(result["next_cursor"], "20");
    }
    let reads = history.0.lock().unwrap();
    assert_eq!(reads.len(), 3);
    assert_eq!(reads[2].1["after_sequence"], "9007199254740993");
}
#[tokio::test]
async fn unavailable_history_is_explicit_instead_of_returning_a_local_work() {
    let server = McpServer::new(Context::default());
    let response = server
        .handle(
            json!({"id":1,"method":"tools/call","params":{"name":"session_list","arguments":{}}}),
        )
        .await
        .unwrap();
    assert!(response["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("not_configured"));
}
#[test]
fn relay_history_paths_preserve_large_sequences_and_require_native_targets() {
    use devmesh_agent::history::history_path;
    let id = uuid::Uuid::new_v4();
    assert_eq!(
        history_path(
            "session_events",
            &json!({"session_id":id,"after_sequence":"9007199254740993","limit":20})
        )
        .unwrap(),
        format!("sessions/{id}/events?after_sequence=9007199254740993&limit=20")
    );
    assert!(history_path("session_get", &json!({})).is_err());
    assert!(history_path("session_get", &json!({"session_id":"../admin"})).is_err());
    assert_eq!(
        history_path("session_list", &json!({"work_id":id,"cursor":"10"})).unwrap(),
        format!("sessions?work_id={id}&cursor=10")
    );
}
