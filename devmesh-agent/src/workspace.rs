use crate::gitea::GiteaClient;
use devmesh_protocol::{Error, Result};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use uuid::Uuid;

#[derive(Debug, serde::Serialize)]
pub struct Workspace {
    pub path: PathBuf,
    pub branch: String,
    pub base_sha: String,
}
pub fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    run_git(Command::new("git").args(args).current_dir(cwd))
}

pub(crate) fn git_remote(
    cwd: &Path,
    args: &[&str],
    remote: &str,
    client: Option<&GiteaClient>,
) -> Result<String> {
    let mut command = Command::new("git");
    command.args(args).current_dir(cwd);
    if let Some(client) = client {
        client.authenticate_git(&mut command, remote);
    }
    run_git(&mut command)
}

fn run_git(command: &mut Command) -> Result<String> {
    let output = command
        .output()
        .map_err(|e| Error::new("git_error", e.to_string()))?;
    if !output.status.success() {
        return Err(Error::new(
            "git_error",
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}
impl Workspace {
    pub fn create_local(
        repo: &Path,
        base: &str,
        work_branch: &str,
        claim: Uuid,
        root: &Path,
    ) -> Result<Self> {
        let base_sha = git(
            repo,
            &["rev-parse", "--verify", &format!("{base}^{{commit}}")],
        )?;
        std::fs::create_dir_all(root).map_err(|e| Error::new("workspace_error", e.to_string()))?;
        let root = std::fs::canonicalize(root)
            .map_err(|e| Error::new("workspace_error", e.to_string()))?;
        let path = root.join(claim.to_string());
        git(
            repo,
            &[
                "worktree",
                "add",
                "-b",
                work_branch,
                path.to_str()
                    .ok_or_else(|| Error::new("workspace_error", "Non UTF-8 path"))?,
                &base_sha,
            ],
        )?;
        Ok(Self {
            path,
            branch: work_branch.to_owned(),
            base_sha,
        })
    }
    pub fn create(
        repo: &Path,
        remote: &str,
        base: &str,
        work_branch: &str,
        claim: Uuid,
        root: &Path,
    ) -> Result<Self> {
        Self::create_with_client(repo, remote, base, work_branch, claim, root, None)
    }
    pub(crate) fn create_with_client(
        repo: &Path,
        remote: &str,
        base: &str,
        work_branch: &str,
        claim: Uuid,
        root: &Path,
        client: Option<&GiteaClient>,
    ) -> Result<Self> {
        let actual = git(repo, &["remote", "get-url", "origin"])?;
        if normalize_remote(&actual) != normalize_remote(remote) {
            return Err(Error::new(
                "repository_mismatch",
                "Git origin does not match configured Gitea repository",
            ));
        }
        git_remote(repo, &["fetch", "origin", base], &actual, client)?;
        Self::create_local(repo, "FETCH_HEAD", work_branch, claim, root)
    }
}
pub fn normalize_remote(remote: &str) -> String {
    let value = remote.trim().trim_end_matches('/').trim_end_matches(".git");
    if let Some(rest) = value.strip_prefix("git@") {
        return rest.replacen(':', "/", 1).to_lowercase();
    }
    value
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("ssh://git@")
        .to_lowercase()
}

pub fn ensure_repository(repo: &Path, remote: &str, auto_clone: bool) -> Result<()> {
    ensure_repository_with_client(repo, remote, auto_clone, None)
}

pub(crate) fn ensure_repository_with_client(
    repo: &Path,
    remote: &str,
    auto_clone: bool,
    client: Option<&GiteaClient>,
) -> Result<()> {
    if repo.exists() {
        return Ok(());
    }
    if !auto_clone {
        return Err(Error::new(
            "workspace_missing",
            "Local repository missing; enable auto_clone or clone it first",
        ));
    }
    let parent = repo
        .parent()
        .ok_or_else(|| Error::new("workspace_error", "Repository parent required"))?;
    std::fs::create_dir_all(parent).map_err(|e| Error::new("workspace_error", e.to_string()))?;
    let path = repo
        .to_str()
        .ok_or_else(|| Error::new("workspace_error", "Non UTF-8 repository path"))?;
    git_remote(parent, &["clone", "--", remote, path], remote, client)?;
    Ok(())
}
