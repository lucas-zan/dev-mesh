use devmesh_agent::runtime::durable_approval;
use uuid::Uuid;
#[test]
fn local_durable_approval_keeps_work_session_epoch_and_decision_bound() {
    let work = Uuid::new_v4();
    let session = Uuid::new_v4();
    let request = durable_approval(work, session, 7, "approval-1", "approve", None).unwrap();
    assert_eq!(request["method"], "resolve_approval");
    assert_eq!(request["work_id"], work.to_string());
    assert_eq!(request["session_id"], session.to_string());
    assert_eq!(request["expected_epoch"], "7");
    assert_eq!(request["args"]["decision"], "approve");
    assert!(durable_approval(work, session, 7, "approval-1", "approve", Some("input")).is_err());
}

#[tokio::test]
async fn rust_adapter_initializes_the_real_durable_host_and_stops_quiescently() {
    use devmesh_agent::runtime::{Launch, Runtime, RuntimeKind};
    let dir = tempfile::tempdir().unwrap();
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("packages/pi-durable-host/src/cli.ts");
    let config = Launch {
        kind: RuntimeKind::PiDurable,
        codex_unattended: false,
        program: "node".into(),
        args: vec![script.to_string_lossy().into_owned()],
        env: vec![],
        cwd: dir.path().to_owned(),
        model: None,
        resume: None,
    };
    let (runtime, _) = Runtime::launch(&config).await.unwrap();
    let result = runtime
        .start(devmesh_agent::runtime::Start {
            work: Uuid::new_v4(),
            session: Uuid::new_v4(),
            cwd: dir.path(),
            issue: &serde_json::json!({}),
            message: "",
            model: None,
            resume: None,
        })
        .await
        .unwrap();
    assert!(result["native_session_id"].as_str().is_some(), "{result}");
    let state = runtime.state.lock().await.clone();
    assert_eq!(state.runtime_version, "1.6.0");
    assert_eq!(state.capabilities["local_input"], false);
    assert!(runtime.stop().await.unwrap());
}
