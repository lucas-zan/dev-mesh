use devmesh_protocol::{can_transition, now, Error, Event, Result, WorkState};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{path::Path, sync::Mutex};
use uuid::Uuid;

pub struct Store {
    db: Mutex<Connection>,
}
fn sql(error: rusqlite::Error) -> Error {
    Error::new("storage_error", error.to_string())
}
fn decode(text: String) -> Result<Value> {
    serde_json::from_str(&text).map_err(|e| Error::new("storage_error", e.to_string()))
}
fn state_name(state: WorkState) -> String {
    serde_json::to_value(state)
        .unwrap()
        .as_str()
        .unwrap()
        .into()
}
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::from_connection(Connection::open(path).map_err(sql)?)
    }
    pub fn memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory().map_err(sql)?)
    }
    fn from_connection(db: Connection) -> Result<Self> {
        db.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(sql)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
        CREATE TABLE IF NOT EXISTS works(id TEXT PRIMARY KEY,project TEXT NOT NULL,number INTEGER NOT NULL,state TEXT NOT NULL,payload TEXT NOT NULL);
        CREATE UNIQUE INDEX IF NOT EXISTS unique_active_issue ON works(project,number) WHERE state IN ('preparing','active','finalizing','recovery_required');
        CREATE TABLE IF NOT EXISTS event_cursors(session TEXT PRIMARY KEY,sequence INTEGER NOT NULL DEFAULT 0,acked INTEGER NOT NULL DEFAULT 0);
        CREATE TABLE IF NOT EXISTS event_spool(session TEXT NOT NULL,sequence INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(session,sequence));
        CREATE TABLE IF NOT EXISTS commands(id TEXT PRIMARY KEY,request TEXT NOT NULL,state TEXT NOT NULL,receipt TEXT);
        CREATE TABLE IF NOT EXISTS outbox(id TEXT PRIMARY KEY,payload TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);").map_err(sql)?;
        Ok(Self { db: Mutex::new(db) })
    }
    pub fn reserve(
        &self,
        project: &str,
        number: u64,
        global: usize,
        per_project: usize,
    ) -> Result<Uuid> {
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(sql)?;
        check_spool(&tx, 0)?;
        let counts = |project: Option<&str>| -> Result<usize> {
            tx.query_row("SELECT count(*) FROM works WHERE state IN ('preparing','active','finalizing','recovery_required') AND (?1 IS NULL OR project=?1)",[project],|r|r.get(0)).map_err(sql)
        };
        if counts(None)? >= global || counts(Some(project))? >= per_project {
            return Err(Error::new("capacity_full", "No local worker capacity"));
        }
        let id = Uuid::new_v4();
        let payload =
            json!({"work_id":id,"project":project,"issue_number":number,"created_at":now()});
        tx.execute(
            "INSERT INTO works VALUES(?1,?2,?3,'preparing',?4)",
            params![id.to_string(), project, number, payload.to_string()],
        )
        .map_err(|e| Error::new("work_conflict", e.to_string()))?;
        tx.commit().map_err(sql)?;
        Ok(id)
    }
    pub fn work(&self, id: Uuid) -> Result<Value> {
        let db = self.db.lock().unwrap();
        let (state, text): (String, String) = db
            .query_row(
                "SELECT state,payload FROM works WHERE id=?1",
                [id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(sql)?;
        let mut value = decode(text)?;
        value["state"] = json!(state);
        Ok(value)
    }
    pub fn update_work(&self, id: Uuid, fields: &Value) -> Result<()> {
        let mut value = self.work(id)?;
        for (key, item) in fields
            .as_object()
            .ok_or_else(|| Error::new("invalid_request", "Expected work fields"))?
        {
            if matches!(
                key.as_str(),
                "work_id" | "project" | "issue_number" | "state"
            ) {
                return Err(Error::new("immutable_binding", key));
            }
            value[key] = item.clone();
        }
        self.db
            .lock()
            .unwrap()
            .execute(
                "UPDATE works SET payload=?2 WHERE id=?1",
                params![id.to_string(), value.to_string()],
            )
            .map_err(sql)?;
        Ok(())
    }
    pub fn transition(&self, id: Uuid, to: WorkState) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(sql)?;
        let from: String = tx
            .query_row(
                "SELECT state FROM works WHERE id=?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .map_err(sql)?;
        let state: WorkState = serde_json::from_value(json!(from))
            .map_err(|e| Error::new("storage_error", e.to_string()))?;
        if !can_transition(state, to) {
            return Err(Error::new("invalid_state", format!("{state:?} -> {to:?}")));
        }
        tx.execute(
            "UPDATE works SET state=?2 WHERE id=?1",
            params![id.to_string(), state_name(to)],
        )
        .map_err(sql)?;
        tx.commit().map_err(sql)?;
        Ok(())
    }
    pub fn active(&self) -> Result<Vec<Value>> {
        let db = self.db.lock().unwrap();
        let mut query=db.prepare("SELECT id FROM works WHERE state IN ('preparing','active','finalizing','recovery_required')").map_err(sql)?;
        let ids = query
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(sql)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql)?;
        drop(query);
        drop(db);
        ids.into_iter()
            .map(|id| self.work(id.parse().unwrap()))
            .collect()
    }
    pub fn append_event(
        &self,
        session: Uuid,
        work: Uuid,
        epoch: &str,
        kind: &str,
        payload: Value,
    ) -> Result<Event> {
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(sql)?;
        tx.execute(
            "INSERT OR IGNORE INTO event_cursors(session) VALUES(?1)",
            [session.to_string()],
        )
        .map_err(sql)?;
        let seq: u64 = tx
            .query_row(
                "SELECT sequence+1 FROM event_cursors WHERE session=?1",
                [session.to_string()],
                |r| r.get(0),
            )
            .map_err(sql)?;
        let mut payload = payload;
        if kind != "message.completed" {
            for field in ["reason", "summary", "message"] {
                if let Some(text) = payload.get(field).and_then(Value::as_str) {
                    payload[field] = crate::collector::sanitize(text, "system")["text"].clone();
                }
            }
        }
        let event = Event {
            event_id: Uuid::new_v4(),
            work_id: work,
            session_id: session,
            sequence: seq.to_string(),
            occurred_at: now(),
            run_epoch: epoch.into(),
            kind: kind.into(),
            payload,
        };
        let text = serde_json::to_string(&event).unwrap();
        check_spool(&tx, text.len())?;
        if text.len() > 65536 {
            return Err(Error::new("payload_too_large", "Event exceeds 64KiB"));
        }
        tx.execute(
            "INSERT INTO event_spool VALUES(?1,?2,?3)",
            params![session.to_string(), seq, text],
        )
        .map_err(sql)?;
        tx.execute(
            "UPDATE event_cursors SET sequence=?2 WHERE session=?1",
            params![session.to_string(), seq],
        )
        .map_err(sql)?;
        tx.commit().map_err(sql)?;
        Ok(event)
    }
    pub fn pending_events(&self, session: Uuid) -> Result<Vec<Event>> {
        let db = self.db.lock().unwrap();
        let mut query = db
            .prepare("SELECT payload FROM event_spool WHERE session=?1 ORDER BY sequence LIMIT 32")
            .map_err(sql)?;
        let rows = query
            .query_map([session.to_string()], |r| r.get::<_, String>(0))
            .map_err(sql)?;
        rows.map(|text| {
            serde_json::from_str(&text.map_err(sql)?)
                .map_err(|e| Error::new("storage_error", e.to_string()))
        })
        .collect()
    }
    pub fn ack(&self, session: Uuid, sequence: u64) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction().map_err(sql)?;
        let highest: u64 = tx
            .query_row(
                "SELECT sequence FROM event_cursors WHERE session=?1",
                [session.to_string()],
                |r| r.get(0),
            )
            .map_err(sql)?;
        if sequence > highest {
            return Err(Error::new("invalid_ack", "ACK exceeds persisted sequence"));
        }
        tx.execute(
            "DELETE FROM event_spool WHERE session=?1 AND sequence<=?2",
            params![session.to_string(), sequence],
        )
        .map_err(sql)?;
        tx.execute(
            "UPDATE event_cursors SET acked=max(acked,?2) WHERE session=?1",
            params![session.to_string(), sequence],
        )
        .map_err(sql)?;
        tx.commit().map_err(sql)?;
        Ok(())
    }
    pub fn journal(&self, id: &str, request: &Value) -> Result<bool> {
        let db = self.db.lock().unwrap();
        let text = request.to_string();
        let existing: Option<String> = db
            .query_row("SELECT request FROM commands WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()
            .map_err(sql)?;
        if let Some(old) = existing {
            return if old == text {
                Ok(false)
            } else {
                Err(Error::new("idempotency_conflict", id))
            };
        }
        db.execute(
            "INSERT INTO commands(id,request,state) VALUES(?1,?2,'delivered')",
            params![id, text],
        )
        .map_err(sql)?;
        Ok(true)
    }
    pub fn command_receipt(&self, id: &str, state: &str, receipt: &Value) -> Result<()> {
        self.db
            .lock()
            .unwrap()
            .execute(
                "UPDATE commands SET state=?2,receipt=?3 WHERE id=?1",
                params![id, state, receipt.to_string()],
            )
            .map_err(sql)?;
        Ok(())
    }
    pub fn queued_commands(&self) -> Result<Vec<Value>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db
            .prepare("SELECT request FROM commands WHERE state='queued' ORDER BY rowid")
            .map_err(sql)?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(sql)?;
        rows.map(|s| decode(s.map_err(sql)?)).collect()
    }
    pub fn uncertain_commands(&self) -> Result<Vec<Value>> {
        let db = self.db.lock().unwrap();
        let mut query = db
            .prepare("SELECT request,receipt FROM commands WHERE state='dispatching'")
            .map_err(sql)?;
        let rows = query
            .query_map([], |row| {
                let request: String = row.get(0)?;
                let receipt: String = row.get(1)?;
                Ok((request, receipt))
            })
            .map_err(sql)?;
        rows.map(|row| {
            let (request, receipt) = row.map_err(sql)?;
            Ok(json!({"request":decode(request)?,"receipt":decode(receipt)?}))
        })
        .collect()
    }
    pub fn queue_frame(&self, frame: &devmesh_protocol::Frame) -> Result<()> {
        if matches!(frame.kind.as_str(), "work.register" | "session.register") {
            let id = if frame.kind == "work.register" {
                &frame.payload["work_id"]
            } else {
                &frame.payload["session_id"]
            };
            self.set_setting(
                &format!("registration:{}:{id}", frame.kind),
                &serde_json::to_value(frame).unwrap(),
            )?;
        }
        self.db
            .lock()
            .unwrap()
            .execute(
                "INSERT OR IGNORE INTO outbox VALUES(?1,?2)",
                params![
                    frame.frame_id.to_string(),
                    serde_json::to_string(frame).unwrap()
                ],
            )
            .map_err(sql)?;
        Ok(())
    }
    pub fn pending_frames(&self) -> Result<Vec<Value>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db
            .prepare("SELECT payload FROM outbox ORDER BY rowid")
            .map_err(sql)?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(sql)?;
        rows.map(|s| decode(s.map_err(sql)?)).collect()
    }
    pub fn ack_frame(&self, id: &str) -> Result<()> {
        self.db
            .lock()
            .unwrap()
            .execute("DELETE FROM outbox WHERE id=?1", [id])
            .map_err(sql)?;
        Ok(())
    }
    pub fn setting(&self, key: &str) -> Result<Option<Value>> {
        let text: Option<String> = self
            .db
            .lock()
            .unwrap()
            .query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(sql)?;
        text.map(decode).transpose()
    }
    pub fn set_setting(&self, key: &str, value: &Value) -> Result<()> {
        self.db.lock().unwrap().execute("INSERT INTO settings VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value.to_string()]).map_err(sql)?;
        Ok(())
    }
}

impl Store {
    pub fn event_sessions(&self) -> Result<Vec<Value>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db
            .prepare("SELECT session,acked FROM event_cursors")
            .map_err(sql)?;
        let rows=stmt.query_map([],|r|Ok(json!({"session_id":r.get::<_,String>(0)?,"acked_sequence":r.get::<_,u64>(1)?.to_string()}))).map_err(sql)?;
        rows.map(|v| v.map_err(sql)).collect()
    }
    pub fn registrations(&self) -> Result<Vec<Value>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db
            .prepare("SELECT value FROM settings WHERE key LIKE 'registration:%' ORDER BY key")
            .map_err(sql)?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(sql)?;
        rows.map(|s| decode(s.map_err(sql)?)).collect()
    }
    pub fn command_record(&self, id: &str) -> Result<Option<Value>> {
        let db = self.db.lock().unwrap();
        db.query_row("SELECT request,state,receipt FROM commands WHERE id=?1",[id],|r|{let request:String=r.get(0)?;let state:String=r.get(1)?;let receipt:Option<String>=r.get(2)?;Ok(json!({"request":serde_json::from_str::<Value>(&request).unwrap(),"state":state,"receipt":receipt.and_then(|s|serde_json::from_str::<Value>(&s).ok())}))}).optional().map_err(sql)
    }
}

fn check_spool(db: &Connection, incoming: usize) -> Result<()> {
    let used: u64 = db
        .query_row(
            "SELECT coalesce(sum(length(CAST(payload AS BLOB))),0) FROM event_spool",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let configured: Option<String> = db
        .query_row(
            "SELECT value FROM settings WHERE key='spool_limit_bytes'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    let limit = configured
        .map(|s| serde_json::from_str::<u64>(&s))
        .transpose()
        .map_err(|e| Error::new("config_error", e.to_string()))?
        .unwrap_or(512 * 1024 * 1024);
    if used >= limit || used + incoming as u64 > limit {
        return Err(Error::new("spool_full","Unacknowledged event spool reached capacity; restore Relay connectivity before starting new work"));
    }
    Ok(())
}
