use crate::{
    gitea::{GiteaClient, Repo, TaskService},
    runtime::{Launch, RuntimeKind},
    worker::WorkerProject,
};
use devmesh_protocol::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub schema_version: u32,
    pub device_id: Uuid,
    pub roles: Vec<String>,
    pub gitea: Endpoint,
    pub relay: Option<Endpoint>,
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub repositories: Option<crate::discovery::RepositoryDefaults>,
    pub state_dir: PathBuf,
    #[serde(default)]
    pub worker: WorkerSettings,
    #[serde(default)]
    pub remote_control: RemoteControl,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub endpoint: String,
    pub credential_ref: String,
    #[serde(default)]
    pub mcp_program: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Project {
    pub repository: Repo,
    pub local_path: PathBuf,
    pub roles: Vec<String>,
    pub runtime: String,
    pub default_branch: String,
    pub concurrency: usize,
    pub auto_claim: bool,
    #[serde(default)]
    pub codex_unattended: bool,
    #[serde(default)]
    pub auto_clone: bool,
    pub remote: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub test_commands: Vec<String>,
    #[serde(default)]
    pub can_prepare: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct WorkerSettings {
    pub enabled: bool,
    pub max_concurrency: usize,
    pub poll_interval_seconds: u64,
    pub idle_grace_seconds: u64,
    pub approval_timeout_seconds: u64,
    pub input_timeout_seconds: u64,
}
impl Default for WorkerSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            max_concurrency: 2,
            poll_interval_seconds: 30,
            idle_grace_seconds: 120,
            approval_timeout_seconds: 900,
            input_timeout_seconds: 1800,
        }
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct RemoteControl {
    pub enabled: bool,
    pub allow_interrupt: bool,
}

impl Config {
    pub fn read(path: &Path) -> Result<Self> {
        let config: Self = serde_json::from_slice(
            &std::fs::read(path).map_err(|e| Error::new("config_error", e.to_string()))?,
        )
        .map_err(|e| Error::new("config_error", e.to_string()))?;
        if config.schema_version != 1 {
            return Err(Error::new(
                "incompatible_version",
                "Unsupported config schema",
            ));
        }
        if let Some(defaults) = &config.repositories {
            if !matches!(defaults.runtime.as_str(), "codex" | "pi" | "pi-durable") {
                return Err(Error::new("config_error", "Invalid default Runtime"));
            }
        }
        for p in &config.projects {
            if !matches!(p.runtime.as_str(), "codex" | "pi" | "pi-durable") || p.concurrency == 0 {
                return Err(Error::new("config_error", "Invalid Runtime or concurrency"));
            }
            if p.codex_unattended && p.runtime != "codex" {
                return Err(Error::new(
                    "config_error",
                    "codex_unattended requires the Codex Runtime",
                ));
            }
        }
        Ok(config)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::new("config_error", e.to_string()))?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(self).unwrap())
            .map_err(|e| Error::new("config_error", e.to_string()))
    }
    pub fn service(&self, index: usize) -> Result<TaskService> {
        let project = self
            .projects
            .get(index)
            .ok_or_else(|| Error::new("not_configured", "Project index out of range"))?;
        Ok(TaskService::new(self.client()?, project.repository.clone()))
    }
    pub fn client(&self) -> Result<GiteaClient> {
        let token = credential(&self.gitea.credential_ref)?;
        let client = GiteaClient::new(&self.gitea.endpoint, &token)?;
        let client = if let Some(program) = &self.gitea.mcp_program {
            client.with_official(program, &self.gitea.endpoint, &token)
        } else {
            client
        };
        Ok(client)
    }
    pub async fn resolved(&self) -> Result<Self> {
        if self.repositories.is_none() {
            return Ok(self.clone());
        }
        crate::discovery::resolve(self, &self.client()?).await
    }
}
impl Project {
    pub fn worker_project(&self, state: &Path) -> WorkerProject {
        WorkerProject {
            key: format!(
                "{}:{}",
                self.repository.instance_id, self.repository.repository_id
            ),
            path: self.local_path.clone(),
            worktrees: state.join("worktrees"),
            remote: self.remote.clone(),
            branch: self.default_branch.clone(),
            concurrency: self.concurrency,
            auto_clone: self.auto_clone,
        }
    }
    pub fn launch(&self, prefix: &Path, cwd: &Path, resume: Option<String>) -> Result<Launch> {
        let (kind, program, args) = match self.runtime.as_str() {
            "codex" => (
                RuntimeKind::Codex,
                prefix
                    .join("toolchains/codex/node_modules/.bin/codex")
                    .to_string_lossy()
                    .into_owned(),
                vec!["app-server".into()],
            ),
            "pi" => {
                let extension =
                    prefix.join("current/share/devmesh/packages/pi-extension/src/index.ts");
                let mut args = vec![
                    "--mode".into(),
                    "rpc".into(),
                    "--extension".into(),
                    extension.to_string_lossy().into_owned(),
                ];
                if let Some(native) = &resume {
                    args.extend(["--session".into(), native.clone()]);
                }
                if let Some(model) = &self.model {
                    args.extend(["--model".into(), model.clone()]);
                }
                (
                    RuntimeKind::Pi,
                    prefix
                        .join("toolchains/pi/node_modules/.bin/pi")
                        .to_string_lossy()
                        .into_owned(),
                    args,
                )
            }
            "pi-durable" => (
                RuntimeKind::PiDurable,
                "node".into(),
                vec![prefix
                    .join("current/share/devmesh/packages/pi-durable-host/src/cli.ts")
                    .to_string_lossy()
                    .into_owned()],
            ),
            _ => return Err(Error::new("invalid_runtime", &self.runtime)),
        };
        Ok(Launch {
            kind,
            codex_unattended: self.codex_unattended,
            program,
            args,
            env: Vec::new(),
            cwd: cwd.into(),
            model: self.model.clone(),
            resume,
        })
    }
}
pub fn credential(reference: &str) -> Result<String> {
    let (kind, key) = reference.split_once(':').ok_or_else(|| {
        Error::new(
            "credential_error",
            "Use env:, keychain: or secret-service: credential reference",
        )
    })?;
    if kind == "env" {
        return std::env::var(key).map_err(|_| {
            Error::new(
                "credential_missing",
                format!("Environment reference {key} is not set"),
            )
        });
    }
    let output = match kind {
        "keychain" => Command::new("security")
            .args(["find-generic-password", "-s", key, "-w"])
            .output(),
        "secret-service" => Command::new("secret-tool")
            .args(["lookup", "service", key])
            .output(),
        _ => {
            return Err(Error::new(
                "credential_error",
                "Unsupported credential provider",
            ))
        }
    }
    .map_err(|_| Error::new("credential_missing", "Credential provider is unavailable"))?;
    if !output.status.success() {
        return Err(Error::new(
            "credential_missing",
            "Credential reference not found",
        ));
    }
    String::from_utf8(output.stdout)
        .map(|s| s.trim().into())
        .map_err(|_| Error::new("credential_error", "Credential must be UTF-8"))
}
