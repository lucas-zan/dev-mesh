use devmesh_agent::gitea::{GiteaClient, GiteaTransport, Repo, TaskService};
use devmesh_protocol::{metadata, replace_metadata, Error, Result};
use reqwest::Method;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

struct State {
    issue: Value,
    pr: Value,
    fail_close: bool,
    closes: usize,
}
struct Gitea(Arc<Mutex<State>>);
#[async_trait::async_trait]
impl GiteaTransport for Gitea {
    async fn request(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value> {
        let mut s = self.0.lock().unwrap();
        if method == Method::GET {
            if path.contains("/pulls/") {
                return Ok(s.pr.clone());
            }
            if path.contains("/labels?") {
                return Ok(json!([{"id":2,"name":"status:done"}]));
            }
            if path.contains("/issues?") {
                return Ok(if path.ends_with("page=1") {
                    json!([s.issue])
                } else {
                    json!([])
                });
            }
            return Ok(s.issue.clone());
        }
        if method == Method::DELETE {
            s.issue["labels"] = json!([]);
            return Ok(Value::Null);
        }
        let body = body.unwrap();
        if method == Method::POST && path.ends_with("/labels") {
            s.issue["labels"] = json!([{"id":2,"name":"status:done"}]);
            return Ok(s.issue["labels"].clone());
        }
        if body["state"] == "closed" {
            s.closes += 1;
            if std::mem::take(&mut s.fail_close) {
                return Err(Error::new("unknown_outcome", "Close response lost"));
            }
            s.issue["state"] = json!("closed");
            return Ok(s.issue.clone());
        }
        assert_eq!(body["content_version"], s.issue["content_version"]);
        s.issue["body"] = body["body"].clone();
        s.issue["content_version"] = json!(s.issue["content_version"].as_u64().unwrap() + 1);
        Ok(s.issue.clone())
    }
}
fn setup() -> (TaskService, Arc<Mutex<State>>) {
    let repo = Repo {
        instance_id: uuid::Uuid::new_v4(),
        repository_id: 42,
        owner: "org".into(),
        name: "repo".into(),
    };
    let sha = "a".repeat(40);
    let meta = json!({"issue_ref":repo.issue_ref(7),"state":"review","state_rev":3,"required_role":"backend","claim":{"state":"released"},"acceptance_criteria":[{"id":"AC1"}],"dependencies":[],"delivery":{"candidate_sha":sha,"evidence":[{"evidence_id":"test","kind":"test","candidate_sha":sha,"outcome":"pass","exit_code":0,"environment":"local"},{"evidence_id":"pr","kind":"pr","candidate_sha":sha,"outcome":"pass","pr":{"repository":{"instance_id":repo.instance_id,"repository_id":42},"number":9,"head_sha":sha,"target_branch":"main"}}],"acceptance_results":[{"id":"AC1","outcome":"pass","evidence_ids":["test"]}]}});
    let state = Arc::new(Mutex::new(State {
        issue: json!({"number":7,"content_version":1,"state":"open","body":replace_metadata("Task",&meta).unwrap(),"labels":[{"id":1,"name":"status:review"}]}),
        pr: json!({"merged":true,"merge_commit_sha":"b".repeat(40),"head":{"sha":sha},"base":{"ref":"main"}}),
        fail_close: false,
        closes: 0,
    }));
    (
        TaskService::new(
            GiteaClient::with_transport(Arc::new(Gitea(state.clone()))),
            repo,
        ),
        state,
    )
}
#[tokio::test]
async fn merged_candidate_finishes_and_close_failure_can_be_repaired() {
    let (service, state) = setup();
    state.lock().unwrap().fail_close = true;
    assert!(service.reconcile_merged(7).await.is_err());
    let done = service.reconcile_merged(7).await.unwrap();
    assert_eq!(done["state"], "done");
    assert_eq!(done["state_rev"], 4);
    service.reconcile_merged(7).await.unwrap();
    let state = state.lock().unwrap();
    assert_eq!(state.issue["state"], "closed");
    assert_eq!(state.closes, 2);
}
#[tokio::test]
async fn wrong_target_or_changed_candidate_cannot_be_closed() {
    for change in ["target", "candidate", "merge"] {
        let (service, state) = setup();
        {
            let mut state = state.lock().unwrap();
            match change {
                "target" => state.pr["base"]["ref"] = json!("other"),
                "candidate" => state.pr["head"]["sha"] = json!("c".repeat(40)),
                _ => state.pr["merged"] = json!(false),
            }
        }
        assert!(service.reconcile_merged(7).await.is_err());
        assert_eq!(state.lock().unwrap().closes, 0);
    }
}
#[tokio::test]
async fn polling_reconciles_delivered_code_without_starting_a_new_work() {
    let (service, state) = setup();
    assert_eq!(service.reconcile_pending().await.unwrap(), vec![7]);
    assert_eq!(
        metadata(state.lock().unwrap().issue["body"].as_str().unwrap()).unwrap()["state"],
        "done"
    );
}
#[tokio::test]
async fn accepting_non_code_work_requires_a_complete_delivery() {
    let (service, state) = setup();
    {
        let mut state = state.lock().unwrap();
        let mut meta = metadata(state.issue["body"].as_str().unwrap()).unwrap();
        meta["required_role"] = json!("qa");
        meta["delivery"] = Value::Null;
        state.issue["body"] = json!(replace_metadata("QA", &meta).unwrap());
    }
    assert!(service.accept_delivery(7).await.is_err());
    assert_eq!(state.lock().unwrap().closes, 0);
}

#[tokio::test]
async fn a_pr_that_already_closed_its_issue_is_not_closed_twice() {
    let (service, state) = setup();
    state.lock().unwrap().issue["state"] = json!("closed");
    assert_eq!(service.reconcile_merged(7).await.unwrap()["state"], "done");
    assert_eq!(state.lock().unwrap().closes, 0);
}
