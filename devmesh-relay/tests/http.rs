use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use devmesh_relay::{
    http::{router, AppState},
    MemoryStore, Relay,
};
use std::{path::PathBuf, sync::Arc};
use tower::ServiceExt;

#[tokio::test]
async fn http_requires_auth_and_does_not_expose_account_secrets() {
    let relay = Arc::new(Relay::new(Arc::new(MemoryStore::default())));
    let enrollment = relay.bootstrap("admin", "test-password").unwrap();
    let app = router(AppState::new(relay, PathBuf::from("web/dist")));
    let unauthenticated = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/me")
                .header("authorization", format!("Bearer {}", enrollment.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(!text.contains("password_hash"));
    assert!(!text.contains("test-password"));
}

#[tokio::test]
async fn historical_issue_comments_are_available_without_a_runtime_session() {
    use serde_json::json;
    let relay = Arc::new(Relay::new(Arc::new(MemoryStore::default())));
    let enrollment = relay.bootstrap("admin", "test-password").unwrap();
    let actor = relay.authenticate(&enrollment.token).unwrap();
    let project = relay
        .create_project(
            &actor,
            json!({"instance_id":"11111111-1111-4111-8111-111111111111","repository_id":42}),
        )
        .unwrap();
    relay
        .record(
            &actor,
            "issue_snapshots",
            "issue",
            json!({"id":"issue","project_id":project["id"],"title":"No Session"}),
            "issue.changed",
        )
        .unwrap();
    relay
        .comment(&actor, "issue", "Keep this historical note", None)
        .unwrap();
    let app = router(AppState::new(relay, PathBuf::from("web/dist")));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/issues/issue")
                .header("authorization", format!("Bearer {}", enrollment.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let issue: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(issue["comments"][0]["message"], "Keep this historical note");
    assert_eq!(issue["permission_flags"]["comment"], true);
}

#[tokio::test]
async fn revoking_the_bootstrap_device_revokes_its_original_key() {
    let relay = Arc::new(Relay::new(Arc::new(MemoryStore::default())));
    let enrollment = relay.bootstrap("admin", "test-password").unwrap();
    let device_actor = relay.authenticate(&enrollment.token).unwrap();
    relay.set_device_online(&device_actor, true).unwrap();
    let app = router(AppState::new(relay.clone(), PathBuf::from("web/dist")));
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/admin/devices/{}", enrollment.device_id))
                .header("authorization", format!("Bearer {}", enrollment.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(relay.authenticate(&enrollment.token).is_err());
    assert_eq!(
        relay
            .store
            .get("devices", &enrollment.device_id)
            .unwrap()
            .unwrap()["online"],
        false
    );
}

#[tokio::test]
async fn event_history_pages_backwards_from_an_immutable_sequence_boundary() {
    use serde_json::json;
    let relay = Arc::new(Relay::new(Arc::new(MemoryStore::default())));
    let enrollment = relay.bootstrap("admin", "test-password").unwrap();
    let actor = relay.authenticate(&enrollment.token).unwrap();
    let project = relay
        .create_project(
            &actor,
            json!({"instance_id":"11111111-1111-4111-8111-111111111111","repository_id":42}),
        )
        .unwrap();
    relay
        .record(
            &actor,
            "sessions",
            "s",
            json!({"project_id":project["id"],"session_id":"s"}),
            "session.changed",
        )
        .unwrap();
    for n in 1..=3 {
        relay.record(&actor,"session_events",&n.to_string(),json!({"project_id":project["id"],"session_id":"s","event_id":n.to_string(),"sequence":n.to_string()}),"session.event").unwrap();
    }
    let app = router(AppState::new(relay, PathBuf::from("web/dist")));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/sessions/s/events?order=desc&before_sequence=3&limit=1")
                .header("authorization", format!("Bearer {}", enrollment.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["items"][0]["sequence"], "2");
}

#[tokio::test]
async fn session_history_list_filters_work_issue_and_project_permissions() {
    use serde_json::{json, Value};
    let relay = Arc::new(Relay::new(Arc::new(MemoryStore::default())));
    let enrollment = relay.bootstrap("admin", "test-password").unwrap();
    let actor = relay.authenticate(&enrollment.token).unwrap();
    let project = relay
        .create_project(
            &actor,
            json!({"instance_id":"11111111-1111-4111-8111-111111111111","repository_id":42}),
        )
        .unwrap();
    let issue_ref = json!({"instance_id":"11111111-1111-4111-8111-111111111111","repository_id":42,"issue_number":7});
    for n in [1, 2] {
        relay.record(&actor,"sessions", &format!("s{n}"),json!({"project_id":project["id"],"session_id":format!("s{n}"),"work_id":format!("w{n}"),"issue_ref":issue_ref}),"session.changed").unwrap();
    }
    let other = relay
        .create_user(&actor, "unrelated", "test-password")
        .unwrap();
    let app = router(AppState::new(relay, PathBuf::from("web/dist")));
    for (token,query,count) in [(&enrollment.token,"work_id=w1",1),(&enrollment.token,"instance_id=11111111-1111-4111-8111-111111111111&repository_id=42&issue_number=7&limit=1",1),(&other.token,"work_id=w1",0)] {
        let response=app.clone().oneshot(Request::builder().uri(format!("/api/v1/sessions?{query}")).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(),StatusCode::OK);
        let body=axum::body::to_bytes(response.into_body(),10240).await.unwrap();
        let page:Value=serde_json::from_slice(&body).unwrap();
        assert_eq!(page["items"].as_array().unwrap().len(),count);
        assert!(page["as_of_cursor"].is_string());
    }
}

#[tokio::test]
async fn removing_an_offline_registration_deletes_its_keys_and_preserves_history() {
    use devmesh_relay::{Change, Repository};
    use serde_json::{json, Value};

    let store = Arc::new(MemoryStore::default());
    let relay = Arc::new(Relay::new(store.clone()));
    let enrollment = relay.bootstrap("admin", "test-password").unwrap();
    let actor = relay.authenticate(&enrollment.token).unwrap();
    let device = &enrollment.device_id;
    let work = json!({"id":"historical-work","device_id":device});
    store
        .commit(
            &[Change::put("work_records", "historical-work", work.clone())],
            &[],
        )
        .unwrap();

    let app = router(AppState::new(relay.clone(), PathBuf::from("web/dist")));
    let remove = || {
        app.clone().oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/admin/devices/{device}/registration"))
                .header("authorization", format!("Bearer {}", enrollment.token))
                .body(Body::empty())
                .unwrap(),
        )
    };

    relay.set_device_online(&actor, true).unwrap();
    assert_eq!(remove().await.unwrap().status(), StatusCode::CONFLICT);
    assert!(relay.authenticate(&enrollment.token).is_ok());

    relay.set_device_online(&actor, false).unwrap();
    let response = remove().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"removed":true})
    );
    assert!(store.get("devices", device).unwrap().is_none());
    assert!(store.list("api_keys").unwrap().is_empty());
    assert!(relay.authenticate(&enrollment.token).is_err());
    assert_eq!(
        store.get("work_records", "historical-work").unwrap(),
        Some(work)
    );
}
