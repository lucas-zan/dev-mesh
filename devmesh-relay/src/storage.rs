//! Storage is a replaceable boundary. PostgreSQL owns atomic writes and feed ordering.
use devmesh_protocol::{Error, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::Write,
    process::{Command, Stdio},
    sync::Mutex,
};

#[derive(Clone)]
pub struct Change {
    pub table: &'static str,
    pub id: String,
    value: Option<Value>,
    pub insert_only: bool,
}
impl Change {
    pub fn put(table: &'static str, id: impl Into<String>, value: Value) -> Self {
        Self {
            table,
            id: id.into(),
            value: Some(value),
            insert_only: false,
        }
    }

    pub fn remove(table: &'static str, id: impl Into<String>) -> Self {
        Self {
            table,
            id: id.into(),
            value: None,
            insert_only: false,
        }
    }
}
#[derive(Clone)]
pub struct Feed {
    pub project: String,
    pub kind: String,
    pub payload: Value,
}
pub trait Repository: Send + Sync {
    fn get(&self, table: &str, id: &str) -> Result<Option<Value>>;
    fn list(&self, table: &str) -> Result<Vec<Value>>;
    fn commit(&self, changes: &[Change], feeds: &[Feed]) -> Result<u64>;
    fn feed(&self, after: u64) -> Result<Vec<Value>>;
    fn cursor(&self) -> Result<u64>;
}
#[derive(Default, Clone)]
struct Data {
    tables: BTreeMap<String, BTreeMap<String, Value>>,
    feed: Vec<Value>,
}
#[derive(Default)]
pub struct MemoryStore {
    data: Mutex<Data>,
}
impl Repository for MemoryStore {
    fn get(&self, table: &str, id: &str) -> Result<Option<Value>> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .tables
            .get(table)
            .and_then(|t| t.get(id))
            .cloned())
    }
    fn list(&self, table: &str) -> Result<Vec<Value>> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .tables
            .get(table)
            .map(|t| t.values().cloned().collect())
            .unwrap_or_default())
    }
    fn commit(&self, changes: &[Change], feeds: &[Feed]) -> Result<u64> {
        let mut data = self.data.lock().unwrap();
        let mut next = data.clone();
        for change in changes {
            let table = next.tables.entry(change.table.into()).or_default();
            let Some(value) = &change.value else {
                table.remove(&change.id);
                continue;
            };
            if change.insert_only && table.contains_key(&change.id) {
                return Err(Error::new("conflict", "Record already exists"));
            }
            table.insert(change.id.clone(), value.clone());
        }
        for feed in feeds {
            let cursor = next.feed.len() + 1;
            next.feed.push(json!({"cursor":cursor.to_string(),"project_id":feed.project,"kind":feed.kind,"payload":feed.payload,"received_at":devmesh_protocol::now()}));
        }
        let cursor = next.feed.len() as u64;
        *data = next;
        Ok(cursor)
    }
    fn feed(&self, after: u64) -> Result<Vec<Value>> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .feed
            .iter()
            .skip(after as usize)
            .take(200)
            .cloned()
            .collect())
    }
    fn cursor(&self) -> Result<u64> {
        Ok(self.data.lock().unwrap().feed.len() as u64)
    }
}

pub const TABLES: &[&str] = &[
    "settings",
    "users",
    "invites",
    "auth_sessions",
    "api_keys",
    "devices",
    "gitea_identities",
    "gitea_instances",
    "projects",
    "project_members",
    "catalog_leases",
    "catalog_batches",
    "issue_snapshots",
    "work_records",
    "sessions",
    "session_segments",
    "session_events",
    "commands",
    "command_receipts",
    "comments",
    "subscriptions",
    "audit_logs",
];
fn table(name: &str) -> Result<&str> {
    if TABLES.contains(&name) {
        Ok(name)
    } else {
        Err(Error::new("storage_error", "Unknown storage collection"))
    }
}
fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub struct PostgresStore {
    url: String,
}
impl PostgresStore {
    pub fn connect(url: String) -> Result<Self> {
        let store = Self { url };
        store.query("SELECT 1")?;
        Ok(store)
    }
    pub fn migrate(&self) -> Result<()> {
        self.query(include_str!("../migrations/001_initial.sql"))?;
        Ok(())
    }
    fn command(&self) -> Command {
        let mut command = Command::new("psql");
        command
            .args(["-X", "-q", "-A", "-t", "-v", "ON_ERROR_STOP=1"])
            .args(["--dbname", &self.url])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
    fn query(&self, sql: &str) -> Result<String> {
        let mut child = self.command().spawn().map_err(|_| {
            Error::new(
                "database_unavailable",
                "psql is required by the PostgreSQL adapter",
            )
        })?;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(sql.as_bytes())
            .map_err(|_| Error::new("database_unavailable", "Database input failed"))?;
        let output = child
            .wait_with_output()
            .map_err(|_| Error::new("database_unavailable", "Database process failed"))?;
        query_output(output)
    }
}

fn query_output(output: std::process::Output) -> Result<String> {
    if !output.status.success() {
        return Err(Error::new(
            "database_error",
            format!(
                "PostgreSQL query failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| Error::new("database_error", "Invalid database output"))
}
impl Repository for PostgresStore {
    fn get(&self, name: &str, id: &str) -> Result<Option<Value>> {
        let text = self.query(&format!(
            "SELECT payload::text FROM {} WHERE id={}",
            table(name)?,
            literal(id)
        ))?;
        if text.trim().is_empty() {
            return Ok(None);
        }
        serde_json::from_str(text.trim())
            .map(Some)
            .map_err(|_| Error::new("database_error", "Invalid persisted JSON"))
    }
    fn list(&self, name: &str) -> Result<Vec<Value>> {
        let text = self.query(&format!(
            "SELECT coalesce(jsonb_agg(payload),'[]'::jsonb)::text FROM {}",
            table(name)?
        ))?;
        serde_json::from_str(text.trim())
            .map_err(|_| Error::new("database_error", "Invalid persisted list"))
    }
    fn commit(&self, changes: &[Change], feeds: &[Feed]) -> Result<u64> {
        let mut sql=String::from("BEGIN; SET LOCAL standard_conforming_strings=on; SELECT cursor FROM feed_counter WHERE id=1 FOR UPDATE;\n");
        for change in changes {
            let name = table(change.table)?;
            let id = literal(&change.id);
            let Some(value) = &change.value else {
                sql.push_str(&format!("DELETE FROM {name} WHERE id={id};\n"));
                continue;
            };
            sql.push_str(&format!(
                "INSERT INTO {}(id,payload) VALUES({},{}) {} ;\n",
                name,
                id,
                format_args!("{}::jsonb", literal(&value.to_string())),
                if change.insert_only {
                    ""
                } else {
                    "ON CONFLICT(id) DO UPDATE SET payload=excluded.payload"
                }
            ));
        }
        for feed in feeds {
            sql.push_str(&format!("UPDATE feed_counter SET cursor=cursor+1 WHERE id=1; INSERT INTO event_feed(cursor,project_id,kind,payload) SELECT cursor,{},{},{}::jsonb FROM feed_counter WHERE id=1;\n",literal(&feed.project),literal(&feed.kind),literal(&feed.payload.to_string())));
        }
        sql.push_str("SELECT cursor FROM feed_counter WHERE id=1; COMMIT;");
        self.query(&sql)?
            .lines()
            .last()
            .unwrap_or("0")
            .trim()
            .parse()
            .map_err(|_| Error::new("database_error", "Invalid feed cursor"))
    }
    fn feed(&self, after: u64) -> Result<Vec<Value>> {
        let sql=format!("SELECT coalesce(jsonb_agg(row ORDER BY n),'[]'::jsonb)::text FROM (SELECT cursor n,jsonb_build_object('cursor',cursor::text,'project_id',project_id,'kind',kind,'payload',payload,'received_at',to_char(received_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')) row FROM event_feed WHERE cursor>{after} ORDER BY cursor LIMIT 200) q");
        serde_json::from_str(self.query(&sql)?.trim())
            .map_err(|_| Error::new("database_error", "Invalid feed"))
    }
    fn cursor(&self) -> Result<u64> {
        self.query("SELECT cursor FROM feed_counter WHERE id=1")?
            .trim()
            .parse()
            .map_err(|_| Error::new("database_error", "Invalid cursor"))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{ExitStatus, Output};

    #[test]
    fn postgres_url_is_an_explicit_connection_parameter() {
        let url = "postgresql://devmesh:synthetic@postgres:5432/devmesh";
        let store = PostgresStore { url: url.into() };
        let command = store.command();
        let args: Vec<_> = command.get_args().collect();
        assert!(args.windows(2).any(|pair| pair == ["--dbname", url]));
        assert!(!command.get_envs().any(|(key, _)| key == "PGDATABASE"));
    }

    #[test]
    fn failed_query_reports_the_actual_database_diagnostic() {
        let error = query_output(Output {
            status: ExitStatus::from_raw(256),
            stdout: vec![],
            stderr: b"psql: error: connection to server at postgres failed\n".to_vec(),
        })
        .unwrap_err();
        assert_eq!(error.code, "database_error");
        assert!(error
            .message
            .contains("connection to server at postgres failed"));
    }

    #[test]
    fn successful_query_returns_stdout() {
        let output = query_output(Output {
            status: ExitStatus::from_raw(0),
            stdout: b"1\n".to_vec(),
            stderr: vec![],
        })
        .unwrap();
        assert_eq!(output, "1\n");
    }
}
