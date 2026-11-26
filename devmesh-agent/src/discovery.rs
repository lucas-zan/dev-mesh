//! Token-visible repository inventory projected into the existing Work contract.
use crate::{
    config::{Config, Project},
    gitea::{GiteaClient, Repo},
};
use devmesh_protocol::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};
use uuid::Uuid;

#[derive(Clone)]
pub struct RepositoryDirectory {
    pub config: Config,
    pub client: GiteaClient,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RepositoryDefaults {
    pub instance_id: Uuid,
    pub workspace_root: PathBuf,
    pub runtime: String,
    #[serde(default)]
    pub auto_claim: bool,
    #[serde(default)]
    pub test_commands: Vec<String>,
}
#[derive(Deserialize)]
struct RepositoryInfo {
    id: u64,
    owner: Owner,
    name: String,
    default_branch: String,
    clone_url: String,
}
#[derive(Deserialize)]
struct Owner {
    login: String,
}

pub async fn inventory(client: &GiteaClient) -> Result<Vec<Value>> {
    let mut repositories = BTreeMap::new();
    let mut page = 1;
    loop {
        let value = client
            .get(&format!("repos/search?page={page}&limit=50&private=true"))
            .await?;
        let items = value["data"]
            .as_array()
            .ok_or_else(|| Error::new("invalid_response", "Expected repository list"))?;
        for item in items {
            let id = item["id"]
                .as_u64()
                .ok_or_else(|| Error::new("invalid_response", "Repository ID missing"))?;
            repositories.insert(id, item.clone());
        }
        if items.len() < 50 {
            break;
        }
        page += 1;
    }
    Ok(repositories.into_values().collect())
}

pub async fn resolve(config: &Config, client: &GiteaClient) -> Result<Config> {
    let Some(defaults) = &config.repositories else {
        return Ok(config.clone());
    };
    let mut resolved = config.clone();
    resolved.projects = inventory(client)
        .await?
        .into_iter()
        .map(|value| {
            let info: RepositoryInfo = serde_json::from_value(value)
                .map_err(|error| Error::new("invalid_response", error.to_string()))?;
            Ok(project(config, defaults, info))
        })
        .collect::<Result<_>>()?;
    Ok(resolved)
}

fn project(config: &Config, defaults: &RepositoryDefaults, info: RepositoryInfo) -> Project {
    let mut result = config
        .projects
        .iter()
        .find(|project| project.repository.repository_id == info.id)
        .cloned()
        .unwrap_or_else(|| Project {
            repository: Repo {
                instance_id: defaults.instance_id,
                repository_id: info.id,
                owner: info.owner.login.clone(),
                name: info.name.clone(),
            },
            local_path: defaults
                .workspace_root
                .join(&info.owner.login)
                .join(&info.name),
            roles: config.roles.clone(),
            runtime: defaults.runtime.clone(),
            model: None,
            default_branch: info.default_branch.clone(),
            concurrency: 1,
            auto_claim: defaults.auto_claim,
            codex_unattended: false,
            auto_clone: true,
            can_prepare: true,
            remote: info.clone_url.clone(),
            test_commands: defaults.test_commands.clone(),
        });
    result.repository.owner = info.owner.login;
    result.repository.name = info.name;
    result
}

pub fn repository_index(config: &Config, id: Option<u64>, legacy: Option<usize>) -> Result<usize> {
    if let Some(id) = id {
        return config
            .projects
            .iter()
            .position(|project| project.repository.repository_id == id)
            .ok_or_else(|| Error::new("not_found", "Repository is not visible to this Token"));
    }
    if let Some(index) = legacy {
        return config
            .projects
            .get(index)
            .map(|_| index)
            .ok_or_else(|| Error::new("invalid_target", "Repository index out of range"));
    }
    if config.projects.len() == 1 || (config.repositories.is_none() && !config.projects.is_empty())
    {
        return Ok(0);
    }
    Err(Error::new(
        "invalid_target",
        "Select repository_id explicitly from repository_list",
    ))
}

pub struct LocalWorkActions {
    pub config: Config,
}
#[async_trait::async_trait]
impl crate::mcp::WorkActions for LocalWorkActions {
    async fn call(&self, method: &str, args: &Value) -> Result<Value> {
        let (method, params) = match method {
            "work_start" => {
                let repository = work_target(args, "repository_id")?;
                let number = work_target(args, "issue_number")?;
                (
                    "local/work",
                    serde_json::json!({"repository_id":repository,"number":number}),
                )
            }
            "work_status" => ("local/status", serde_json::json!({})),
            _ => {
                return Err(Error::new(
                    "unbound_context",
                    "This operation requires a claimed Work",
                ))
            }
        };
        let response = crate::daemon::request_local(&self.config.state_dir.join("daemon.sock"),
            &serde_json::json!({"jsonrpc":"2.0","id":Uuid::new_v4(),"method":method,"params":params})).await?;
        if let Some(error) = response.get("error") {
            return Err(Error::new(
                error["data"]["code"].as_str().unwrap_or("work_failed"),
                error["message"].as_str().unwrap_or("Local Worker failed"),
            ));
        }
        Ok(response["result"].clone())
    }
}

fn work_target(args: &Value, field: &str) -> Result<u64> {
    args.get(field)
        .or_else(|| args["issue_ref"].get(field))
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or_else(|| Error::new("invalid_request", format!("Positive {field} required")))
}
