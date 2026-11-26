//! Official MCP handles ordinary reads/writes. CAS and exact-version reads stay REST.
use super::{GiteaClient, GiteaTransport};
use crate::runtime::process::Peer;
use devmesh_protocol::{Error, Result};
use reqwest::Method;
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};
use tokio::sync::OnceCell;

struct OfficialTransport {
    rest: GiteaClient,
    program: String,
    endpoint: String,
    token: String,
    peer: OnceCell<Peer>,
}
impl GiteaClient {
    pub fn with_official(self, program: &str, endpoint: &str, token: &str) -> Self {
        Self::with_transport(Arc::new(OfficialTransport {
            rest: self,
            program: program.into(),
            endpoint: endpoint.into(),
            token: token.into(),
            peer: OnceCell::new(),
        }))
    }
}
impl OfficialTransport {
    async fn peer(&self) -> Result<&Peer> {
        self.peer.get_or_try_init(||async{
   let args=vec!["-t".into(),"stdio".into(),"--host".into(),self.endpoint.clone(),"--tools".into(),"get_me,issue_read,list_issues,issue_write,list_pull_requests,pull_request_read,pull_request_write".into()];
   let (peer,_)=Peer::spawn(&self.program,&args,Path::new("."),&[("GITEA_ACCESS_TOKEN".into(),self.token.clone())])?;
   peer.call(json!({"jsonrpc":"2.0","method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"devmesh","version":env!("CARGO_PKG_VERSION")}}})).await?;
   peer.send(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).await?;
   Ok(peer)
  }).await
    }
}
#[async_trait::async_trait]
impl GiteaTransport for OfficialTransport {
    fn authenticate_git(&self, command: &mut std::process::Command, remote: &str) {
        self.rest.authenticate_git(command, remote);
    }

    async fn bytes(&self, url: &str) -> Result<Vec<u8>> {
        self.rest.bytes(url).await
    }
    async fn request(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value> {
        let Some((name, args)) = official_operation(&method, path, body) else {
            return self.rest.request(method, path, body).await;
        };
        let value=decode_mcp(self.peer().await?.call(json!({"jsonrpc":"2.0","method":"tools/call","params":{"name":name,"arguments":args}})).await?)?;
        // Official slim Issue responses can omit the CAS token. Exact CAS reads are a narrow REST exception.
        if name == "issue_read" && args["method"] == "get" && value["content_version"].is_null() {
            return self.rest.get(path).await;
        }
        if name == "issue_write" && args["method"] == "create" && value["content_version"].is_null()
        {
            let number = value["number"].as_u64().ok_or_else(|| {
                Error::new(
                    "invalid_response",
                    "Official MCP create omitted Issue number",
                )
            })?;
            return self.rest.get(&format!("{path}/{number}")).await;
        }
        Ok(value)
    }
}
pub fn decode_mcp(value: Value) -> Result<Value> {
    if value["isError"] == true {
        return Err(Error::new(
            "gitea_mcp_error",
            "Official Gitea MCP rejected operation",
        ));
    }
    if let Some(structured) = value.get("structuredContent") {
        return Ok(structured.clone());
    }
    let text = value["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    serde_json::from_str(&text)
        .map_err(|_| Error::new("invalid_response", "Official MCP result is not JSON"))
}
pub fn official_operation(
    method: &Method,
    path: &str,
    body: Option<&Value>,
) -> Option<(&'static str, Value)> {
    if *method == Method::GET && path == "user" {
        return Some(("get_me", json!({})));
    }
    let (base, query) = path.split_once('?').unwrap_or((path, ""));
    let parts = base.split('/').collect::<Vec<_>>();
    if parts.len() < 4 || parts[0] != "repos" {
        return None;
    }
    let mut args = json!({"owner":parts[1],"repo":parts[2]});
    if parts[3] == "issues" && parts.len() == 4 {
        if *method == Method::GET {
            args["type"] = json!("issues");
            for (key, value) in reqwest::Url::parse(&format!("http://query/?{query}"))
                .ok()?
                .query_pairs()
            {
                match key.as_ref() {
                    "state" => args["state"] = json!(value),
                    "page" => args["page"] = json!(value.parse::<u64>().ok()?),
                    "limit" => args["per_page"] = json!(value.parse::<u64>().ok()?),
                    _ => {}
                }
            }
            return Some(("list_issues", args));
        }
        if *method == Method::POST {
            args["method"] = json!("create");
            args["title"] = body?["title"].clone();
            args["body"] = body?["body"].clone();
            return Some(("issue_write", args));
        }
    }
    if parts[3] == "pulls" && parts.len() == 4 {
        if *method == Method::POST {
            args["method"] = json!("create");
            for field in ["title", "body", "head", "base"] {
                args[field] = body?[field].clone();
            }
            return Some(("pull_request_write", args));
        }
        if *method == Method::GET {
            for (key, value) in reqwest::Url::parse(&format!("http://query/?{query}"))
                .ok()?
                .query_pairs()
            {
                match key.as_ref() {
                    "state" => args["state"] = json!(value),
                    "page" => args["page"] = json!(value.parse::<u64>().ok()?),
                    "limit" => args["per_page"] = json!(value.parse::<u64>().ok()?),
                    _ => {}
                }
            }
            return Some(("list_pull_requests", args));
        }
    }
    let number = parts.get(4)?.parse::<u64>().ok()?;
    if parts[3] == "pulls" && parts.len() == 5 && *method == Method::GET {
        args["method"] = json!("get");
        args["pull_number"] = json!(number);
        return Some(("pull_request_read", args));
    }
    if parts[3] == "issues" {
        args["issue_number"] = json!(number);
        if parts.len() == 5 && *method == Method::GET {
            args["method"] = json!("get");
            return Some(("issue_read", args));
        }
        if parts.get(5) == Some(&"comments") && parts.len() == 6 && *method == Method::POST {
            args["method"] = json!("add_comment");
            args["body"] = body?["body"].clone();
            return Some(("issue_write", args));
        }
    }
    None
}
