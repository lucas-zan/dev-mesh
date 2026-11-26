use std::io::Write;
use std::process::{Command, Stdio};
#[test]
fn help_lists_runtime_install_and_local_worker_commands() {
    let output = Command::new(env!("CARGO_BIN_EXE_devmesh"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    for command in [
        "install",
        "init",
        "doctor",
        "worker",
        "mcp",
        "rollback",
        "authorize",
    ] {
        assert!(text.contains(command), "missing {command}");
    }
}
#[test]
fn standalone_mcp_initializes_without_secrets_and_stdout_is_json_only() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_devmesh"))
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n")
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["result"]["serverInfo"]["name"], "devmesh");
}

#[test]
fn local_inputs_require_one_explicit_answer_format() {
    use devmesh_agent::cli::input_reply;
    let args = |values: &[&str]| values.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        input_reply(&args(&["inputs", "reply", "--text", "DevMesh"])).unwrap(),
        serde_json::json!({"text":"DevMesh"})
    );
    assert_eq!(
        input_reply(&args(&[
            "inputs",
            "reply",
            "--answers-json",
            r#"{"q":["x"]}"#
        ]))
        .unwrap(),
        serde_json::json!({"answers":{"q":["x"]}})
    );
    assert!(input_reply(&args(&["inputs", "reply"])).is_err());
    assert!(input_reply(&args(&[
        "inputs",
        "reply",
        "--text",
        "x",
        "--answers-json",
        "{}"
    ]))
    .is_err());
}

#[test]
fn starting_worker_returns_without_holding_terminal_output_and_reports_its_log() {
    let dir = tempfile::tempdir().unwrap();
    let mut config: serde_json::Value =
        serde_json::from_str(include_str!("../../examples/config.json")).unwrap();
    config["state_dir"] = serde_json::json!(dir.path().join("state"));
    config["worker"]["enabled"] = serde_json::json!(true);
    config["projects"] = serde_json::json!([]);
    config["relay"] = serde_json::Value::Null;
    let path = dir.path().join("config.json");
    std::fs::write(&path, config.to_string()).unwrap();
    let cli = env!("CARGO_BIN_EXE_devmesh");
    let output = Command::new(cli)
        .args(["worker", "start", "--config"])
        .arg(&path)
        .output()
        .unwrap();
    // On unrestricted machines a daemon may have started. Always request cleanup before asserting.
    for _ in 0..50 {
        if Command::new(cli)
            .args(["worker", "stop", "--config"])
            .arg(&path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(output.status.success());
    let reply: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let log = reply["log"]
        .as_str()
        .expect("background startup must report its own log");
    assert!(std::path::Path::new(log).exists());
}
