use crate::{Actor, Change, Feed, Relay};
use devmesh_protocol::{sequence, Error, Event, Result};
use serde_json::{json, Value};

impl Relay {
    pub fn set_device_online(&self, actor: &Actor, online: bool) -> Result<()> {
        let _lock = self.lock.lock().unwrap();
        let id = actor
            .device_id
            .as_deref()
            .ok_or_else(|| Error::new("permission_denied", "Agent device key required"))?;
        let device = self
            .store
            .get("devices", id)?
            .ok_or_else(|| Error::new("not_found", "Device not found"))?;
        if device["owner"] != actor.user_id {
            return Err(Error::new("permission_denied", "Device owner mismatch"));
        }
        self.write_device_presence(id, device, online)
    }
    pub(crate) fn disconnect_device(&self, id: &str) -> Result<()> {
        let _lock = self.lock.lock().unwrap();
        let device = self
            .store
            .get("devices", id)?
            .ok_or_else(|| Error::new("not_found", "Device not found"))?;
        self.write_device_presence(id, device, false)
    }
    pub(crate) fn remove_device(&self, actor: &Actor, id: &str) -> Result<()> {
        self.admin(actor)?;
        let _lock = self.lock.lock().unwrap();
        let device = self
            .store
            .get("devices", id)?
            .ok_or_else(|| Error::new("not_found", "Device not found"))?;
        if device["online"] == true {
            return Err(Error::new(
                "conflict",
                "Disconnect the device before removing it",
            ));
        }
        let mut changes = vec![Change::remove("devices", id)];
        for key in self
            .store
            .list("api_keys")?
            .iter()
            .filter(|key| key["device_id"] == id)
        {
            changes.push(Change::remove("api_keys", field(key, "id")?));
        }
        self.store.commit(&changes, &[])?;
        Ok(())
    }
    fn write_device_presence(&self, id: &str, mut device: Value, online: bool) -> Result<()> {
        let changed = device["online"] != online;
        device["online"] = json!(online);
        device["last_seen"] = json!(devmesh_protocol::now());
        let projects: std::collections::BTreeSet<String> = if changed {
            self.store
                .list("work_records")?
                .into_iter()
                .filter(|w| w["device_id"] == id)
                .filter_map(|w| w["project_id"].as_str().map(str::to_owned))
                .collect()
        } else {
            Default::default()
        };
        let feeds = projects.into_iter().map(|project| Feed {
            project, kind: "device.changed".into(),
            payload: json!({"device_id": id, "online": online, "last_seen": device["last_seen"]}),
        }).collect::<Vec<_>>();
        self.store
            .commit(&[Change::put("devices", id, device)], &feeds)?;
        Ok(())
    }
    pub fn register_work(&self, actor: &Actor, mut work: Value) -> Result<Value> {
        let _lock = self.lock.lock().unwrap();
        let id = field(&work, "work_id")?.to_owned();
        let (project, new_project) = self.work_project(actor, &work)?;
        let project_id = field(&project, "id")?.to_owned();
        if work["issue_ref"]["instance_id"] != project["instance_id"]
            || work["issue_ref"]["repository_id"] != project["repository_id"]
        {
            return Err(Error::new(
                "invalid_target",
                "Issue reference does not match project",
            ));
        }
        let device = actor
            .device_id
            .as_deref()
            .ok_or_else(|| Error::new("permission_denied", "Agent device key required"))?;
        if let Some(existing) = self.store.get("work_records", &id)? {
            if existing["owner"] != actor.user_id
                || existing["device_id"] != device
                || existing["claim_id"] != work["claim_id"]
                || existing["issue_ref"] != work["issue_ref"]
            {
                return Err(Error::new("invalid_target", "Work binding is immutable"));
            }
            return Ok(existing);
        }
        work["project_id"] = json!(project_id);
        work["owner"] = json!(actor.user_id);
        work["device_id"] = json!(device);
        let issue_id = self
            .store
            .list("issue_snapshots")?
            .iter()
            .find(|i| i["issue_ref"] == work["issue_ref"])
            .and_then(|i| i["id"].as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        work["issue_id"] = json!(issue_id);
        let mut changes = vec![Change::put("work_records", &id, work.clone())];
        if new_project {
            changes.push(Change::put("projects", &project_id, project));
        }
        if self.store.get("issue_snapshots", &issue_id)?.is_none() {
            let source = &work["source"];
            let title = source["issue_title"]
                .as_str()
                .filter(|title| !title.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Issue #{}", work["issue_ref"]["issue_number"]));
            changes.push(Change::put(
                "issue_snapshots",
                &issue_id,
                json!({
                    "id":issue_id,"issue_id":issue_id,"issue_ref":work["issue_ref"],
                    "project_id":project_id,"title":title,"url":source["issue_url"],
                    "role":work["role"],"source_state":"open"
                }),
            ));
        }
        self.store.commit(
            &changes,
            &[Feed {
                project: project_id,
                kind: "work.changed".into(),
                payload: work.clone(),
            }],
        )?;
        Ok(work)
    }
    fn work_project(&self, actor: &Actor, work: &Value) -> Result<(Value, bool)> {
        if let Some(id) = work["project_id"].as_str() {
            self.require(actor, id, "ingest")?;
            let project = self
                .store
                .get("projects", id)?
                .ok_or_else(|| Error::new("not_found", "Project not found"))?;
            return Ok((project, false));
        }
        let reference = &work["issue_ref"];
        let instance = field(reference, "instance_id")?;
        let repository = reference["repository_id"]
            .as_u64()
            .filter(|id| *id > 0)
            .ok_or_else(|| Error::new("invalid_target", "Repository ID required"))?;
        if uuid::Uuid::parse_str(instance).is_err()
            || reference["issue_number"].as_u64().is_none_or(|n| n == 0)
        {
            return Err(Error::new(
                "invalid_target",
                "Valid Issue reference required",
            ));
        }
        if let Some(project) = self.store.list("projects")?.into_iter().find(|project| {
            project["instance_id"] == instance && project["repository_id"] == repository
        }) {
            self.require(actor, field(&project, "id")?, "ingest")?;
            return Ok((project, false));
        }
        self.admin(actor)?;
        let name = field(&work["source"], "repository_name")?;
        let id = uuid::Uuid::new_v4();
        self.require(actor, &id.to_string(), "ingest")?;
        Ok((
            json!({"id":id,"project_id":id,"name":name,"instance_id":instance,"repository_id":repository}),
            true,
        ))
    }
    pub fn register_session(&self, actor: &Actor, mut session: Value) -> Result<Value> {
        let _lock = self.lock.lock().unwrap();
        let id = field(&session, "session_id")?.to_owned();
        let work_id = field(&session, "work_id")?;
        let work = self
            .store
            .get("work_records", work_id)?
            .ok_or_else(|| Error::new("invalid_target", "Register Work first"))?;
        self.own(actor, &work)?;
        if let Some(existing) = self.store.get("sessions", &id)? {
            self.own(actor, &existing)?;
            if existing["work_id"] != session["work_id"] {
                return Err(Error::new("invalid_target", "Session Work is immutable"));
            }
            if existing["runtime_instance_id"] == session["runtime_instance_id"] {
                return Ok(existing);
            }
            if existing["runtime_instance_id"] != session["runtime_instance_id"] {
                self.store.commit(
                    &[Change::put(
                        "session_segments",
                        format!("{id}:{}", existing["runtime_instance_id"]),
                        existing,
                    )],
                    &[],
                )?;
            }
        }
        if !session["parent_session_id"].is_null() {
            let parent = self
                .store
                .get("sessions", field(&session, "parent_session_id")?)?
                .ok_or_else(|| Error::new("invalid_target", "Parent Session missing"))?;
            if parent["work_id"] != session["work_id"] {
                return Err(Error::new(
                    "invalid_target",
                    "Parent belongs to another Work",
                ));
            }
        }
        session["project_id"] = work["project_id"].clone();
        session["owner"] = json!(actor.user_id);
        session["device_id"] = work["device_id"].clone();
        session["issue_ref"] = work["issue_ref"].clone();
        session["run_epoch"] = json!("0");
        let project = field(&work, "project_id")?.to_owned();
        self.store.commit(
            &[Change::put("sessions", id, session.clone())],
            &[
                Feed {
                    project: project.clone(),
                    kind: "session.changed".into(),
                    payload: session.clone(),
                },
                Feed {
                    project,
                    kind: "issue.changed".into(),
                    payload: json!({"issue_id":work["issue_id"],"issue_ref":work["issue_ref"]}),
                },
            ],
        )?;
        Ok(session)
    }
    pub fn own(&self, actor: &Actor, record: &Value) -> Result<()> {
        self.require(actor, field(record, "project_id")?, "ingest")?;
        if record["owner"] != actor.user_id
            || record["device_id"].as_str() != actor.device_id.as_deref()
        {
            return Err(Error::new(
                "permission_denied",
                "Agent does not own this Work/Session",
            ));
        }
        Ok(())
    }
    pub fn update_session(&self, actor: &Actor, update: Value) -> Result<Value> {
        let _lock = self.lock.lock().unwrap();
        let id = field(&update, "session_id")?;
        let mut session = self
            .store
            .get("sessions", id)?
            .ok_or_else(|| Error::new("not_found", "Session not registered"))?;
        self.own(actor, &session)?;
        if session["runtime_instance_id"] != update["runtime_instance_id"] {
            return Err(Error::new("stale_instance", "Runtime instance differs"));
        }
        if !session["native_session_id"].is_null()
            && session["native_session_id"] != update["native_session_id"]
        {
            return Err(Error::new(
                "invalid_target",
                "Native session binding is immutable",
            ));
        }
        for key in [
            "native_session_id",
            "state",
            "run_epoch",
            "remote_control",
            "observed_at",
        ] {
            if let Some(value) = update.get(key) {
                session[key] = value.clone();
            }
        }
        let project = field(&session, "project_id")?.to_owned();
        self.store.commit(
            &[Change::put("sessions", id, session.clone())],
            &[Feed {
                project,
                kind: "session.changed".into(),
                payload: session.clone(),
            }],
        )?;
        Ok(session)
    }
    pub fn ingest(&self, actor: &Actor, value: Value) -> Result<u64> {
        let _lock = self.lock.lock().unwrap();
        let event: Event = serde_json::from_value(value.clone())
            .map_err(|e| Error::new("invalid_schema", e.to_string()))?;
        let session_id = event.session_id.to_string();
        let seq = sequence(&event.sequence)?;
        if seq == 0 {
            return Err(Error::new("invalid_sequence", "Event sequence starts at 1"));
        }
        let session = self
            .store
            .get("sessions", &session_id)?
            .ok_or_else(|| Error::new("invalid_target", "Register Session before events"))?;
        self.own(actor, &session)?;
        if session["work_id"] != event.work_id.to_string() {
            return Err(Error::new(
                "invalid_target",
                "Event belongs to another Work",
            ));
        }
        if event.kind == "message.completed"
            && (event.payload.get("content_policy").is_none()
                || event.payload.get("raw_transcript").is_some())
        {
            return Err(Error::new(
                "invalid_schema",
                "Only policy-filtered visible messages are accepted",
            ));
        }
        let id = format!("{session_id}:{seq}");
        if let Some(old) = self.store.get("session_events", &id)? {
            if old != value {
                return Err(Error::new(
                    "event_conflict",
                    "Same sequence has different content",
                ));
            }
            return self.acked(&session_id);
        }
        if self
            .store
            .list("session_events")?
            .iter()
            .any(|e| e["event_id"] == value["event_id"])
        {
            return Err(Error::new("event_conflict", "Event ID reused"));
        }
        let mut ordered = self
            .store
            .list("session_events")?
            .into_iter()
            .filter(|e| e["session_id"] == session_id)
            .collect::<Vec<_>>();
        let previous_ack = highest_contiguous(&ordered);
        ordered.push(value.clone());
        ordered.sort_by_key(|e| {
            e["sequence"]
                .as_str()
                .unwrap_or("0")
                .parse::<u64>()
                .unwrap_or(0)
        });
        let next_ack = highest_contiguous(&ordered);
        let mut changes = vec![Change::put("session_events", id, value.clone())];
        let mut feeds = Vec::new();
        let mut projected = session.clone();
        let mut work = self
            .store
            .get("work_records", &event.work_id.to_string())?
            .ok_or_else(|| Error::new("invalid_target", "Work missing"))?;
        for value in ordered.iter().filter(|e| {
            let n = e["sequence"]
                .as_str()
                .unwrap_or("0")
                .parse::<u64>()
                .unwrap_or(0);
            n > previous_ack && n <= next_ack
        }) {
            match value["type"].as_str().unwrap_or("") {
                "session.state" => {
                    projected["state"] = value["payload"]["state"].clone();
                }
                "turn.started" => {
                    projected["state"] = json!("running");
                    projected["run_epoch"] = value["run_epoch"].clone();
                }
                "turn.completed" => {
                    projected["state"] = json!("idle");
                }
                "approval.requested" => {
                    projected["state"] = json!(if value["payload"]["kind"] == "input" {
                        "waiting_input"
                    } else {
                        "waiting_approval"
                    });
                }
                "approval.resolved" => {
                    if value["payload"]["outcome"] != "unknown"
                        && matches!(
                            projected["state"].as_str(),
                            Some("waiting_input" | "waiting_approval")
                        )
                    {
                        projected["state"] = json!("running");
                    }
                }
                "work.state" => {
                    let from: devmesh_protocol::WorkState =
                        serde_json::from_value(work["state"].clone())
                            .map_err(|_| Error::new("invalid_state", "Unknown Work state"))?;
                    let to: devmesh_protocol::WorkState =
                        serde_json::from_value(value["payload"]["state"].clone())
                            .map_err(|_| Error::new("invalid_state", "Unknown Work state"))?;
                    if from != to && !devmesh_protocol::can_transition(from, to) {
                        return Err(Error::new("invalid_state", "Illegal Work transition"));
                    }
                    work["state"] = value["payload"]["state"].clone();
                }
                _ => {}
            }
            feeds.push(Feed {
                project: field(&session, "project_id")?.into(),
                kind: "session.event".into(),
                payload: value.clone(),
            });
        }
        changes.push(Change::put("work_records", event.work_id.to_string(), work));
        if projected != session {
            changes.push(Change::put("sessions", &session_id, projected));
        }
        self.store.commit(&changes, &feeds)?;
        Ok(next_ack)
    }
    fn acked(&self, session: &str) -> Result<u64> {
        let mut seqs = self
            .store
            .list("session_events")?
            .into_iter()
            .filter(|e| e["session_id"] == session)
            .map(|e| sequence(e["sequence"].as_str().unwrap_or("")))
            .collect::<Result<Vec<_>>>()?;
        seqs.sort_unstable();
        Ok(seqs
            .into_iter()
            .enumerate()
            .take_while(|(i, seq)| *seq == *i as u64 + 1)
            .count() as u64)
    }
    pub fn events(&self, actor: &Actor, session: &str) -> Result<Vec<Value>> {
        self.visible(actor, "sessions", session)?;
        let mut events = self
            .store
            .list("session_events")?
            .into_iter()
            .filter(|e| e["session_id"] == session)
            .collect::<Vec<_>>();
        events.sort_by_key(|e| {
            e["sequence"]
                .as_str()
                .unwrap_or("0")
                .parse::<u64>()
                .unwrap_or(0)
        });
        Ok(events)
    }
}
pub fn field<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    value[name]
        .as_str()
        .ok_or_else(|| Error::new("invalid_request", format!("{name} required")))
}
pub fn issue_key(reference: &Value) -> String {
    format!(
        "{}:{}:{}",
        reference["instance_id"].as_str().unwrap_or(""),
        reference["repository_id"],
        reference["issue_number"]
    )
}

fn highest_contiguous(events: &[Value]) -> u64 {
    let mut seqs = events
        .iter()
        .filter_map(|e| e["sequence"].as_str().and_then(|s| s.parse::<u64>().ok()))
        .collect::<Vec<_>>();
    seqs.sort_unstable();
    seqs.into_iter()
        .enumerate()
        .take_while(|(i, n)| *n == *i as u64 + 1)
        .count() as u64
}
