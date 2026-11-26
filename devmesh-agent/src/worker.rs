//! Local scheduling owns deterministic state. Runtime I/O is supplied by a narrow driver.
use crate::{
    gitea::{ClaimContext, TaskService},
    storage::Store,
    workspace::Workspace,
};
use devmesh_protocol::{Error, Result, WorkState};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};
use uuid::Uuid;

#[derive(Clone)]
pub struct WorkerProject {
    pub key: String,
    pub path: PathBuf,
    pub worktrees: PathBuf,
    pub remote: String,
    pub branch: String,
    pub concurrency: usize,
    pub auto_clone: bool,
}
pub struct Worker {
    pub store: Arc<Store>,
    pub service: TaskService,
    pub project: WorkerProject,
    pub max_concurrency: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PreparedWork {
    pub work_id: Uuid,
    pub session_id: Uuid,
    pub issue_number: u64,
    pub claim: Value,
    pub workspace: PathBuf,
    pub branch: String,
    pub base_sha: String,
    pub metadata: Value,
    #[serde(default)]
    pub issue_source: Value,
    #[serde(default)]
    pub dependency_snapshot: Vec<Value>,
}

impl Worker {
    pub async fn prepare(&self, number: u64, ctx: &ClaimContext) -> Result<PreparedWork> {
        crate::workspace::ensure_repository_with_client(
            &self.project.path,
            &self.project.remote,
            self.project.auto_clone,
            Some(&self.service.client),
        )?;
        let id = self.store.reserve(
            &self.project.key,
            number,
            self.max_concurrency,
            self.project.concurrency,
        )?;
        // The reserved work is durable before any remote claim side effect.
        let intended = Uuid::new_v4();
        self.store.update_work(
            id,
            &json!({"claim_request_id":intended,"claim_context":ctx}),
        )?;
        let claim = match self.service.claim_with_id(number, ctx, intended).await {
            Ok(claim) => claim,
            Err(error) => {
                self.store.transition(id, WorkState::Finalizing)?;
                if error.code == "unknown_outcome" || error.code == "repair_pending" {
                    self.store.update_work(
                        id,
                        &json!({"recovery_required":true,"reason":error.to_string()}),
                    )?;
                    // A potentially successful claim cannot free this local slot.
                } else {
                    self.store.transition(id, WorkState::Failed)?;
                }
                return Err(error);
            }
        };
        self.store.update_work(
            id,
            &json!({"claim":claim,"issue_ref":self.service.repo.issue_ref(number)}),
        )?;
        match self.prepare_claimed(id, number, &claim).await {
            Ok(work) => Ok(work),
            Err(error) => {
                self.store.transition(id, WorkState::Finalizing)?;
                self.service
                    .release(number, &claim, "blocked", &error.to_string(), true)
                    .await?;
                self.store.transition(id, WorkState::Blocked)?;
                Err(error)
            }
        }
    }
    async fn prepare_claimed(&self, id: Uuid, number: u64, claim: &Value) -> Result<PreparedWork> {
        let (issue, metadata) = self.service.verify_claim(number, claim).await?;
        let dependency_snapshot = self.service.resolve_dependencies(&metadata).await?;
        let revision = candidate_revision(
            &metadata,
            &dependency_snapshot,
            self.service.repo.repository_id,
        )?;
        let workspace = Workspace::create_with_client(
            &self.project.path,
            &self.project.remote,
            revision.as_deref().unwrap_or(&self.project.branch),
            number,
            claim["claim_id"].as_str().unwrap().parse().unwrap(),
            &self.project.worktrees,
            Some(&self.service.client),
        )?;
        let prepared = PreparedWork {
            work_id: id,
            session_id: Uuid::new_v4(),
            issue_number: number,
            claim: claim.clone(),
            workspace: workspace.path,
            branch: workspace.branch,
            base_sha: workspace.base_sha,
            metadata,
            issue_source: json!({
                "repository_name": format!("{}/{}", self.service.repo.owner, self.service.repo.name),
                "issue_title": crate::collector::sanitize(issue["title"].as_str().unwrap_or(""), "system")["text"],
                "issue_url": issue["html_url"].as_str().unwrap_or("")
            }),
            dependency_snapshot,
        };
        self.store.update_work(id, &json!({"prepared":prepared}))?;
        Ok(prepared)
    }
    pub async fn mark_running(
        &self,
        work: &PreparedWork,
        instance: Uuid,
        native: &str,
    ) -> Result<()> {
        let (issue, mut meta) = self
            .service
            .verify_claim(work.issue_number, &work.claim)
            .await?;
        meta["state"] = json!("running");
        meta["state_rev"] = json!(meta["state_rev"].as_u64().unwrap_or(0) + 1);
        let issue = self.service.write(&issue, &meta).await?;
        self.service.project_status(&issue, "running").await?;
        self.store.update_work(
            work.work_id,
            &json!({"runtime_instance_id":instance,"native_session_id":native}),
        )?;
        let previous = self.store.work(work.work_id)?["state"].clone();
        self.store.transition(work.work_id, WorkState::Active)?;
        self.store.append_event(
            work.session_id,
            work.work_id,
            "0",
            "work.state",
            json!({"previous":previous,"state":"active","reason":"native session started"}),
        )?;
        Ok(())
    }
    pub async fn finalize(
        &self,
        work: &PreparedWork,
        state: WorkState,
        reason: &str,
        quiesced: bool,
    ) -> Result<()> {
        let current: WorkState =
            serde_json::from_value(self.store.work(work.work_id)?["state"].clone())
                .map_err(|e| Error::new("storage_error", e.to_string()))?;
        if current.terminal() {
            return if current == state {
                Ok(())
            } else {
                Err(Error::new(
                    "invalid_state",
                    "Work already ended with another outcome",
                ))
            };
        }
        self.store.update_work(
            work.work_id,
            &json!({"finalization_target":state,"finalization_reason":reason}),
        )?;
        if !quiesced {
            if current == WorkState::Active {
                self.store
                    .transition(work.work_id, WorkState::RecoveryRequired)?;
                self.store.append_event(
                    work.session_id,
                    work.work_id,
                    "0",
                    "work.state",
                    json!({"previous":current,"state":"recovery_required","reason":reason}),
                )?;
            }
            return Err(Error::new(
                "recovery_required",
                "Runtime quiescence is unknown",
            ));
        }
        if current != WorkState::Finalizing {
            self.store.transition(work.work_id, WorkState::Finalizing)?;
            self.store.append_event(
                work.session_id,
                work.work_id,
                "0",
                "work.state",
                json!({"previous":current,"state":"finalizing","reason":reason}),
            )?;
        }
        let issue_state = if state == WorkState::Completed {
            "review"
        } else {
            "blocked"
        };
        self.service
            .release(work.issue_number, &work.claim, issue_state, reason, true)
            .await?;
        let record = self.store.work(work.work_id)?;
        if !record["runtime_instance_id"].is_null() {
            let epoch = record["native_state"]["epoch"]
                .as_u64()
                .unwrap_or(0)
                .to_string();
            self.store.append_event(work.session_id,work.work_id,&epoch,"session.state",json!({"previous":null,"state":match state{WorkState::Failed=>"failed",WorkState::Cancelled=>"cancelled",_=>"ended"},"connection":"unknown","runtime_instance_id":record["runtime_instance_id"],"run_epoch":epoch}))?;
        }
        self.store.append_event(
            work.session_id,
            work.work_id,
            "0",
            "work.state",
            json!({"previous":"finalizing","state":state,"reason":reason}),
        )?;
        self.store.transition(work.work_id, state)
    }
    pub async fn candidates(&self, ctx: &ClaimContext) -> Result<Vec<u64>> {
        let mut candidates = Vec::new();
        let role = format!("role:{}", ctx.role);
        for summary in self.service.client.list(&self.service.repo, "open").await? {
            if !has_label(&summary, "status:ready") || !has_label(&summary, &role) {
                continue;
            }
            let Some(number) = summary["number"].as_u64() else {
                continue;
            };
            let (issue, meta) = match self.service.read(number).await {
                Ok(candidate) => candidate,
                Err(error)
                    if matches!(
                        error.code.as_str(),
                        "not_found" | "missing_metadata" | "invalid_metadata" | "invalid_target"
                    ) =>
                {
                    continue
                }
                Err(error) => return Err(error),
            };
            if self.service.check_eligibility(&issue, &meta, ctx).is_ok() {
                candidates.push((
                    meta["priority"].as_str().unwrap_or("p3").to_owned(),
                    issue["created_at"].as_str().unwrap_or("").to_owned(),
                    number,
                ));
            }
        }
        candidates.sort();
        Ok(candidates.into_iter().map(|(_, _, n)| n).collect())
    }
}

fn has_label(issue: &Value, name: &str) -> bool {
    issue["labels"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|label| label.as_str().or_else(|| label["name"].as_str()) == Some(name))
}

pub fn candidate_revision(
    meta: &Value,
    snapshot: &[Value],
    repository: u64,
) -> Result<Option<String>> {
    if meta["required_role"] != "qa" {
        return Ok(None);
    }
    let mut candidate = None;
    for item in snapshot
        .iter()
        .filter(|item| item.get("head_sha").is_some())
    {
        if item["dependency"]["producer_issue"]["repository_id"] != repository {
            return Err(Error::new(
                "unsupported_dependency",
                "Configure QA in the candidate repository",
            ));
        }
        let sha = item["head_sha"]
            .as_str()
            .ok_or_else(|| Error::new("dependency_not_ready", "Candidate SHA missing"))?;
        if candidate.as_deref().is_some_and(|previous| previous != sha) {
            return Err(Error::new(
                "dependency_not_ready",
                "QA Work requires a single candidate SHA",
            ));
        }
        candidate = Some(sha.to_owned());
    }
    Ok(candidate)
}
