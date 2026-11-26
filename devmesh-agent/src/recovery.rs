//! Recovery checks the recorded process start time; it never signals a reused PID.
use devmesh_protocol::{Error, Result};
use serde_json::{json, Value};
use std::process::Command;
fn start_time(pid: u32) -> Result<Option<String>> {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "lstart="])
        .output()
        .map_err(|e| Error::new("recovery_required", e.to_string()))?;
    if !output.status.success() {
        if !output.stderr.is_empty() {
            return Err(Error::new(
                "recovery_required",
                "Cannot inspect recorded Runtime process",
            ));
        }
        return Ok(None);
    }
    Ok(Some(
        String::from_utf8_lossy(&output.stdout).trim().to_owned(),
    ))
}
pub fn process_identity(pid: u32) -> Result<Value> {
    let start = start_time(pid)?.ok_or_else(|| {
        Error::new(
            "runtime_unavailable",
            "Runtime exited before process identity was recorded",
        )
    })?;
    Ok(json!({"pid":pid,"group_id":pid,"start_time":start}))
}
pub fn original_exited(identity: &Value) -> Result<bool> {
    let pid = identity["pid"]
        .as_u64()
        .ok_or_else(|| Error::new("recovery_required", "Recorded Runtime PID is missing"))?
        as u32;
    let observed = start_time(pid)?;
    if !identity_exited(identity, observed.as_deref())? {
        return Ok(false);
    }
    let group = identity["group_id"].as_u64().ok_or_else(|| {
        Error::new(
            "recovery_required",
            "Owned Runtime process group is missing",
        )
    })?;
    let output = Command::new("ps")
        .args(["-axo", "pgid="])
        .output()
        .map_err(|_| Error::new("recovery_required", "Cannot inspect owned process group"))?;
    if !output.status.success() {
        return Err(Error::new(
            "recovery_required",
            "Cannot confirm Runtime children exited",
        ));
    }
    Ok(!String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line.trim().parse::<u64>() == Ok(group)))
}
pub fn identity_exited(identity: &Value, observed: Option<&str>) -> Result<bool> {
    let expected = identity["start_time"].as_str().ok_or_else(|| {
        Error::new(
            "recovery_required",
            "Recorded process start time is missing",
        )
    })?;
    Ok(observed.is_none_or(|current| current != expected))
}

pub fn lock_daemon(directory: &std::path::Path) -> Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join("daemon.lock"))
        .map_err(|e| Error::new("daemon_error", e.to_string()))?;
    file.try_lock().map_err(|_| {
        Error::new(
            "already_running",
            "Another daemon owns this state directory",
        )
    })?;
    Ok(file)
}
pub fn mark_interrupted(store: &crate::storage::Store) -> Result<()> {
    for pending in store.uncertain_commands()? {
        let mut receipt = pending["receipt"].clone();
        let id = pending["request"]["command_id"].as_str().unwrap();
        let sequence = devmesh_protocol::sequence(receipt["receipt_seq"].as_str().unwrap())? + 1;
        receipt["receipt_seq"] = json!(sequence.to_string());
        receipt["state"] = json!("unknown");
        receipt["reason_code"] = json!("supervisor_restarted_during_native_admission");
        receipt["at"] = json!(devmesh_protocol::now());
        store.command_receipt(id, "unknown", &receipt)?;
        store.queue_frame(&devmesh_protocol::Frame::new("command.receipt", receipt))?;
    }
    for record in store.active()? {
        let id = record["work_id"].as_str().unwrap().parse().unwrap();
        if record["state"] == "active" {
            store.transition(id, devmesh_protocol::WorkState::RecoveryRequired)?;
            if let Some(session) = record
                .pointer("/prepared/session_id")
                .and_then(Value::as_str)
            {
                store.append_event(session.parse().unwrap(),id,"0","work.state",json!({"previous":"active","state":"recovery_required","reason":"supervisor restarted; original runtime must be checked"}))?;
            }
        }
        store.update_work(id,&json!({"recovery_required":true,"recovery_reason":"supervisor restarted; use worker recover after checking original runtime"}))?;
    }
    Ok(())
}
