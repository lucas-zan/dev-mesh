//! One local process owns scheduling, Work routing and native Runtime instances.
use crate::{
    config::Config,
    gitea::ClaimContext,
    mcp::{Context, McpServer},
    runtime::Runtime,
    session::ManagedWork,
    storage::Store,
    worker::Worker,
};
use devmesh_protocol::{Error, Frame, Result, WorkState};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::Mutex,
};
use uuid::Uuid;

pub type WorkMap = Arc<Mutex<HashMap<String, Arc<ManagedWork>>>>;
pub async fn serve(config: Config, prefix: PathBuf) -> Result<()> {
    std::fs::create_dir_all(&config.state_dir)
        .map_err(|e| Error::new("daemon_error", e.to_string()))?;
    let _ownership = crate::recovery::lock_daemon(&config.state_dir)?;
    let path = config.state_dir.join("daemon.sock");
    if path.exists() {
        use std::os::unix::fs::FileTypeExt;
        if !std::fs::symlink_metadata(&path)
            .map_err(|e| Error::new("daemon_error", e.to_string()))?
            .file_type()
            .is_socket()
        {
            return Err(Error::new(
                "daemon_error",
                "daemon.sock is not an owned socket",
            ));
        }
        if tokio::net::UnixStream::connect(&path).await.is_ok() {
            return Err(Error::new(
                "already_running",
                "A server is listening on the daemon socket",
            ));
        }
        std::fs::remove_file(&path).map_err(|e| Error::new("daemon_error", e.to_string()))?;
    }
    let listener=tokio::net::UnixListener::bind(&path).map_err(|e|Error::new("daemon_error",format!("Cannot bind local daemon socket: {e}; use worker status before removing a stale socket")))?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| Error::new("daemon_error", e.to_string()))?;
    let store = Arc::new(Store::open(&config.state_dir.join("agent.db"))?);
    crate::recovery::mark_interrupted(&store)?;
    let works: WorkMap = Arc::new(Mutex::new(HashMap::new()));
    let controller = tokio::spawn(crate::relay_bridge::run_queue(store.clone(), works.clone()));
    let shutdown = Arc::new(tokio::sync::Notify::new());
    let bridge_config = config.clone();
    let bridge_store = store.clone();
    let bridge_works = works.clone();
    let bridge = tokio::spawn(async move {
        crate::relay_bridge::run(bridge_config, bridge_store, bridge_works).await;
    });
    let task = tokio::spawn(schedule(
        config.clone(),
        prefix.clone(),
        store.clone(),
        works.clone(),
        path.clone(),
    ));
    loop {
        tokio::select! {
            _=shutdown.notified()=>break,
            signal=tokio::signal::ctrl_c()=>{signal.map_err(|e|Error::new("daemon_error",e.to_string()))?;break;},
            accepted=listener.accept()=>{
                let(stream,_)=accepted.map_err(|e|Error::new("daemon_error",e.to_string()))?;
                let works=works.clone();let store=store.clone();let shutdown=shutdown.clone();let config=config.clone();let prefix=prefix.clone();
                tokio::spawn(async move{let _=handle_ipc(stream,works,store,shutdown,config,prefix).await;});
            }
        }
    }
    task.abort();
    let active = works.lock().await.values().cloned().collect::<Vec<_>>();
    for work in active {
        work.closing.store(true, Ordering::SeqCst);
        *work.finish.lock().await = Some((WorkState::Cancelled, "daemon stopped".into()));
    }
    while !works.lock().await.is_empty() {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    controller.abort();
    bridge.abort();
    std::fs::remove_file(path).map_err(|e| Error::new("daemon_error", e.to_string()))?;
    Ok(())
}
async fn schedule(
    config: Config,
    prefix: PathBuf,
    store: Arc<Store>,
    works: WorkMap,
    socket: PathBuf,
) {
    loop {
        let config = match config.resolved().await {
            Ok(config) => config,
            Err(error) => {
                eprintln!("Repository discovery: {error}");
                tokio::time::sleep(std::time::Duration::from_secs(
                    config.worker.poll_interval_seconds.max(1),
                ))
                .await;
                continue;
            }
        };
        let paused = store.setting("paused").ok().flatten() == Some(json!(true));
        if config.worker.enabled {
            for (index, project) in config.projects.iter().enumerate() {
                let service = match config.service(index) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("{e}");
                        continue;
                    }
                };
                if let Err(error) = service.reconcile_pending().await {
                    eprintln!("Reconcile: {error}");
                }
                if paused || !project.auto_claim {
                    continue;
                }
                let user = match service.client.user().await {
                    Ok(u) => u,
                    Err(e) => {
                        eprintln!("{e}");
                        continue;
                    }
                };
                for role in project.roles.iter().filter(|r| config.roles.contains(r)) {
                    let worker = Arc::new(Worker {
                        store: store.clone(),
                        service: service.clone(),
                        project: project.worker_project(&config.state_dir),
                        max_concurrency: config.worker.max_concurrency,
                    });
                    let ctx = ClaimContext {
                        owner: user["id"].as_u64().unwrap_or(0),
                        device_id: config.device_id,
                        worker_id: Uuid::new_v4(),
                        role: role.clone(),
                        runtime: project.runtime.clone(),
                    };
                    let candidates = match worker.candidates(&ctx).await {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("{e}");
                            continue;
                        }
                    };
                    for number in candidates {
                        if store.setting("paused").ok().flatten() == Some(json!(true)) {
                            break;
                        }
                        match worker.prepare(number, &ctx).await {
                            Ok(prepared) => {
                                let project = project.clone();
                                let config = config.clone();
                                let prefix = prefix.clone();
                                let works = works.clone();
                                let socket = socket.clone();
                                let worker = worker.clone();
                                tokio::spawn(async move {
                                    if let Err(error) = run_work(
                                        worker, prepared, project, config, prefix, works, socket,
                                    )
                                    .await
                                    {
                                        eprintln!("{error}");
                                    }
                                });
                            }
                            Err(e) if e.code == "capacity_full" => break,
                            Err(e) => eprintln!("{e}"),
                        }
                    }
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(
            config.worker.poll_interval_seconds.max(1),
        ))
        .await;
    }
}
async fn run_work(
    worker: Arc<Worker>,
    prepared: crate::worker::PreparedWork,
    project: crate::config::Project,
    config: Config,
    prefix: PathBuf,
    works: WorkMap,
    socket: PathBuf,
) -> Result<()> {
    let token = Uuid::new_v4().to_string();
    let previous = worker.store.work(prepared.work_id)?;
    let resume = if previous["resume_requested"] == true {
        Some(
            previous["native_state"]["resume_ref"]
                .as_str()
                .ok_or_else(|| {
                    Error::new(
                        "unsupported_capability",
                        "Native resume reference is missing",
                    )
                })?
                .to_owned(),
        )
    } else {
        None
    };
    let epoch_base = previous["native_state"]["epoch"].as_u64().unwrap_or(0);
    let mut launch = project.launch(&prefix, &prepared.workspace, resume.clone())?;
    launch
        .env
        .push(("DEVMESH_RUN_EPOCH_BASE".into(), epoch_base.to_string()));
    launch.env.extend([
        (
            "DEVMESH_APPROVAL_TIMEOUT_SECONDS".into(),
            config.worker.approval_timeout_seconds.to_string(),
        ),
        (
            "DEVMESH_INPUT_TIMEOUT_SECONDS".into(),
            config.worker.input_timeout_seconds.to_string(),
        ),
        (
            "DEVMESH_SOCKET".into(),
            socket.to_string_lossy().into_owned(),
        ),
        ("DEVMESH_WORK_TOKEN".into(), token.clone()),
    ]);
    if project.runtime == "codex" {
        let command = prefix.join("bin/devmesh");
        let settings = [
            format!("mcp_servers.devmesh.command={}", json!(command)),
            "mcp_servers.devmesh.args=[\"mcp\"]".into(),
            format!("mcp_servers.devmesh.env.DEVMESH_SOCKET={}", json!(socket)),
            format!(
                "mcp_servers.devmesh.env.DEVMESH_WORK_TOKEN={}",
                json!(token)
            ),
        ];
        let mut args = Vec::new();
        for setting in settings {
            args.extend(["-c".into(), setting]);
        }
        args.push("app-server".into());
        launch.args = args;
    }
    let (runtime, mut events) = match Runtime::launch(&launch).await {
        Ok(v) => v,
        Err(e) => {
            let failed = crate::runtime::NativeState {
                status: "failed".into(),
                capabilities: json!({"message_events":false,"tool_events":false,"steer":false,"follow_up":false,"prompt":false,"interrupt":false,"local_approval":false,"local_input":false,"resume":false,"child_sessions":false}),
                ..Default::default()
            };
            worker.store.update_work(
                prepared.work_id,
                &json!({"runtime_instance_id":failed.instance}),
            )?;
            register(&worker, &prepared, &config, &project, &failed).await?;
            worker
                .finalize(&prepared, WorkState::Failed, &e.to_string(), true)
                .await?;
            return Err(e);
        }
    };
    if project.runtime != "pi-durable" {
        runtime.state.lock().await.epoch = epoch_base;
    }
    let work = Arc::new(ManagedWork::new(
        worker.clone(),
        prepared.clone(),
        Arc::new(runtime),
        project.test_commands.clone(),
    ));
    works.lock().await.insert(token.clone(), work.clone());
    let result:Result<()>=async {
    let identity=work.runtime.process_identity().await?;
    worker.store.update_work(prepared.work_id,&json!({"process_identity":identity,"runtime_instance_id":work.runtime.state.lock().await.instance}))?;
    register(&worker,&prepared,&config,&project,&work.runtime.state.lock().await.clone()).await?;
    let initial=format!("Use DevMesh collaboration skill at {}. Read repository AGENTS.md and use issue_get to read the full Issue body. Complete this Issue's acceptance criteria using controlled DevMesh tools. Work context: {}. Requirements:\n{}",prefix.join("current/share/devmesh/skills/devmesh/SKILL.md").display(),json!({"work_id":prepared.work_id,"issue_ref":worker.service.repo.issue_ref(prepared.issue_number),"role":prepared.metadata["required_role"],"dependency_snapshot":prepared.dependency_snapshot,"resuming":resume.is_some()}),prepared.metadata);
    worker.store.update_work(prepared.work_id,&json!({"start_requested":true}))?;
    work.runtime
        .start(crate::runtime::Start {
            work: prepared.work_id,
            session: prepared.session_id,
            cwd: &prepared.workspace,
            issue: &worker.service.repo.issue_ref(prepared.issue_number),
            message: &initial,
            model: project.model.as_deref(),
            resume: resume.as_deref(),
        })
        .await?;
    let native = work.runtime.state.lock().await.clone();
    worker
        .mark_running(
            &prepared,
            native.instance,
            native.native_id.as_deref().unwrap_or(""),
        )
        .await?;
    worker.store.update_work(prepared.work_id,&json!({"native_state":native,"recovery_required":false,"resume_requested":false}))?;
    worker.store.queue_frame(&Frame::new("session.status",json!({"session_id":prepared.session_id,"runtime_instance_id":native.instance,"native_session_id":native.native_id,"state":native.status,"run_epoch":native.epoch.to_string(),"remote_control":{"enabled":config.remote_control.enabled,"allow_steer":true,"allow_follow_up":true,"allow_prompt":true,"allow_interrupt":config.remote_control.allow_interrupt},"observed_at":devmesh_protocol::now()})))?;
    let mut idle = tokio::time::Instant::now();
    loop {
        if let Some((state, reason)) = work.finish.lock().await.take() {
            work.closing.store(true, Ordering::SeqCst);
            let stopped = work.runtime.stop().await?;
            worker.finalize(&prepared, state, &reason, stopped).await?;
            break;
        }
        tokio::select! {
            value=events.recv()=>{
                match value {
                    Ok(event)=>{
                        if event["type"]=="runtime.disconnected" {worker.finalize(&prepared,WorkState::Failed,"runtime disconnected; quiescence unknown",false).await?;break;}
                        if event["type"]=="command.native_receipt"{continue;}
                        let native=work.runtime.state.lock().await.clone();
                        worker.store.append_event(prepared.session_id,prepared.work_id,&native.epoch.to_string(),event["type"].as_str().unwrap_or("x.native"),event["payload"].clone())?;
                        worker.store.update_work(prepared.work_id,&json!({"native_state":native}))?;idle=tokio::time::Instant::now();
                    },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count))=>{worker.store.append_event(prepared.session_id,prepared.work_id,"0","capture.gap",json!({"reason":"local_event_buffer","dropped_count":count,"source_range":null}))?;},
                    Err(_)=>{worker.finalize(&prepared,WorkState::Failed,"event stream ended",false).await?;break;}
                }
            },
            _=tokio::time::sleep(std::time::Duration::from_millis(200))=>{
                let state=work.runtime.state.lock().await.clone();
                let seconds=idle.elapsed().as_secs();
                let deadline=match state.status.as_str(){"idle"=>config.worker.idle_grace_seconds,"waiting_approval"=>config.worker.approval_timeout_seconds,"waiting_input"=>config.worker.input_timeout_seconds,_=>u64::MAX};
                if seconds>=deadline {*work.finish.lock().await=Some((WorkState::Blocked,format!("{} timeout or missing delivery",state.status)));}
            }
        }
    }
    Ok(())
    }.await;
    works.lock().await.remove(&token);
    if let Err(error) = &result {
        work.closing.store(true, Ordering::SeqCst);
        let state = worker.store.work(prepared.work_id)?;
        if matches!(
            state["state"].as_str(),
            Some("active" | "preparing" | "recovery_required")
        ) {
            let quiesced = work.runtime.stop().await.unwrap_or(false);
            let _ = worker
                .finalize(&prepared, WorkState::Failed, &error.to_string(), quiesced)
                .await;
        }
        worker.store.update_work(
            prepared.work_id,
            &json!({"recovery_reason":error.to_string()}),
        )?;
    }
    result
}
async fn register(
    worker: &Worker,
    work: &crate::worker::PreparedWork,
    config: &Config,
    project: &crate::config::Project,
    native: &crate::runtime::NativeState,
) -> Result<()> {
    let mut registration = json!({"work_id":work.work_id,"project_id":null,"issue_ref":worker.service.repo.issue_ref(work.issue_number),"claim_id":work.claim["claim_id"],"claim_generation":work.claim["generation"],"role":work.metadata["required_role"],"state":"preparing","started_at":devmesh_protocol::now()});
    if !work.issue_source.is_null() {
        registration["source"] = work.issue_source.clone();
    }
    worker
        .store
        .queue_frame(&Frame::new("work.register", registration))?;
    worker.store.queue_frame(&Frame::new("session.register",json!({"session_id":work.session_id,"work_id":work.work_id,"parent_session_id":null,"runtime":project.runtime,"runtime_version":native.runtime_version,"adapter_version":env!("CARGO_PKG_VERSION"),"runtime_instance_id":native.instance,"native_session_id":native.native_id,"capabilities":native.capabilities,"remote_control":{"enabled":config.remote_control.enabled,"allow_steer":true,"allow_follow_up":true,"allow_prompt":true,"allow_interrupt":config.remote_control.allow_interrupt},"state":native.status})))?;
    Ok(())
}
async fn handle_ipc(
    stream: tokio::net::UnixStream,
    works: WorkMap,
    store: Arc<Store>,
    shutdown: Arc<tokio::sync::Notify>,
    config: Config,
    prefix: PathBuf,
) -> Result<()> {
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| Error::new("ipc_error", e.to_string()))?
    {
        let request: Value = serde_json::from_str(&line)
            .map_err(|e| Error::new("invalid_request", e.to_string()))?;
        let handled: Result<Value> = async {
            let response = if request["method"] == "local/work" {
                start_manual(&request, &config, &prefix, &works, &store).await?
            } else if request["method"] == "local/recover" {
                recover_work(&request, &config, &prefix, &works, &store).await?
            } else if request["method"]
                .as_str()
                .unwrap_or("")
                .starts_with("local/")
            {
                local(&request, &works, &store, &shutdown).await?
            } else {
                let token = request["token"].as_str().unwrap_or("");
                let work = works
                    .lock()
                    .await
                    .get(token)
                    .cloned()
                    .ok_or_else(|| Error::new("permission_denied", "Unbound Work token"))?;
                let config = config.resolved().await?;
                let bound = work.worker.service.repo.repository_id;
                let projects = config
                    .projects
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.repository.repository_id != bound)
                    .map(|(index, _)| config.service(index))
                    .collect::<Result<_>>()?;
                let context = Context {
                    directory: None,
                    projects,
                    service: Some(work.worker.service.clone()),
                    claim: None,
                    role: work.prepared.metadata["required_role"]
                        .as_str()
                        .unwrap_or("")
                        .into(),
                    can_prepare: false,
                    work: Some(work),
                    history: crate::history::configured(config.relay.as_ref()),
                };
                McpServer::new(context)
                    .handle(request.clone())
                    .await
                    .unwrap_or(Value::Null)
            };
            Ok(response)
        }
        .await;
        let response = match handled {
            Ok(value) => value,
            Err(error) => {
                json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32000,"message":error.message,"data":{"code":error.code}}})
            }
        };
        write
            .write_all(format!("{response}\n").as_bytes())
            .await
            .map_err(|e| Error::new("ipc_error", e.to_string()))?;
    }
    Ok(())
}
async fn local(
    request: &Value,
    works: &WorkMap,
    store: &Store,
    shutdown: &tokio::sync::Notify,
) -> Result<Value> {
    let result = match request["method"].as_str().unwrap_or("") {
        "local/status" => {
            json!({"works":store.active()?,"paused":store.setting("paused")?.unwrap_or(json!(false)),"catalog":store.setting("catalog:last_status")?})
        }
        "local/pause" => {
            store.set_setting("paused", &json!(true))?;
            json!({"paused":true})
        }
        "local/resume" => {
            store.set_setting("paused", &json!(false))?;
            json!({"paused":false})
        }
        "local/stop" => {
            shutdown.notify_one();
            json!({"stopping":true})
        }
        "local/approvals" => {
            let active = works.lock().await.values().cloned().collect::<Vec<_>>();
            let mut result = Vec::new();
            for work in active {
                let native = work.runtime.state.lock().await;
                for (id, request) in &native.approvals {
                    result.push(
                        json!({"work_id":work.prepared.work_id,"approval_id":native.approval_bindings.get(id).map(|binding|binding.id.to_string()).unwrap_or_else(||id.clone()),"request":request}),
                    );
                }
            }
            json!(result)
        }
        "local/resolve" => {
            let active = works.lock().await.values().cloned().collect::<Vec<_>>();
            let work = active
                .iter()
                .find(|w| json!(w.prepared.work_id) == request["params"]["work_id"])
                .ok_or_else(|| Error::new("not_found", "Work not active"))?;
            work.runtime
                .resolve(
                    request["params"]["approval_id"].as_str().unwrap_or(""),
                    request["params"]["decision"].as_str().unwrap_or("deny"),
                    request["params"]["text"].as_str(),
                    request["params"].get("answers"),
                )
                .await?;
            json!({"accepted":true})
        }
        _ => return Err(Error::new("invalid_request", "Unknown local method")),
    };
    Ok(json!({"jsonrpc":"2.0","id":request["id"],"result":result}))
}
pub async fn request_local(path: &Path, request: &Value) -> Result<Value> {
    let mut stream = tokio::net::UnixStream::connect(path)
        .await
        .map_err(|e| Error::new("daemon_unavailable", e.to_string()))?;
    stream
        .write_all(format!("{request}\n").as_bytes())
        .await
        .map_err(|e| Error::new("ipc_error", e.to_string()))?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .await
        .map_err(|e| Error::new("ipc_error", e.to_string()))?;
    serde_json::from_str(&line).map_err(|e| Error::new("ipc_error", e.to_string()))
}

pub fn manual_role(
    meta: &Value,
    device_roles: &[String],
    project_roles: &[String],
) -> Result<String> {
    let role = meta["required_role"]
        .as_str()
        .ok_or_else(|| Error::new("invalid_metadata", "Issue role missing"))?
        .to_owned();
    if !device_roles.contains(&role) || !project_roles.contains(&role) {
        return Err(Error::new(
            "permission_denied",
            "Issue role is not enabled on this device/project",
        ));
    }
    Ok(role)
}
async fn start_manual(
    request: &Value,
    config: &Config,
    prefix: &Path,
    works: &WorkMap,
    store: &Arc<Store>,
) -> Result<Value> {
    let config = config.resolved().await?;
    let index = crate::discovery::repository_index(
        &config,
        request["params"]["repository_id"].as_u64(),
        request["params"]["project"]
            .as_u64()
            .map(|value| value as usize),
    )?;
    let project = config
        .projects
        .get(index)
        .ok_or_else(|| Error::new("invalid_target", "Project missing"))?
        .clone();
    let number = request["params"]["number"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| Error::new("invalid_request", "Positive Issue number required"))?;
    let service = config.service(index)?;
    let metadata = service.read(number).await?.1;
    let role = manual_role(&metadata, &config.roles, &project.roles)?;
    let user = service.client.user().await?;
    let ctx = ClaimContext {
        owner: user["id"]
            .as_u64()
            .ok_or_else(|| Error::new("invalid_response", "Gitea user ID missing"))?,
        device_id: config.device_id,
        worker_id: Uuid::new_v4(),
        role,
        runtime: project.runtime.clone(),
    };
    let worker = Arc::new(Worker {
        store: store.clone(),
        service,
        project: project.worker_project(&config.state_dir),
        max_concurrency: config.worker.max_concurrency,
    });
    let prepared = worker.prepare(number, &ctx).await?;
    let work_id = prepared.work_id;
    let config = config.clone();
    let prefix = prefix.to_owned();
    let works = works.clone();
    let socket = config.state_dir.join("daemon.sock");
    tokio::spawn(async move {
        if let Err(error) = run_work(worker, prepared, project, config, prefix, works, socket).await
        {
            eprintln!("{error}");
        }
    });
    Ok(json!({"jsonrpc":"2.0","id":request["id"],"result":{"work_id":work_id,"state":"preparing"}}))
}

async fn recover_work(
    request: &Value,
    config: &Config,
    prefix: &Path,
    works: &WorkMap,
    store: &Arc<Store>,
) -> Result<Value> {
    let config = config.resolved().await?;
    let id: Uuid = request["params"]["work_id"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| Error::new("invalid_request", "Work UUID required"))?;
    if works
        .lock()
        .await
        .values()
        .any(|work| work.prepared.work_id == id)
    {
        return Err(Error::new(
            "invalid_state",
            "Work already has a live supervisor",
        ));
    }
    let record = store.work(id)?;
    let state: WorkState = serde_json::from_value(record["state"].clone())
        .map_err(|_| Error::new("storage_error", "Unknown Work state"))?;
    if state.terminal() {
        return Err(Error::new("invalid_state", "Terminal Work cannot recover"));
    }
    let (index, project) = config
        .projects
        .iter()
        .enumerate()
        .find(|(_, project)| project.worker_project(&config.state_dir).key == record["project"])
        .ok_or_else(|| Error::new("invalid_target", "Original project is no longer configured"))?;
    let service = config.service(index)?;
    if record["prepared"].is_null() {
        return settle_claim(request, id, &record, &service, store).await;
    }
    if record["start_requested"] == true
        && !crate::recovery::original_exited(&record["process_identity"])?
    {
        return Err(Error::new(
            "recovery_required",
            "Original Runtime is still running; no second process will start",
        ));
    }
    let prepared: crate::worker::PreparedWork = serde_json::from_value(record["prepared"].clone())
        .map_err(|_| Error::new("storage_error", "Prepared Work missing"))?;
    let worker = Arc::new(Worker {
        store: store.clone(),
        service,
        project: project.worker_project(&config.state_dir),
        max_concurrency: config.worker.max_concurrency,
    });
    if request["params"]["resume"] != true {
        let target = record
            .get("finalization_target")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or(WorkState::Blocked);
        worker
            .finalize(
                &prepared,
                target,
                record["finalization_reason"]
                    .as_str()
                    .unwrap_or("local recovery confirmed original Runtime exited"),
                true,
            )
            .await?;
        return Ok(
            json!({"jsonrpc":"2.0","id":request["id"],"result":{"work_id":id,"state":target}}),
        );
    }
    if state == WorkState::Finalizing {
        return Err(Error::new(
            "invalid_state",
            "Finalizing Work must settle before any new execution",
        ));
    }
    worker
        .service
        .verify_claim(prepared.issue_number, &prepared.claim)
        .await?;
    manual_role(&prepared.metadata, &config.roles, &project.roles)?;
    if worker
        .service
        .resolve_dependencies(&prepared.metadata)
        .await?
        != prepared.dependency_snapshot
    {
        return Err(Error::new(
            "revalidate",
            "Dependency versions changed; settle and prepare a new Work",
        ));
    }
    if record["native_state"]["resume_ref"].as_str().is_none() {
        return Err(Error::new(
            "unsupported_capability",
            "Original Runtime did not publish a resume reference",
        ));
    }
    store.update_work(id, &json!({"resume_requested":true}))?;
    let project = project.clone();
    let config = config.clone();
    let prefix = prefix.to_owned();
    let works = works.clone();
    let socket = config.state_dir.join("daemon.sock");
    tokio::spawn(async move {
        if let Err(error) = run_work(worker, prepared, project, config, prefix, works, socket).await
        {
            eprintln!("Recovery: {error}");
        }
    });
    Ok(json!({"jsonrpc":"2.0","id":request["id"],"result":{"work_id":id,"state":"recovering"}}))
}
async fn settle_claim(
    request: &Value,
    id: Uuid,
    record: &Value,
    service: &crate::gitea::TaskService,
    store: &Store,
) -> Result<Value> {
    if request["params"]["resume"] == true {
        return Err(Error::new(
            "invalid_state",
            "No Runtime was prepared; settle the claim intent first",
        ));
    }
    let number = record["issue_number"].as_u64().unwrap();
    let (_, metadata) = service.read(number).await?;
    if record["state"] != "finalizing" {
        store.transition(id, WorkState::Finalizing)?;
    }
    if metadata["claim"]["claim_id"] == record["claim_request_id"]
        && metadata["claim"]["state"] == "active"
    {
        service
            .release(
                number,
                &metadata["claim"],
                "blocked",
                "claim intent recovered before native execution",
                true,
            )
            .await?;
    }
    store.transition(id, WorkState::Failed)?;
    Ok(
        json!({"jsonrpc":"2.0","id":request["id"],"result":{"work_id":id,"state":"failed","reason":"No native execution was started"}}),
    )
}
