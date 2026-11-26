mod common;
use serde_json::json;
#[test]
fn commands_are_idempotent_and_old_epochs_and_final_work_are_rejected() {
    let (relay, actor, session) = common::setup();
    let request = json!({"type":"steer","target":{"project_id":session["project_id"],"work_id":session["work_id"],"session_id":session["session_id"],"runtime_instance_id":session["runtime_instance_id"],"expected_run_epoch":"1","claim_generation":1},"message":"keep API stable","ttl_seconds":30});
    let first = relay
        .command(
            &actor,
            session["session_id"].as_str().unwrap(),
            "same-key",
            request.clone(),
        )
        .unwrap();
    let second = relay
        .command(
            &actor,
            session["session_id"].as_str().unwrap(),
            "same-key",
            request.clone(),
        )
        .unwrap();
    assert_eq!(first["command_id"], second["command_id"]);
    let mut changed = request.clone();
    changed["message"] = json!("different");
    assert_eq!(
        relay
            .command(
                &actor,
                session["session_id"].as_str().unwrap(),
                "same-key",
                changed
            )
            .unwrap_err()
            .code,
        "idempotency_conflict"
    );
    let mut stale = request;
    stale["target"]["expected_run_epoch"] = json!("0");
    assert_eq!(
        relay
            .command(
                &actor,
                session["session_id"].as_str().unwrap(),
                "old",
                stale
            )
            .unwrap_err()
            .code,
        "stale_turn"
    );
    let receipt = json!({"command_id":first["command_id"],"receipt_seq":"1","state":"accepted","disposition":"queued","reason_code":null,"evidence_ref":null,"at":devmesh_protocol::now()});
    relay.receipt(&actor, receipt).unwrap();
    assert_eq!(
        relay
            .command_history(&actor, session["session_id"].as_str().unwrap())
            .unwrap()[0]["state"],
        "accepted"
    );
}

#[test]
fn reconnect_replays_only_undelivered_commands_and_expiration_is_observable() {
    let (relay, actor, session) = common::setup();
    let request = json!({"type":"steer","target":{"project_id":session["project_id"],"work_id":session["work_id"],"session_id":session["session_id"],"runtime_instance_id":session["runtime_instance_id"],"expected_run_epoch":"1","claim_generation":1},"message":"keep API stable","ttl_seconds":30});
    let mut command = relay
        .command(
            &actor,
            session["session_id"].as_str().unwrap(),
            "reconnect",
            request,
        )
        .unwrap();
    assert_eq!(relay.pending_commands(&actor).unwrap().len(), 1);
    command["expires_at"] = json!("2000-01-01T00:00:00Z");
    relay
        .store
        .commit(
            &[devmesh_relay::Change::put(
                "commands",
                command["command_id"].as_str().unwrap(),
                command.clone(),
            )],
            &[],
        )
        .unwrap();
    assert!(relay.pending_commands(&actor).unwrap().is_empty());
    assert_eq!(
        relay
            .command_history(&actor, session["session_id"].as_str().unwrap())
            .unwrap()[0]["state"],
        "expired"
    );
}

#[test]
fn expiry_waits_for_device_evidence_after_dispatch_has_started() {
    let (relay, actor, session) = common::setup();
    let request = json!({"type":"steer","target":{"project_id":session["project_id"],"work_id":session["work_id"],"session_id":session["session_id"],"runtime_instance_id":session["runtime_instance_id"],"expected_run_epoch":"1","claim_generation":1},"message":"task","ttl_seconds":30});
    let mut command = relay
        .command(
            &actor,
            session["session_id"].as_str().unwrap(),
            "uncertain-delivery",
            request,
        )
        .unwrap();
    relay
        .mark_dispatched(command["command_id"].as_str().unwrap())
        .unwrap();
    command["dispatched"] = json!(true);
    command["expires_at"] = json!("2000-01-01T00:00:00Z");
    relay
        .store
        .commit(
            &[devmesh_relay::Change::put(
                "commands",
                command["command_id"].as_str().unwrap(),
                command.clone(),
            )],
            &[],
        )
        .unwrap();
    let pending = relay.pending_commands(&actor).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["state"], "pending");
}
