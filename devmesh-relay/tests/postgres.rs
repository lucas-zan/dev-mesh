use devmesh_relay::{Change, PostgresStore, Repository};
use serde_json::json;
#[test]
#[ignore = "requires a dedicated DEVMESH_TEST_DATABASE_URL"]
fn postgres_commits_atomically_and_retains_records_across_reconnect() {
    let url = std::env::var("DEVMESH_TEST_DATABASE_URL").expect("Use a dedicated test database");
    let store = PostgresStore::connect(url.clone()).unwrap();
    store.migrate().unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let first = Change::put("settings", &id, json!({"value":1}));
    store.commit(std::slice::from_ref(&first), &[]).unwrap();
    let mut conflict = first;
    conflict.insert_only = true;
    let rejected = format!("{id}:not-committed");
    assert!(store
        .commit(
            &[
                Change::put("settings", &rejected, json!({"value":2})),
                conflict
            ],
            &[]
        )
        .is_err());
    assert!(store.get("settings", &rejected).unwrap().is_none());
    let reopened = PostgresStore::connect(url).unwrap();
    assert_eq!(reopened.get("settings", &id).unwrap().unwrap()["value"], 1);
}
