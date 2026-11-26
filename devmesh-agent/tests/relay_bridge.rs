use devmesh_agent::{relay_bridge::AgentLink, storage::Store};
use serde_json::{json, Value};
use std::sync::Arc;

#[test]
fn event_ack_and_registration_receipts_remove_only_confirmed_records() {
    let store = Arc::new(Store::memory().unwrap());
    let link = AgentLink::new(uuid::Uuid::new_v4(), store.clone());
    let session = uuid::Uuid::new_v4();
    let work = uuid::Uuid::new_v4();
    store
        .append_event(
            session,
            work,
            "1",
            "message.completed",
            devmesh_agent::collector::sanitize("done", "agent"),
        )
        .unwrap();
    let ack = devmesh_protocol::Frame::new(
        "events.ack",
        json!({"session_id":session,"acked_sequence":"1","missing_sequences":[]}),
    );
    link.accept(&ack).unwrap();
    assert!(store.pending_events(session).unwrap().is_empty());
    let frame = devmesh_protocol::Frame::new("work.register", json!({"work_id":work}));
    store.queue_frame(&frame).unwrap();
    let mut receipt = devmesh_protocol::Frame::new(
        "record.ack",
        json!({"record_type":"work","record_id":frame.frame_id,"result":"accepted"}),
    );
    receipt.reply_to = Some(frame.frame_id);
    link.accept(&receipt).unwrap();
    assert!(store.pending_frames().unwrap().is_empty());
}

#[test]
fn catalog_projection_keeps_source_identity_and_no_raw_body() {
    let repo = devmesh_agent::gitea::Repo {
        instance_id: uuid::Uuid::new_v4(),
        repository_id: 42,
        owner: "test".into(),
        name: "repo".into(),
    };
    let source = json!({"number":7,"title":"Issue","state":"open","body":"private source code","updated_at":"2026-11-25T12:00:00Z","assignees":[],"html_url":"https://example.invalid/issues/7"});
    let item = devmesh_agent::relay_bridge::catalog_item(&repo, &source).unwrap();
    assert_eq!(item["issue_ref"]["repository_id"], 42);
    assert!(!item.to_string().contains("private source code"));
    let _: Value = item;
}

#[test]
fn a_busy_catalog_publisher_does_not_disconnect_or_delete_work_events() {
    let store = Arc::new(Store::memory().unwrap());
    let link = AgentLink::new(uuid::Uuid::new_v4(), store.clone());
    let session = uuid::Uuid::new_v4();
    let work = uuid::Uuid::new_v4();
    store
        .append_event(session, work, "1", "turn.completed", json!({}))
        .unwrap();
    let acquire = devmesh_protocol::Frame::new("catalog.acquire", json!({"project_id":"project"}));
    store.queue_frame(&acquire).unwrap();
    let mut error = devmesh_protocol::Frame::new(
        "protocol.error",
        json!({"code":"lease_busy","message":"Another publisher owns the lease","retryable":false}),
    );
    error.reply_to = Some(acquire.frame_id);
    link.accept(&error).unwrap();
    assert!(store.pending_frames().unwrap().is_empty());
    assert_eq!(store.pending_events(session).unwrap().len(), 1);
}
