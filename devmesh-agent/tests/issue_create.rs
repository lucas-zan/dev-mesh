use devmesh_agent::gitea::{GiteaClient, GiteaTransport, Repo, TaskService};
use devmesh_protocol::{metadata, Error, Result};
use reqwest::Method;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct State {
    issue: Option<Value>,
    creates: usize,
    lose_create_response: bool,
    fail_write: bool,
    fail_labels: bool,
}
struct Server(Arc<Mutex<State>>);
fn labels() -> Value {
    json!([{"id":1,"name":"role:backend"},{"id":2,"name":"status:backlog"},
        {"id":3,"name":"type:feature"},{"id":4,"name":"priority:p1"},
        {"id":5,"name":"agent:allowed"}])
}
#[async_trait::async_trait]
impl GiteaTransport for Server {
    async fn request(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value> {
        let mut s = self.0.lock().unwrap();
        match (method.as_str(), path) {
            ("GET", "user") => Ok(json!({"id":14})),
            ("GET", p) if p.contains("issues?") => {
                if p.ends_with("page=1") {
                    Ok(json!(s.issue.iter().collect::<Vec<_>>()))
                } else {
                    Ok(json!([]))
                }
            }
            ("GET", p) if p.ends_with("/labels?limit=100") => Ok(labels()),
            ("GET", _) => Ok(s.issue.clone().unwrap()),
            ("POST", p) if p.ends_with("/issues") => {
                s.creates += 1;
                s.issue = Some(
                    json!({"number":7,"content_version":1,"body":body.unwrap()["body"],"labels":[{"id":99,"name":"human-note"}]}),
                );
                if std::mem::take(&mut s.lose_create_response) {
                    Err(Error::new("unknown_outcome", "Response lost after create"))
                } else {
                    Ok(s.issue.clone().unwrap())
                }
            }
            ("PATCH", _) => {
                if std::mem::take(&mut s.fail_write) {
                    return Err(Error::new("claim_conflict", "CAS failed"));
                }
                let issue = s.issue.as_mut().unwrap();
                assert_eq!(body.unwrap()["content_version"], issue["content_version"]);
                issue["body"] = body.unwrap()["body"].clone();
                issue["content_version"] = json!(issue["content_version"].as_u64().unwrap() + 1);
                Ok(issue.clone())
            }
            ("POST" | "PUT", p) if p.ends_with("/labels") => {
                if std::mem::take(&mut s.fail_labels) {
                    return Err(Error::new("unavailable", "Labels failed"));
                }
                let issue = s.issue.as_mut().unwrap();
                if method == Method::PUT {
                    issue["labels"] = json!([]);
                }
                for id in body.unwrap()["labels"].as_array().unwrap() {
                    if issue["labels"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|l| l["id"] == *id)
                    {
                        continue;
                    }
                    let label = labels()
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|l| l["id"] == *id)
                        .unwrap()
                        .clone();
                    issue["labels"].as_array_mut().unwrap().push(label);
                }
                Ok(issue["labels"].clone())
            }
            _ => panic!("Unexpected request {method} {path}"),
        }
    }
}
fn service(state: State) -> (TaskService, Arc<Mutex<State>>) {
    let state = Arc::new(Mutex::new(state));
    let service = TaskService::new(
        GiteaClient::with_transport(Arc::new(Server(state.clone()))),
        Repo {
            instance_id: uuid::Uuid::new_v4(),
            repository_id: 42,
            owner: "org".into(),
            name: "repo".into(),
        },
    );
    (service, state)
}
fn input() -> Value {
    json!({"title":"Implement API","description":"Human description","dedup_key":"stable-request",
        "required_role":"backend","type":"feature","priority":"p1","automation_requested":true,
        "acceptance_criteria":[{"id":"AC1","text":"API works"}]})
}
#[tokio::test]
async fn retries_finish_the_same_issue_after_each_creation_boundary_fails() {
    for state in [
        State {
            lose_create_response: true,
            ..State::default()
        },
        State {
            fail_write: true,
            ..State::default()
        },
        State {
            fail_labels: true,
            ..State::default()
        },
    ] {
        let (service, server) = service(state);
        assert!(service.create(&input(), "product").await.is_err());
        let original = metadata(
            server.lock().unwrap().issue.as_ref().unwrap()["body"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        let repaired = service.create(&input(), "product").await.unwrap();
        assert_eq!(repaired["task_id"], original["task_id"]);
        assert_eq!(repaired["issue_ref"], service.repo.issue_ref(7));
        assert_eq!(service.create(&input(), "product").await.unwrap(), repaired);
        let server = server.lock().unwrap();
        assert_eq!(server.creates, 1);
        assert_eq!(
            server.issue.as_ref().unwrap()["labels"]
                .as_array()
                .unwrap()
                .len(),
            6
        );
    }
}
#[tokio::test]
async fn initial_creation_preserves_human_labels() {
    let (service, server) = service(State::default());
    service.create(&input(), "product").await.unwrap();
    assert!(server.lock().unwrap().issue.as_ref().unwrap()["labels"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l["id"] == 99));
}
