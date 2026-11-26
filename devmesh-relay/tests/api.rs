use devmesh_relay::{MemoryStore, Relay};
use serde_json::json;
use std::sync::Arc;

#[test]
fn bootstrap_is_once_and_project_permissions_are_explicit() {
    let relay = Relay::new(Arc::new(MemoryStore::default()));
    let first = relay.bootstrap("admin", "test-password").unwrap();
    assert!(relay.bootstrap("second", "pw").is_err());
    let actor = relay.authenticate(&first.token).unwrap();
    let project=relay.create_project(&actor,json!({"instance_id":"11111111-1111-4111-8111-111111111111","repository_id":42,"name":"example/repo"})).unwrap();
    assert_eq!(relay.projects(&actor).unwrap().len(), 1);
    let user = relay.create_user(&actor, "viewer", "password").unwrap();
    let viewer = relay.authenticate(&user.token).unwrap();
    assert!(relay.projects(&viewer).unwrap().is_empty());
    relay
        .set_member(
            &actor,
            project["id"].as_str().unwrap(),
            &viewer.user_id,
            vec!["read".into()],
        )
        .unwrap();
    assert_eq!(relay.projects(&viewer).unwrap().len(), 1);
    assert!(!relay
        .permitted(&viewer, project["id"].as_str().unwrap(), "steer")
        .unwrap());
}

#[test]
fn password_login_creates_a_web_session_and_rejects_wrong_passwords() {
    let relay = Relay::new(Arc::new(MemoryStore::default()));
    relay.bootstrap("admin", "test-password").unwrap();
    let token = relay.login("admin", "test-password").unwrap();
    let actor = relay.authenticate(&token).unwrap();
    assert!(actor.admin);
    assert!(actor.device_id.is_none());
    assert_eq!(
        relay.login("admin", "wrong").unwrap_err().code,
        "unauthenticated"
    );
}

#[test]
fn project_creation_notifies_visible_clients_only_once() {
    let relay = Relay::new(Arc::new(MemoryStore::default()));
    let admin = relay.bootstrap("admin", "password").unwrap();
    let actor = relay.authenticate(&admin.token).unwrap();
    let viewer = relay.create_user(&actor, "viewer", "password").unwrap();
    let viewer = relay.authenticate(&viewer.token).unwrap();
    let input = json!({"instance_id":"11111111-1111-4111-8111-111111111111","repository_id":42});
    let project = relay.create_project(&actor, input.clone()).unwrap();
    assert_eq!(
        relay.create_project(&actor, input).unwrap()["id"],
        project["id"]
    );
    let events = relay.feed(&actor, 0, None).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["kind"], "project.changed");
    assert_eq!(events[0]["project_id"], project["id"]);
    assert!(relay.feed(&viewer, 0, None).unwrap().is_empty());
}
