use crate::gitea::TaskService;
use devmesh_protocol::{Error, Result};
use reqwest::Method;
use serde_json::Value;
use std::path::Path;

fn missing(message: &str) -> Error {
    Error::new("missing_delivery", message)
}
fn valid_sha(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        matches!(s.len(), 40 | 64)
            && s.bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    })
}

pub fn validate_delivery(meta: &Value, report: &Value) -> Result<()> {
    let evidence = report["evidence"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| missing("No evidence"))?;
    let role = meta["required_role"].as_str().unwrap_or("");
    if matches!(role, "backend" | "frontend" | "qa") && !valid_sha(&report["candidate_sha"]) {
        return Err(missing("Immutable candidate SHA required"));
    }
    for item in evidence {
        if !item["candidate_sha"].is_null() && item["candidate_sha"] != report["candidate_sha"] {
            return Err(Error::new(
                "stale_evidence",
                "Evidence belongs to another candidate",
            ));
        }
        if item["kind"] == "artifact" {
            validate_artifact(&item["artifact"])?;
        }
        if item["kind"] == "test"
            && (item["outcome"] != "pass"
                || item["exit_code"] != 0
                || item["environment"].as_str().is_none_or(str::is_empty))
        {
            return Err(missing("Test did not pass or environment missing"));
        }
    }
    let results = report["acceptance_results"]
        .as_array()
        .ok_or_else(|| missing("Acceptance results required"))?;
    for ac in meta["acceptance_criteria"]
        .as_array()
        .ok_or_else(|| missing("Acceptance criteria missing"))?
    {
        let result = results
            .iter()
            .find(|r| r["id"] == ac["id"] && r["outcome"] == "pass")
            .ok_or_else(|| missing("Every acceptance criterion must pass"))?;
        let ids = result["evidence_ids"]
            .as_array()
            .filter(|ids| !ids.is_empty())
            .ok_or_else(|| missing("Acceptance evidence missing"))?;
        if ids.iter().any(|id| {
            !evidence
                .iter()
                .any(|e| e["evidence_id"] == *id && e["outcome"] == "pass")
        }) {
            return Err(missing("Unresolved acceptance evidence"));
        }
    }
    if matches!(role, "backend" | "frontend")
        && (!evidence.iter().any(|e| e["kind"] == "pr")
            || !evidence.iter().any(|e| e["kind"] == "test"))
    {
        return Err(missing("PR and test evidence required"));
    }
    if role == "qa" && !evidence.iter().any(|e| e["kind"] == "test") {
        return Err(missing("QA requires observed test evidence"));
    }
    if matches!(role, "product" | "qa") && !evidence.iter().any(|e| e["kind"] == "artifact") {
        return Err(missing("Immutable report/document artifact required"));
    }
    Ok(())
}

pub fn validate_artifact(artifact: &Value) -> Result<()> {
    let hash = artifact["sha256"]
        .as_str()
        .is_some_and(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()));
    let repo = artifact["repository"]["repository_id"].as_u64().is_some();
    let immutable = match artifact["kind"].as_str().unwrap_or("") {
        "git_file" => {
            valid_sha(&artifact["commit_sha"])
                && artifact["path"].as_str().is_some_and(|p| !p.is_empty())
        }
        "release_asset" => artifact["asset_id"].as_u64().is_some(),
        _ => false,
    };
    if !hash || !repo || !immutable {
        return Err(missing(
            "Artifact requires repository, immutable version and SHA256",
        ));
    }
    Ok(())
}

impl TaskService {
    pub async fn verify_delivery(
        &self,
        number: u64,
        claim: &Value,
        report: &Value,
        workspace: &Path,
        target_branch: &str,
    ) -> Result<Value> {
        let (issue, mut meta) = self.verify_claim(number, claim).await?;
        validate_delivery(&meta, report)?;
        for evidence in report["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "artifact")
        {
            self.verify_artifact(&evidence["artifact"]).await?;
        }
        if matches!(meta["required_role"].as_str(), Some("backend" | "frontend")) {
            let head = crate::workspace::git(workspace, &["rev-parse", "HEAD"])?;
            if report["candidate_sha"] != head {
                return Err(Error::new(
                    "stale_evidence",
                    "Workspace HEAD differs from candidate",
                ));
            }
            for evidence in report["evidence"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["kind"] == "pr")
            {
                let pr = self
                    .client
                    .get(&format!(
                        "{}/pulls/{}",
                        self.repo.path(),
                        evidence["pr"]["number"]
                    ))
                    .await?;
                if evidence["pr"]["repository"]
                    != serde_json::json!({"instance_id":self.repo.instance_id,"repository_id":self.repo.repository_id})
                {
                    return Err(Error::new(
                        "invalid_target",
                        "PR belongs to another repository",
                    ));
                }
                validate_pr_candidate(&pr, &evidence["pr"], &head, target_branch)?;
            }
        }
        if meta["required_role"] == "qa" {
            let dependencies = self.resolve_dependencies(&meta).await?;
            if let Some(candidate) =
                crate::worker::candidate_revision(&meta, &dependencies, self.repo.repository_id)?
            {
                if report["candidate_sha"] != candidate {
                    return Err(Error::new(
                        "stale_evidence",
                        "QA tests must target the selected dependency candidate",
                    ));
                }
            }
        }
        meta["delivery"] = report.clone();
        meta["state_rev"] = Value::from(meta["state_rev"].as_u64().unwrap_or(0) + 1);
        self.write(&issue, &meta).await?;
        Ok(meta)
    }
    pub async fn push_candidate(
        &self,
        number: u64,
        claim: &Value,
        workspace: &Path,
        branch: &str,
    ) -> Result<String> {
        self.verify_claim(number, claim).await?;
        if !branch.starts_with(&format!("devmesh/issue-{number}/")) {
            return Err(Error::new(
                "invalid_target",
                "Only this task branch may be pushed",
            ));
        }
        let remote = crate::workspace::git(workspace, &["remote", "get-url", "--push", "origin"])?;
        crate::workspace::git_remote(
            workspace,
            &["push", "origin", &format!("HEAD:refs/heads/{branch}")],
            &remote,
            Some(&self.client),
        )?;
        self.verify_claim(number, claim).await?;
        crate::workspace::git(workspace, &["rev-parse", "HEAD"])
    }
    pub async fn reconcile_merged(&self, number: u64) -> Result<Value> {
        let (issue, meta) = self.read(number).await?;
        if meta["state"] == "done" {
            return self.finish_done(issue, meta).await;
        }
        require_review(&meta)?;
        validate_delivery(&meta, &meta["delivery"])?;
        let prs = meta["delivery"]["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "pr")
            .collect::<Vec<_>>();
        if prs.is_empty() {
            return Err(missing("No candidate PR"));
        }
        for item in prs {
            let pr = self
                .client
                .get(&format!(
                    "{}/pulls/{}",
                    self.repo.path(),
                    item["pr"]["number"]
                ))
                .await?;
            if pr["merged"] != true {
                return Err(Error::new(
                    "not_merged_yet",
                    "Candidate PR is awaiting merge",
                ));
            }
            if pr["head"]["sha"] != meta["delivery"]["candidate_sha"]
                || item["pr"]["head_sha"] != meta["delivery"]["candidate_sha"]
                || pr["base"]["ref"] != item["pr"]["target_branch"]
                || !valid_sha(&pr["merge_commit_sha"])
                || item["pr"]["repository"]
                    != serde_json::json!({"instance_id":self.repo.instance_id,"repository_id":self.repo.repository_id})
            {
                return Err(Error::new(
                    "stale_evidence",
                    "Merged PR differs from the delivered candidate or target",
                ));
            }
        }
        self.finish_done(issue, meta).await
    }
    pub async fn accept_delivery(&self, number: u64) -> Result<Value> {
        let (issue, meta) = self.read(number).await?;
        if !matches!(meta["required_role"].as_str(), Some("product" | "qa")) {
            return Err(Error::new(
                "invalid_state",
                "Code delivery requires an actual merged PR",
            ));
        }
        if meta["state"] == "done" {
            return self.finish_done(issue, meta).await;
        }
        require_review(&meta)?;
        validate_delivery(&meta, &meta["delivery"])?;
        for evidence in meta["delivery"]["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "artifact")
        {
            self.verify_artifact(&evidence["artifact"]).await?;
        }
        let dependencies = self.resolve_dependencies(&meta).await?;
        if let Some(candidate) =
            crate::worker::candidate_revision(&meta, &dependencies, self.repo.repository_id)?
        {
            if meta["delivery"]["candidate_sha"] != candidate {
                return Err(Error::new(
                    "stale_evidence",
                    "QA report is for a different candidate",
                ));
            }
        }
        self.finish_done(issue, meta).await
    }
    async fn finish_done(&self, mut issue: Value, mut meta: Value) -> Result<Value> {
        if meta["state"] != "done" {
            meta["state"] = Value::from("done");
            meta["state_rev"] = Value::from(meta["state_rev"].as_u64().unwrap_or(0) + 1);
            issue = self.write(&issue, &meta).await?;
        }
        self.project_status(&issue, "done").await?;
        let current = self
            .client
            .issue(&self.repo, issue["number"].as_u64().unwrap())
            .await?;
        if current["state"] != "closed" {
            self.client
                .request(
                    Method::PATCH,
                    &format!("{}/issues/{}", self.repo.path(), issue["number"]),
                    Some(&serde_json::json!({"state":"closed"})),
                )
                .await?;
        }
        Ok(meta)
    }
    pub async fn reconcile_pending(&self) -> Result<Vec<u64>> {
        let mut completed = Vec::new();
        for issue in self.client.list(&self.repo, "all").await? {
            let Ok(meta) = devmesh_protocol::metadata(issue["body"].as_str().unwrap_or("")) else {
                continue;
            };
            if !matches!(meta["state"].as_str(), Some("review" | "done"))
                || !matches!(meta["required_role"].as_str(), Some("backend" | "frontend"))
            {
                continue;
            }
            if meta["state"] == "done" && issue["state"] == "closed" {
                continue;
            }
            let number = issue["number"]
                .as_u64()
                .ok_or_else(|| Error::new("invalid_response", "Issue number missing"))?;
            match self.reconcile_merged(number).await {
                Ok(_) => completed.push(number),
                Err(error) if error.code == "not_merged_yet" => {}
                Err(error) => eprintln!("Issue #{number} reconcile: {error}"),
            }
        }
        Ok(completed)
    }
}

pub fn validate_pr_candidate(
    pr: &Value,
    reference: &Value,
    candidate: &str,
    target: &str,
) -> Result<()> {
    if pr["head"]["sha"] != candidate
        || reference["head_sha"] != candidate
        || pr["base"]["ref"] != target
        || reference["target_branch"] != target
        || pr["state"] != "open"
        || pr["draft"] == true
    {
        return Err(Error::new(
            "stale_evidence",
            "PR is not the tested candidate on the configured target branch",
        ));
    }
    Ok(())
}

fn require_review(meta: &Value) -> Result<()> {
    if meta["state"] != "review" || meta["claim"]["state"] == "active" {
        return Err(Error::new(
            "invalid_state",
            "Only released review work can be accepted",
        ));
    }
    Ok(())
}
