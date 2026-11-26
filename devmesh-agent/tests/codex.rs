use devmesh_agent::runtime::{
    codex::{normalize, request},
    NativeState,
};
use serde_json::json;

#[test]
fn steer_matches_native_thread_and_turn_and_refuses_idle() {
    let state = NativeState {
        native_id: Some("thread-1".into()),
        turn_id: Some("turn-1".into()),
        status: "running".into(),
        epoch: 3,
        ..Default::default()
    };
    let value = request("steer", &state, "preserve compatibility", Some("3")).unwrap();
    assert_eq!(value["method"], "turn/steer");
    assert_eq!(value["params"]["expectedTurnId"], "turn-1");
    assert_eq!(value["params"]["threadId"], "thread-1");
    assert!(request("steer", &state, "text", Some("2")).is_err());
    let idle = NativeState {
        status: "idle".into(),
        ..state
    };
    assert!(request("steer", &idle, "text", Some("3")).is_err());
}
#[test]
fn visible_messages_and_approval_are_mapped_without_hidden_reasoning() {
    let mut state = NativeState::default();
    let start = normalize(
        &json!({"method":"turn/started","params":{"threadId":"thread-1","turn":{"id":"turn-1"}}}),
        &mut state,
    );
    assert_eq!(state.status, "running");
    assert!(!start.is_empty());
    let message = normalize(
        &json!({"method":"item/completed","params":{"item":{"type":"agentMessage","id":"m1","text":"Analysis complete"}}}),
        &mut state,
    );
    assert_eq!(message[0]["type"], "message.completed");
    let hidden = normalize(
        &json!({"method":"item/completed","params":{"item":{"type":"reasoning","text":"private reasoning"}}}),
        &mut state,
    );
    assert!(hidden.is_empty());
    normalize(
        &json!({"id":"approval-1","method":"item/commandExecution/requestApproval","params":{"threadId":"thread-1","turnId":"turn-1","command":"git status"}}),
        &mut state,
    );
    assert_eq!(state.status, "waiting_approval");
    assert!(state.approvals.contains_key("approval-1"));
}

#[tokio::test]
async fn a_completed_turn_observed_before_prompt_response_stays_idle() {
    use devmesh_agent::runtime::{Launch, Runtime, RuntimeKind, Start};
    let script = r#"import sys,json,time
for line in sys.stdin:
 r=json.loads(line)
 if 'id' not in r:continue
 result={}
 if r['method']=='thread/start':result={'thread':{'id':'thread'}}
 if r['method']=='turn/start':
  print(json.dumps({'method':'turn/started','params':{'threadId':'thread','turn':{'id':'turn','status':'inProgress'}}}),flush=True)
  print(json.dumps({'method':'turn/completed','params':{'threadId':'thread','turn':{'id':'turn','status':'completed'}}}),flush=True)
  time.sleep(.05)
  result={'turn':{'id':'turn','status':'completed'}}
 print(json.dumps({'id':r['id'],'result':result}),flush=True)
"#;
    let launch = Launch {
        kind: RuntimeKind::Codex,
        codex_unattended: false,
        program: "python3".into(),
        args: vec!["-u".into(), "-c".into(), script.into()],
        env: vec![],
        cwd: std::env::current_dir().unwrap(),
        model: None,
        resume: None,
    };
    let (runtime, _) = Runtime::launch(&launch).await.unwrap();
    runtime
        .start(Start {
            work: uuid::Uuid::new_v4(),
            session: uuid::Uuid::new_v4(),
            cwd: &launch.cwd,
            issue: &serde_json::json!({}),
            message: "task",
            model: None,
            resume: None,
        })
        .await
        .unwrap();
    assert_eq!(runtime.state.lock().await.status, "idle");
    assert!(runtime.stop().await.unwrap());
}

#[test]
fn permission_requests_grant_only_requested_permissions_for_one_turn() {
    use devmesh_agent::runtime::codex::approval_result;
    let request = json!({"method":"item/permissions/requestApproval","params":{"permissions":{"network":{"enabled":true}}}});
    let accepted = approval_result(&request, "approve", None).unwrap();
    assert_eq!(accepted["scope"], "turn");
    assert_eq!(accepted["permissions"], request["params"]["permissions"]);
    let denied = approval_result(&request, "deny", None).unwrap();
    assert_eq!(denied["permissions"], json!({}));
}

#[test]
fn local_multi_question_answers_are_complete_and_bound_to_native_question_ids() {
    use devmesh_agent::runtime::codex::{approval_answers, approval_result};
    let request = json!({"method":"item/tool/requestUserInput","params":{"questions":[{"id":"framework"},{"id":"database"}]}});
    let answers = r#"{"framework":["React"],"database":["PostgreSQL"]}"#;
    let result = approval_answers(&request, &serde_json::from_str(answers).unwrap()).unwrap();
    assert_eq!(
        result,
        json!({"answers":{"framework":{"answers":["React"]},"database":{"answers":["PostgreSQL"]}}})
    );
    for invalid in [
        r#"{"framework":["React"]}"#,
        r#"{"framework":["React"],"other":["PostgreSQL"]}"#,
        r#"{"framework":"React","database":["PostgreSQL"]}"#,
    ] {
        assert!(approval_answers(&request, &serde_json::from_str(invalid).unwrap()).is_err());
    }
    let single =
        json!({"method":"item/tool/requestUserInput","params":{"questions":[{"id":"name"}]}});
    assert_eq!(
        approval_result(&single, "reply", Some("DevMesh")).unwrap(),
        json!({"answers":{"name":{"answers":["DevMesh"]}}})
    );
}

#[test]
fn approval_and_input_deadlines_follow_worker_configuration() {
    let mut state = NativeState {
        approval_timeout_seconds: 17,
        input_timeout_seconds: 23,
        ..NativeState::default()
    };
    for (method, seconds) in [
        ("item/commandExecution/requestApproval", 17),
        ("item/tool/requestUserInput", 23),
    ] {
        let before = chrono::Utc::now();
        let events = devmesh_agent::runtime::codex::normalize(
            &json!({"method":method,"id":method,"params":{}}),
            &mut state,
        );
        let deadline = chrono::DateTime::parse_from_rfc3339(
            events[0]["payload"]["deadline"].as_str().unwrap(),
        )
        .unwrap();
        let delta = deadline.signed_duration_since(before).num_seconds();
        assert!((seconds - 1..=seconds).contains(&delta), "{delta}");
    }
}

#[test]
fn user_input_cannot_be_resolved_as_a_command_approval() {
    let request =
        json!({"method":"item/tool/requestUserInput","params":{"questions":[{"id":"q"}]}});
    assert!(devmesh_agent::runtime::codex::approval_result(&request, "approve", None).is_err());
}

#[test]
fn codex_terminal_rows_expose_safe_action_and_test_summary() {
    let mut state = NativeState::default();
    let started = normalize(
        &json!({"method":"item/started","params":{"item":{
            "id":"read","type":"commandExecution","command":"sed -n '1,80p' src/lib.rs","cwd":"/tmp/issue-worktree",
            "commandActions":[{"type":"read","path":"/tmp/issue-worktree/src/lib.rs","name":"lib.rs"}]
        }}}),
        &mut state,
    );
    assert_eq!(started[0]["payload"]["summary"], "读取 src/lib.rs");
    let completed = normalize(
        &json!({"method":"item/completed","params":{"item":{
            "id":"test","type":"commandExecution","command":"cargo test --workspace --offline","cwd":"/tmp/issue-worktree",
            "status":"completed","exitCode":1,"aggregatedOutput":"token=private-secret\ntest result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s"
        }}}),
        &mut state,
    );
    let payload = &completed[0]["payload"];
    assert_eq!(payload["summary"], "cargo test --workspace --offline");
    assert_eq!(payload["outcome"], "failed");
    assert_eq!(payload["exit_code"], 1);
    assert_eq!(payload["output"], "[已脱敏]\ntest result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s");
    assert_eq!(payload["content_policy"]["redacted"], true);
    assert!(!payload.to_string().contains("private"));
}

#[test]
fn codex_does_not_publish_arbitrary_command_arguments_or_private_paths() {
    let mut state = NativeState::default();
    let rows = normalize(
        &json!({"method":"item/completed","params":{"item":{
            "id":"read","type":"commandExecution","command":"cat /Users/a/.env","cwd":"/tmp/issue-worktree","status":"completed","exitCode":0,
            "commandActions":[{"type":"read","path":"/Users/a/.env"}],
            "aggregatedOutput":"SECRET=value\nOK"
        }}}),
        &mut state,
    );
    let payload = &rows[0]["payload"];
    assert_eq!(payload["summary"], "读取文件");
    assert!(payload.get("output").is_none());
    assert_eq!(payload["content_policy"]["omitted"], true);
    assert!(!payload.to_string().contains("/Users/"));
}

#[test]
fn codex_shares_the_visible_plan_with_step_statuses() {
    let mut state = NativeState::default();
    let rows = normalize(
        &json!({"method":"turn/plan/updated","params":{
            "turnId":"turn","plan":[
                {"step":"Read the project rules","status":"completed"},
                {"step":"Add regression tests","status":"inProgress"},
                {"step":"Verify changes","status":"pending"}
            ]
        }}),
        &mut state,
    );
    assert_eq!(rows[0]["type"], "plan.updated");
    assert_eq!(
        rows[0]["payload"],
        json!({"plan_id":"turn","steps":[
            {"id":"1","text":"Read the project rules","status":"done"},
            {"id":"2","text":"Add regression tests","status":"running"},
            {"id":"3","text":"Verify changes","status":"pending"}
        ]})
    );
}

#[test]
fn codex_shares_command_arguments_and_sanitized_failure_output() {
    let mut state = NativeState::default();
    let rows = normalize(
        &json!({"method":"item/completed","params":{"item":{
            "id":"test","type":"commandExecution","cwd":"/tmp/work",
            "command":"npm --workspace web test",
            "status":"completed","exitCode":1,
            "aggregatedOutput":"AssertionError: expected 741, got 752\nAPI_TOKEN=private-value\nFAIL admission-count.test.ts"
        }}}),
        &mut state,
    );
    let payload = &rows[0]["payload"];
    assert_eq!(payload["summary"], "npm --workspace web test");
    assert!(payload["output"]
        .as_str()
        .unwrap()
        .contains("expected 741, got 752"));
    assert!(payload["output"]
        .as_str()
        .unwrap()
        .contains("FAIL admission-count.test.ts"));
    assert!(!payload.to_string().contains("private-value"));
}

#[test]
fn codex_exposes_controlled_tool_failure_messages() {
    let mut state = NativeState::default();
    let rows = normalize(
        &json!({"method":"item/completed","params":{"item":{
            "id":"delivery","type":"mcpToolCall","tool":"work_report_delivery","status":"failed",
            "error":{"message":"Candidate has no passing test evidence. token=private"},
            "durationMs":10
        }}}),
        &mut state,
    );
    assert_eq!(
        rows[0]["payload"]["summary"],
        "Candidate has no passing test evidence. [已脱敏]"
    );
    assert_eq!(rows[0]["payload"]["outcome"], "failed");
    assert!(!rows[0].to_string().contains("private"));
}
