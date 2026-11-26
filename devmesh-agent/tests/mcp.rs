use devmesh_agent::mcp::{Context, McpServer};
use serde_json::{json, Value};

#[tokio::test]
async fn mcp_initialize_lists_tools_without_credentials() {
    let server = McpServer::new(Context::default());
    let init = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}))
        .await
        .unwrap();
    assert_eq!(init["result"]["serverInfo"]["name"], "devmesh");
    let response = server
        .handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
        .await
        .unwrap();
    let tools = response["result"]["tools"].as_array().unwrap();
    for name in [
        "issue_create",
        "issue_claim",
        "work_report_delivery",
        "project_list",
    ] {
        assert!(tools.iter().any(|t| t["name"] == name));
    }
    assert!(!tools.iter().any(|t| t["name"] == "raw_write"));
}

#[tokio::test]
async fn unbound_work_and_unknown_methods_are_rejected() {
    let server = McpServer::new(Context::default());
    let response=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"work_report_delivery","arguments":{"work_id":"someone-else"}}})).await.unwrap();
    assert_eq!(response["result"]["isError"], true);
    assert!(response["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("unbound_context"));
    let unknown = server
        .handle(json!({"jsonrpc":"2.0","id":2,"method":"arbitrary"}))
        .await
        .unwrap();
    assert_eq!(unknown["error"]["code"], -32601);
    let notification = server
        .handle(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
        .await;
    assert_eq!(notification, None::<Value>);
}

#[tokio::test]
async fn project_list_exposes_only_configured_repositories_for_cross_project_work() {
    use devmesh_agent::gitea::{GiteaClient, GiteaTransport, Repo, TaskService};
    struct NoNetwork;
    #[async_trait::async_trait]
    impl GiteaTransport for NoNetwork {
        async fn request(
            &self,
            _: reqwest::Method,
            _: &str,
            _: Option<&Value>,
        ) -> devmesh_protocol::Result<Value> {
            panic!("Project inventory must not call Gitea");
        }
    }
    let client = GiteaClient::with_transport(std::sync::Arc::new(NoNetwork));
    let instance = uuid::Uuid::new_v4();
    let service = |id| {
        TaskService::new(
            client.clone(),
            Repo {
                instance_id: instance,
                repository_id: id,
                owner: "org".into(),
                name: format!("repo-{id}"),
            },
        )
    };
    let server = McpServer::new(Context {
        service: Some(service(1)),
        projects: vec![service(2)],
        ..Context::default()
    });
    let response=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"project_list","arguments":{}}})).await.unwrap();
    let inventory: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(inventory.as_array().unwrap().len(), 2);
    let denied=server.handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"issue_get","arguments":{"issue_ref":{"instance_id":instance,"repository_id":99,"issue_number":1}}}})).await.unwrap();
    assert_eq!(denied["result"]["isError"], true);
}
