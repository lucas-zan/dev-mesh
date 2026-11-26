use super::TaskService;
use devmesh_protocol::{metadata, now, Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimContext {
    pub owner: u64,
    pub device_id: Uuid,
    pub worker_id: Uuid,
    pub role: String,
    pub runtime: String,
}

impl TaskService {
    pub async fn claim(&self, number: u64, ctx: &ClaimContext) -> Result<Value> {
        self.claim_with_id(number, ctx, Uuid::new_v4()).await
    }
    pub async fn claim_with_id(
        &self,
        number: u64,
        ctx: &ClaimContext,
        intended: Uuid,
    ) -> Result<Value> {
        let (issue, mut meta) = self.read(number).await?;
        if meta["claim"]["claim_id"] == intended.to_string() {
            let (_, current) = self.verify_claim(number, &meta["claim"]).await?;
            if current["claim"]["owner_gitea_user_id"] != ctx.owner
                || current["claim"]["device_id"] != ctx.device_id.to_string()
            {
                return Err(Error::new(
                    "claim_conflict",
                    "Claim intent belongs to another worker",
                ));
            }
            self.project_status(&issue, current["state"].as_str().unwrap_or("claimed"))
                .await?;
            return Ok(current["claim"].clone());
        }
        self.check_eligibility(&issue, &meta, ctx)?;
        if self.client.user().await?["id"] != ctx.owner {
            return Err(Error::new(
                "permission_denied",
                "Claim owner differs from authenticated Gitea user",
            ));
        }
        self.validate_ready(&meta).await?;
        let claim = json!({"claim_id":intended,"generation":meta["claim_generation"].as_u64().unwrap_or(0)+1,"owner_gitea_user_id":ctx.owner,"device_id":ctx.device_id,"worker_id":ctx.worker_id,"execution_role":ctx.role,"runtime":ctx.runtime,"state":"active","claimed_at":now(),"released_at":null,"release_reason":null});
        meta["claim_generation"] = claim["generation"].clone();
        meta["claim"] = claim.clone();
        meta["state"] = json!("claimed");
        meta["state_rev"] = json!(meta["state_rev"].as_u64().unwrap_or(0) + 1);
        let updated = self.write(&issue, &meta).await?;
        self.verify_claim(number, &claim)
            .await
            .map_err(|error| Error::new("repair_pending", error.message))?;
        self.project_status(&updated, "claimed").await?;
        Ok(claim)
    }
    pub fn check_eligibility(&self, issue: &Value, meta: &Value, ctx: &ClaimContext) -> Result<()> {
        let assigned = issue["assignees"]
            .as_array()
            .is_some_and(|a| !a.is_empty() && !a.iter().any(|u| u["id"] == ctx.owner));
        if issue["state"] != "open"
            || meta["state"] != "ready"
            || meta["automation_allowed"] != true
            || meta["required_role"] != ctx.role
            || meta["claim"]["state"] == "active"
            || assigned
        {
            return Err(Error::new(
                "ineligible",
                "Issue is not eligible for this worker",
            ));
        }
        for (prefix, expected) in [
            ("status", "ready"),
            ("role", ctx.role.as_str()),
            ("agent", "allowed"),
            ("priority", meta["priority"].as_str().unwrap_or("")),
            ("type", meta["type"].as_str().unwrap_or("")),
        ] {
            let wanted = format!("{prefix}:{expected}");
            let values = issue["labels"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|l| l["name"].as_str())
                .filter(|n| n.starts_with(&format!("{prefix}:")))
                .collect::<Vec<_>>();
            if values != vec![wanted.as_str()] {
                return Err(Error::new(
                    "metadata_conflict",
                    format!("Labels conflict: {prefix}"),
                ));
            }
        }
        Ok(())
    }
    pub async fn verify_claim(&self, number: u64, claim: &Value) -> Result<(Value, Value)> {
        let (issue, meta) = self.read(number).await?;
        let current = &meta["claim"];
        if issue["state"] != "open"
            || current["state"] != "active"
            || current["claim_id"] != claim["claim_id"]
            || current["generation"] != claim["generation"]
        {
            return Err(Error::new("claim_conflict", "Claim is no longer active"));
        }
        let assigned = issue["assignees"].as_array().is_some_and(|a| {
            !a.is_empty() && !a.iter().any(|u| u["id"] == claim["owner_gitea_user_id"])
        });
        if assigned
            || meta["required_role"] != claim["execution_role"]
            || meta["automation_allowed"] != true
        {
            return Err(Error::new(
                "permission_denied",
                "Task authorization changed",
            ));
        }
        Ok((issue, meta))
    }
    pub async fn release(
        &self,
        number: u64,
        claim: &Value,
        state: &str,
        reason: &str,
        quiesced: bool,
    ) -> Result<Value> {
        if !quiesced {
            return Err(Error::new(
                "recovery_required",
                "Runtime must be confirmed quiescent",
            ));
        }
        if !matches!(state, "review" | "done" | "blocked" | "ready") {
            return Err(Error::new("invalid_state", state));
        }
        let (previous, metadata) = self.read(number).await?;
        if metadata["claim"]["state"] == "released"
            && metadata["claim"]["claim_id"] == claim["claim_id"]
            && metadata["claim"]["generation"] == claim["generation"]
            && metadata["state"] == state
        {
            self.project_status(&previous, state).await?;
            return Ok(metadata);
        }
        let (issue, mut meta) = self.verify_claim(number, claim).await?;
        meta["claim"]["state"] = json!("released");
        meta["claim"]["released_at"] = json!(now());
        meta["claim"]["release_reason"] = json!(reason);
        meta["state"] = json!(state);
        meta["state_rev"] = json!(meta["state_rev"].as_u64().unwrap_or(0) + 1);
        if state == "blocked" {
            meta["blocked_reason"] = json!(reason);
        }
        let updated = self.write(&issue, &meta).await?;
        self.project_status(&updated, state).await?;
        Ok(meta)
    }
    pub async fn resolve_dependencies(&self, meta: &Value) -> Result<Vec<Value>> {
        let mut resolved = Vec::new();
        for dep in meta["dependencies"].as_array().into_iter().flatten() {
            let reference = &dep["producer_issue"];
            if reference["instance_id"] != self.repo.instance_id.to_string() {
                return Err(Error::new(
                    "unsupported_dependency",
                    "Cross-instance dependency requires a configured Gitea client",
                ));
            }
            let repo = self
                .client
                .get(&format!("repositories/{}", reference["repository_id"]))
                .await?;
            let path = format!(
                "repos/{}/{}",
                repo["owner"]["login"].as_str().unwrap_or(""),
                repo["name"].as_str().unwrap_or("")
            );
            let issue = self
                .client
                .get(&format!("{path}/issues/{}", reference["issue_number"]))
                .await?;
            let producer = metadata(issue["body"].as_str().unwrap_or(""))?;
            match dep["condition"].as_str().unwrap_or("") {
                "issue_closed" if issue["state"] == "closed" && producer["state"] == "done" => {
                    crate::delivery::validate_delivery(&producer, &producer["delivery"]).map_err(
                        |_| {
                            Error::new(
                                "dependency_not_ready",
                                "Closed producer has no valid delivery",
                            )
                        },
                    )?;
                }
                "artifact_ready" => {
                    let artifact = if let Some(explicit) = dep.get("artifact") {
                        explicit.clone()
                    } else {
                        producer["delivery"]["evidence"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .find(|e| {
                                e["kind"] == "artifact"
                                    && e["artifact_key"] == dep["artifact_key"]
                                    && e["outcome"] == "pass"
                            })
                            .map(|e| e["artifact"].clone())
                            .ok_or_else(|| {
                                Error::new("dependency_not_ready", "Named artifact missing")
                            })?
                    };
                    self.verify_artifact(&artifact).await?;
                    resolved.push(json!({"dependency":dep,"artifact":artifact}));
                    continue;
                }
                "pr_ready" | "pr_merged" => {
                    let number = dep
                        .pointer("/selector/pr_number")
                        .or_else(|| dep.get("pr_number"))
                        .or_else(|| {
                            producer["delivery"]["evidence"]
                                .as_array()
                                .and_then(|e| e.iter().find(|e| e["kind"] == "pr"))
                                .and_then(|e| e.pointer("/pr/number"))
                        })
                        .ok_or_else(|| {
                            Error::new("dependency_not_ready", "PR selector unresolved")
                        })?;
                    let pr = self.client.get(&format!("{path}/pulls/{number}")).await?;
                    if dep["condition"] == "pr_merged" && pr["merged"] != true
                        || dep["condition"] == "pr_ready"
                            && (pr["state"] != "open" || pr["draft"] == true)
                    {
                        return Err(Error::new("dependency_not_ready", "PR not ready"));
                    }
                    if dep
                        .get("required_sha")
                        .is_some_and(|sha| *sha != pr["head"]["sha"])
                    {
                        return Err(Error::new(
                            "dependency_not_ready",
                            "PR candidate SHA differs from dependency",
                        ));
                    }
                    resolved.push(json!({"dependency":dep,"head_sha":pr["head"]["sha"]}));
                    continue;
                }
                _ => {
                    return Err(Error::new(
                        "dependency_not_ready",
                        "Dependency condition not satisfied",
                    ))
                }
            }
            resolved.push(json!({"dependency":dep,"delivery":producer["delivery"]}));
        }
        Ok(resolved)
    }
}
