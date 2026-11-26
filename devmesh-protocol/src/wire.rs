use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const VERSION: &str = "1.1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub event_id: Uuid,
    pub work_id: Uuid,
    pub session_id: Uuid,
    pub sequence: String,
    pub occurred_at: String,
    pub run_epoch: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub version: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub frame_id: Uuid,
    pub sent_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<Uuid>,
    pub payload: Value,
}
impl Frame {
    pub fn new(kind: &str, payload: Value) -> Self {
        Self {
            version: VERSION.into(),
            kind: kind.into(),
            frame_id: Uuid::new_v4(),
            sent_at: now(),
            reply_to: None,
            payload,
        }
    }
    pub fn check_version(&self) -> Result<()> {
        if self.version == VERSION {
            Ok(())
        } else {
            Err(Error::new("incompatible_version", &self.version))
        }
    }
}

pub fn sequence(value: &str) -> Result<u64> {
    if value.len() > 1 && value.starts_with('0') {
        return Err(Error::new("invalid_sequence", value));
    }
    value
        .parse()
        .map_err(|_| Error::new("invalid_sequence", value))
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn unix_timestamp(seconds: u64) -> String {
    chrono::DateTime::from_timestamp(seconds as i64, 0)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
