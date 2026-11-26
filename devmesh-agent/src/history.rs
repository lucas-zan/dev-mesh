//! Authorized historical records come from Relay, independently of live Work ownership.
use crate::config::{credential, Endpoint};
use devmesh_protocol::{Error, Result};
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

#[async_trait::async_trait]
pub trait HistoryReader: Send + Sync {
    async fn read(&self, tool: &str, args: &Value) -> Result<Value>;
}
struct HttpHistory {
    endpoint: Endpoint,
    client: reqwest::Client,
}
pub fn configured(endpoint: Option<&Endpoint>) -> Option<Arc<dyn HistoryReader>> {
    endpoint.map(|endpoint| {
        Arc::new(HttpHistory {
            endpoint: endpoint.clone(),
            client: reqwest::Client::new(),
        }) as Arc<dyn HistoryReader>
    })
}
#[async_trait::async_trait]
impl HistoryReader for HttpHistory {
    async fn read(&self, tool: &str, args: &Value) -> Result<Value> {
        let path = history_path(tool, args)?;
        let token = credential(&self.endpoint.credential_ref)?;
        let response = self
            .client
            .get(format!(
                "{}/api/v1/{path}",
                self.endpoint.endpoint.trim_end_matches('/')
            ))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|_| Error::new("unavailable", "Relay history unavailable"))?;
        let success = response.status().is_success();
        let body: Value = response
            .json()
            .await
            .map_err(|_| Error::new("invalid_response", "Relay history response is not JSON"))?;
        if !success {
            return Err(Error::new(
                body["error"]["code"].as_str().unwrap_or("relay_error"),
                body["error"]["message"]
                    .as_str()
                    .unwrap_or("Relay history request failed"),
            ));
        }
        Ok(body)
    }
}
pub fn history_path(tool: &str, args: &Value) -> Result<String> {
    let mut path = match tool {
        "session_list" => "sessions".to_owned(),
        "session_get" | "session_events" => {
            let id = args["session_id"]
                .as_str()
                .and_then(|s| Uuid::parse_str(s).ok())
                .ok_or_else(|| Error::new("invalid_request", "session_id UUID required"))?;
            format!(
                "sessions/{id}{}",
                if tool == "session_events" {
                    "/events"
                } else {
                    ""
                }
            )
        }
        _ => return Err(Error::new("unknown_tool", tool)),
    };
    if tool == "session_get" {
        return Ok(path);
    }
    let mut query = reqwest::Url::parse("http://localhost").unwrap();
    {
        let mut pairs = query.query_pairs_mut();
        for key in if tool == "session_list" {
            &["work_id"][..]
        } else {
            &["after_sequence"][..]
        } {
            if let Some(value) = args.get(key) {
                pairs.append_pair(key, &scalar(value)?);
            }
        }
        if tool == "session_list" {
            if let Some(reference) = args.get("issue_ref") {
                for key in ["instance_id", "repository_id", "issue_number"] {
                    pairs.append_pair(key, &scalar(&reference[key])?);
                }
            }
        }
        for key in ["cursor", "limit"] {
            if let Some(value) = args.get(key) {
                pairs.append_pair(key, &scalar(value)?);
            }
        }
    }
    if let Some(q) = query.query().filter(|q| !q.is_empty()) {
        path.push('?');
        path.push_str(q);
    }
    Ok(path)
}
fn scalar(value: &Value) -> Result<String> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Number(number) => Ok(number.to_string()),
        _ => Err(Error::new(
            "invalid_request",
            "History query requires a scalar value",
        )),
    }
}
