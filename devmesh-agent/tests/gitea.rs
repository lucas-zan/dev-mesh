use devmesh_agent::gitea::{ClaimContext, GiteaClient, GiteaTransport, Repo, TaskService};
use devmesh_protocol::{Error, Result};
use reqwest::Method;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

struct TestGitea(Arc<Mutex<Value>>);
fn labels() -> Value {
    json!([{"id":10,"name":"status:claimed"},{"id":11,"name":"status:blocked"},{"id":12,"name":"status:ready"},{"id":13,"name":"status:running"},{"id":14,"name":"status:review"},{"id":15,"name":"agent:allowed"}])
}

#[tokio::test]
async fn explicit_authorization_enables_claim_and_is_idempotent() {
    let (service, issue, ctx) = setup().await;
    {
        let mut issue = issue.lock().unwrap();
        let mut meta = devmesh_protocol::metadata(issue["body"].as_str().unwrap()).unwrap();
        meta["automation_allowed"] = json!(false);
        issue["body"] =
            json!(devmesh_protocol::replace_metadata("Task requirements", &meta).unwrap());
        issue["labels"][2]["name"] = json!("agent:manual-only");
    }
    assert_eq!(
        service.claim(128, &ctx).await.unwrap_err().code,
        "ineligible"
    );
    let authorized = service.authorize(128).await.unwrap();
    assert_eq!(authorized["automation_allowed"], true);
    let version = issue.lock().unwrap()["content_version"].clone();
    assert_eq!(service.authorize(128).await.unwrap(), authorized);
    assert_eq!(issue.lock().unwrap()["content_version"], version);
    service.claim(128, &ctx).await.unwrap();
    assert_eq!(
        service.authorize(128).await.unwrap_err().code,
        "claim_conflict"
    );
}
#[async_trait::async_trait]
impl GiteaTransport for TestGitea {
    async fn request(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value> {
        tokio::task::yield_now().await;
        if path.starts_with("repositories/") {
            return Ok(json!({"owner":{"login":"test"},"name":"repo"}));
        }
        if path == "user" {
            return Ok(json!({"id":14,"login":"test-user"}));
        }
        if method == Method::GET && path.contains("/labels") {
            return Ok(labels());
        }
        let mut issue = self.0.lock().unwrap();
        if method == Method::GET {
            return Ok(issue.clone());
        }
        if method == Method::DELETE {
            let id = path.rsplit('/').next().unwrap().parse::<u64>().unwrap();
            issue["labels"]
                .as_array_mut()
                .unwrap()
                .retain(|label| label["id"] != id);
            return Ok(Value::Null);
        }
        let body = body.unwrap();
        if method == Method::POST && path.ends_with("/labels") {
            for id in body["labels"].as_array().unwrap() {
                if issue["labels"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|label| label["id"] == *id)
                {
                    continue;
                }
                let label = labels()
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|label| label["id"] == *id)
                    .unwrap()
                    .clone();
                issue["labels"].as_array_mut().unwrap().push(label);
            }
            return Ok(issue["labels"].clone());
        }
        if method == Method::PUT {
            let mut preserved = issue["labels"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|l| {
                    body["labels"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|id| *id == l["id"])
                })
                .cloned()
                .collect::<Vec<_>>();
            for id in body["labels"].as_array().unwrap() {
                if let Some(label) = labels().as_array().unwrap().iter().find(|l| l["id"] == *id) {
                    preserved.push(label.clone());
                }
            }
            issue["labels"] = json!(preserved);
            return Ok(issue["labels"].clone());
        }
        if body["content_version"] != issue["content_version"] {
            return Err(Error::new("claim_conflict", "CAS conflict"));
        }
        issue["body"] = body["body"].clone();
        issue["content_version"] = json!(issue["content_version"].as_u64().unwrap() + 1);
        Ok(issue.clone())
    }
}

async fn setup() -> (TaskService, Arc<Mutex<Value>>, ClaimContext) {
    let metadata: Value = serde_json::from_str(include_str!(
        "../../devmesh-protocol/fixtures/issue-ready.json"
    ))
    .unwrap();
    let body = devmesh_protocol::replace_metadata("Task requirements", &metadata).unwrap();
    let issue = Arc::new(Mutex::new(
        json!({"number":128,"body":body,"content_version":1,"state":"open","assignees":[],"labels":[{"id":1,"name":"status:ready"},{"id":2,"name":"role:backend"},{"id":3,"name":"agent:allowed"},{"id":4,"name":"priority:p1"},{"id":5,"name":"type:feature"}]}),
    ));
    let repo = Repo {
        instance_id: metadata["issue_ref"]["instance_id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
        repository_id: 1024,
        owner: "test".into(),
        name: "repo".into(),
    };
    let service = TaskService::new(
        GiteaClient::with_transport(Arc::new(TestGitea(issue.clone()))),
        repo,
    );
    let ctx = ClaimContext {
        owner: 14,
        device_id: Uuid::new_v4(),
        worker_id: Uuid::new_v4(),
        role: "backend".into(),
        runtime: "codex".into(),
    };
    (service, issue, ctx)
}

#[tokio::test]
async fn twenty_workers_only_one_claims_and_assignees_are_unchanged() {
    let (service, issue, ctx) = setup().await;
    let mut jobs = Vec::new();
    for _ in 0..20 {
        let service = service.clone();
        let ctx = ctx.clone();
        jobs.push(tokio::spawn(async move { service.claim(128, &ctx).await }));
    }
    let mut successes = 0;
    for job in jobs {
        if job.await.unwrap().is_ok() {
            successes += 1;
        }
    }
    assert_eq!(successes, 1);
    let current = issue.lock().unwrap();
    assert_eq!(current["assignees"], json!([]));
    let meta = devmesh_protocol::metadata(current["body"].as_str().unwrap()).unwrap();
    assert_eq!(meta["claim_generation"], 1);
}

#[tokio::test]
async fn manual_assignee_and_inconsistent_labels_block_claim() {
    let (service, issue, ctx) = setup().await;
    issue.lock().unwrap()["assignees"] = json!([{"id":99}]);
    assert_eq!(
        service.claim(128, &ctx).await.unwrap_err().code,
        "ineligible"
    );
    issue.lock().unwrap()["assignees"] = json!([]);
    issue.lock().unwrap()["labels"][1]["name"] = json!("role:frontend");
    assert_eq!(
        service.claim(128, &ctx).await.unwrap_err().code,
        "metadata_conflict"
    );
}

#[tokio::test]
async fn release_requires_quiescence_and_rework_can_be_claimed_again() {
    let (service, _issue, ctx) = setup().await;
    let claim = service.claim(128, &ctx).await.unwrap();
    assert!(service
        .release(128, &claim, "blocked", "dependency", false)
        .await
        .is_err());
    service
        .release(128, &claim, "blocked", "dependency", true)
        .await
        .unwrap();
    service
        .prepare(128, "ready", "dependency resolved")
        .await
        .unwrap();
    let second = service.claim(128, &ctx).await.unwrap();
    assert_eq!(second["generation"], 2);
    assert_ne!(claim["claim_id"], second["claim_id"]);
}

#[tokio::test]
async fn persisted_claim_intent_is_recoverable_without_creating_another_generation() {
    let (service, _, ctx) = setup().await;
    let intended = Uuid::new_v4();
    let first = service.claim_with_id(128, &ctx, intended).await.unwrap();
    let recovered = service.claim_with_id(128, &ctx, intended).await.unwrap();
    assert_eq!(first, recovered);
    assert_eq!(recovered["claim_id"], intended.to_string());
    assert_eq!(recovered["generation"], 1);
}

#[tokio::test]
async fn dependencies_use_producer_issue_and_require_the_selected_candidate() {
    let (service, issue, _) = setup().await;
    let mut producer =
        devmesh_protocol::metadata(issue.lock().unwrap()["body"].as_str().unwrap()).unwrap();
    producer["state"] = json!("done");
    {
        let mut remote = issue.lock().unwrap();
        remote["state"] = json!("closed");
        remote["body"] = json!(devmesh_protocol::replace_metadata("producer", &producer).unwrap());
    }
    let dep = json!({"dependencies":[{"dependency_id":Uuid::new_v4(),"producer_issue":service.repo.issue_ref(128),"condition":"issue_closed"}]});
    assert!(service.resolve_dependencies(&dep).await.is_err());
    producer["acceptance_criteria"] = json!([{"id":"AC1"}]);
    let sha = "a".repeat(40);
    producer["delivery"] = json!({"candidate_sha":sha,"evidence":[{"evidence_id":"test","kind":"test","candidate_sha":sha,"outcome":"pass","exit_code":0,"environment":"local"},{"evidence_id":"pr","kind":"pr","candidate_sha":sha,"outcome":"pass"}],"acceptance_results":[{"id":"AC1","outcome":"pass","evidence_ids":["test"]}]});
    issue.lock().unwrap()["body"] =
        json!(devmesh_protocol::replace_metadata("producer", &producer).unwrap());
    assert_eq!(service.resolve_dependencies(&dep).await.unwrap().len(), 1);
    let artifact = json!({"dependencies":[{"producer_issue":service.repo.issue_ref(128),"condition":"artifact_ready","artifact_key":"api"}]});
    assert_eq!(
        service
            .resolve_dependencies(&artifact)
            .await
            .unwrap_err()
            .code,
        "dependency_not_ready"
    );
}

#[tokio::test]
async fn projecting_status_does_not_replace_a_concurrent_human_label() {
    let (service, issue, _) = setup().await;
    let stale = issue.lock().unwrap().clone();
    issue.lock().unwrap()["labels"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":99,"name":"human:review"}));
    service.project_status(&stale, "claimed").await.unwrap();
    assert!(issue.lock().unwrap()["labels"]
        .as_array()
        .unwrap()
        .iter()
        .any(|label| label["id"] == 99));
}

#[tokio::test]
async fn worker_lifecycle_releases_claim_only_after_quiescence_and_ends_the_session() {
    use devmesh_agent::{
        storage::Store,
        worker::{Worker, WorkerProject},
    };
    use devmesh_protocol::WorkState;
    let (service, issue, ctx) = setup().await;
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    let git = |args: &[&str]| {
        let result = std::process::Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    };
    git(&["init", "-b", "main"]);
    git(&["config", "user.name", "test"]);
    git(&["config", "user.email", "test@example.invalid"]);
    std::fs::write(repo.join("file"), "base").unwrap();
    git(&["add", "file"]);
    git(&["commit", "-m", "base"]);
    git(&["remote", "add", "origin", repo.to_str().unwrap()]);
    let store = Arc::new(Store::memory().unwrap());
    let worker = Worker {
        store: store.clone(),
        service,
        project: WorkerProject {
            key: "repo".into(),
            path: repo.clone(),
            worktrees: dir.path().join("trees"),
            remote: repo.to_string_lossy().into_owned(),
            branch: "main".into(),
            concurrency: 1,
            auto_clone: false,
        },
        max_concurrency: 1,
    };
    let work = worker.prepare(128, &ctx).await.unwrap();
    worker
        .mark_running(&work, Uuid::new_v4(), "native")
        .await
        .unwrap();
    assert_eq!(
        worker
            .finalize(&work, WorkState::Blocked, "unknown process", false)
            .await
            .unwrap_err()
            .code,
        "recovery_required"
    );
    assert_eq!(
        store.work(work.work_id).unwrap()["state"],
        "recovery_required"
    );
    assert_eq!(
        devmesh_protocol::metadata(issue.lock().unwrap()["body"].as_str().unwrap()).unwrap()
            ["claim"]["state"],
        "active"
    );
    worker
        .finalize(&work, WorkState::Blocked, "confirmed stopped", true)
        .await
        .unwrap();
    assert_eq!(
        devmesh_protocol::metadata(issue.lock().unwrap()["body"].as_str().unwrap()).unwrap()
            ["claim"]["state"],
        "released"
    );
    assert!(store
        .pending_events(work.session_id)
        .unwrap()
        .iter()
        .any(|event| event.kind == "session.state" && event.payload["state"] == "ended"));
}

#[tokio::test]
async fn preparing_a_task_rejects_dependency_cycles() {
    let (service, issue, _) = setup().await;
    let mut meta =
        devmesh_protocol::metadata(issue.lock().unwrap()["body"].as_str().unwrap()).unwrap();
    meta["dependencies"] = json!([{"dependency_id":Uuid::new_v4(),"producer_issue":service.repo.issue_ref(128),"condition":"issue_closed"}]);
    assert_eq!(
        service.validate_ready(&meta).await.unwrap_err().code,
        "dependency_cycle"
    );
}

#[tokio::test]
async fn replaying_a_quiescent_release_repairs_projection_without_new_claims() {
    let (service, _, ctx) = setup().await;
    let claim = service.claim(128, &ctx).await.unwrap();
    let first = service
        .release(128, &claim, "blocked", "stopped", true)
        .await
        .unwrap();
    let replay = service
        .release(128, &claim, "blocked", "stopped", true)
        .await
        .unwrap();
    assert_eq!(first, replay);
}

#[test]
fn a_mutating_server_failure_does_not_claim_that_cas_was_rejected() {
    use devmesh_agent::gitea::response_code;
    assert_eq!(
        response_code(&reqwest::Method::PATCH, 500),
        "unknown_outcome"
    );
    assert_eq!(
        response_code(&reqwest::Method::PATCH, 409),
        "claim_conflict"
    );
    assert_eq!(response_code(&reqwest::Method::GET, 500), "gitea_error");
}
