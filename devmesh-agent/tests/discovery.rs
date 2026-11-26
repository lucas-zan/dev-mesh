use devmesh_agent::{
    config::Config,
    discovery::resolve,
    gitea::{GiteaClient, GiteaTransport},
};
use devmesh_protocol::{Error, Result};
use reqwest::Method;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

struct Directory {
    pages: Mutex<Vec<Value>>,
    calls: Mutex<Vec<String>>,
}
#[async_trait::async_trait]
impl GiteaTransport for Directory {
    async fn request(&self, _: Method, path: &str, _: Option<&Value>) -> Result<Value> {
        self.calls.lock().unwrap().push(path.into());
        if !path.starts_with("repos/search?") {
            return Ok(json!([]));
        }
        let mut pages = self.pages.lock().unwrap();
        if pages.is_empty() {
            return Err(Error::new("permission_denied", "Token rejected"));
        }
        Ok(json!({"data":pages.remove(0)}))
    }
}
fn config() -> Config {
    serde_json::from_value(json!({"schema_version":1,"device_id":"11111111-1111-4111-8111-111111111111",
      "roles":["backend"],"gitea":{"endpoint":"http://git.test","credential_ref":"env:UNUSED"},
      "relay":null,"state_dir":"/tmp/discovery-state","projects":[],
      "repositories":{"instance_id":"22222222-2222-4222-8222-222222222222","workspace_root":"/tmp/repos","runtime":"pi","auto_claim":false}})).unwrap()
}
fn repo(id: u64, owner: &str, name: &str) -> Value {
    json!({"id":id,"owner":{"login":owner},"name":name,"default_branch":"main","clone_url":format!("http://git.test/{owner}/{name}.git")})
}
#[tokio::test]
async fn all_accessible_repositories_are_paged_deduplicated_and_projected() {
    let first = (1..=50)
        .map(|id| repo(id, "org", &format!("repo{id}")))
        .collect::<Vec<_>>();
    let transport = Arc::new(Directory {
        pages: Mutex::new(vec![
            json!(first),
            json!([repo(50, "org", "repo50"), repo(51, "collaborator", "last")]),
        ]),
        calls: Mutex::new(vec![]),
    });
    let resolved = resolve(&config(), &GiteaClient::with_transport(transport.clone()))
        .await
        .unwrap();
    assert_eq!(resolved.projects.len(), 51);
    assert_eq!(
        resolved.projects[50].local_path.to_str().unwrap(),
        "/tmp/repos/collaborator/last"
    );
    assert_eq!(resolved.projects[50].runtime, "pi");
    assert!(resolved.projects[50].auto_clone);
    assert!(!resolved.projects[50].auto_claim);
    assert_eq!(transport.calls.lock().unwrap().len(), 2);
    assert!(transport.calls.lock().unwrap()[1].contains("page=2"));
}
#[tokio::test]
async fn rediscovery_removes_inaccessible_repositories_and_adds_new_ones() {
    let transport = Arc::new(Directory {
        pages: Mutex::new(vec![
            json!([repo(1, "me", "old")]),
            json!([repo(2, "org", "new")]),
        ]),
        calls: Mutex::new(vec![]),
    });
    let client = GiteaClient::with_transport(transport);
    let initial = resolve(&config(), &client).await.unwrap();
    let updated = resolve(&initial, &client).await.unwrap();
    assert_eq!(updated.projects.len(), 1);
    assert_eq!(updated.projects[0].repository.repository_id, 2);
}
#[tokio::test]
async fn empty_directory_is_valid_but_permission_error_is_not_silently_hidden() {
    let transport = Arc::new(Directory {
        pages: Mutex::new(vec![json!([])]),
        calls: Mutex::new(vec![]),
    });
    let client = GiteaClient::with_transport(transport);
    assert!(resolve(&config(), &client)
        .await
        .unwrap()
        .projects
        .is_empty());
    assert_eq!(
        resolve(&config(), &client).await.err().unwrap().code,
        "permission_denied"
    );
}

#[tokio::test]
async fn mcp_refreshes_token_inventory_and_routes_by_repository_id_without_enrollment() {
    use devmesh_agent::{
        discovery::RepositoryDirectory,
        mcp::{Context, McpServer},
    };
    let transport = Arc::new(Directory {
        pages: Mutex::new(vec![
            json!([repo(1, "org", "first"), repo(2, "team", "second")]),
            json!([repo(2, "team", "second")]),
            json!([]),
        ]),
        calls: Mutex::new(vec![]),
    });
    let server = McpServer::new(Context {
        directory: Some(RepositoryDirectory {
            config: config(),
            client: GiteaClient::with_transport(transport.clone()),
        }),
        ..Context::default()
    });
    let call = |name: &str, args: Value| json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}});
    let response = server
        .handle(call("repository_list", json!({})))
        .await
        .unwrap();
    let list: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 2);
    let response = server
        .handle(call("issue_list", json!({"repository_id":2})))
        .await
        .unwrap();
    assert_eq!(response["result"]["isError"], false);
    assert!(transport
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|path| path.starts_with("repos/team/second/issues")));
    let response = server
        .handle(call("repository_list", json!({})))
        .await
        .unwrap();
    assert_eq!(response["result"]["content"][0]["text"], "[]");
}

#[test]
fn multiple_repositories_require_an_explicit_target() {
    use devmesh_agent::discovery::repository_index;
    let mut config = config();
    let template = |id| json!({"repository":{"instance_id":"22222222-2222-4222-8222-222222222222","repository_id":id,"owner":"org","name":format!("repo{id}")},"local_path":format!("/tmp/repo{id}"),"roles":["backend"],"runtime":"pi","default_branch":"main","concurrency":1,"auto_claim":false,"auto_clone":true,"remote":"http://git.test/repo.git","can_prepare":true});
    config.projects = vec![
        serde_json::from_value(template(1)).unwrap(),
        serde_json::from_value(template(2)).unwrap(),
    ];
    assert!(repository_index(&config, None, None).is_err());
    assert_eq!(repository_index(&config, Some(2), None).unwrap(), 1);
    assert!(repository_index(&config, Some(3), None).is_err());
}

#[test]
fn unconfigured_repositories_can_run_their_own_actual_test_command() {
    use devmesh_agent::session::test_command;
    assert_eq!(
        test_command(&[], &json!({"command":"npm test"})).unwrap(),
        "npm test"
    );
    assert_eq!(
        test_command(&["cargo test".into()], &json!({"index":0})).unwrap(),
        "cargo test"
    );
    assert!(test_command(&["cargo test".into()], &json!({"command":"npm test"})).is_err());
    assert!(test_command(&[], &json!({"command":""})).is_err());
}

#[tokio::test]
async fn writing_a_discovered_repository_reaches_gitea_without_local_enrollment() {
    use devmesh_agent::{
        discovery::RepositoryDirectory,
        mcp::{Context, McpServer},
    };
    struct WritableDirectory {
        writes: Mutex<Vec<String>>,
    }
    #[async_trait::async_trait]
    impl GiteaTransport for WritableDirectory {
        async fn request(&self, method: Method, path: &str, _: Option<&Value>) -> Result<Value> {
            if path.starts_with("repos/search?") {
                return Ok(json!({"data":[repo(1,"org","one"),repo(2,"team","two")]}));
            }
            if method == Method::GET {
                return Ok(json!([]));
            }
            self.writes.lock().unwrap().push(path.into());
            Err(Error::new(
                "gitea_error",
                "Gitea rejected write for this Token",
            ))
        }
    }
    let transport = Arc::new(WritableDirectory {
        writes: Mutex::new(vec![]),
    });
    let server = McpServer::new(Context {
        directory: Some(RepositoryDirectory {
            config: config(),
            client: GiteaClient::with_transport(transport.clone()),
        }),
        can_prepare: true,
        ..Context::default()
    });
    let response=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"issue_create","arguments":{"repository_id":2,"title":"Task","required_role":"backend"}}})).await.unwrap();
    assert!(response["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("gitea_error"));
    assert_eq!(
        *transport.writes.lock().unwrap(),
        vec!["repos/team/two/labels"]
    );
}

#[test]
fn git_credentials_use_the_configured_token_for_same_origin_http_only() {
    let rest = GiteaClient::new("https://git.test/gitea", "synthetic-pat").unwrap();
    let official = rest
        .clone()
        .with_official("unused", "https://git.test/gitea", "synthetic-pat");
    for client in [rest, official] {
        let mut command = std::process::Command::new("git");
        client.authenticate_git(&mut command, "https://git.test/gitea/team/private.git");
        let env = command
            .get_envs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            env[std::ffi::OsStr::new("GIT_CONFIG_VALUE_0")].unwrap(),
            "Authorization: Bearer synthetic-pat"
        );
        assert!(!format!("{:?}", command.get_args()).contains("synthetic-pat"));
        for url in [
            "https://git.test:8443/team/repo.git",
            "http://git.test/team/repo.git",
            "https://another.test/team/repo.git",
            "git@git.test:team/repo.git",
        ] {
            let mut command = std::process::Command::new("git");
            client.authenticate_git(&mut command, url);
            assert_eq!(command.get_envs().count(), 0);
        }
    }
}

#[tokio::test]
async fn mcp_can_view_an_ordinary_issue_in_a_discovered_repository() {
    use devmesh_agent::{
        discovery::RepositoryDirectory,
        mcp::{Context, McpServer},
    };
    struct OrdinaryIssue;
    #[async_trait::async_trait]
    impl GiteaTransport for OrdinaryIssue {
        async fn request(&self, _: Method, path: &str, _: Option<&Value>) -> Result<Value> {
            if path.starts_with("repos/search?") {
                return Ok(json!({"data":[repo(2,"team","two")]}));
            }
            assert_eq!(path, "repos/team/two/issues/7");
            Ok(json!({"number":7,"body":"Already in Gitea"}))
        }
    }
    let server = McpServer::new(Context {
        directory: Some(RepositoryDirectory {
            config: config(),
            client: GiteaClient::with_transport(Arc::new(OrdinaryIssue)),
        }),
        ..Context::default()
    });
    let result = server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"issue_get","arguments":{"issue_ref":{"instance_id":"22222222-2222-4222-8222-222222222222","repository_id":2,"issue_number":7}}}})).await.unwrap();
    assert_eq!(result["result"]["isError"], false);
}

#[tokio::test]
async fn existing_repository_keeps_explicit_runtime_branch_and_ssh_transport() {
    let transport = Arc::new(Directory {
        pages: Mutex::new(vec![json!([repo(7, "team", "app")])]),
        calls: Mutex::new(vec![]),
    });
    let client = GiteaClient::with_transport(transport);
    let mut config = config();
    let mut project = resolve(&config, &client).await.unwrap().projects.remove(0);
    project.remote = "git@git.test:team/app.git".into();
    project.default_branch = "release".into();
    project.runtime = "codex".into();
    config.projects.push(project);
    let transport = Arc::new(Directory {
        pages: Mutex::new(vec![json!([repo(7, "team", "app")])]),
        calls: Mutex::new(vec![]),
    });
    let updated = resolve(&config, &GiteaClient::with_transport(transport))
        .await
        .unwrap();
    assert_eq!(updated.projects[0].remote, "git@git.test:team/app.git");
    assert_eq!(updated.projects[0].default_branch, "release");
    assert_eq!(updated.projects[0].runtime, "codex");
}

#[tokio::test]
async fn real_git_can_clone_a_private_http_repository_with_only_the_configured_token() {
    use axum::{
        extract::{Path, State},
        http::{HeaderMap, StatusCode},
        Router,
    };
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("private.git");
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "--bare", repo.to_str().unwrap()]);
    git(&["-C", repo.to_str().unwrap(), "update-server-info"]);
    async fn serve_private_file(
        State(repo): State<std::path::PathBuf>,
        Path(path): Path<String>,
        headers: HeaderMap,
    ) -> (StatusCode, Vec<u8>) {
        if headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            != Some("Bearer synthetic-pat")
        {
            return (StatusCode::UNAUTHORIZED, Vec::new());
        }
        match tokio::fs::read(repo.join(path)).await {
            Ok(bytes) => (StatusCode::OK, bytes),
            Err(_) => (StatusCode::NOT_FOUND, Vec::new()),
        }
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new()
        .route("/private.git/*path", axum::routing::get(serve_private_file))
        .with_state(repo);
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let remote = format!("{endpoint}/private.git");
    let client = GiteaClient::new(&endpoint, "synthetic-pat").unwrap();
    let mut command = std::process::Command::new("git");
    command.args([
        "clone",
        &remote,
        temp.path().join("checkout").to_str().unwrap(),
    ]);
    client.authenticate_git(&mut command, &remote);
    let result = tokio::process::Command::from(command)
        .output()
        .await
        .unwrap();
    server.abort();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let config = std::fs::read_to_string(temp.path().join("checkout/.git/config")).unwrap();
    assert!(config.contains(&remote));
    assert!(!config.contains("synthetic-pat"));
}

#[tokio::test]
async fn work_start_rejects_missing_repository_or_issue_before_contacting_worker() {
    use devmesh_agent::{discovery::LocalWorkActions, mcp::WorkActions};
    let dir = tempfile::tempdir().unwrap();
    let mut config = config();
    config.state_dir = dir.path().to_owned();
    let work = LocalWorkActions { config };
    for args in [
        json!({"issue_number":7}),
        json!({"repository_id":2}),
        json!({"repository_id":2,"issue_number":0}),
    ] {
        let error = work.call("work_start", &args).await.unwrap_err();
        assert_eq!(error.code, "invalid_request");
    }
}

#[tokio::test]
async fn work_start_routes_the_explicit_repository_to_the_worker_and_surfaces_denials() {
    use devmesh_agent::{discovery::LocalWorkActions, mcp::WorkActions};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let dir = tempfile::tempdir().unwrap();
    let mut config = config();
    config.state_dir = dir.path().to_owned();
    let listener = tokio::net::UnixListener::bind(dir.path().join("daemon.sock")).unwrap();
    let peer = tokio::spawn(async move {
        for (repository, response) in [
            (2, json!({"result":{"state":"preparing"}})),
            (
                3,
                json!({"error":{"data":{"code":"not_found"},"message":"Repository unavailable"}}),
            ),
        ] {
            let (stream, _) = listener.accept().await.unwrap();
            let (input, mut output) = stream.into_split();
            let mut line = String::new();
            BufReader::new(input).read_line(&mut line).await.unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["method"], "local/work");
            assert_eq!(request["params"]["repository_id"], repository);
            assert_eq!(request["params"]["number"], 7);
            output
                .write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        }
    });
    let work = LocalWorkActions { config };
    let outcome = work
        .call("work_start", &json!({"repository_id":2,"issue_number":7}))
        .await
        .unwrap();
    assert_eq!(outcome["state"], "preparing");
    let error = work
        .call("work_start", &json!({"repository_id":3,"issue_number":7}))
        .await
        .unwrap_err();
    assert_eq!(error.code, "not_found");
    peer.await.unwrap();
}
