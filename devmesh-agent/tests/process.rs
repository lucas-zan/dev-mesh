use devmesh_agent::runtime::process::Peer;
use serde_json::json;

#[tokio::test]
async fn stdio_correlates_replies_and_preserves_server_requests() {
    let script = r#"import sys,json
for line in sys.stdin:
 request=json.loads(line)
 print(json.dumps({'id':'approval-1','method':'item/commandExecution/requestApproval','params':{}}),flush=True)
 print(json.dumps({'id':request['id'],'result':{'pong':True}}),flush=True)
"#;
    let (peer, mut events) = Peer::spawn(
        "python3",
        &["-u".into(), "-c".into(), script.into()],
        std::path::Path::new("."),
        &[],
    )
    .unwrap();
    let result = peer.call(json!({"method":"ping"})).await.unwrap();
    assert_eq!(result["pong"], true);
    let event = events.recv().await.unwrap();
    assert_eq!(event["id"], "approval-1");
    peer.terminate().await.unwrap();
}

#[tokio::test]
async fn host_unknown_admission_remains_unknown_and_is_not_reported_as_rejection() {
    let script = r#"import sys,json
for line in sys.stdin:
 request=json.loads(line)
 print(json.dumps({'type':'response','request_id':request['request_id'],'ok':False,'error':{'code':'unknown_outcome','message':'Admission outcome is unknown'}}),flush=True)
"#;
    let (peer, _) = Peer::spawn(
        "python3",
        &["-u".into(), "-c".into(), script.into()],
        std::path::Path::new("."),
        &[],
    )
    .unwrap();
    let error = peer
        .call(json!({"type":"request","method":"steer"}))
        .await
        .unwrap_err();
    assert_eq!(error.code, "unknown_outcome");
    peer.terminate().await.unwrap();
}
