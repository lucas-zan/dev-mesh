//! Versioned, owned-file installation. User configuration and data are separate.
use devmesh_protocol::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct BundleFile {
    pub path: String,
    pub sha256: String,
    pub executable: bool,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct BundleManifest {
    pub version: String,
    pub files: Vec<BundleFile>,
}
fn io(error: std::io::Error) -> Error {
    Error::new("installation_error", error.to_string())
}
fn manifest(source: &Path) -> Result<BundleManifest> {
    serde_json::from_slice(&fs::read(source.join("manifest.json")).map_err(io)?)
        .map_err(|e| Error::new("invalid_manifest", e.to_string()))
}
fn safe_path(path: &str) -> Result<PathBuf> {
    let path = PathBuf::from(path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(Error::new(
            "invalid_manifest",
            "Bundle paths must be relative",
        ));
    }
    Ok(path)
}
fn verify(source: &Path, manifest: &BundleManifest) -> Result<()> {
    safe_path(&manifest.version)?;
    for file in &manifest.files {
        let path = source.join(safe_path(&file.path)?);
        let hash = format!("{:x}", Sha256::digest(fs::read(path).map_err(io)?));
        if hash != file.sha256 {
            return Err(Error::new("checksum_mismatch", &file.path));
        }
    }
    Ok(())
}
pub fn install_bundle(prefix: &Path, source: &Path) -> Result<String> {
    let manifest = manifest(source)?;
    verify(source, &manifest)?;
    fs::create_dir_all(prefix.join("releases")).map_err(io)?;
    let destination = prefix.join("releases").join(&manifest.version);
    if destination.exists() {
        verify(&destination, &manifest)?;
        switch(prefix, &manifest.version)?;
        return Ok(manifest.version);
    }
    let staging =
        prefix
            .join("releases")
            .join(format!(".{}-{}", manifest.version, uuid::Uuid::new_v4()));
    fs::create_dir(&staging).map_err(io)?;
    for file in &manifest.files {
        let path = staging.join(&file.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io)?;
        }
        fs::copy(source.join(&file.path), &path).map_err(io)?;
        #[cfg(unix)]
        if file.executable {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(io)?;
        }
    }
    fs::copy(source.join("manifest.json"), staging.join("manifest.json")).map_err(io)?;
    fs::rename(staging, &destination).map_err(io)?;
    switch(prefix, &manifest.version)?;
    Ok(manifest.version)
}
#[cfg(unix)]
fn switch(prefix: &Path, version: &str) -> Result<()> {
    use std::os::unix::fs::symlink;
    let temporary = prefix.join(format!(".current-{}", uuid::Uuid::new_v4()));
    symlink(Path::new("releases").join(version), &temporary).map_err(io)?;
    fs::rename(temporary, prefix.join("current")).map_err(io)?;
    fs::create_dir_all(prefix.join("bin")).map_err(io)?;
    let bin = prefix.join("bin/devmesh");
    if !bin.exists() && fs::symlink_metadata(&bin).is_err() {
        symlink("../current/bin/devmesh", bin).map_err(io)?;
    }
    Ok(())
}
#[cfg(not(unix))]
fn switch(_: &Path, _: &str) -> Result<()> {
    Err(Error::new(
        "unsupported_platform",
        "V1 supports macOS and Linux",
    ))
}

pub fn rollback(prefix: &Path, version: &str) -> Result<()> {
    safe_path(version)?;
    let release = prefix.join("releases").join(version);
    verify(&release, &manifest(&release)?)?;
    switch(prefix, version)
}
pub fn uninstall(prefix: &Path) -> Result<()> {
    if let Ok(versions) = fs::read_dir(prefix.join("releases")) {
        for entry in versions {
            let release = entry.map_err(io)?.path();
            if !release.join("manifest.json").is_file() {
                continue;
            }
            let manifest = manifest(&release)?;
            for file in manifest.files {
                let path = release.join(safe_path(&file.path)?);
                if path.is_file() {
                    let hash = format!("{:x}", Sha256::digest(fs::read(&path).map_err(io)?));
                    if hash == file.sha256 {
                        fs::remove_file(path).map_err(io)?;
                    }
                }
            }
            fs::remove_file(release.join("manifest.json")).map_err(io)?;
        }
    }
    for (path, target) in [
        ("current", None),
        ("bin/devmesh", Some(PathBuf::from("../current/bin/devmesh"))),
    ] {
        let path = prefix.join(path);
        if let Ok(actual) = fs::read_link(&path) {
            if target.is_none_or(|t| t == actual) {
                fs::remove_file(path).map_err(io)?;
            }
        }
    }
    Ok(())
}

pub fn install_runtime(prefix: &Path, runtime: &str) -> Result<PathBuf> {
    let (package, executable) = match runtime {
        "codex" => ("@openai/codex@0.172.0", "codex"),
        "pi" => ("@earendil-works/pi-coding-agent@1.6.0", "pi"),
        "pi-durable" => ("@earendil-works/pi-durable@1.6.0", "node"),
        _ => return Err(Error::new("invalid_runtime", runtime)),
    };
    let directory = if runtime == "pi-durable" {
        prefix.join("current/share/devmesh")
    } else {
        prefix.join("toolchains").join(runtime)
    };
    fs::create_dir_all(&directory).map_err(io)?;
    let mut command = Command::new("npm");
    command
        .args([
            "install",
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
            "--save-exact",
            "--prefix",
        ])
        .arg(&directory)
        .arg(package);
    if runtime == "pi-durable" {
        command.args(["@earendil-works/pi-ai@1.6.0", "@earendil-works/chord@1.6.0"]);
    }
    let status = command.status().map_err(io)?;
    if !status.success() {
        return Err(Error::new(
            "runtime_install_failed",
            format!("npm failed installing {runtime}"),
        ));
    }
    Ok(if runtime == "pi-durable" {
        PathBuf::from(executable)
    } else {
        directory.join("node_modules/.bin").join(executable)
    })
}

pub fn restore_current(prefix: &Path, previous: Option<&str>) -> Result<()> {
    if let Some(version) = previous {
        return rollback(prefix, version);
    }
    for (relative, target) in [
        ("current", None),
        ("bin/devmesh", Some(PathBuf::from("../current/bin/devmesh"))),
    ] {
        let path = prefix.join(relative);
        if let Ok(actual) = fs::read_link(&path) {
            if target.is_none_or(|expected| expected == actual) {
                fs::remove_file(path).map_err(io)?;
            }
        }
    }
    Ok(())
}
pub fn install_official_mcp(prefix: &Path) -> Result<PathBuf> {
    let bin = prefix.join("toolchains/gitea/bin");
    fs::create_dir_all(&bin).map_err(io)?;
    let status = Command::new("go")
        .env("GOBIN", &bin)
        .args(["install", "gitea.com/gitea/gitea-mcp@v1.13.0"])
        .status()
        .map_err(io)?;
    if !status.success() {
        return Err(Error::new(
            "runtime_install_failed",
            "Official Gitea MCP installation failed",
        ));
    }
    Ok(bin.join("gitea-mcp"))
}
