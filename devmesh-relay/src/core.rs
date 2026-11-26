use crate::storage::{Change, Feed, Repository};
use devmesh_protocol::{now, Error, Result};
use ring::pbkdf2;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    num::NonZeroU32,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Actor {
    pub user_id: String,
    pub admin: bool,
    pub device_id: Option<String>,
    pub scopes: Vec<String>,
}
#[derive(Debug, serde::Serialize)]
pub struct Enrollment {
    pub user_id: String,
    pub token: String,
    pub device_id: String,
}
pub struct Relay {
    pub store: Arc<dyn Repository>,
    pub lock: Mutex<()>,
}
fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
fn password_hash(password: &str, salt: &str) -> String {
    let mut output = [0u8; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(600000).unwrap(),
        salt.as_bytes(),
        password.as_bytes(),
        &mut output,
    );
    output.iter().map(|byte| format!("{byte:02x}")).collect()
}
impl Relay {
    pub fn new(store: Arc<dyn Repository>) -> Self {
        Self {
            store,
            lock: Mutex::new(()),
        }
    }
    pub fn bootstrap(&self, username: &str, password: &str) -> Result<Enrollment> {
        let _lock = self.lock.lock().unwrap();
        if self.store.get("settings", "bootstrap")?.is_some() {
            return Err(Error::new(
                "already_initialized",
                "Administrator already exists",
            ));
        }
        self.enroll(username, password, true, true)
    }
    pub fn create_user(&self, actor: &Actor, username: &str, password: &str) -> Result<Enrollment> {
        self.admin(actor)?;
        let _lock = self.lock.lock().unwrap();
        self.enroll(username, password, false, false)
    }
    fn enroll(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        bootstrap: bool,
    ) -> Result<Enrollment> {
        if username.trim().is_empty() || password.is_empty() {
            return Err(Error::new(
                "invalid_request",
                "Username and password required",
            ));
        }
        if self
            .store
            .list("users")?
            .iter()
            .any(|u| u["username"] == username)
        {
            return Err(Error::new("conflict", "Username exists"));
        }
        let id = Uuid::new_v4().to_string();
        let salt = Uuid::new_v4().to_string();
        let device = Uuid::new_v4().to_string();
        let token = format!("dm_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let user = json!({"id":id,"user_id":id,"username":username,"display_name":username,"admin":admin,"salt":salt,"password_hash":password_hash(password,&salt),"password_algorithm":"pbkdf2-sha256-600000","enabled":true});
        let mut changes = vec![
            Change::put("users", &id, user),
            Change::put(
                "api_keys",
                hash(&token),
                json!({"id":hash(&token),"user_id":id,"device_id":device,"scopes":["read","comment","steer","interrupt","ingest","catalog:write","manage"],"revoked":false}),
            ),
            Change::put(
                "devices",
                &device,
                json!({"id":device,"device_id":device,"owner":id,"online":false}),
            ),
        ];
        if bootstrap {
            let mut change = Change::put("settings", "bootstrap", json!({"user_id":id}));
            change.insert_only = true;
            changes.push(change);
        }
        self.store.commit(&changes, &[])?;
        Ok(Enrollment {
            user_id: id,
            token,
            device_id: device,
        })
    }
    pub fn login(&self, username: &str, password: &str) -> Result<String> {
        let _lock = self.lock.lock().unwrap();
        let user = self
            .store
            .list("users")?
            .into_iter()
            .find(|u| u["username"] == username && u["enabled"] == true)
            .ok_or_else(|| Error::new("unauthenticated", "Invalid login"))?;
        let expected = password_hash(password, user["salt"].as_str().unwrap_or(""));
        if !constant_eq(
            expected.as_bytes(),
            user["password_hash"].as_str().unwrap_or("").as_bytes(),
        ) {
            return Err(Error::new("unauthenticated", "Invalid login"));
        }
        let token = format!("dm_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        self.store.commit(&[Change::put("auth_sessions",hash(&token),json!({"user_id":user["id"],"scopes":["read","comment","steer","interrupt","manage"],"revoked":false,"expires_at":(chrono::Utc::now()+chrono::Duration::hours(12)).to_rfc3339_opts(chrono::SecondsFormat::Secs,true)}))],&[])?;
        Ok(token)
    }
    pub fn authenticate(&self, token: &str) -> Result<Actor> {
        let key = self
            .store
            .get("api_keys", &hash(token))?
            .or(self.store.get("auth_sessions", &hash(token))?)
            .ok_or_else(|| Error::new("unauthenticated", "Credential not found"))?;
        if key["revoked"] == true
            || key["expires_at"]
                .as_str()
                .is_some_and(|s| s <= now().as_str())
        {
            return Err(Error::new(
                "unauthenticated",
                "Credential expired or revoked",
            ));
        }
        let id = key["user_id"]
            .as_str()
            .ok_or_else(|| Error::new("unauthenticated", "No account"))?;
        let user = self
            .store
            .get("users", id)?
            .ok_or_else(|| Error::new("unauthenticated", "Account not found"))?;
        if user["enabled"] != true {
            return Err(Error::new("permission_denied", "Account disabled"));
        }
        Ok(Actor {
            user_id: id.into(),
            admin: user["admin"] == true,
            device_id: key["device_id"].as_str().map(str::to_owned),
            scopes: key["scopes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect(),
        })
    }
    pub fn admin(&self, actor: &Actor) -> Result<()> {
        if actor.admin {
            Ok(())
        } else {
            Err(Error::new("permission_denied", "Administrator required"))
        }
    }
    pub fn permitted(&self, actor: &Actor, project: &str, permission: &str) -> Result<bool> {
        if !actor.scopes.iter().any(|s| s == permission) {
            return Ok(false);
        }
        if actor.admin {
            return Ok(true);
        }
        Ok(self
            .store
            .get("project_members", &format!("{project}:{}", actor.user_id))?
            .is_some_and(|m| {
                m["permissions"]
                    .as_array()
                    .is_some_and(|p| p.iter().any(|v| v == permission))
            }))
    }
    pub fn require(&self, actor: &Actor, project: &str, permission: &str) -> Result<()> {
        if self.permitted(actor, project, permission)? {
            Ok(())
        } else {
            Err(Error::new("permission_denied", permission))
        }
    }
    pub fn create_project(&self, actor: &Actor, mut value: Value) -> Result<Value> {
        self.admin(actor)?;
        let _lock = self.lock.lock().unwrap();
        if value["repository_id"].as_u64().is_none()
            || value["instance_id"]
                .as_str()
                .and_then(|s| s.parse::<Uuid>().ok())
                .is_none()
        {
            return Err(Error::new(
                "invalid_request",
                "Repository numeric ID and instance UUID required",
            ));
        }
        if let Some(existing) = self.store.list("projects")?.into_iter().find(|p| {
            p["instance_id"] == value["instance_id"] && p["repository_id"] == value["repository_id"]
        }) {
            return Ok(existing);
        }
        let id = Uuid::new_v4().to_string();
        value["id"] = json!(id);
        value["project_id"] = json!(id);
        value["stale"] = json!(true);
        self.store.commit(
            &[Change::put("projects", &id, value.clone())],
            &[Feed {
                project: id.clone(),
                kind: "project.changed".into(),
                payload: json!({"project_id": id}),
            }],
        )?;
        Ok(value)
    }
    pub fn set_member(
        &self,
        actor: &Actor,
        project: &str,
        user: &str,
        permissions: Vec<String>,
    ) -> Result<()> {
        self.require(actor, project, "manage")?;
        if self.store.get("users", user)?.is_none() {
            return Err(Error::new("not_found", "User not found"));
        }
        self.store.commit(&[Change::put("project_members",format!("{project}:{user}"),json!({"project_id":project,"user_id":user,"permissions":permissions,"allowed_roles":["backend","frontend","product","qa"]}))],&[])?;
        Ok(())
    }
    pub fn projects(&self, actor: &Actor) -> Result<Vec<Value>> {
        self.store
            .list("projects")?
            .into_iter()
            .filter_map(
                |p| match self.permitted(actor, p["id"].as_str().unwrap_or(""), "read") {
                    Ok(true) => Some(Ok(p)),
                    Ok(false) => None,
                    Err(e) => Some(Err(e)),
                },
            )
            .collect()
    }
    pub fn feed(&self, actor: &Actor, after: u64, session: Option<&str>) -> Result<Vec<Value>> {
        self.store
            .feed(after)?
            .into_iter()
            .filter_map(|item| {
                let project = item["project_id"].as_str().unwrap_or("");
                let visible = self.permitted(actor, project, "read");
                match visible {
                    Ok(true)
                        if session.is_none_or(|s| {
                            item["payload"]["session_id"] == s
                                || item["payload"]["target"]["session_id"] == s
                        }) =>
                    {
                        Some(Ok(item))
                    }
                    Ok(_) => None,
                    Err(e) => Some(Err(e)),
                }
            })
            .collect()
    }
    pub fn visible(&self, actor: &Actor, table: &str, id: &str) -> Result<Value> {
        let value = self
            .store
            .get(table, id)?
            .ok_or_else(|| Error::new("not_found", "Record not found"))?;
        if self.permitted(actor, value["project_id"].as_str().unwrap_or(""), "read")? {
            Ok(value)
        } else {
            Err(Error::new("not_found", "Record not found"))
        }
    }
    pub fn record(
        &self,
        actor: &Actor,
        table: &'static str,
        id: &str,
        value: Value,
        kind: &str,
    ) -> Result<Value> {
        let project = value["project_id"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_request", "project_id required"))?
            .to_owned();
        self.require(actor, &project, "ingest")?;
        self.store.commit(
            &[Change::put(table, id, value.clone())],
            &[Feed {
                project,
                kind: kind.into(),
                payload: value.clone(),
            }],
        )?;
        Ok(value)
    }
}
fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |x, (a, b)| x | (a ^ b)) == 0
}
