use crate::gitea::TaskService;
use base64::Engine;
use devmesh_protocol::{Error, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
impl TaskService {
    pub async fn repository_path(&self, reference: &Value) -> Result<String> {
        if reference["instance_id"] != self.repo.instance_id.to_string() {
            return Err(Error::new(
                "unsupported_dependency",
                "Gitea instance is not configured",
            ));
        }
        let repository = self
            .client
            .get(&format!("repositories/{}", reference["repository_id"]))
            .await?;
        let owner = repository["owner"]["login"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_response", "Repository owner missing"))?;
        let name = repository["name"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_response", "Repository name missing"))?;
        Ok(format!("repos/{owner}/{name}"))
    }
    pub async fn verify_artifact(&self, artifact: &Value) -> Result<()> {
        crate::delivery::validate_artifact(artifact)?;
        let path = self.repository_path(&artifact["repository"]).await?;
        let bytes = match artifact["kind"].as_str().unwrap_or("") {
            "git_file" => self.git_artifact(&path, artifact).await?,
            "release_asset" => self.release_artifact(&path, artifact).await?,
            _ => {
                return Err(Error::new(
                    "unsupported_capability",
                    "Unknown artifact type",
                ))
            }
        };
        if artifact["sha256"] != format!("{:x}", Sha256::digest(&bytes)) {
            return Err(Error::new(
                "stale_evidence",
                "Artifact content hash differs from evidence",
            ));
        }
        Ok(())
    }
    async fn git_artifact(&self, repository: &str, artifact: &Value) -> Result<Vec<u8>> {
        let path = artifact["path"].as_str().unwrap();
        if path.split('/').any(|p| matches!(p, ".." | "." | "")) {
            return Err(Error::new(
                "invalid_target",
                "Artifact path must identify a repository file",
            ));
        }
        let mut encoded = reqwest::Url::parse("http://artifact/").unwrap();
        encoded.path_segments_mut().unwrap().extend(path.split('/'));
        let file = self
            .client
            .get(&format!(
                "{repository}/contents/{}?ref={}",
                encoded.path().trim_start_matches('/'),
                artifact["commit_sha"].as_str().unwrap()
            ))
            .await?;
        if file["type"] != "file" || file["encoding"] != "base64" {
            return Err(Error::new(
                "invalid_response",
                "Gitea artifact must be a base64 file",
            ));
        }
        let encoded = file["content"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_response", "Artifact content missing"))?
            .split_whitespace()
            .collect::<String>();
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| Error::new("invalid_response", "Artifact encoding invalid"))
    }
    async fn release_artifact(&self, repository: &str, artifact: &Value) -> Result<Vec<u8>> {
        let mut page = 1;
        loop {
            let releases = self
                .client
                .get(&format!("{repository}/releases?limit=50&page={page}"))
                .await?;
            let releases = releases
                .as_array()
                .ok_or_else(|| Error::new("invalid_response", "Release list missing"))?;
            if releases.is_empty() {
                return Err(Error::new(
                    "not_found",
                    "Release asset does not belong to repository",
                ));
            }
            for release in releases.iter().filter(|r| r["draft"] != true) {
                if let Some(asset) = release["assets"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|a| a["id"] == artifact["asset_id"])
                {
                    let url = asset["browser_download_url"].as_str().ok_or_else(|| {
                        Error::new("invalid_response", "Asset download URL missing")
                    })?;
                    return self.client.bytes(url).await;
                }
            }
            page += 1;
        }
    }
}
