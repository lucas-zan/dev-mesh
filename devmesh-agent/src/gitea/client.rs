use devmesh_protocol::{Error, Result};
use reqwest::{Client, Method, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Command;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    pub instance_id: Uuid,
    pub repository_id: u64,
    pub owner: String,
    pub name: String,
}
impl Repo {
    pub fn path(&self) -> String {
        format!("repos/{}/{}", self.owner, self.name)
    }
    pub fn issue_ref(&self, number: u64) -> Value {
        json!({"instance_id":self.instance_id,"repository_id":self.repository_id,"issue_number":number})
    }
}

#[async_trait::async_trait]
pub trait GiteaTransport: Send + Sync {
    fn authenticate_git(&self, _command: &mut Command, _remote: &str) {}
    async fn bytes(&self, _url: &str) -> Result<Vec<u8>> {
        Err(Error::new(
            "unsupported_capability",
            "Binary artifact download unavailable",
        ))
    }
    async fn request(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value>;
}

#[derive(Clone)]
pub struct GiteaClient {
    transport: std::sync::Arc<dyn GiteaTransport>,
}
struct HttpTransport {
    client: Client,
    base: Url,
    token: String,
}

#[async_trait::async_trait]
impl GiteaTransport for HttpTransport {
    fn authenticate_git(&self, command: &mut Command, remote: &str) {
        let Ok(url) = Url::parse(remote) else {
            return;
        };
        if url.origin() != self.base.origin() {
            return;
        }
        command
            .env("GIT_CONFIG_COUNT", "2")
            .env("GIT_CONFIG_KEY_0", format!("http.{url}.extraHeader"))
            .env(
                "GIT_CONFIG_VALUE_0",
                format!("Authorization: Bearer {}", self.token),
            )
            .env("GIT_CONFIG_KEY_1", format!("http.{url}.followRedirects"))
            .env("GIT_CONFIG_VALUE_1", "false")
            .env("GIT_TERMINAL_PROMPT", "0");
    }

    async fn bytes(&self, url: &str) -> Result<Vec<u8>> {
        let url = Url::parse(url).map_err(|_| Error::new("invalid_target", "Invalid asset URL"))?;
        if url.origin() != self.base.origin() {
            return Err(Error::new(
                "invalid_target",
                "Asset download must originate from configured Gitea",
            ));
        }
        let response = self
            .client
            .get(url)
            .header("Authorization", format!("token {}", self.token))
            .send()
            .await
            .map_err(|_| Error::new("unavailable", "Asset download failed"))?;
        if !response.status().is_success() {
            return Err(Error::new(
                "gitea_error",
                format!("Asset HTTP {}", response.status()),
            ));
        }
        Ok(response
            .bytes()
            .await
            .map_err(|_| Error::new("unavailable", "Asset download interrupted"))?
            .to_vec())
    }
    async fn request(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value> {
        let url = self
            .base
            .join(path)
            .map_err(|e| Error::new("invalid_request", e.to_string()))?;
        let mut request = self
            .client
            .request(method.clone(), url)
            .header("Authorization", format!("token {}", self.token));
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.map_err(|e| {
            Error::new(
                if e.is_timeout()
                    || (!matches!(method, Method::GET | Method::HEAD) && !e.is_connect())
                {
                    "unknown_outcome"
                } else {
                    "unavailable"
                },
                "Gitea request failed",
            )
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::new(
                response_code(&method, status.as_u16()),
                format!("Gitea HTTP {status}"),
            ));
        }
        if status == reqwest::StatusCode::NO_CONTENT {
            return Ok(Value::Null);
        }
        response.json().await.map_err(|_| {
            Error::new(
                if matches!(method, Method::GET | Method::HEAD) {
                    "invalid_response"
                } else {
                    "unknown_outcome"
                },
                "Expected Gitea JSON",
            )
        })
    }
}

impl GiteaClient {
    pub fn authenticate_git(&self, command: &mut Command, remote: &str) {
        self.transport.authenticate_git(command, remote);
    }

    pub fn new(endpoint: &str, token: &str) -> Result<Self> {
        let base = Url::parse(&format!("{}/api/v1/", endpoint.trim_end_matches('/')))
            .map_err(|e| Error::new("invalid_endpoint", e.to_string()))?;
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| Error::new("transport_error", e.to_string()))?;
        Ok(Self::with_transport(std::sync::Arc::new(HttpTransport {
            client,
            base,
            token: token.into(),
        })))
    }
    pub fn with_transport(transport: std::sync::Arc<dyn GiteaTransport>) -> Self {
        Self { transport }
    }
    pub async fn request(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value> {
        self.transport.request(method, path, body).await
    }
    pub async fn get(&self, path: &str) -> Result<Value> {
        self.request(Method::GET, path, None).await
    }
    pub async fn bytes(&self, url: &str) -> Result<Vec<u8>> {
        self.transport.bytes(url).await
    }
    pub async fn user(&self) -> Result<Value> {
        self.get("user").await
    }
    pub async fn issue(&self, repo: &Repo, number: u64) -> Result<Value> {
        self.get(&format!("{}/issues/{number}", repo.path())).await
    }
    pub async fn cas(&self, repo: &Repo, number: u64, version: u64, body: &str) -> Result<Value> {
        self.request(
            Method::PATCH,
            &format!("{}/issues/{number}", repo.path()),
            Some(&json!({"body":body,"content_version":version})),
        )
        .await
    }
    pub async fn list(&self, repo: &Repo, state: &str) -> Result<Vec<Value>> {
        let mut result = Vec::new();
        let mut page = 1;
        loop {
            let value = self
                .get(&format!(
                    "{}/issues?state={state}&type=issues&limit=50&page={page}",
                    repo.path()
                ))
                .await?;
            let items = value
                .as_array()
                .ok_or_else(|| Error::new("invalid_response", "Expected Issue list"))?;
            result.extend(items.iter().cloned());
            if items.is_empty() {
                return Ok(result);
            }
            page += 1;
        }
    }
}

pub fn response_code(method: &Method, status: u16) -> &'static str {
    match status {
        409 => "claim_conflict",
        401 => "unauthenticated",
        403 => "permission_denied",
        404 => "not_found",
        500..=599 if !matches!(*method, Method::GET | Method::HEAD) => "unknown_outcome",
        _ => "gitea_error",
    }
}
