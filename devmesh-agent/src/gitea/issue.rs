use super::{GiteaClient, Repo};
use devmesh_protocol::{issue_transition, metadata, replace_metadata, Error, Result};
use reqwest::Method;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Clone)]
pub struct TaskService {
    pub client: GiteaClient,
    pub repo: Repo,
}
impl TaskService {
    pub fn new(client: GiteaClient, repo: Repo) -> Self {
        Self { client, repo }
    }
    pub async fn read(&self, number: u64) -> Result<(Value, Value)> {
        let issue = self.client.issue(&self.repo, number).await?;
        let meta = metadata(
            issue["body"]
                .as_str()
                .ok_or_else(|| Error::new("invalid_metadata", "Missing body"))?,
        )?;
        if meta["issue_ref"] != self.repo.issue_ref(number) {
            return Err(Error::new(
                "invalid_target",
                "Issue reference does not match configured repository",
            ));
        }
        Ok((issue, meta))
    }
    pub async fn write(&self, issue: &Value, meta: &Value) -> Result<Value> {
        let version = issue["content_version"]
            .as_u64()
            .ok_or_else(|| Error::new("unsupported_capability", "Gitea content_version missing"))?;
        let body = replace_metadata(issue["body"].as_str().unwrap_or(""), meta)?;
        self.client
            .cas(
                &self.repo,
                issue["number"]
                    .as_u64()
                    .ok_or_else(|| Error::new("invalid_response", "Missing Issue number"))?,
                version,
                &body,
            )
            .await
    }
    pub async fn authorize(&self, number: u64) -> Result<Value> {
        let (issue, mut meta) = self.read(number).await?;
        if meta["claim"]["state"] == "active" {
            return Err(Error::new(
                "claim_conflict",
                "Active Claim cannot be authorized",
            ));
        }
        if meta["automation_allowed"] != true {
            meta["automation_allowed"] = json!(true);
            meta["state_rev"] = json!(meta["state_rev"].as_u64().unwrap_or(0) + 1);
            self.write(&issue, &meta).await?;
        }
        self.project_label(&issue, "agent", "allowed").await?;
        Ok(meta)
    }
    pub async fn prepare(&self, number: u64, state: &str, reason: &str) -> Result<Value> {
        let (issue, mut meta) = self.read(number).await?;
        if meta["claim"]["state"] == "active" {
            return Err(Error::new(
                "claim_conflict",
                "Active Claim cannot be prepared",
            ));
        }
        let from = meta["state"].as_str().unwrap_or("");
        if !issue_transition(from, state) {
            return Err(Error::new("invalid_state", format!("{from} -> {state}")));
        }
        if state == "ready" {
            self.validate_ready(&meta).await?;
        }
        meta["state"] = json!(state);
        meta["state_rev"] = json!(meta["state_rev"].as_u64().unwrap_or(0) + 1);
        if state == "rework" {
            meta["rework_reason"] = json!(reason);
        }
        if state == "blocked" {
            meta["blocked_reason"] = json!(reason);
        }
        let updated = self.write(&issue, &meta).await?;
        self.project_status(&updated, state).await?;
        Ok(meta)
    }
    pub async fn validate_ready(&self, meta: &Value) -> Result<()> {
        if !matches!(
            meta["required_role"].as_str(),
            Some("backend" | "frontend" | "product" | "qa")
        ) || meta["acceptance_criteria"]
            .as_array()
            .is_none_or(|v| v.is_empty())
        {
            return Err(Error::new(
                "invalid_metadata",
                "Role and acceptance criteria required",
            ));
        }
        self.check_dependency_cycles(meta).await?;
        self.resolve_dependencies(meta).await.map(|_| ())
    }
    pub async fn project_status(&self, issue: &Value, state: &str) -> Result<()> {
        self.project_status_labels(issue, state)
            .await
            .map_err(|error| Error::new("repair_pending", error.message))
    }
    async fn project_status_labels(&self, issue: &Value, state: &str) -> Result<()> {
        self.project_label(issue, "status", state).await
    }
    async fn project_label(&self, issue: &Value, prefix: &str, value: &str) -> Result<()> {
        let all = self
            .client
            .get(&format!("{}/labels?limit=100", self.repo.path()))
            .await?;
        let wanted = format!("{prefix}:{value}");
        let id = all
            .as_array()
            .into_iter()
            .flatten()
            .find(|label| label["name"] == wanted)
            .and_then(|label| label["id"].as_u64())
            .ok_or_else(|| {
                Error::new(
                    "repair_pending",
                    format!("Missing repository label {wanted}"),
                )
            })?;
        let number = issue["number"]
            .as_u64()
            .ok_or_else(|| Error::new("invalid_response", "Issue number missing"))?;
        let fresh = self.client.issue(&self.repo, number).await?;
        let path = format!("{}/issues/{number}/labels", self.repo.path());
        self.client
            .request(Method::POST, &path, Some(&json!({"labels":[id]})))
            .await?;
        for previous in fresh["labels"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|label| {
                label["name"]
                    .as_str()
                    .is_some_and(|name| name.starts_with(&format!("{prefix}:")))
                    && label["id"] != id
            })
        {
            self.client
                .request(Method::DELETE, &format!("{path}/{}", previous["id"]), None)
                .await?;
        }
        Ok(())
    }
    pub async fn initialize_labels(&self) -> Result<()> {
        let mut names = Vec::new();
        for (prefix, values) in [
            ("role", vec!["backend", "frontend", "product", "qa"]),
            (
                "status",
                vec![
                    "backlog", "ready", "claimed", "running", "review", "blocked", "rework", "done",
                ],
            ),
            ("priority", vec!["p0", "p1", "p2", "p3"]),
            (
                "type",
                vec!["requirement", "feature", "bug", "test", "improvement"],
            ),
            ("agent", vec!["allowed", "manual-only"]),
        ] {
            names.extend(values.into_iter().map(|v| format!("{prefix}:{v}")));
        }
        let current = self
            .client
            .get(&format!("{}/labels?limit=100", self.repo.path()))
            .await?;
        for name in names {
            if current
                .as_array()
                .into_iter()
                .flatten()
                .any(|l| l["name"] == name)
            {
                continue;
            }
            self.client
                .request(
                    Method::POST,
                    &format!("{}/labels", self.repo.path()),
                    Some(&json!({"name":name,"color":"5865f2"})),
                )
                .await?;
        }
        Ok(())
    }
    pub async fn create(&self, input: &Value, creator_role: &str) -> Result<Value> {
        let role = input["required_role"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_request", "required_role missing"))?;
        let title = input["title"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| Error::new("invalid_request", "title missing"))?;
        let dedup = input["dedup_key"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_request", "dedup_key missing"))?;
        let dedup = format!("{:x}", Sha256::digest(dedup.as_bytes()));
        for issue in self.client.list(&self.repo, "all").await? {
            if let Ok(meta) = metadata(issue["body"].as_str().unwrap_or("")) {
                if meta["dedup_key"] == dedup {
                    return self.finish_create(issue, meta).await;
                }
            }
        }
        let user = self.client.user().await?;
        let meta = json!({"schema_version":"1.1","task_id":Uuid::new_v4(),"issue_ref":null,"creator_gitea_user_id":user["id"],"creator_role":creator_role,"required_role":role,"type":input["type"],"priority":input["priority"],"automation_allowed":input["automation_requested"].as_bool().unwrap_or(false),"acceptance_criteria":input["acceptance_criteria"],"dedup_key":dedup,"parent_issue":input.get("parent_issue").unwrap_or(&Value::Null),"related_issues":[],"dependencies":input.get("dependencies").unwrap_or(&json!([])),"state":"backlog","state_rev":0,"claim_generation":0,"claim":null,"delivery":null,"blocked_reason":null,"rework_reason":null});
        let body = replace_metadata(input["description"].as_str().unwrap_or(""), &meta)?;
        let created = self
            .client
            .request(
                Method::POST,
                &format!("{}/issues", self.repo.path()),
                Some(&json!({"title":title,"body":body})),
            )
            .await?;
        self.finish_create(created, meta).await
    }
    async fn finish_create(&self, issue: Value, mut meta: Value) -> Result<Value> {
        let number = issue["number"]
            .as_u64()
            .ok_or_else(|| Error::new("invalid_response", "Missing Issue number"))?;
        let expected = self.repo.issue_ref(number);
        let updated = if meta["issue_ref"].is_null() {
            meta["issue_ref"] = expected;
            self.write(&issue, &meta).await?
        } else if meta["issue_ref"] == expected {
            issue
        } else {
            return Err(Error::new(
                "invalid_target",
                "Existing Issue reference differs",
            ));
        };
        self.project_all_labels(&updated, &meta).await?;
        Ok(meta)
    }
    async fn project_all_labels(&self, issue: &Value, meta: &Value) -> Result<()> {
        let labels = self
            .client
            .get(&format!("{}/labels?limit=100", self.repo.path()))
            .await?;
        let wanted = [
            format!("role:{}", meta["required_role"].as_str().unwrap()),
            format!("status:{}", meta["state"].as_str().unwrap()),
            format!("type:{}", meta["type"].as_str().unwrap_or("feature")),
            format!("priority:{}", meta["priority"].as_str().unwrap_or("p1")),
            format!(
                "agent:{}",
                if meta["automation_allowed"] == true {
                    "allowed"
                } else {
                    "manual-only"
                }
            ),
        ];
        let ids: Vec<_> = wanted
            .iter()
            .map(|name| {
                labels
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|l| l["name"] == *name)
                    .and_then(|l| l["id"].as_u64())
                    .ok_or_else(|| Error::new("repair_pending", format!("Missing label {name}")))
            })
            .collect::<Result<_>>()?;
        self.client
            .request(
                Method::POST,
                &format!("{}/issues/{}/labels", self.repo.path(), issue["number"]),
                Some(&json!({"labels":ids})),
            )
            .await?;
        Ok(())
    }
}
