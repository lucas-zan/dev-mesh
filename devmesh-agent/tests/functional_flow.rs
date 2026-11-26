//! Real Git, shell tests and native subprocess IPC; Gitea/model responses are test doubles.
use devmesh_agent::{
    gitea::{ClaimContext, GiteaClient, GiteaTransport, Repo, TaskService},
    mcp::WorkActions,
    runtime::{Launch, Runtime, RuntimeKind, Start},
    session::ManagedWork,
    storage::Store,
    worker::{Worker, WorkerProject},
    workspace::git,
};
use devmesh_protocol::{metadata, replace_metadata, Error, Result, WorkState};
use reqwest::Method;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use uuid::Uuid;

struct State {
    issue: Value,
    issue_summary: Option<Value>,
    producer: Option<Value>,
    pr: Option<Value>,
}
struct Gitea {
    state: Arc<Mutex<State>>,
    remote: PathBuf,
}
fn labels() -> Value {
    json!([{"id":1,"name":"status:ready"},{"id":2,"name":"status:claimed"},
    {"id":3,"name":"status:running"},{"id":4,"name":"status:review"},{"id":5,"name":"status:done"},
    {"id":6,"name":"status:blocked"}])
}
#[async_trait::async_trait]
impl GiteaTransport for Gitea {
    async fn request(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value> {
        tokio::task::yield_now().await;
        let mut s = self.state.lock().unwrap();
        if method == Method::GET {
            return self.read(path, &s);
        }
        if method == Method::DELETE {
            let id = path.rsplit('/').next().unwrap().parse::<u64>().unwrap();
            s.issue["labels"]
                .as_array_mut()
                .unwrap()
                .retain(|l| l["id"] != id);
            return Ok(Value::Null);
        }
        let body = body.unwrap();
        if path.ends_with("/labels") {
            for id in body["labels"].as_array().unwrap() {
                if !s.issue["labels"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|l| l["id"] == *id)
                {
                    let label = labels()
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|l| l["id"] == *id)
                        .unwrap()
                        .clone();
                    s.issue["labels"].as_array_mut().unwrap().push(label);
                }
            }
            return Ok(s.issue["labels"].clone());
        }
        if path.ends_with("/pulls") {
            let sha = git(&self.remote, &["rev-parse", body["head"].as_str().unwrap()])?;
            s.pr = Some(
                json!({"number":9,"head":{"sha":sha,"ref":body["head"]},"base":{"ref":body["base"]},"state":"open","draft":false,"merged":false}),
            );
            return Ok(s.pr.clone().unwrap());
        }
        if body["state"] == "closed" {
            s.issue["state"] = json!("closed");
            return Ok(s.issue.clone());
        }
        if body["content_version"] != s.issue["content_version"] {
            return Err(Error::new("claim_conflict", "CAS differs"));
        }
        s.issue["body"] = body["body"].clone();
        s.issue["content_version"] = json!(s.issue["content_version"].as_u64().unwrap() + 1);
        Ok(s.issue.clone())
    }
}
impl Gitea {
    fn read(&self, path: &str, s: &State) -> Result<Value> {
        if path == "user" {
            return Ok(json!({"id":14,"login":"worker"}));
        }
        if path.starts_with("repositories/") {
            return Ok(json!({"owner":{"login":"org"},"name":"repo"}));
        }
        if path.contains("/contents/") {
            use base64::Engine;
            let (file, sha) = path
                .split("/contents/")
                .nth(1)
                .unwrap()
                .split_once("?ref=")
                .unwrap();
            let content = git(&self.remote, &["show", &format!("{sha}:{file}")])?;
            return Ok(
                json!({"type":"file","encoding":"base64","content":base64::engine::general_purpose::STANDARD.encode(content.as_bytes())}),
            );
        }
        if path.contains("/labels?") {
            return Ok(labels());
        }
        if path.contains("/pulls/") {
            return Ok(s.pr.clone().unwrap());
        }
        if path.contains("/pulls?") {
            return Ok(if path.ends_with("page=1") {
                json!(s.pr.iter().collect::<Vec<_>>())
            } else {
                json!([])
            });
        }
        if path.ends_with("/issues/127") {
            return Ok(s.producer.clone().unwrap());
        }
        if path.contains("/issues?") {
            return Ok(if path.ends_with("page=1") {
                json!([s.issue_summary.as_ref().unwrap_or(&s.issue)])
            } else {
                json!([])
            });
        }
        Ok(s.issue.clone())
    }
}
struct Fixture {
    _dir: tempfile::TempDir,
    remote: PathBuf,
    worker: Arc<Worker>,
    state: Arc<Mutex<State>>,
    ctx: ClaimContext,
}
fn fixture(role: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let remote = dir.path().join("remote");
    std::fs::create_dir(&remote).unwrap();
    git(&remote, &["init", "-b", "main"]).unwrap();
    git(&remote, &["config", "user.name", "Test"]).unwrap();
    git(&remote, &["config", "user.email", "test@example.invalid"]).unwrap();
    std::fs::write(remote.join("answer.txt"), "0").unwrap();
    commit(&remote, "base");
    let repo = Repo {
        instance_id: Uuid::new_v4(),
        repository_id: 42,
        owner: "org".into(),
        name: "repo".into(),
    };
    let mut meta: Value = serde_json::from_str(include_str!(
        "../../devmesh-protocol/fixtures/issue-ready.json"
    ))
    .unwrap();
    meta["issue_ref"] = repo.issue_ref(128);
    meta["required_role"] = json!(role);
    meta["dependencies"] = json!([]);
    meta["acceptance_criteria"] = json!([{"id":"AC1","text":"answer is 42"}]);
    let issue = json!({"number":128,"content_version":1,"state":"open","body":replace_metadata("Implement answer",&meta).unwrap(),"assignees":[],"labels":[{"id":1,"name":"status:ready"},{"name":format!("role:{role}")},{"name":"agent:allowed"},{"name":format!("priority:{}",meta["priority"].as_str().unwrap())},{"name":format!("type:{}",meta["type"].as_str().unwrap())}]});
    let state = Arc::new(Mutex::new(State {
        issue,
        issue_summary: None,
        producer: None,
        pr: None,
    }));
    let service = TaskService::new(
        GiteaClient::with_transport(Arc::new(Gitea {
            state: state.clone(),
            remote: remote.clone(),
        })),
        repo,
    );
    let worker = Arc::new(Worker {
        store: Arc::new(Store::memory().unwrap()),
        service,
        project: WorkerProject {
            key: "project".into(),
            path: dir.path().join("checkout"),
            remote: remote.to_string_lossy().into_owned(),
            worktrees: dir.path().join("worktrees"),
            branch: "main".into(),
            concurrency: 1,
            auto_clone: true,
        },
        max_concurrency: 1,
    });
    let ctx = ClaimContext {
        owner: 14,
        device_id: Uuid::new_v4(),
        worker_id: Uuid::new_v4(),
        role: role.into(),
        runtime: "codex".into(),
    };
    Fixture {
        _dir: dir,
        remote,
        worker,
        state,
        ctx,
    }
}
fn commit(path: &Path, message: &str) {
    git(path, &["add", "."]).unwrap();
    git(
        path,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-m",
            message,
        ],
    )
    .unwrap();
}
async fn start(f: &Fixture, mutate: bool) -> ManagedWork {
    let prepared = f.worker.prepare(128, &f.ctx).await.unwrap();
    launch_work(f.worker.clone(), prepared, mutate).await
}
async fn launch_work(
    worker: Arc<Worker>,
    prepared: devmesh_agent::worker::PreparedWork,
    mutate: bool,
) -> ManagedWork {
    let script = r#"import sys,json,pathlib,subprocess,time
for line in sys.stdin:
 r=json.loads(line)
 if 'id' not in r:continue
 result={}
 if r['method']=='thread/start':
  assert r['params']['approvalPolicy']=='never'
  assert r['params']['sandbox']=='danger-full-access'
  result={'thread':{'id':'native-thread'}}
 if r['method']=='turn/start':
  if sys.argv[1]=='true':
   pathlib.Path('answer.txt').write_text('42')
   subprocess.run(['git','add','.'],check=True,stdout=subprocess.DEVNULL)
   subprocess.run(['git','-c','user.name=Test','-c','user.email=test@example.invalid','commit','-m','implement answer'],check=True,stdout=subprocess.DEVNULL)
  print(json.dumps({'method':'turn/started','params':{'threadId':'native-thread','turn':{'id':'turn'}}}),flush=True)
  print(json.dumps({'method':'turn/completed','params':{'turn':{'id':'turn','status':'completed'}}}),flush=True)
  time.sleep(.03)
  result={'turn':{'id':'turn','status':'completed'}}
 print(json.dumps({'id':r['id'],'result':result}),flush=True)
"#;
    let launch = Launch {
        kind: RuntimeKind::Codex,
        codex_unattended: true,
        program: "python3".into(),
        args: vec!["-u".into(), "-c".into(), script.into(), mutate.to_string()],
        env: vec![],
        cwd: prepared.workspace.clone(),
        model: None,
        resume: None,
    };
    let (runtime, _) = Runtime::launch(&launch).await.unwrap();
    let runtime = Arc::new(runtime);
    runtime
        .start(Start {
            work: prepared.work_id,
            session: prepared.session_id,
            cwd: &prepared.workspace,
            issue: &prepared.metadata,
            message: "Implement answer",
            model: None,
            resume: None,
        })
        .await
        .unwrap();
    let native = runtime.state.lock().await.clone();
    worker
        .mark_running(
            &prepared,
            native.instance,
            native.native_id.as_deref().unwrap(),
        )
        .await
        .unwrap();
    ManagedWork::new(
        worker,
        prepared,
        runtime,
        vec!["test \"$(cat answer.txt)\" = 42".into()],
    )
}
async fn test_evidence(work: &ManagedWork) -> Value {
    let unconfigured = ManagedWork::new(
        work.worker.clone(),
        work.prepared.clone(),
        work.runtime.clone(),
        Vec::new(),
    );
    let observed = unconfigured
        .call(
            "work_run_test",
            &json!({"command":"test \"$(cat answer.txt)\" = 42", "acceptance_ids":["AC1"]}),
        )
        .await
        .unwrap();
    assert_eq!(observed["outcome"], "pass");
    work.call(
        "work_run_test",
        &json!({"index":0,"acceptance_ids":["AC1"]}),
    )
    .await
    .unwrap()
}
async fn finish(work: &ManagedWork) {
    assert!(work.runtime.stop().await.unwrap());
    work.worker
        .finalize(
            &work.prepared,
            WorkState::Completed,
            "delivery verified",
            true,
        )
        .await
        .unwrap();
    assert!(work.worker.store.active().unwrap().is_empty());
}
#[tokio::test]
async fn code_issue_runs_from_empty_checkout_through_tests_pr_merge_and_close() {
    let f = fixture("backend");
    let work = start(&f, true).await;
    let test = test_evidence(&work).await;
    assert_eq!(test["outcome"], "pass");
    let candidate = work.call("work_push", &json!({})).await.unwrap()["sha"].clone();
    let pr = work
        .call(
            "work_create_pr",
            &json!({"title":"Implement answer","body":"Closes #128"}),
        )
        .await
        .unwrap();
    let evidence = json!([test,{"evidence_id":Uuid::new_v4(),"kind":"pr","candidate_sha":candidate,"outcome":"pass","summary":"Implementation PR","acceptance_ids":["AC1"],"pr":{"repository":{"instance_id":f.worker.service.repo.instance_id,"repository_id":42},"number":pr["number"],"head_sha":candidate,"target_branch":"main"}}]);
    let report = json!({"candidate_sha":candidate,"evidence":evidence,"acceptance_results":[{"id":"AC1","outcome":"pass","evidence_ids":[test["evidence_id"]]}]});
    work.call("work_report_delivery", &report).await.unwrap();
    finish(&work).await;
    assert_eq!(
        metadata(f.state.lock().unwrap().issue["body"].as_str().unwrap()).unwrap()["state"],
        "review"
    );
    git(
        &f.remote,
        &[
            "merge",
            "--no-ff",
            &work.prepared.branch,
            "-m",
            "merge reviewed candidate",
        ],
    )
    .unwrap();
    {
        let mut s = f.state.lock().unwrap();
        s.pr.as_mut().unwrap()["merged"] = json!(true);
        s.pr.as_mut().unwrap()["merge_commit_sha"] =
            json!(git(&f.remote, &["rev-parse", "HEAD"]).unwrap());
    }
    assert_eq!(
        f.worker.service.reconcile_pending().await.unwrap(),
        vec![128]
    );
    assert_eq!(f.state.lock().unwrap().issue["state"], "closed");
}

#[tokio::test]
async fn qa_tests_the_pr_candidate_and_publishes_a_separate_report_commit() {
    use sha2::{Digest, Sha256};
    let f = fixture("qa");
    git(&f.remote, &["checkout", "-b", "candidate"]).unwrap();
    std::fs::write(f.remote.join("answer.txt"), "42").unwrap();
    commit(&f.remote, "candidate");
    let candidate = git(&f.remote, &["rev-parse", "HEAD"]).unwrap();
    git(&f.remote, &["checkout", "main"]).unwrap();
    {
        let mut s = f.state.lock().unwrap();
        s.pr = Some(
            json!({"number":9,"head":{"sha":candidate,"ref":"candidate"},"base":{"ref":"main"},"state":"open","draft":false,"merged":false}),
        );
        let mut meta = metadata(s.issue["body"].as_str().unwrap()).unwrap();
        let mut producer = meta.clone();
        producer["issue_ref"] = f.worker.service.repo.issue_ref(127);
        producer["dependencies"] = json!([]);
        s.producer = Some(
            json!({"number":127,"state":"open","body":replace_metadata("Producer",&producer).unwrap()}),
        );
        meta["dependencies"] = json!([{"dependency_id":Uuid::new_v4(),"producer_issue":f.worker.service.repo.issue_ref(127),"condition":"pr_ready","selector":{"pr_number":9}}]);
        s.issue["body"] = json!(replace_metadata("QA candidate", &meta).unwrap());
    }
    let work = start(&f, false).await;
    assert_eq!(work.prepared.base_sha, candidate);
    let test = test_evidence(&work).await;
    assert_eq!(test["outcome"], "pass");
    assert_eq!(test["candidate_sha"], candidate);
    std::fs::write(work.prepared.workspace.join("qa-report.md"), "QA passed").unwrap();
    commit(&work.prepared.workspace, "QA report");
    let report_commit = work.call("work_push", &json!({})).await.unwrap()["sha"].clone();
    assert_ne!(report_commit, candidate);
    let artifact = json!({"evidence_id":Uuid::new_v4(),"kind":"artifact","candidate_sha":candidate,"outcome":"pass","summary":"QA report","acceptance_ids":["AC1"],"artifact":{"kind":"git_file","repository":{"instance_id":f.worker.service.repo.instance_id,"repository_id":42},"commit_sha":report_commit,"path":"qa-report.md","sha256":format!("{:x}",Sha256::digest(b"QA passed"))}});
    let report = json!({"candidate_sha":candidate,"evidence":[test,artifact],"acceptance_results":[{"id":"AC1","outcome":"pass","evidence_ids":[test["evidence_id"]]}]});
    work.call("work_report_delivery", &report).await.unwrap();
    finish(&work).await;
    f.worker.service.accept_delivery(128).await.unwrap();
    assert_eq!(f.state.lock().unwrap().issue["state"], "closed");
}

#[tokio::test]
async fn twenty_workers_launch_only_one_native_runtime_for_one_issue() {
    let f = fixture("backend");
    devmesh_agent::workspace::ensure_repository(
        &f.worker.project.path,
        &f.worker.project.remote,
        true,
    )
    .unwrap();
    let mut handles = Vec::new();
    for _ in 0..20 {
        let worker = Arc::new(Worker {
            store: Arc::new(Store::memory().unwrap()),
            service: f.worker.service.clone(),
            project: f.worker.project.clone(),
            max_concurrency: 1,
        });
        let mut context = f.ctx.clone();
        context.worker_id = Uuid::new_v4();
        handles.push(tokio::spawn(async move {
            match worker.prepare(128, &context).await {
                Ok(prepared) => Some(launch_work(worker, prepared, false).await),
                Err(error) => {
                    assert!(matches!(
                        error.code.as_str(),
                        "claim_conflict" | "ineligible" | "repair_pending"
                    ));
                    None
                }
            }
        }));
    }
    let mut running = Vec::new();
    for handle in handles {
        if let Some(work) = handle.await.unwrap() {
            running.push(work);
        }
    }
    assert_eq!(running.len(), 1);
    let work = running.pop().unwrap();
    assert!(work.runtime.stop().await.unwrap());
    work.worker
        .finalize(
            &work.prepared,
            WorkState::Blocked,
            "concurrency acceptance finished",
            true,
        )
        .await
        .unwrap();
    assert!(work.worker.store.active().unwrap().is_empty());
}

#[tokio::test]
async fn automatic_discovery_validates_the_full_issue_behind_the_official_mcp_summary() {
    let f = fixture("backend");
    {
        let mut gitea = f.state.lock().unwrap();
        gitea.issue["assignees"] = json!([{"id":14,"login":"worker"}]);
        gitea.issue_summary = Some(json!({
            "number":128, "state":"open", "assignees":["worker"],
            "labels":["status:ready","role:backend","agent:allowed", "priority:p2", "type:feature"]
        }));
    }
    assert_eq!(f.worker.candidates(&f.ctx).await.unwrap(), vec![128]);
    f.state.lock().unwrap().issue["assignees"] = json!([{"id":4,"login":"another"}]);
    assert!(f.worker.candidates(&f.ctx).await.unwrap().is_empty());
    {
        let mut gitea = f.state.lock().unwrap();
        gitea.issue["assignees"] = json!([]);
        gitea.issue["state"] = json!("closed");
    }
    assert!(f.worker.candidates(&f.ctx).await.unwrap().is_empty());
}
