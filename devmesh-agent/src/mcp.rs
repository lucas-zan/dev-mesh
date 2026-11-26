//! MCP stdio entry point. Identity and work bindings are supplied by the launcher.
use crate::gitea::{ClaimContext, TaskService};
use devmesh_protocol::{Error, Result};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[async_trait::async_trait]
pub trait WorkActions: Send + Sync {
    async fn call(&self, method: &str, args: &Value) -> Result<Value>;
}

#[derive(Default, Clone)]
pub struct Context {
    pub directory: Option<crate::discovery::RepositoryDirectory>,
    pub service: Option<TaskService>,
    pub projects: Vec<TaskService>,
    pub claim: Option<ClaimContext>,
    pub role: String,
    pub can_prepare: bool,
    pub work: Option<Arc<dyn WorkActions>>,
    pub history: Option<Arc<dyn crate::history::HistoryReader>>,
}
pub struct McpServer {
    context: Context,
}
impl McpServer {
    pub fn new(context: Context) -> Self {
        Self { context }
    }
    pub async fn handle(&self, request: Value) -> Option<Value> {
        let id = request.get("id")?.clone();
        let result = match request["method"].as_str().unwrap_or("") {
            "initialize" => Ok(
                json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"devmesh","version":env!("CARGO_PKG_VERSION")}}),
            ),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools":tools()})),
            "tools/call" => {
                let name = request["params"]["name"].as_str().unwrap_or("");
                let args = request["params"]
                    .get("arguments")
                    .cloned()
                    .unwrap_or(json!({}));
                let result = self.call(name, &args).await;
                Ok(match result {
                    Ok(value) => {
                        json!({"content":[{"type":"text","text":value.to_string()}],"isError":false})
                    }
                    Err(error) => {
                        json!({"content":[{"type":"text","text":json!({"error":{"code":error.code,"message":error.message}}).to_string()}],"isError":true})
                    }
                })
            }
            _ => Err(json!({"code":-32601,"message":"Method not found"})),
        };
        Some(match result {
            Ok(value) => json!({"jsonrpc":"2.0","id":id,"result":value}),
            Err(error) => json!({"jsonrpc":"2.0","id":id,"error":error}),
        })
    }
    async fn call(&self, name: &str, args: &Value) -> Result<Value> {
        if matches!(name, "session_list" | "session_get" | "session_events") {
            return self
                .context
                .history
                .as_ref()
                .ok_or_else(|| Error::new("not_configured", "Relay history is not configured"))?
                .read(name, args)
                .await;
        }
        if name.starts_with("work_") || matches!(name, "issue_release" | "issue_comment") {
            let work = self
                .context
                .work
                .as_ref()
                .ok_or_else(|| Error::new("unbound_context", "No trusted Work context"))?;
            return work.call(name, args).await;
        }
        if let Some(directory) = &self.context.directory {
            let config = crate::discovery::resolve(&directory.config, &directory.client).await?;
            let mut context = self.context.clone();
            context.directory = None;
            context.service = None;
            context.projects = config
                .projects
                .iter()
                .map(|project| {
                    TaskService::new(directory.client.clone(), project.repository.clone())
                })
                .collect();
            return Self::new(context).call_repository(name, args).await;
        }
        self.call_repository(name, args).await
    }
    async fn call_repository(&self, name: &str, args: &Value) -> Result<Value> {
        if matches!(name, "project_list" | "repository_list") {
            return Ok(json!(self
                .context
                .service
                .iter()
                .chain(self.context.projects.iter())
                .map(|s| json!({"repository":s.repo,"role":self.context.role}))
                .collect::<Vec<_>>()));
        }
        let requested = args
            .pointer("/issue_ref/repository_id")
            .or_else(|| args.get("repository_id"))
            .and_then(Value::as_u64);
        let service = if let Some(id) = requested {
            self.context
                .service
                .iter()
                .chain(self.context.projects.iter())
                .find(|s| s.repo.repository_id == id)
                .ok_or_else(|| {
                    Error::new("permission_denied", "Target repository is not configured")
                })?
        } else {
            self.context
                .service
                .as_ref()
                .or_else(|| (self.context.projects.len() == 1).then(|| &self.context.projects[0]))
                .ok_or_else(|| {
                    Error::new(
                        "invalid_target",
                        "Select repository_id from repository_list",
                    )
                })?
        };
        if args.get("user_id").is_some() || args.get("claim").is_some() {
            return Err(Error::new(
                "invalid_request",
                "Identity and claim are launcher-owned",
            ));
        }
        if let Some(reference) = args.get("issue_ref") {
            if reference["instance_id"] != service.repo.instance_id.to_string()
                || reference["repository_id"] != service.repo.repository_id
            {
                return Err(Error::new(
                    "permission_denied",
                    "Target is not the configured project",
                ));
            }
        }
        let number = args
            .pointer("/issue_ref/issue_number")
            .and_then(Value::as_u64);
        match name {
            "issue_create" => {
                service.initialize_labels().await?;
                service.create(args, &self.context.role).await
            }
            "issue_get" => {
                service
                    .client
                    .issue(&service.repo, required_number(number)?)
                    .await
            }
            "issue_list" => Ok(json!(service.client.list(&service.repo, "all").await?)),
            "issue_claim" => {
                let ctx =
                    self.context.claim.as_ref().ok_or_else(|| {
                        Error::new("unbound_context", "No trusted worker identity")
                    })?;
                service.claim(required_number(number)?, ctx).await
            }
            "issue_prepare" | "issue_unblock" | "issue_request_rework" => {
                if !self.context.can_prepare
                    || self
                        .context
                        .service
                        .as_ref()
                        .is_some_and(|s| s.repo.repository_id != service.repo.repository_id)
                {
                    return Err(Error::new(
                        "permission_denied",
                        "Project prepare permission required",
                    ));
                }
                let state = if name == "issue_request_rework" {
                    "rework"
                } else {
                    "ready"
                };
                service.initialize_labels().await?;
                service
                    .prepare(
                        required_number(number)?,
                        state,
                        args["reason"].as_str().unwrap_or(""),
                    )
                    .await
            }
            _ => Err(Error::new("unknown_tool", name)),
        }
    }
    pub async fn serve_stdio(&self) -> Result<()> {
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        let mut output = tokio::io::stdout();
        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|e| Error::new("ipc_error", e.to_string()))?
        {
            let response = match serde_json::from_str(&line) {
                Ok(request) => self.handle(request).await,
                Err(_) => Some(
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}),
                ),
            };
            if let Some(response) = response {
                output
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .map_err(|e| Error::new("ipc_error", e.to_string()))?;
                output
                    .flush()
                    .await
                    .map_err(|e| Error::new("ipc_error", e.to_string()))?;
            }
        }
        Ok(())
    }
}
fn required_number(number: Option<u64>) -> Result<u64> {
    number.ok_or_else(|| Error::new("invalid_request", "issue_ref.issue_number required"))
}
fn tools() -> Vec<Value> {
    serde_json::from_str(include_str!("../resources/tools.json"))
        .expect("Generated tool contracts are valid JSON")
}
