use devmesh_relay::{Actor, MemoryStore, Relay};
use serde_json::{json, Value};
use std::sync::Arc;
pub fn setup() -> (Relay, Actor, Value) {
    let relay = Relay::new(Arc::new(MemoryStore::default()));
    let initial = relay.bootstrap("admin", "test-only-password").unwrap();
    let actor = relay.authenticate(&initial.token).unwrap();
    let project=relay.create_project(&actor,json!({"instance_id":"11111111-1111-4111-8111-111111111111","repository_id":42,"name":"test/repo"})).unwrap();
    let mut work: Value = serde_json::from_str(include_str!(
        "../../../devmesh-protocol/fixtures/wire-work.json"
    ))
    .unwrap();
    work["payload"]["project_id"] = project["id"].clone();
    work["payload"]["issue_ref"] =
        json!({"instance_id":project["instance_id"],"repository_id":42,"issue_number":128});
    work["payload"]["state"] = json!("active");
    relay
        .register_work(&actor, work["payload"].clone())
        .unwrap();
    let mut session: Value = serde_json::from_str(include_str!(
        "../../../devmesh-protocol/fixtures/wire-session.json"
    ))
    .unwrap();
    session["payload"]["state"] = json!("running");
    let mut registered = relay
        .register_session(&actor, session["payload"].clone())
        .unwrap();
    relay.set_device_online(&actor, true).unwrap();
    let status = json!({"session_id":registered["session_id"],"runtime_instance_id":registered["runtime_instance_id"],"native_session_id":registered["native_session_id"],"run_epoch":"1","state":"running","remote_control":registered["remote_control"],"observed_at":devmesh_protocol::now()});
    relay.update_session(&actor, status).unwrap();
    registered["run_epoch"] = json!("1");
    (relay, actor, registered)
}
