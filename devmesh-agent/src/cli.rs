use crate::{
    config::{credential, Config},
    daemon,
    gitea::ClaimContext,
    installer,
    mcp::{Context, McpServer},
};
use devmesh_protocol::{Error, Result};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use uuid::Uuid;

const HELP:&str="DevMesh\n\n  install --source BUNDLE [--prefix DIR] [--runtime all|none|codex,pi,pi-durable] [--offline]\n  init --from CONFIG.json [--config PATH]\n  configure [--config PATH]  Choose Worker roles\n  doctor [--config PATH] [--prefix DIR] [--verify]\n  worker start|pause|resume|status|stop|recover --work UUID [--resume] [--config PATH]\n  approvals list|resolve --work UUID --id ID --decision approve|deny\n  inputs reply --work UUID --id ID (--text TEXT | --answers-json JSON)\n  issue list|get|init|authorize|prepare|rework|unblock|reconcile|accept --number N [--project INDEX]\n  repos\n  work --number N --repository-id ID\n  mcp [--config PATH]\n  update --source BUNDLE [--prefix DIR]\n  rollback --version VERSION [--prefix DIR]\n  uninstall [--prefix DIR]\n";
pub fn option(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|p| p[1].clone())
}
fn required(args: &[String], name: &str) -> Result<String> {
    option(args, name).ok_or_else(|| Error::new("invalid_request", format!("{name} is required")))
}
fn default_prefix() -> PathBuf {
    std::env::var_os("DEVMESH_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share/devmesh")
        })
}
fn print(value: &Value) {
    println!("{}", serde_json::to_string_pretty(value).unwrap());
}

pub async fn run(args: Vec<String>) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("--help");
    if matches!(command, "--help" | "help" | "-h") {
        println!("{HELP}");
        return Ok(());
    }
    if command == "--version" {
        println!("devmesh {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let prefix = option(&args, "--prefix")
        .map(PathBuf::from)
        .unwrap_or_else(default_prefix);
    let config_path = option(&args, "--config")
        .map(PathBuf::from)
        .unwrap_or_else(|| prefix.join("config.json"));
    match command {
        "install" | "update" => {
            let previous = std::fs::read_link(prefix.join("current"))
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
            let version =
                installer::install_bundle(&prefix, Path::new(&required(&args, "--source")?))?;
            if !args.iter().any(|a| a == "--offline") {
                let requested = option(&args, "--runtime").unwrap_or("all".into());
                let runtimes = runtime_names(&requested)?;
                for runtime in runtimes {
                    if let Err(error) = installer::install_runtime(&prefix, runtime) {
                        installer::restore_current(&prefix, previous.as_deref())?;
                        return Err(error);
                    }
                }
                if let Err(error) = installer::install_official_mcp(&prefix) {
                    installer::restore_current(&prefix, previous.as_deref())?;
                    return Err(error);
                }
            }
            print(
                &json!({"installed":version,"prefix":prefix,"runtime_validation":"run devmesh doctor; installation is not E2E certification"}),
            );
            Ok(())
        }
        "rollback" => installer::rollback(&prefix, &required(&args, "--version")?),
        "uninstall" => installer::uninstall(&prefix),
        "init" => {
            if config_path.exists() && !args.iter().any(|a| a == "--replace") {
                return Err(Error::new(
                    "config_exists",
                    "Configuration exists; use --replace explicitly",
                ));
            }
            let mut config = Config::read(Path::new(&required(&args, "--from")?))?;
            let mcp = prefix.join("toolchains/gitea/bin/gitea-mcp");
            if config.gitea.mcp_program.as_deref() == Some("gitea-mcp") && mcp.is_file() {
                config.gitea.mcp_program = Some(mcp.to_string_lossy().into_owned());
            }
            config.save(&config_path)?;
            print(
                &json!({"configured":config_path,"worker_enabled":config.worker.enabled,"remote_control_enabled":config.remote_control.enabled,"credentials":"not checked; use doctor --verify at final validation"}),
            );
            Ok(())
        }
        "configure" => configure_roles(&prefix, &config_path).await,
        "doctor" => doctor(&prefix, &config_path, args.iter().any(|a| a == "--verify")).await,
        "mcp" => mcp(&config_path).await,
        "daemon" => daemon::serve(Config::read(&config_path)?, prefix).await,
        "worker" => worker_cli(&args, &config_path, &prefix).await,
        "approvals" | "inputs" => approval_cli(&args, &config_path).await,
        "issue" | "work" => issue_cli(command, &args, &config_path).await,
        "repos" => {
            let config = Config::read(&config_path)?.resolved().await?;
            print(&json!(config
                .projects
                .iter()
                .map(|project| &project.repository)
                .collect::<Vec<_>>()));
            Ok(())
        }
        _ => Err(Error::new(
            "invalid_request",
            format!("Unknown command {command}; use --help"),
        )),
    }
}
async fn configure_roles(prefix: &Path, path: &Path) -> Result<()> {
    let helper = prefix.join("current/setup_local.py");
    let status = tokio::process::Command::new("python3")
        .arg(helper)
        .arg("--roles-config")
        .arg(path)
        .status()
        .await
        .map_err(|error| Error::new("config_error", error.to_string()))?;
    if !status.success() {
        return Err(Error::new(
            "config_error",
            "Worker role setup did not complete",
        ));
    }
    Ok(())
}

async fn mcp(config_path: &Path) -> Result<()> {
    if let (Ok(path), Ok(token)) = (
        std::env::var("DEVMESH_SOCKET"),
        std::env::var("DEVMESH_WORK_TOKEN"),
    ) {
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        let mut output = tokio::io::stdout();
        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|e| Error::new("ipc_error", e.to_string()))?
        {
            let mut value: Value = serde_json::from_str(&line)
                .map_err(|e| Error::new("invalid_request", e.to_string()))?;
            if value.get("id").is_none() {
                continue;
            }
            value["token"] = json!(token);
            let response = daemon::request_local(Path::new(&path), &value).await?;
            output
                .write_all(format!("{response}\n").as_bytes())
                .await
                .map_err(|e| Error::new("ipc_error", e.to_string()))?;
        }
        return Ok(());
    }
    let context = if config_path.exists() {
        let config = Config::read(config_path)?;
        let client = config.client()?;
        let user = client.user().await?;
        let role = config.roles.first().cloned().unwrap_or("product".into());
        let runtime = config
            .repositories
            .as_ref()
            .map(|defaults| defaults.runtime.clone())
            .or_else(|| {
                config
                    .projects
                    .first()
                    .map(|project| project.runtime.clone())
            })
            .unwrap_or_default();
        let discovery_enabled = config.repositories.is_some();
        let can_prepare = discovery_enabled
            || config
                .projects
                .first()
                .is_some_and(|project| project.can_prepare);
        let mut services = config.projects.iter().map(|project| {
            crate::gitea::TaskService::new(client.clone(), project.repository.clone())
        });
        let service = if discovery_enabled {
            None
        } else {
            services.next()
        };
        Context {
            directory: config.repositories.as_ref().map(|_| {
                crate::discovery::RepositoryDirectory {
                    config: config.clone(),
                    client: client.clone(),
                }
            }),
            claim: Some(ClaimContext {
                owner: user["id"].as_u64().unwrap_or(0),
                device_id: config.device_id,
                worker_id: Uuid::new_v4(),
                role: role.clone(),
                runtime,
            }),
            projects: services.collect(),
            service,
            role,
            can_prepare,
            work: Some(std::sync::Arc::new(crate::discovery::LocalWorkActions {
                config: config.clone(),
            })),
            history: crate::history::configured(config.relay.as_ref()),
        }
    } else {
        Context::default()
    };
    McpServer::new(context).serve_stdio().await
}
async fn doctor(prefix: &Path, path: &Path, verify: bool) -> Result<()> {
    let mut runtimes = Vec::new();
    for (name, program, args) in [
        (
            "codex",
            prefix.join("toolchains/codex/node_modules/.bin/codex"),
            vec!["--version"],
        ),
        (
            "pi",
            prefix.join("toolchains/pi/node_modules/.bin/pi"),
            vec!["--version"],
        ),
        ("pi-durable", PathBuf::from("node"), vec!["--version"]),
    ] {
        let output = Command::new(&program).args(args).output();
        runtimes.push(match output{Ok(o)=>json!({"runtime":name,"installed":o.status.success()&&(name!="pi-durable"||prefix.join("current/share/devmesh/node_modules/@earendil-works/pi-durable/package.json").is_file()),"version":String::from_utf8_lossy(&o.stdout).trim(),"native_e2e":"not verified"}),Err(_)=>json!({"runtime":name,"installed":false,"reason":"executable unavailable","native_e2e":"not verified"})});
    }
    let mut report = json!({"platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,"runtimes":runtimes,"configured":path.exists()});
    if verify {
        let config = Config::read(path)?.resolved().await?;
        let user = config.client()?.user().await?;
        report["gitea"] =
            json!({"user_id":user["id"],"repository_count":config.projects.len(),"matches":true});
        let mut native = Vec::new();
        for project in &config.projects {
            if native
                .iter()
                .any(|item: &Value| item["runtime"] == project.runtime)
            {
                continue;
            }
            let cwd = config
                .state_dir
                .join("doctor")
                .join(Uuid::new_v4().to_string());
            std::fs::create_dir_all(&cwd)
                .map_err(|e| Error::new("diagnostic_failed", e.to_string()))?;
            let launch = project.launch(prefix, &cwd, None)?;
            let result=async {
                let (runtime,_)=crate::runtime::Runtime::launch(&launch).await?;
                if project.runtime=="pi-durable"{runtime.start(crate::runtime::Start{work:Uuid::new_v4(),session:Uuid::new_v4(),cwd:&cwd,issue:&json!({}),message:"",model:project.model.as_deref(),resume:None}).await?;}
                let state=runtime.state.lock().await.clone();let quiesced=runtime.stop().await?;
                Ok::<_,Error>(json!({"runtime":project.runtime,"handshake":true,"runtime_version":state.runtime_version,"capabilities":state.capabilities,"quiesced":quiesced,"native_e2e":"not verified"}))
            }.await;
            native.push(match result {Ok(value)=>value,Err(error)=>json!({"runtime":project.runtime,"handshake":false,"reason":error.code,"native_e2e":"not verified"})});
        }
        report["native_protocols"] = json!(native);
        if let Some(relay) = &config.relay {
            let key = credential(&relay.credential_ref)?;
            let response = reqwest::Client::new()
                .get(format!(
                    "{}/api/v1/me",
                    relay.endpoint.trim_end_matches('/')
                ))
                .bearer_auth(key)
                .send()
                .await
                .map_err(|_| Error::new("unavailable", "Relay verification failed"))?;
            report["relay"] = json!({"authenticated":response.status().is_success()});
        }
    }
    print(&report);
    Ok(())
}
async fn worker_cli(args: &[String], path: &Path, prefix: &Path) -> Result<()> {
    let config = Config::read(path)?;
    let action = args.get(1).map(String::as_str).unwrap_or("status");
    if action == "recover" {
        let response=daemon::request_local(&config.state_dir.join("daemon.sock"),&json!({"jsonrpc":"2.0","id":Uuid::new_v4(),"method":"local/recover","params":{"work_id":required(args,"--work")?,"resume":args.iter().any(|arg|arg=="--resume")}})).await?;
        print(&response);
        return Ok(());
    }
    if action == "start" {
        if !config.worker.enabled {
            return Err(Error::new(
                "worker_disabled",
                "Set worker.enabled after local authorization",
            ));
        }
        std::fs::create_dir_all(&config.state_dir)
            .map_err(|e| Error::new("daemon_error", e.to_string()))?;
        let log = config.state_dir.join("daemon.log");
        let output = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)
            .map_err(|e| Error::new("daemon_error", e.to_string()))?;
        let mut command = Command::new(
            std::env::current_exe().map_err(|e| Error::new("daemon_error", e.to_string()))?,
        );
        command
            .args(["daemon", "--config"])
            .arg(path)
            .arg("--prefix")
            .arg(prefix)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(output);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let child = command
            .spawn()
            .map_err(|e| Error::new("daemon_error", e.to_string()))?;
        print(
            &json!({"pid":child.id(),"state":"starting","log":log,"confirmation":"use worker status"}),
        );
        return Ok(());
    }
    let response = daemon::request_local(
        &config.state_dir.join("daemon.sock"),
        &json!({"jsonrpc":"2.0","id":Uuid::new_v4(),"method":format!("local/{action}")}),
    )
    .await?;
    print(&response);
    Ok(())
}
pub fn input_reply(args: &[String]) -> Result<Value> {
    let text = option(args, "--text");
    let answers = option(args, "--answers-json");
    if args.first().is_some_and(|v| v == "inputs") && (text.is_some() == answers.is_some()) {
        return Err(Error::new(
            "invalid_request",
            "Choose --text or --answers-json",
        ));
    }
    if let Some(answers) = answers {
        let value: Value = serde_json::from_str(&answers)
            .map_err(|_| Error::new("invalid_request", "Answers must be valid JSON"))?;
        return Ok(json!({"answers": value}));
    }
    Ok(json!({"text":text}))
}
async fn approval_cli(args: &[String], path: &Path) -> Result<()> {
    let config = Config::read(path)?;
    let action = args.get(1).map(String::as_str).unwrap_or("list");
    let (method, params) = if action == "list" {
        ("local/approvals", json!({}))
    } else {
        let mut params = input_reply(args)?;
        params["work_id"] = json!(required(args, "--work")?);
        params["approval_id"] = json!(required(args, "--id")?);
        params["decision"] = json!(option(args, "--decision").unwrap_or("deny".into()));
        ("local/resolve", params)
    };
    let response = daemon::request_local(
        &config.state_dir.join("daemon.sock"),
        &json!({"jsonrpc":"2.0","id":Uuid::new_v4(),"method":method,"params":params}),
    )
    .await?;
    print(&response);
    Ok(())
}
async fn issue_cli(command: &str, args: &[String], path: &Path) -> Result<()> {
    let config = Config::read(path)?;
    let config = config.resolved().await?;
    let repository = option(args, "--repository-id")
        .map(|value| value.parse::<u64>())
        .transpose()
        .map_err(|_| Error::new("invalid_request", "Numeric repository ID required"))?;
    let legacy = option(args, "--project")
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| Error::new("invalid_request", "Project index must be an integer"))?;
    let index = crate::discovery::repository_index(&config, repository, legacy)?;
    if command == "work" {
        let number = required(args, "--number")?
            .parse::<u64>()
            .map_err(|_| Error::new("invalid_request", "Positive Issue number required"))?;
        let response=daemon::request_local(&config.state_dir.join("daemon.sock"),&json!({"jsonrpc":"2.0","id":Uuid::new_v4(),"method":"local/work","params":{"repository_id":config.projects[index].repository.repository_id,"number":number}})).await?;
        if let Some(error) = response.get("error") {
            return Err(Error::new(
                error["data"]["code"].as_str().unwrap_or("work_failed"),
                error["message"].as_str().unwrap_or("Work failed"),
            ));
        }
        print(&response["result"]);
        return Ok(());
    }
    let service = config.service(index)?;
    let action = args.get(1).map(String::as_str).unwrap_or("list");
    if action == "list" {
        print(&json!(service.client.list(&service.repo, "all").await?));
        return Ok(());
    }
    if action == "init" {
        service.initialize_labels().await?;
        return Ok(());
    }
    let number = required(args, "--number")?
        .parse::<u64>()
        .map_err(|_| Error::new("invalid_request", "Issue number must be positive integer"))?;
    if action == "get" {
        print(&service.client.issue(&service.repo, number).await?);
        return Ok(());
    }
    if !config.projects[index].can_prepare {
        return Err(Error::new(
            "permission_denied",
            "Project policy does not allow preparing Issues",
        ));
    }
    if action == "accept" {
        print(&service.accept_delivery(number).await?);
        return Ok(());
    }
    if action == "authorize" {
        service.initialize_labels().await?;
        print(&service.authorize(number).await?);
        return Ok(());
    }
    if action == "reconcile" {
        print(&service.reconcile_merged(number).await?);
        return Ok(());
    }
    let state = if action == "rework" {
        "rework"
    } else {
        "ready"
    };
    print(
        &service
            .prepare(number, state, &option(args, "--reason").unwrap_or_default())
            .await?,
    );
    Ok(())
}

fn runtime_names(requested: &str) -> Result<Vec<&str>> {
    if requested == "none" {
        return Ok(Vec::new());
    }
    let names = if requested == "all" {
        vec!["codex", "pi", "pi-durable"]
    } else {
        requested.split(',').collect()
    };
    if names
        .iter()
        .any(|name| !matches!(*name, "codex" | "pi" | "pi-durable"))
    {
        return Err(Error::new("invalid_runtime", requested));
    }
    Ok(names)
}

#[cfg(test)]
mod runtime_selection_tests {
    #[test]
    fn runtime_downloads_support_none_and_selected_lists() {
        assert!(super::runtime_names("none").unwrap().is_empty());
        assert_eq!(
            super::runtime_names("codex,pi").unwrap(),
            vec!["codex", "pi"]
        );
        assert_eq!(
            super::runtime_names("all").unwrap(),
            vec!["codex", "pi", "pi-durable"]
        );
        assert!(super::runtime_names("other").is_err());
    }
}
