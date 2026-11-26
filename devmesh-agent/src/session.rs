use crate::{
    collector::sanitize,
    mcp::WorkActions,
    runtime::Runtime,
    worker::{PreparedWork, Worker},
};
use devmesh_protocol::{Error, Result, WorkState};
use reqwest::Method;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::Mutex;

pub struct ManagedWork {
    pub worker: Arc<Worker>,
    pub prepared: PreparedWork,
    pub runtime: Arc<Runtime>,
    pub finish: Mutex<Option<(WorkState, String)>>,
    pub(crate) operation: Mutex<()>,
    pub closing: AtomicBool,
    pub tests: Vec<String>,
}
impl ManagedWork {
    pub fn new(
        worker: Arc<Worker>,
        prepared: PreparedWork,
        runtime: Arc<Runtime>,
        tests: Vec<String>,
    ) -> Self {
        Self {
            worker,
            prepared,
            runtime,
            finish: Mutex::new(None),
            operation: Mutex::new(()),
            closing: AtomicBool::new(false),
            tests,
        }
    }
    async fn test(&self, args: &Value) -> Result<Value> {
        let command = test_command(&self.tests, args)?;
        self.worker
            .service
            .verify_claim(self.prepared.issue_number, &self.prepared.claim)
            .await?;
        let sha = crate::workspace::git(&self.prepared.workspace, &["rev-parse", "HEAD"])?;
        let output = tokio::process::Command::new("sh")
            .args(["-lc", &command])
            .current_dir(&self.prepared.workspace)
            .output()
            .await
            .map_err(|e| Error::new("test_failed", e.to_string()))?;
        let summary = sanitize(
            &format!(
                "$ {}\n{}\n{}",
                command,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
            "system",
        );
        let evidence = json!({"evidence_id":uuid::Uuid::new_v4(),"kind":"test","candidate_sha":sha,"outcome":if output.status.success(){"pass"}else{"fail"},"exit_code":output.status.code().unwrap_or(-1),"environment":"local-worktree","summary":summary["text"],"acceptance_ids":args.get("acceptance_ids").cloned().unwrap_or(json!([]))});
        self.worker.store.set_setting(
            &format!(
                "test:{}:{}",
                self.prepared.work_id,
                evidence["evidence_id"].as_str().unwrap()
            ),
            &evidence,
        )?;
        Ok(evidence)
    }
    async fn create_pr(&self, args: &Value) -> Result<Value> {
        let title = args["title"]
            .as_str()
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| Error::new("invalid_request", "PR title required"))?;
        let base = &self.worker.project.branch;
        if args
            .get("target_branch")
            .is_some_and(|branch| *branch != *base)
        {
            return Err(Error::new(
                "invalid_target",
                "PR target must match configured branch",
            ));
        }
        let path = format!("{}/pulls", self.worker.service.repo.path());
        let mut page = 1;
        loop {
            let list = self
                .worker
                .service
                .client
                .get(&format!("{path}?state=open&limit=50&page={page}"))
                .await?;
            let list = list
                .as_array()
                .ok_or_else(|| Error::new("invalid_response", "PR list missing"))?;
            if let Some(existing) = list
                .iter()
                .find(|pr| pr["head"]["ref"] == self.prepared.branch && pr["base"]["ref"] == *base)
            {
                return Ok(existing.clone());
            }
            if list.is_empty() {
                break;
            }
            page += 1;
        }
        self.worker
            .service
            .verify_claim(self.prepared.issue_number, &self.prepared.claim)
            .await?;
        self.worker.service.client.request(Method::POST,&path,Some(&json!({"head":self.prepared.branch,"base":base,"title":title,"body":args["body"]}))).await
    }
    async fn report(&self, args: &Value) -> Result<Value> {
        if self
            .worker
            .store
            .queued_commands()?
            .iter()
            .any(|command| command["target"]["work_id"] == self.prepared.work_id.to_string())
        {
            return Err(Error::new(
                "pending_follow_up",
                "Complete accepted follow-up commands before final delivery",
            ));
        }
        let current = self
            .worker
            .service
            .resolve_dependencies(&self.prepared.metadata)
            .await?;
        if current != self.prepared.dependency_snapshot {
            self.closing.store(true, Ordering::SeqCst);
            *self.finish.lock().await = Some((
                WorkState::Blocked,
                "dependency version changed; revalidate".into(),
            ));
            return Err(Error::new(
                "revalidate",
                "Dependency version changed after Work started",
            ));
        }
        let mut report = json!({"work_id":self.prepared.work_id,"claim_generation":self.prepared.claim["generation"],"candidate_sha":args["candidate_sha"],"evidence":args.get("evidence").or_else(||args.get("evidence_refs")),"acceptance_results":args["acceptance_results"],"reported_at":devmesh_protocol::now()});
        for evidence in report["evidence"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| e["kind"] == "test")
        {
            let key = format!(
                "test:{}:{}",
                self.prepared.work_id,
                evidence["evidence_id"].as_str().unwrap_or("")
            );
            let real = self.worker.store.setting(&key)?.ok_or_else(|| {
                Error::new(
                    "missing_delivery",
                    "Run configured tests through work_run_test before reporting",
                )
            })?;
            if real != *evidence {
                return Err(Error::new(
                    "stale_evidence",
                    "Reported test differs from observed test",
                ));
            }
        }
        report["reported_at"] = json!(devmesh_protocol::now());
        self.worker
            .service
            .verify_delivery(
                self.prepared.issue_number,
                &self.prepared.claim,
                &report,
                &self.prepared.workspace,
                &self.worker.project.branch,
            )
            .await?;
        self.worker
            .store
            .update_work(self.prepared.work_id, &json!({"delivery":report}))?;
        self.closing.store(true, Ordering::SeqCst);
        *self.finish.lock().await = Some((WorkState::Completed, "delivery verified".into()));
        Ok(json!({"accepted":true,"state":"finalizing","delivery":report}))
    }
}
#[async_trait::async_trait]
impl WorkActions for ManagedWork {
    async fn call(&self, method: &str, args: &Value) -> Result<Value> {
        if let Some(id) = args.get("work_id") {
            if id != &json!(self.prepared.work_id) {
                return Err(Error::new("invalid_target", "Work is bound by launcher"));
            }
        }
        if matches!(method, "work_status" | "work_start") {
            return self.worker.store.work(self.prepared.work_id);
        }
        let _operation = self.operation.lock().await;
        if self.closing.load(Ordering::SeqCst) {
            return Err(Error::new("invalid_state", "Work is finalizing"));
        }
        self.worker
            .service
            .verify_claim(self.prepared.issue_number, &self.prepared.claim)
            .await?;
        match method {
            "work_run_test" => self.test(args).await,
            "work_report_delivery" => self.report(args).await,
            "work_finish" | "work_report_blocked" | "issue_release" => {
                self.closing.store(true, Ordering::SeqCst);
                *self.finish.lock().await = Some((
                    WorkState::Blocked,
                    args["reason"].as_str().unwrap_or("missing_delivery").into(),
                ));
                Ok(json!({"accepted":true,"state":"finalizing"}))
            }
            "issue_comment" => {
                let reference = args.get("issue_ref").cloned().unwrap_or(
                    self.worker
                        .service
                        .repo
                        .issue_ref(self.prepared.issue_number),
                );
                if reference
                    != self
                        .worker
                        .service
                        .repo
                        .issue_ref(self.prepared.issue_number)
                {
                    return Err(Error::new(
                        "invalid_target",
                        "Comment must belong to this Work",
                    ));
                }
                self.worker
                    .service
                    .client
                    .request(
                        Method::POST,
                        &format!(
                            "{}/issues/{}/comments",
                            self.worker.service.repo.path(),
                            self.prepared.issue_number
                        ),
                        Some(&json!({"body":args["text"]})),
                    )
                    .await
            }
            "work_create_pr" => self.create_pr(args).await,
            "work_push" => Ok(
                json!({"sha":self.worker.service.push_candidate(self.prepared.issue_number,&self.prepared.claim,&self.prepared.workspace,&self.prepared.branch).await?}),
            ),
            _ => Err(Error::new("unsupported_capability", method)),
        }
    }
}

pub fn test_command(tests: &[String], args: &Value) -> Result<String> {
    if let Some(command) = args.get("command") {
        if !tests.is_empty() || args.get("index").is_some() {
            return Err(Error::new(
                "invalid_request",
                "Use the configured test index",
            ));
        }
        return command
            .as_str()
            .filter(|command| !command.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| Error::new("invalid_request", "Repository test command required"));
    }
    let index = args["index"].as_u64().ok_or_else(|| {
        Error::new(
            "invalid_request",
            "Test index or repository command required",
        )
    })? as usize;
    tests
        .get(index)
        .cloned()
        .ok_or_else(|| Error::new("invalid_request", "Test command not configured"))
}
