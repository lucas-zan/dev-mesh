use crate::{
    events::{field, issue_key},
    Actor, Change, Feed, Relay,
};
use devmesh_protocol::{now, sequence, Error, Result};
use serde_json::{json, Value};
use std::collections::HashSet;
use uuid::Uuid;
impl Relay {
    pub fn acquire_catalog(&self, actor: &Actor, project: &str) -> Result<Value> {
        self.require(actor, project, "catalog:write")?;
        let _lock = self.lock.lock().unwrap();
        let device = actor
            .device_id
            .as_deref()
            .ok_or_else(|| Error::new("permission_denied", "Agent device required"))?;
        let previous = self.store.get("catalog_leases", project)?;
        if let Some(lease) = &previous {
            if lease["publisher_device"] != device
                && lease["expires_at"]
                    .as_str()
                    .is_some_and(|s| s > now().as_str())
            {
                return Err(Error::new(
                    "lease_busy",
                    "Another publisher holds the project lease",
                ));
            }
        }
        let same_active = previous.as_ref().is_some_and(|lease| {
            lease["publisher_device"] == device
                && lease["expires_at"]
                    .as_str()
                    .is_some_and(|expires| expires > now().as_str())
        });
        let epoch = previous
            .as_ref()
            .and_then(|lease| lease["epoch"].as_str())
            .and_then(|epoch| epoch.parse::<u64>().ok())
            .unwrap_or(0)
            + u64::from(!same_active);
        let lease = json!({"project_id":project,"publisher_device":device,"epoch":epoch.to_string(),"expires_at":(chrono::Utc::now()+chrono::Duration::seconds(90)).to_rfc3339_opts(chrono::SecondsFormat::Secs,true),"renew_after_seconds":30,"last_snapshot_seq":if same_active{previous.as_ref().unwrap()["last_snapshot_seq"].clone()}else{json!("0")}});
        self.store.commit(
            &[Change::put("catalog_leases", project, lease.clone())],
            &[],
        )?;
        Ok(lease)
    }
    pub fn catalog_page(&self, actor: &Actor, page: Value) -> Result<Value> {
        let _lock = self.lock.lock().unwrap();
        let project = field(&page, "project_id")?;
        self.require(actor, project, "catalog:write")?;
        let mut lease = self
            .store
            .get("catalog_leases", project)?
            .ok_or_else(|| Error::new("catalog_conflict", "Acquire catalog lease first"))?;
        if lease["publisher_device"].as_str() != actor.device_id.as_deref()
            || lease["epoch"] != page["epoch"]
            || lease["expires_at"]
                .as_str()
                .is_none_or(|s| s <= now().as_str())
        {
            return Err(Error::new(
                "catalog_conflict",
                "Catalog publisher lease is stale",
            ));
        }
        let sequence = sequence(field(&page, "snapshot_seq")?)?;
        let last = devmesh_protocol::sequence(lease["last_snapshot_seq"].as_str().unwrap_or("0"))?;
        if sequence <= last {
            return Ok(
                json!({"project_id":project,"epoch":lease["epoch"],"snapshot_id":page["snapshot_id"],"page_index":page["page_index"],"committed":sequence==last,"conflicts":[]}),
            );
        }
        let key = format!("{project}:{}:{}", page["snapshot_id"], page["page_index"]);
        if let Some(existing) = self.store.get("catalog_batches", &key)? {
            if existing != page {
                return Err(Error::new("idempotency_conflict", "Catalog page changed"));
            }
        }
        self.store
            .commit(&[Change::put("catalog_batches", key, page.clone())], &[])?;
        let mut pages = self
            .store
            .list("catalog_batches")?
            .into_iter()
            .filter(|p| {
                p["snapshot_id"] == page["snapshot_id"]
                    && p["epoch"] == page["epoch"]
                    && p["project_id"] == project
            })
            .collect::<Vec<_>>();
        pages.sort_by_key(|p| p["page_index"].as_u64().unwrap_or(u64::MAX));
        if pages
            .iter()
            .any(|p| p["snapshot_seq"] != page["snapshot_seq"] || p["scope"] != page["scope"])
        {
            return Err(Error::new(
                "invalid_snapshot",
                "Snapshot pages disagree on sequence or scope",
            ));
        }
        let complete = pages.last().is_some_and(|p| p["is_last"] == true)
            && pages
                .iter()
                .enumerate()
                .all(|(i, p)| p["page_index"].as_u64() == Some(i as u64));
        if !complete {
            return Ok(
                json!({"project_id":project,"epoch":lease["epoch"],"snapshot_id":page["snapshot_id"],"page_index":page["page_index"],"committed":false,"conflicts":[]}),
            );
        }
        let project_record = self
            .store
            .get("projects", project)?
            .ok_or_else(|| Error::new("not_found", "Project missing"))?;
        let previous = self.store.list("issue_snapshots")?;
        let mut changes = Vec::new();
        let mut seen = HashSet::new();
        let mut conflicts = Vec::new();
        for item in pages
            .iter()
            .flat_map(|p| p["items"].as_array().into_iter().flatten())
        {
            if item["issue_ref"]["instance_id"] != project_record["instance_id"]
                || item["issue_ref"]["repository_id"] != project_record["repository_id"]
            {
                return Err(Error::new(
                    "invalid_target",
                    "Catalog item does not belong to project",
                ));
            }
            let ref_key = issue_key(&item["issue_ref"]);
            if !seen.insert(ref_key.clone()) {
                return Err(Error::new(
                    "invalid_snapshot",
                    "Issue appears more than once in snapshot",
                ));
            }
            let old = previous
                .iter()
                .find(|p| p["issue_ref"] == item["issue_ref"]);
            if let Some(old) = old {
                let revision = source_revision(old, item)?;
                if revision == std::cmp::Ordering::Less {
                    continue;
                }
                if revision == std::cmp::Ordering::Equal
                    && old["metadata_hash"] != item["metadata_hash"]
                    && !old["metadata_hash"].is_null()
                {
                    let key = format!("conflict:{}:{ref_key}", page["epoch"]);
                    let prior = self.store.get("catalog_batches", &key)?;
                    if prior
                        .as_ref()
                        .is_none_or(|p| p["metadata_hash"] != item["metadata_hash"])
                    {
                        changes.push(Change::put("catalog_batches", key, item.clone()));
                        conflicts.push(item["issue_ref"].clone());
                        continue;
                    }
                }
            }
            let mut updated = item.clone();
            let id = old
                .and_then(|p| p["id"].as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            updated["id"] = json!(id);
            updated["issue_id"] = json!(id);
            updated["project_id"] = json!(project);
            updated["synced_at"] = json!(now());
            updated["stale"] = json!(false);
            updated["missing"] = json!(false);
            changes.push(Change::put("issue_snapshots", id, updated));
        }
        if page["scope"] == "all" && conflicts.is_empty() {
            for mut old in previous.into_iter().filter(|p| {
                p["project_id"] == project && !seen.contains(&issue_key(&p["issue_ref"]))
            }) {
                old["missing"] = json!(true);
                old["stale"] = json!(true);
                changes.push(Change::put(
                    "issue_snapshots",
                    field(&old, "id")?.to_owned(),
                    old,
                ));
            }
        }
        lease["last_snapshot_seq"] = json!(sequence.to_string());
        changes.push(Change::put("catalog_leases", project, lease.clone()));
        let mut project_record = project_record;
        project_record["last_synced_at"] = json!(now());
        project_record["stale"] = json!(false);
        changes.push(Change::put("projects", project, project_record));
        self.store.commit(
            &changes,
            &[Feed {
                project: project.into(),
                kind: "catalog.changed".into(),
                payload: json!({"project_id":project,"conflicts":conflicts,"synced_at":now()}),
            }],
        )?;
        Ok(
            json!({"project_id":project,"epoch":lease["epoch"],"snapshot_id":page["snapshot_id"],"page_index":page["page_index"],"committed":true,"conflicts":conflicts}),
        )
    }
    pub fn workspace_projects(&self, actor: &Actor) -> Result<Vec<Value>> {
        let sessions = self.store.list("sessions")?;
        Ok(self
            .projects(actor)?
            .into_iter()
            .filter(|project| {
                sessions
                    .iter()
                    .any(|session| session["project_id"] == project["id"])
            })
            .collect())
    }
    pub fn workspace_issues(&self, actor: &Actor, project: &str) -> Result<Vec<Value>> {
        self.issues_for(actor, project, true)
    }
    pub fn issues(&self, actor: &Actor, project: &str) -> Result<Vec<Value>> {
        self.issues_for(actor, project, false)
    }
    fn issues_for(&self, actor: &Actor, project: &str, with_sessions: bool) -> Result<Vec<Value>> {
        self.require(actor, project, "read")?;
        let sessions = self
            .store
            .list("sessions")?
            .into_iter()
            .filter(|session| session["project_id"] == project)
            .collect::<Vec<_>>();
        let mut issues = self
            .store
            .list("issue_snapshots")?
            .into_iter()
            .filter(|issue| {
                issue["project_id"] == project
                    && (!with_sessions
                        || sessions
                            .iter()
                            .any(|session| session["issue_ref"] == issue["issue_ref"]))
            })
            .collect::<Vec<_>>();
        for issue in &mut issues {
            issue["active_sessions"] = json!(sessions
                .iter()
                .filter(|s| s["issue_ref"] == issue["issue_ref"]
                    && !matches!(s["state"].as_str(), Some("ended" | "failed" | "cancelled")))
                .cloned()
                .collect::<Vec<_>>());
        }
        issues.sort_by_key(|i| i["issue_ref"]["issue_number"].as_u64().unwrap_or(0));
        Ok(issues)
    }
}

fn source_revision(old: &Value, new: &Value) -> Result<std::cmp::Ordering> {
    if let (Some(before), Some(after)) = (
        old["content_version"].as_str(),
        new["content_version"].as_str(),
    ) {
        let ordering = sequence(after)?.cmp(&sequence(before)?);
        if ordering != std::cmp::Ordering::Equal {
            return Ok(ordering);
        }
    }
    let parse = |text: &str| {
        chrono::DateTime::parse_from_rfc3339(text)
            .map_err(|_| Error::new("invalid_schema", "Catalog timestamp invalid"))
    };
    let Some(old_time) = old["source_updated_at"].as_str() else {
        return Ok(std::cmp::Ordering::Greater);
    };
    Ok(parse(field(new, "source_updated_at")?)?.cmp(&parse(old_time)?))
}
