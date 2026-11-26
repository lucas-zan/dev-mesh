use devmesh_protocol::{can_transition, metadata, replace_metadata, WorkState};
use serde_json::json;

#[test]
fn terminal_work_cannot_restart() {
    assert!(can_transition(WorkState::Active, WorkState::Finalizing));
    assert!(!can_transition(WorkState::Completed, WorkState::Active));
    assert!(!can_transition(WorkState::Active, WorkState::Completed));
}

#[test]
fn metadata_replacement_preserves_human_body_and_rejects_duplicates() {
    let body = "Human requirements\n";
    let value = json!({"schema_version":"1.1", "state":"backlog"});
    let updated = replace_metadata(body, &value).unwrap();
    assert!(updated.starts_with(body));
    assert_eq!(metadata(&updated).unwrap(), value);
    let twice = format!("{updated}\n{updated}");
    assert!(metadata(&twice).is_err());
    assert_eq!(replace_metadata(&updated, &value).unwrap(), updated);
}

#[test]
fn wire_sequences_and_utc_timestamps_are_canonical() {
    assert_eq!(
        devmesh_protocol::sequence("18446744073709551615").unwrap(),
        u64::MAX
    );
    assert!(devmesh_protocol::sequence("01").is_err());
    assert!(devmesh_protocol::sequence("18446744073709551616").is_err());
    assert_eq!(devmesh_protocol::unix_timestamp(0), "1970-01-01T00:00:00Z");
}
