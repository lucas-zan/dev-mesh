use devmesh_agent::{
    collector::sanitize,
    controller::{check_command, ControlTarget},
    storage::Store,
};
use devmesh_protocol::WorkState;
use serde_json::json;
use uuid::Uuid;

#[test]
fn secrets_and_source_are_removed_before_spool_and_ack_is_contiguous() {
    let text="Result\n```rust\nlet secret = 42;\n```\nOPENAI_API_KEY=sk-test-secret\n/home/alice/project";
    let payload = sanitize(text, "agent");
    let shared = payload.to_string();
    assert!(!shared.contains("sk-test-secret"));
    assert!(!shared.contains("let secret"));
    assert!(!shared.contains("/home/alice"));
    assert_eq!(payload["content_policy"]["omitted"], true);
    let store = Store::memory().unwrap();
    let session = Uuid::new_v4();
    let work = Uuid::new_v4();
    let first = store
        .append_event(session, work, "1", "message.completed", payload.clone())
        .unwrap();
    let second = store
        .append_event(session, work, "1", "message.completed", payload)
        .unwrap();
    assert_eq!(first.sequence, "1");
    assert_eq!(second.sequence, "2");
    assert_eq!(store.pending_events(session).unwrap().len(), 2);
    store.ack(session, 1).unwrap();
    assert_eq!(store.pending_events(session).unwrap().len(), 1);
    assert!(store.ack(session, 3).is_err());
}

#[test]
fn stale_or_terminal_commands_are_rejected_and_journal_detects_conflicts() {
    let target = ControlTarget {
        work_state: WorkState::Active,
        session_state: "running".into(),
        instance: Uuid::new_v4(),
        epoch: "2".into(),
        generation: 1,
        enabled: true,
    };
    let mut command = json!({"type":"steer","target":{"runtime_instance_id":target.instance,"expected_run_epoch":"2","claim_generation":1},"expires_at":"2099-01-01T00:00:00Z"});
    assert!(check_command(&target, &command, "2026-11-25T00:00:00Z").is_ok());
    command["target"]["expected_run_epoch"] = json!("1");
    assert!(check_command(&target, &command, "2026-11-25T00:00:00Z").is_err());
    command["target"]["expected_run_epoch"] = json!("2");
    let ended = ControlTarget {
        work_state: WorkState::Completed,
        ..target
    };
    assert!(check_command(&ended, &command, "2026-11-25T00:00:00Z").is_err());
    let store = Store::memory().unwrap();
    assert!(store.journal("cmd", &command).unwrap());
    assert!(!store.journal("cmd", &command).unwrap());
    command["message"] = json!("different");
    assert!(store.journal("cmd", &command).is_err());
}

#[test]
fn full_spool_refuses_new_events_and_work_without_deleting_unacknowledged_data() {
    let store = Store::memory().unwrap();
    let session = Uuid::new_v4();
    let work = Uuid::new_v4();
    let event = store
        .append_event(
            session,
            work,
            "1",
            "message.completed",
            sanitize("keep this record", "agent"),
        )
        .unwrap();
    store.set_setting("spool_limit_bytes", &json!(1)).unwrap();
    assert_eq!(
        store
            .append_event(session, work, "1", "turn.completed", json!({}))
            .unwrap_err()
            .code,
        "spool_full"
    );
    assert_eq!(store.pending_events(session).unwrap().len(), 1);
    assert_eq!(
        store.reserve("repo", 1, 1, 1).unwrap_err().code,
        "spool_full"
    );
    store.ack(session, event.sequence.parse().unwrap()).unwrap();
    assert!(store.reserve("repo", 1, 1, 1).is_ok());
}

#[test]
fn operational_failure_reasons_are_filtered_before_persistence() {
    let store = Store::memory().unwrap();
    let session = Uuid::new_v4();
    let work = Uuid::new_v4();
    store.append_event(session,work,"0","work.state",json!({"previous":"preparing","state":"finalizing","reason":"OPENAI_API_KEY=sk-private /Users/person/private"})).unwrap();
    let persisted = store.pending_events(session).unwrap()[0]
        .payload
        .to_string();
    assert!(!persisted.contains("sk-private"));
    assert!(!persisted.contains("/Users/person"));
}
