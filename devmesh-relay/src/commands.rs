use crate::{events::field, Actor, Change, Feed, Relay};
use devmesh_protocol::{now, Error, Result};
use serde_json::{json, Value};
use uuid::Uuid;
impl Relay {
    pub fn command(
        &self,
        actor: &Actor,
        session_id: &str,
        idempotency: &str,
        request: Value,
    ) -> Result<Value> {
        let _lock = self.lock.lock().unwrap();
        if idempotency.is_empty() {
            return Err(Error::new("invalid_request", "Idempotency-Key required"));
        }
        let session = self.visible(actor, "sessions", session_id)?;
        let project = field(&session, "project_id")?.to_owned();
        let kind = field(&request, "type")?;
        self.require(
            actor,
            &project,
            if kind == "interrupt" {
                "interrupt"
            } else {
                "steer"
            },
        )?;
        if let Some(old) = self
            .store
            .list("commands")?
            .into_iter()
            .find(|c| c["sender_id"] == actor.user_id && c["idempotency_key"] == idempotency)
        {
            if old["request"] != request {
                return Err(Error::new(
                    "idempotency_conflict",
                    "Idempotency key reused with a different request",
                ));
            }
            return Ok(old);
        }
        let target = &request["target"];
        for key in ["project_id", "session_id", "work_id", "runtime_instance_id"] {
            if target[key] != session[key] {
                return Err(Error::new(
                    if key == "runtime_instance_id" {
                        "stale_instance"
                    } else {
                        "invalid_target"
                    },
                    key,
                ));
            }
        }
        if target["expected_run_epoch"] != session["run_epoch"] {
            return Err(Error::new("stale_turn", "Run epoch changed"));
        }
        let work = self
            .store
            .get("work_records", field(&session, "work_id")?)?
            .ok_or_else(|| Error::new("invalid_target", "Work missing"))?;
        if target["claim_generation"] != work["claim_generation"] {
            return Err(Error::new("claim_conflict", "Claim generation changed"));
        }
        if work["state"] != "active" {
            return Err(Error::new("invalid_state", "Work is not active"));
        }
        let device = self
            .store
            .get("devices", field(&session, "device_id")?)?
            .ok_or_else(|| Error::new("offline", "Device missing"))?;
        if device["online"] != true {
            return Err(Error::new("offline", "Device offline"));
        }
        if session["remote_control"]["enabled"] != true
            || session["remote_control"][format!("allow_{kind}")] != true
        {
            return Err(Error::new(
                "remote_control_disabled",
                "Local remote-control policy denies operation",
            ));
        }
        if session["capabilities"][kind] != true {
            return Err(Error::new("unsupported_capability", kind));
        }
        let valid = match kind {
            "steer" | "follow_up" => session["state"] == "running",
            "prompt" => session["state"] == "idle",
            "interrupt" => matches!(
                session["state"].as_str(),
                Some("running" | "waiting_input" | "waiting_approval")
            ),
            _ => false,
        };
        if !valid {
            return Err(Error::new(
                "invalid_state",
                "Control does not match current Session state",
            ));
        }
        let ttl = request["ttl_seconds"]
            .as_i64()
            .unwrap_or(if kind == "follow_up" { 600 } else { 30 });
        if !(1..=3600).contains(&ttl) {
            return Err(Error::new("invalid_request", "TTL must be 1..3600 seconds"));
        }
        let id = Uuid::new_v4().to_string();
        let command = json!({"command_id":id,"sender_id":actor.user_id,"sender":{"user_id":actor.user_id},"type":kind,"target":target,"message":request["message"],"issued_at":now(),"expires_at":(chrono::Utc::now()+chrono::Duration::seconds(ttl)).to_rfc3339_opts(chrono::SecondsFormat::Secs,true),"state":"pending","project_id":project,"session_id":session_id,"device_id":session["device_id"],"idempotency_key":idempotency,"request":request});
        self.store.commit(
            &[Change::put("commands", id, command.clone())],
            &[Feed {
                project,
                kind: "command.changed".into(),
                payload: command.clone(),
            }],
        )?;
        Ok(command)
    }
    pub fn receipt(&self, actor: &Actor, receipt: Value) -> Result<Value> {
        let _lock = self.lock.lock().unwrap();
        let id = field(&receipt, "command_id")?;
        let mut command = self
            .store
            .get("commands", id)?
            .ok_or_else(|| Error::new("not_found", "Command missing"))?;
        let session = self
            .store
            .get("sessions", field(&command, "session_id")?)?
            .ok_or_else(|| Error::new("not_found", "Session missing"))?;
        self.own(actor, &session)?;
        let key = format!("{id}:{}", receipt["receipt_seq"].as_str().unwrap_or(""));
        if let Some(old) = self.store.get("command_receipts", &key)? {
            if old == receipt {
                return Ok(command);
            }
            return Err(Error::new(
                "idempotency_conflict",
                "Receipt sequence reused",
            ));
        }
        let from = field(&command, "state")?;
        let to = field(&receipt, "state")?;
        if !receipt_transition(from, to) {
            return Err(Error::new(
                "invalid_state",
                format!("Command {from} -> {to}"),
            ));
        }
        command["state"] = json!(to);
        command["latest_receipt"] = receipt.clone();
        self.store.commit(
            &[
                Change::put("command_receipts", key, receipt.clone()),
                Change::put("commands", id, command.clone()),
            ],
            &[Feed {
                project: field(&command, "project_id")?.into(),
                kind: "command.changed".into(),
                payload: command.clone(),
            }],
        )?;
        Ok(command)
    }
    pub fn expire_commands(&self) -> Result<()> {
        let _lock = self.lock.lock().unwrap();
        let mut changes = Vec::new();
        let mut feed = Vec::new();
        for mut command in self.store.list("commands")? {
            let cancellable = command["state"] == "pending" && command["dispatched"] != true;
            if !cancellable
                || command["expires_at"]
                    .as_str()
                    .is_none_or(|expires| expires > now().as_str())
            {
                continue;
            }
            command["state"] = json!("expired");
            command["expiry_source"] = json!("relay_clock");
            changes.push(Change::put(
                "commands",
                field(&command, "command_id")?,
                command.clone(),
            ));
            feed.push(Feed {
                project: field(&command, "project_id")?.into(),
                kind: "command.changed".into(),
                payload: command,
            });
        }
        if !changes.is_empty() {
            self.store.commit(&changes, &feed)?;
        }
        Ok(())
    }
    pub fn mark_dispatched(&self, id: &str) -> Result<()> {
        let _lock = self.lock.lock().unwrap();
        let mut command = self
            .store
            .get("commands", id)?
            .ok_or_else(|| Error::new("not_found", "Command missing"))?;
        command["dispatched"] = json!(true);
        self.store
            .commit(&[Change::put("commands", id, command)], &[])?;
        Ok(())
    }
    pub fn pending_commands(&self, actor: &Actor) -> Result<Vec<Value>> {
        self.expire_commands()?;
        let device = actor
            .device_id
            .as_deref()
            .ok_or_else(|| Error::new("permission_denied", "Agent device required"))?;
        self.store
            .list("commands")?
            .into_iter()
            .filter(|command| {
                command["device_id"] == device
                    && matches!(command["state"].as_str(), Some("pending" | "delivered"))
            })
            .filter_map(|command| {
                match self.permitted(
                    actor,
                    command["project_id"].as_str().unwrap_or(""),
                    "ingest",
                ) {
                    Ok(true) => Some(Ok(command)),
                    Ok(false) => None,
                    Err(error) => Some(Err(error)),
                }
            })
            .collect()
    }
    pub fn command_history(&self, actor: &Actor, session: &str) -> Result<Vec<Value>> {
        self.visible(actor, "sessions", session)?;
        self.expire_commands()?;
        Ok(self
            .store
            .list("commands")?
            .into_iter()
            .filter(|c| c["session_id"] == session)
            .collect())
    }
    pub fn comment(
        &self,
        actor: &Actor,
        issue_id: &str,
        message: &str,
        work: Option<&str>,
    ) -> Result<Value> {
        let issue = self.visible(actor, "issue_snapshots", issue_id)?;
        let project = field(&issue, "project_id")?.to_owned();
        self.require(actor, &project, "comment")?;
        if let Some(id) = work {
            let work = self.visible(actor, "work_records", id)?;
            if work["issue_id"] != issue_id {
                return Err(Error::new(
                    "invalid_target",
                    "Work belongs to another Issue",
                ));
            }
        }
        let id = Uuid::new_v4().to_string();
        let comment = json!({"comment_id":id,"issue_id":issue_id,"project_id":project,"work_id":work,"sender_id":actor.user_id,"message":message,"created_at":now()});
        self.store.commit(
            &[Change::put("comments", id, comment.clone())],
            &[Feed {
                project,
                kind: "comment.created".into(),
                payload: comment.clone(),
            }],
        )?;
        Ok(comment)
    }
}
fn receipt_transition(from: &str, to: &str) -> bool {
    from == to
        || matches!(
            (from, to),
            (
                "pending",
                "delivered" | "accepted" | "rejected" | "expired" | "cancelled" | "unknown"
            ) | (
                "delivered",
                "accepted" | "applied" | "rejected" | "expired" | "cancelled" | "unknown"
            ) | ("accepted", "applied" | "cancelled" | "expired" | "unknown")
                | ("unknown", "applied" | "rejected")
        )
}

pub fn dispatch_frame(command: &Value) -> devmesh_protocol::Frame {
    devmesh_protocol::Frame::new(
        "command.dispatch",
        json!({"command_id":command["command_id"],"sender_id":command["sender_id"],"type":command["type"],"target":command["target"],"message":command["message"],"issued_at":command["issued_at"],"expires_at":command["expires_at"]}),
    )
}
