use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use devmesh_agent::{relay_bridge::AgentLink, storage::Store};
use devmesh_protocol::{socket::Socket, Frame};
use devmesh_relay::{http::AppState, MemoryStore, Relay};
use serde_json::json;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tower::ServiceExt;

#[tokio::test]
async fn agent_uploads_real_wire_frames_and_acknowledges_after_relay_commit() {
    let relay = Arc::new(Relay::new(Arc::new(MemoryStore::default())));
    let enrolled = relay.bootstrap("admin", "synthetic-password").unwrap();
    let actor = relay.authenticate(&enrolled.token).unwrap();
    let instance = "11111111-1111-4111-8111-111111111111";
    let _empty = relay
        .create_project(
            &actor,
            json!({"instance_id":instance,"repository_id":7,"name":"test/unused"}),
        )
        .unwrap();
    assert!(relay.workspace_projects(&actor).unwrap().is_empty());
    let mut work: Frame = serde_json::from_str(include_str!(
        "../../devmesh-protocol/fixtures/wire-work.json"
    ))
    .unwrap();
    work.payload["project_id"] = serde_json::Value::Null;
    work.payload["issue_ref"] =
        json!({"instance_id":instance,"repository_id":42,"issue_number":128});
    work.payload["source"] = json!({"repository_name":"test/repo","issue_title":"Implement tests","issue_url":"https://git.test/test/repo/issues/128"});
    work.payload["state"] = json!("active");
    let mut session: Frame = serde_json::from_str(include_str!(
        "../../devmesh-protocol/fixtures/wire-session.json"
    ))
    .unwrap();
    session.payload["state"] = json!("running");
    let sid = session.payload["session_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let wid = work.payload["work_id"].as_str().unwrap().parse().unwrap();
    let store = Arc::new(Store::memory().unwrap());
    store
        .set_setting("catalog:last_status", &json!({"state":"needs_refresh"}))
        .unwrap();
    store
        .queue_frame(&Frame::new(
            "catalog.acquire",
            json!({"project_id":_empty["id"]}),
        ))
        .unwrap();
    store.queue_frame(&work).unwrap();
    store.queue_frame(&session).unwrap();
    store
        .append_event(sid, wid, "1", "turn.started", json!({"run_epoch":"1"}))
        .unwrap();
    store
        .append_event(
            sid,
            wid,
            "1",
            "message.completed",
            devmesh_agent::collector::sanitize("Implemented tests", "agent"),
        )
        .unwrap();
    let state = AppState::new(relay.clone(), PathBuf::new());
    let state_for_task = state.clone();
    let key = enrolled.token.clone();
    let actor_for_task = actor.clone();
    let (a, b) = tokio::io::duplex(131072);
    let server = tokio::spawn(async move {
        devmesh_relay::connection::run(state_for_task, key, actor_for_task, Socket::new(b, true))
            .await
    });
    let client_store = store.clone();
    let device = enrolled.device_id.parse().unwrap();
    let client = tokio::spawn(async move {
        AgentLink::new(device, client_store)
            .transfer(
                Socket::new(a, false),
                Arc::new(tokio::sync::Mutex::new(HashMap::new())),
                Default::default(),
            )
            .await
    });
    for _ in 0..100 {
        if store.pending_events(sid).unwrap().is_empty() {
            break;
        }
        if client.is_finished() {
            panic!("client stopped: {:?}", client.await);
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        store.pending_events(sid).unwrap().is_empty(),
        "events must be ACKed"
    );
    let events = relay.events(&actor, &sid.to_string()).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(
        store.setting("catalog:last_status").unwrap(),
        Some(json!(null))
    );
    assert!(!store
        .pending_frames()
        .unwrap()
        .iter()
        .any(|frame| frame["type"] == "catalog.acquire"));
    assert!(relay
        .store
        .get("catalog_leases", _empty["id"].as_str().unwrap())
        .unwrap()
        .is_none());
    assert_eq!(events[1]["payload"]["text"], "Implemented tests");
    assert_eq!(
        relay.visible(&actor, "sessions", &sid.to_string()).unwrap()["run_epoch"],
        "1"
    );
    let projects = relay.workspace_projects(&actor).unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["name"], "test/repo");
    let issues = relay
        .workspace_issues(&actor, projects[0]["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0]["title"], "Implement tests");
    assert_eq!(issues[0]["issue_ref"], work.payload["issue_ref"]);
    let app = devmesh_relay::http::router(state);
    let request = Request::builder()
        .uri("/api/v1/projects?with_sessions=true")
        .header("Authorization", format!("Bearer {}", enrolled.token))
        .body(Body::empty())
        .unwrap();
    let reply = app.oneshot(request).await.unwrap();
    assert_eq!(reply.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(reply.into_body(), 65536).await.unwrap()).unwrap();
    assert_eq!(body["items"], json!(projects));
    client.abort();
    server.abort();
}
