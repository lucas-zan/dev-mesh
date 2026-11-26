use devmesh_agent::{config::Config, runtime::Runtime};
use serde_json::{json, Value};
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

fn write_config(path: &Path, runtime: &str, policy: Option<Value>) {
    let mut project = json!({
        "repository": {"instance_id":"22222222-2222-4222-8222-222222222222","repository_id":2,"owner":"team","name":"repo"},
        "local_path":path.parent().unwrap(), "roles":["product"], "runtime":runtime,
        "default_branch":"main","concurrency":1,"auto_claim":true,"remote":"https://git.example/team/repo.git"
    });
    if let Some(policy) = policy {
        project["codex_unattended"] = policy;
    }
    let config = json!({
        "schema_version":1, "device_id":"11111111-1111-4111-8111-111111111111",
        "roles":["product"], "state_dir":path.parent().unwrap(), "projects":[project],
        "gitea":{"endpoint":"https://git.example","credential_ref":"env:UNUSED"}
    });
    fs::write(path, config.to_string()).unwrap();
}

fn fake_codex(prefix: &Path) {
    let path = prefix.join("toolchains/codex/node_modules/.bin/codex");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        r#"#!/usr/bin/env python3
import json, pathlib, sys
if '--version' in sys.argv:
    print('test-codex')
    sys.exit()
for line in sys.stdin:
    request = json.loads(line)
    if 'id' not in request:
        continue
    result = {}
    if request['method'] in ('thread/start', 'thread/resume'):
        pathlib.Path('thread-request.json').write_text(line)
        result = {'thread': {'id': 'test-thread'}}
    print(json.dumps({'id': request['id'], 'result': result}), flush=True)
"#,
    )
    .unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[tokio::test]
async fn codex_uses_project_policy_for_new_and_resumed_threads() {
    let dir = tempfile::tempdir().unwrap();
    fake_codex(dir.path());
    let path = dir.path().join("config.json");
    for (setting, resume, sandbox, policy) in [
        (None, None, "workspace-write", "on-request"),
        (
            Some(json!(false)),
            Some("test-thread"),
            "workspace-write",
            "on-request",
        ),
        (Some(json!(true)), None, "danger-full-access", "never"),
        (
            Some(json!(true)),
            Some("test-thread"),
            "danger-full-access",
            "never",
        ),
    ] {
        write_config(&path, "codex", setting);
        let config = Config::read(&path).unwrap();
        let launch = config.projects[0]
            .launch(dir.path(), dir.path(), resume.map(str::to_owned))
            .unwrap();
        let (runtime, _) = Runtime::launch(&launch).await.unwrap();
        let request: Value =
            serde_json::from_slice(&fs::read(dir.path().join("thread-request.json")).unwrap())
                .unwrap();
        assert_eq!(
            request["method"],
            if resume.is_some() {
                "thread/resume"
            } else {
                "thread/start"
            }
        );
        assert_eq!(request["params"]["threadId"], json!(resume));
        assert_eq!(request["params"]["sandbox"], sandbox);
        assert_eq!(request["params"]["approvalPolicy"], policy);
        assert!(runtime.stop().await.unwrap());
    }
}

#[test]
fn unattended_setting_rejects_other_runtimes_and_invalid_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    for (runtime, setting) in [
        ("pi", json!(true)),
        ("pi-durable", json!(true)),
        ("codex", json!("true")),
    ] {
        write_config(&path, runtime, Some(setting));
        assert_eq!(Config::read(&path).err().unwrap().code, "config_error");
    }
}
