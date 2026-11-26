use devmesh_agent::recovery::{original_exited, process_identity};
#[test]
#[ignore = "requires OS process inspection permission, unavailable in this sandbox"]
fn recovery_distinguishes_the_original_process_from_an_exited_or_reused_pid() {
    let mut child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .unwrap();
    let inspected = process_identity(child.id());
    if inspected.is_err() {
        child.kill().unwrap();
        child.wait().unwrap();
    }
    let identity = inspected.unwrap();
    assert!(!original_exited(&identity).unwrap());
    let mut reused = identity.clone();
    reused["start_time"] = serde_json::json!("different process start");
    assert!(original_exited(&reused).unwrap());
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(original_exited(&identity).unwrap());
}

#[test]
fn recovery_requires_exact_start_time_and_rejects_missing_identity() {
    use devmesh_agent::recovery::identity_exited;
    let identity = serde_json::json!({"pid":42,"start_time":"recorded start"});
    assert!(!identity_exited(&identity, Some("recorded start")).unwrap());
    assert!(identity_exited(&identity, Some("new process start")).unwrap());
    assert!(identity_exited(&identity, None).unwrap());
    assert!(identity_exited(&serde_json::json!({}), None).is_err());
}

#[test]
fn daemon_ownership_is_exclusive_and_restart_marks_work_for_explicit_recovery() {
    use devmesh_agent::{
        recovery::{lock_daemon, mark_interrupted},
        storage::Store,
    };
    use devmesh_protocol::WorkState;
    let dir = tempfile::tempdir().unwrap();
    let lock = lock_daemon(dir.path()).unwrap();
    assert!(lock_daemon(dir.path()).is_err());
    drop(lock);
    assert!(lock_daemon(dir.path()).is_ok());
    let store = Store::memory().unwrap();
    let id = store.reserve("repo", 1, 1, 1).unwrap();
    store.transition(id, WorkState::Active).unwrap();
    mark_interrupted(&store).unwrap();
    let work = store.work(id).unwrap();
    assert_eq!(work["state"], "recovery_required");
    assert_eq!(work["recovery_required"], true);
    assert!(store.reserve("repo", 1, 1, 1).is_err());
}

#[test]
fn interrupted_native_admission_is_marked_unknown_and_never_requeued() {
    use devmesh_agent::{recovery::mark_interrupted, storage::Store};
    use serde_json::json;
    let store = Store::memory().unwrap();
    let command = json!({"command_id":"command","type":"prompt","target":{}});
    store.journal("command", &command).unwrap();
    store.command_receipt("command","dispatching",&json!({"command_id":"command","receipt_seq":"1","state":"delivered","disposition":"none","reason_code":null,"evidence_ref":null,"at":devmesh_protocol::now()})).unwrap();
    mark_interrupted(&store).unwrap();
    let record = store.command_record("command").unwrap().unwrap();
    assert_eq!(record["state"], "unknown");
    assert_eq!(record["receipt"]["receipt_seq"], "2");
    assert!(store.queued_commands().unwrap().is_empty());
    assert!(!store.journal("command", &command).unwrap());
}
