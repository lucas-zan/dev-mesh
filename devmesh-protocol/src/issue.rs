use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

const START: &str = "<!-- devmesh:issue";
const END: &str = "-->";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct IssueRef {
    pub instance_id: Uuid,
    pub repository_id: u64,
    pub issue_number: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Backend,
    Frontend,
    Product,
    Qa,
}

pub fn metadata(body: &str) -> Result<Value> {
    let (start, end) = block_range(body)?;
    serde_json::from_str(body[start + START.len()..end - END.len()].trim())
        .map_err(|e| Error::new("invalid_metadata", e.to_string()))
}

fn block_range(body: &str) -> Result<(usize, usize)> {
    let mut positions = body.match_indices(START);
    let start = positions
        .next()
        .ok_or_else(|| Error::new("missing_metadata", "No DevMesh issue block"))?
        .0;
    if positions.next().is_some() {
        return Err(Error::new("invalid_metadata", "Duplicate issue blocks"));
    }
    let end = body[start..]
        .find(END)
        .ok_or_else(|| Error::new("invalid_metadata", "Unterminated issue block"))?;
    Ok((start, start + end + END.len()))
}

pub fn replace_metadata(body: &str, value: &Value) -> Result<String> {
    let block = format!(
        "{START}\n{}\n{END}",
        serde_json::to_string_pretty(value).unwrap()
    );
    if !body.contains(START) {
        return Ok(format!("{body}\n{block}"));
    }
    let (start, end) = block_range(body)?;
    Ok(format!("{}{block}{}", &body[..start], &body[end..]))
}
