use crate::gitea::TaskService;
use devmesh_protocol::{metadata, Error, Result};
use serde_json::Value;
use std::collections::HashSet;
impl TaskService {
    pub async fn check_dependency_cycles(&self, root: &Value) -> Result<()> {
        let reference = &root["issue_ref"];
        if reference.is_null() {
            return Ok(());
        }
        let mut stack = vec![(reference.clone(), false)];
        let mut active = HashSet::new();
        let mut visited = HashSet::new();
        while let Some((reference, exit)) = stack.pop() {
            let key = reference.to_string();
            if exit {
                active.remove(&key);
                visited.insert(key);
                continue;
            }
            if active.contains(&key) {
                return Err(Error::new(
                    "dependency_cycle",
                    "Task dependency graph contains a cycle",
                ));
            }
            if visited.contains(&key) {
                continue;
            }
            active.insert(key);
            stack.push((reference.clone(), true));
            let task = if reference == root["issue_ref"] {
                root.clone()
            } else {
                let path = self.repository_path(&reference).await?;
                let issue = self
                    .client
                    .get(&format!("{path}/issues/{}", reference["issue_number"]))
                    .await?;
                metadata(issue["body"].as_str().ok_or_else(|| {
                    Error::new("invalid_response", "Dependency Issue body missing")
                })?)?
            };
            stack.extend(
                task["dependencies"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .rev()
                    .map(|dep| (dep["producer_issue"].clone(), false)),
            );
        }
        Ok(())
    }
}
