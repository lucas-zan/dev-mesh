use devmesh_agent::{storage::Store, workspace::Workspace};
use devmesh_protocol::WorkState;
use tempfile::tempdir;
use uuid::Uuid;

#[test]
fn reserved_slots_are_atomic_and_waiting_keeps_capacity() {
    let store = Store::memory().unwrap();
    let work = store.reserve("repo", 128, 1, 1).unwrap();
    assert!(store.reserve("repo", 129, 1, 1).is_err());
    store.transition(work, WorkState::Active).unwrap();
    assert!(store.reserve("repo", 129, 1, 1).is_err());
    assert!(store.transition(work, WorkState::Completed).is_err());
    store.transition(work, WorkState::Finalizing).unwrap();
    store.transition(work, WorkState::Completed).unwrap();
    assert!(store.reserve("repo", 129, 1, 1).is_ok());
}

#[test]
fn restart_preserves_nonterminal_work_and_immutable_issue() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("agent.db");
    let id = {
        let store = Store::open(&path).unwrap();
        store.reserve("repo", 128, 2, 1).unwrap()
    };
    let store = Store::open(&path).unwrap();
    assert_eq!(store.work(id).unwrap()["issue_number"], 128);
    assert!(store.reserve("repo", 128, 2, 1).is_err());
    assert_eq!(store.active().unwrap().len(), 1);
}

#[test]
fn worktree_keeps_dirty_main_directory_and_pins_base() {
    use std::process::Command;
    let dir = tempdir().unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-b", "main"]);
    git(&["config", "user.email", "test@example.invalid"]);
    git(&["config", "user.name", "test"]);
    std::fs::write(dir.path().join("file"), "base").unwrap();
    git(&["add", "file"]);
    git(&["commit", "-m", "base"]);
    std::fs::write(dir.path().join("file"), "user changes").unwrap();
    let work = Workspace::create_local(
        dir.path(),
        "HEAD",
        128,
        Uuid::new_v4(),
        &dir.path().join("trees"),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("file")).unwrap(),
        "user changes"
    );
    assert_eq!(
        std::fs::read_to_string(work.path.join("file")).unwrap(),
        "base"
    );
    assert!(!work.base_sha.is_empty());
}

#[test]
fn workspace_preflight_clones_only_when_requested_and_reuses_existing_checkout() {
    use devmesh_agent::workspace::{ensure_repository, git};
    let dir = tempdir().unwrap();
    let remote = dir.path().join("remote");
    std::fs::create_dir(&remote).unwrap();
    git(&remote, &["init", "-b", "main"]).unwrap();
    git(
        &remote,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "base",
        ],
    )
    .unwrap();
    let checkout = dir.path().join("checkout");
    assert!(ensure_repository(&checkout, remote.to_str().unwrap(), false).is_err());
    assert!(!checkout.exists());
    ensure_repository(&checkout, remote.to_str().unwrap(), true).unwrap();
    std::fs::write(checkout.join("human-note"), "preserve").unwrap();
    ensure_repository(&checkout, remote.to_str().unwrap(), true).unwrap();
    assert_eq!(
        std::fs::read_to_string(checkout.join("human-note")).unwrap(),
        "preserve"
    );
    assert_eq!(
        git(&checkout, &["rev-parse", "HEAD"]).unwrap(),
        git(&remote, &["rev-parse", "HEAD"]).unwrap()
    );
}

#[test]
fn qa_worktree_revision_uses_the_fixed_dependency_candidate() {
    use devmesh_agent::worker::candidate_revision;
    use serde_json::json;
    let meta = json!({"required_role":"qa"});
    let snapshot = json!([{"dependency":{"producer_issue":{"repository_id":42},"condition":"pr_ready"},"head_sha":"a".repeat(40)}]);
    assert_eq!(
        candidate_revision(&meta, snapshot.as_array().unwrap(), 42).unwrap(),
        Some("a".repeat(40))
    );
    let conflict = json!([snapshot[0],{"dependency":{"producer_issue":{"repository_id":42},"condition":"pr_ready"},"head_sha":"b".repeat(40)}]);
    assert!(candidate_revision(&meta, conflict.as_array().unwrap(), 42).is_err());
    assert_eq!(
        candidate_revision(
            &json!({"required_role":"backend"}),
            snapshot.as_array().unwrap(),
            42
        )
        .unwrap(),
        None
    );
}
