mod common;
use serde_json::{json, Value};

#[test]
fn persistent_ack_deduplicates_and_never_crosses_gaps() {
    let (relay, actor, session) = common::setup();
    let initial_cursor = relay.store.cursor().unwrap();
    let mut fixture: Value = serde_json::from_str(include_str!(
        "../../devmesh-protocol/fixtures/wire-events.json"
    ))
    .unwrap();
    let first = fixture["payload"]["events"][0].clone();
    assert_eq!(relay.ingest(&actor, first.clone()).unwrap(), 1);
    assert_eq!(relay.ingest(&actor, first.clone()).unwrap(), 1);
    let mut third = first.clone();
    third["sequence"] = json!("3");
    third["event_id"] = json!(uuid::Uuid::new_v4());
    assert_eq!(relay.ingest(&actor, third).unwrap(), 1);
    let mut second = first.clone();
    second["sequence"] = json!("2");
    second["event_id"] = json!(uuid::Uuid::new_v4());
    assert_eq!(relay.ingest(&actor, second).unwrap(), 3);
    fixture["payload"]["events"][0]["payload"]["text"] = json!("conflicting body");
    assert_eq!(
        relay
            .ingest(&actor, fixture["payload"]["events"][0].clone())
            .unwrap_err()
            .code,
        "event_conflict"
    );
    assert_eq!(
        relay
            .events(&actor, session["session_id"].as_str().unwrap())
            .unwrap()
            .len(),
        3
    );
    assert_eq!(relay.store.cursor().unwrap(), initial_cursor + 3);
}
#[test]
fn another_users_device_cannot_take_over_session() {
    let (relay, actor, session) = common::setup();
    let other = relay.create_user(&actor, "other", "password").unwrap();
    let other = relay.authenticate(&other.token).unwrap();
    relay
        .set_member(
            &actor,
            session["project_id"].as_str().unwrap(),
            &other.user_id,
            vec!["read".into(), "ingest".into()],
        )
        .unwrap();
    assert!(relay.register_session(&other, session).is_err());
}

#[test]
fn out_of_order_events_do_not_regress_session_projection() {
    let (relay, actor, session) = common::setup();
    let sid = session["session_id"].as_str().unwrap();
    let envelope = |sequence: &str, kind: &str| json!({"event_id":uuid::Uuid::new_v4(),"work_id":session["work_id"],"session_id":sid,"sequence":sequence,"occurred_at":devmesh_protocol::now(),"run_epoch":"1","type":kind,"payload":{"run_epoch":"1","outcome":"completed"}});
    let third = envelope("3", "turn.completed");
    assert_eq!(relay.ingest(&actor, third).unwrap(), 0);
    assert_eq!(
        relay.visible(&actor, "sessions", sid).unwrap()["state"],
        "running"
    );
    relay.ingest(&actor, envelope("1", "turn.started")).unwrap();
    relay.ingest(&actor, envelope("2", "turn.started")).unwrap();
    assert_eq!(
        relay.visible(&actor, "sessions", sid).unwrap()["state"],
        "idle"
    );
}

#[test]
fn device_presence_changes_are_visible_without_publishing_each_heartbeat() {
    let (relay, actor, _) = common::setup();
    let cursor = relay.store.cursor().unwrap();
    relay.set_device_online(&actor, false).unwrap();
    let changes = relay.feed(&actor, cursor, None).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0]["kind"], "device.changed");
    assert_eq!(changes[0]["payload"]["online"], false);
    relay.set_device_online(&actor, false).unwrap();
    assert_eq!(relay.feed(&actor, cursor, None).unwrap().len(), 1);
}
