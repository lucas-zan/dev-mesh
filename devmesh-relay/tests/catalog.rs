mod common;
use serde_json::json;
#[test]
fn catalog_requires_publisher_epoch_and_complete_snapshot_before_missing() {
    let (relay, actor, session) = common::setup();
    let project = session["project_id"].as_str().unwrap();
    let lease = relay.acquire_catalog(&actor, project).unwrap();
    let item = json!({"issue_ref":{"instance_id":"11111111-1111-4111-8111-111111111111","repository_id":42,"issue_number":7},"title":"new task","source_state":"open","canonical_state":"ready","source_updated_at":"2026-11-25T10:00:00Z","content_version":1,"metadata_hash":"aaa","role":"backend","assignees":[],"url":"https://example.invalid/issues/7"});
    let batch = json!({"project_id":project,"epoch":lease["epoch"],"snapshot_id":uuid::Uuid::new_v4(),"snapshot_seq":"1","page_index":0,"is_last":true,"scope":"all","items":[item]});
    assert_eq!(
        relay.catalog_page(&actor, batch.clone()).unwrap()["committed"],
        true
    );
    let mut stale = batch;
    stale["epoch"] = json!("0");
    assert!(relay.catalog_page(&actor, stale).is_err());
    let incomplete = json!({"project_id":project,"epoch":lease["epoch"],"snapshot_id":uuid::Uuid::new_v4(),"snapshot_seq":"2","page_index":1,"is_last":true,"scope":"all","items":[]});
    assert_eq!(
        relay.catalog_page(&actor, incomplete).unwrap()["committed"],
        false
    );
    let issues = relay.issues(&actor, project).unwrap();
    assert!(issues
        .iter()
        .any(|i| i["title"] == "new task" && i["missing"] != true));
}

#[test]
fn expired_publisher_lease_gets_a_new_epoch_even_for_the_same_device() {
    let (relay, actor, session) = common::setup();
    let project = session["project_id"].as_str().unwrap();
    let mut first = relay.acquire_catalog(&actor, project).unwrap();
    first["expires_at"] = json!("2000-01-01T00:00:00Z");
    relay
        .store
        .commit(
            &[devmesh_relay::Change::put(
                "catalog_leases",
                project,
                first.clone(),
            )],
            &[],
        )
        .unwrap();
    let second = relay.acquire_catalog(&actor, project).unwrap();
    assert_ne!(first["epoch"], second["epoch"]);
}

#[test]
fn a_higher_content_version_wins_without_two_timestamp_observations() {
    let (relay, actor, session) = common::setup();
    let project = session["project_id"].as_str().unwrap();
    let lease = relay.acquire_catalog(&actor, project).unwrap();
    let mut item = json!({"issue_ref":{"instance_id":"11111111-1111-4111-8111-111111111111","repository_id":42,"issue_number":7},"title":"old","source_state":"open","canonical_state":"ready","source_updated_at":"2026-11-25T10:00:00Z","content_version":"1","metadata_hash":"aaa","role":"backend","assignees":[],"url":"https://example.invalid/issues/7"});
    let batch = |sequence: &str, item: serde_json::Value| json!({"project_id":project,"epoch":lease["epoch"],"snapshot_id":uuid::Uuid::new_v4(),"snapshot_seq":sequence,"page_index":0,"is_last":true,"scope":"all","items":[item]});
    relay
        .catalog_page(&actor, batch("1", item.clone()))
        .unwrap();
    item["title"] = json!("new");
    item["content_version"] = json!("2");
    item["metadata_hash"] = json!("bbb");
    let result = relay.catalog_page(&actor, batch("2", item)).unwrap();
    assert!(result["conflicts"].as_array().unwrap().is_empty());
    assert!(relay
        .issues(&actor, project)
        .unwrap()
        .iter()
        .any(|issue| issue["title"] == "new"));
}
