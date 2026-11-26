use devmesh_protocol::{Error, Result, WorkState};
use serde_json::Value;
use uuid::Uuid;

pub struct ControlTarget {
    pub work_state: WorkState,
    pub session_state: String,
    pub instance: Uuid,
    pub epoch: String,
    pub generation: u64,
    pub enabled: bool,
}

pub fn check_command(target: &ControlTarget, command: &Value, now: &str) -> Result<()> {
    if !target.enabled {
        return Err(Error::new(
            "remote_control_disabled",
            "Remote control is disabled locally",
        ));
    }
    if target.work_state != WorkState::Active {
        return Err(Error::new("invalid_state", "Work is not active"));
    }
    if command["expires_at"]
        .as_str()
        .is_none_or(|deadline| deadline <= now)
    {
        return Err(Error::new("command_expired", "Command expired"));
    }
    let to = &command["target"];
    if to["runtime_instance_id"] != target.instance.to_string() {
        return Err(Error::new("stale_instance", "Runtime instance changed"));
    }
    if to["expected_run_epoch"] != target.epoch {
        return Err(Error::new("stale_turn", "Top-level run changed"));
    }
    if to["claim_generation"] != target.generation {
        return Err(Error::new("claim_conflict", "Claim generation changed"));
    }
    let valid = match command["type"].as_str().unwrap_or("") {
        "steer" | "follow_up" => target.session_state == "running",
        "prompt" => target.session_state == "idle",
        "interrupt" => matches!(
            target.session_state.as_str(),
            "running" | "waiting_approval" | "waiting_input"
        ),
        _ => false,
    };
    if !valid {
        return Err(Error::new(
            "invalid_state",
            "Control not allowed in current Session state",
        ));
    }
    Ok(())
}
