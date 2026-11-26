use crate::{events::field, Actor, Change, Relay};
use axum::{
    body::Body,
    extract::{Path, Query, Request, State},
    http::{header, HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{delete, get, post, put},
    Json, Router,
};
use devmesh_protocol::Error;
use futures_util::stream;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    convert::Infallible,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::{mpsc, Notify};
use uuid::Uuid;

#[derive(Clone)]
pub struct Connection {
    pub epoch: Uuid,
    pub output: mpsc::Sender<String>,
    pub stop: Arc<Notify>,
}
#[derive(Clone)]
pub struct AppState {
    pub relay: Arc<Relay>,
    pub connections: Arc<Mutex<HashMap<String, Connection>>>,
    pub web_dir: PathBuf,
}
impl AppState {
    pub fn new(relay: Arc<Relay>, web_dir: PathBuf) -> Self {
        Self {
            relay,
            connections: Arc::new(Mutex::new(HashMap::new())),
            web_dir,
        }
    }
}
pub struct ApiError(pub Error);
impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        Self(e)
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.0.code.as_str() {
            "unauthenticated" => StatusCode::UNAUTHORIZED,
            "permission_denied" | "remote_control_disabled" => StatusCode::FORBIDDEN,
            "not_found" => StatusCode::NOT_FOUND,
            "offline" | "unavailable" | "database_unavailable" => StatusCode::SERVICE_UNAVAILABLE,
            "invalid_request" | "invalid_schema" | "invalid_target" => StatusCode::BAD_REQUEST,
            "cursor_expired" | "command_expired" => StatusCode::GONE,
            _ => StatusCode::CONFLICT,
        };
        (status,Json(json!({"request_id":Uuid::new_v4(),"error":{"code":self.0.code,"message":self.0.message,"retryable":false}}))).into_response()
    }
}
type Api<T> = std::result::Result<T, ApiError>;
fn token(headers: &HeaderMap) -> devmesh_protocol::Result<String> {
    if let Some(auth) = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
    {
        return Ok(auth.into());
    }
    headers
        .get(header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| {
            s.split(';')
                .find_map(|p| p.trim().strip_prefix("devmesh_session="))
        })
        .map(str::to_owned)
        .ok_or_else(|| Error::new("unauthenticated", "Login required"))
}
fn actor(state: &AppState, headers: &HeaderMap) -> devmesh_protocol::Result<Actor> {
    state.relay.authenticate(&token(headers)?)
}
fn writable(headers: &HeaderMap) -> devmesh_protocol::Result<()> {
    if headers.get(header::AUTHORIZATION).is_some() {
        return Ok(());
    }
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split_once("://").map(|(_, host)| host));
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok());
    if origin.is_some() && origin == host {
        Ok(())
    } else {
        Err(Error::new(
            "permission_denied",
            "Same-origin request required",
        ))
    }
}
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { Json(json!({"status":"ok"})) }))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/me", get(me))
        .route("/api/v1/projects", get(projects))
        .route("/api/v1/projects/:id/issues", get(issues))
        .route("/api/v1/issues/:id", get(issue))
        .route("/api/v1/issues/:id/works", get(works))
        .route("/api/v1/issues/:id/comments", post(comment))
        .route("/api/v1/sessions", get(sessions))
        .route("/api/v1/sessions/:id", get(session))
        .route("/api/v1/sessions/:id/events", get(events))
        .route("/api/v1/sessions/:id/messages", get(events))
        .route("/api/v1/sessions/:id/commands", get(commands).post(command))
        .route("/api/v1/events/stream", get(feed))
        .route("/api/v1/sessions/:id/stream", get(session_feed))
        .route("/api/v1/devices", get(devices))
        .route(
            "/api/v1/subscriptions",
            get(subscriptions).put(save_subscriptions),
        )
        .route(
            "/api/v1/admin/:collection",
            post(admin_create).get(admin_list),
        )
        .route("/api/v1/admin/projects/:id/members", put(member))
        .route("/api/v1/admin/keys/:id", delete(revoke))
        .route("/api/v1/admin/devices/:id", delete(revoke_device))
        .route(
            "/api/v1/admin/devices/:id/registration",
            delete(remove_device),
        )
        .route("/ws/v1/connect", get(websocket))
        .fallback(static_file)
        .layer(axum::extract::DefaultBodyLimit::max(1024 * 1024))
        .with_state(state)
}
async fn login(State(state): State<AppState>, Json(body): Json<Value>) -> Api<Response> {
    let token = state
        .relay
        .login(field(&body, "username")?, field(&body, "password")?)?;
    let mut response = Json(json!({"authenticated":true})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        format!("devmesh_session={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=43200")
            .parse()
            .unwrap(),
    );
    Ok(response)
}
async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Api<Response> {
    writable(&headers)?;
    let token = token(&headers)?;
    let _ = state.relay.authenticate(&token)?;
    use sha2::{Digest, Sha256};
    let id = format!("{:x}", Sha256::digest(token.as_bytes()));
    if let Some(mut session) = state.relay.store.get("auth_sessions", &id)? {
        session["revoked"] = json!(true);
        state
            .relay
            .store
            .commit(&[Change::put("auth_sessions", id, session)], &[])?;
    }
    let mut response = Json(json!({"logged_out":true})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        "devmesh_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"
            .parse()
            .unwrap(),
    );
    Ok(response)
}
async fn me(State(state): State<AppState>, headers: HeaderMap) -> Api<Json<Value>> {
    let actor = actor(&state, &headers)?;
    let projects = state.relay.projects(&actor)?;
    let user = state.relay.store.get("users", &actor.user_id)?;
    let username = user.as_ref().and_then(|u| u["username"].as_str());
    let display_name = user
        .as_ref()
        .and_then(|u| u["display_name"].as_str())
        .or(username);
    Ok(Json(json!({
        "user_id": actor.user_id,
        "username": username,
        "display_name": display_name,
        "admin": actor.admin,
        "scopes": actor.scopes,
        "projects": projects
    })))
}
async fn projects(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Api<Json<Value>> {
    let _snapshot = state.relay.lock.lock().unwrap();
    let actor = actor(&state, &headers)?;
    let items = if query
        .get("with_sessions")
        .is_some_and(|value| value == "true")
    {
        state.relay.workspace_projects(&actor)?
    } else {
        state.relay.projects(&actor)?
    };
    Ok(Json(page(items, &query, state.relay.store.cursor()?)?))
}
async fn issues(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Api<Json<Value>> {
    let _snapshot = state.relay.lock.lock().unwrap();
    let actor = actor(&state, &headers)?;
    let issues = if query
        .get("with_sessions")
        .is_some_and(|value| value == "true")
    {
        state.relay.workspace_issues(&actor, &id)?
    } else {
        state.relay.issues(&actor, &id)?
    };
    let list = issues
        .into_iter()
        .filter(|i| {
            query.get("q").is_none_or(|q| {
                i["title"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&q.to_lowercase())
            }) && query.get("role").is_none_or(|r| i["role"] == *r)
                && query
                    .get("state")
                    .is_none_or(|r| i["canonical_state"] == *r)
        })
        .collect();
    Ok(Json(page(list, &query, state.relay.store.cursor()?)?))
}
async fn issue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Api<Json<Value>> {
    let _snapshot = state.relay.lock.lock().unwrap();
    let actor = actor(&state, &headers)?;
    let mut issue = state.relay.visible(&actor, "issue_snapshots", &id)?;
    let user_names: HashMap<String, String> = state
        .relay
        .store
        .list("users")?
        .into_iter()
        .filter_map(|u| {
            let uid = u["id"].as_str()?.to_owned();
            let name = u["display_name"]
                .as_str()
                .or_else(|| u["username"].as_str())?
                .to_owned();
            Some((uid, name))
        })
        .collect();
    issue["comments"] = json!(state
        .relay
        .store
        .list("comments")?
        .into_iter()
        .filter(|c| c["issue_id"] == id)
        .map(|mut c| {
            if let Some(name) = c["sender_id"].as_str().and_then(|uid| user_names.get(uid)) {
                c["sender_name"] = json!(name);
            }
            c
        })
        .collect::<Vec<_>>());
    issue["permission_flags"] =
        json!({"comment":state.relay.permitted(&actor,field(&issue,"project_id")?,"comment")?});
    issue["as_of_cursor"] = json!(state.relay.store.cursor()?.to_string());
    Ok(Json(issue))
}
async fn works(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Api<Json<Value>> {
    let _snapshot = state.relay.lock.lock().unwrap();
    let actor = actor(&state, &headers)?;
    state.relay.visible(&actor, "issue_snapshots", &id)?;
    let sessions = state.relay.store.list("sessions")?;
    let list = state
        .relay
        .store
        .list("work_records")?
        .into_iter()
        .filter(|w| w["issue_id"] == id)
        .map(|mut work| {
            work["sessions"] = json!(sessions
                .iter()
                .filter(|s| s["work_id"] == work["work_id"])
                .cloned()
                .collect::<Vec<_>>());
            work
        })
        .collect();
    Ok(Json(page(list, &query, state.relay.store.cursor()?)?))
}
async fn sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Api<Json<Value>> {
    let _snapshot = state.relay.lock.lock().unwrap();
    let actor = actor(&state, &headers)?;
    let reference = history_issue_ref(&query)?;
    let mut items = Vec::new();
    for session in state.relay.store.list("sessions")? {
        if query
            .get("work_id")
            .is_some_and(|id| session["work_id"] != *id)
            || reference
                .as_ref()
                .is_some_and(|r| session["issue_ref"] != *r)
        {
            continue;
        }
        if state
            .relay
            .permitted(&actor, field(&session, "project_id")?, "read")?
        {
            items.push(session);
        }
    }
    items.sort_by(|a, b| a["session_id"].as_str().cmp(&b["session_id"].as_str()));
    Ok(Json(page(items, &query, state.relay.store.cursor()?)?))
}
fn history_issue_ref(query: &HashMap<String, String>) -> devmesh_protocol::Result<Option<Value>> {
    let keys = ["instance_id", "repository_id", "issue_number"];
    let count = keys.iter().filter(|key| query.contains_key(**key)).count();
    if count == 0 {
        return Ok(None);
    }
    if count != keys.len() {
        return Err(Error::new(
            "invalid_request",
            "Complete Issue reference required",
        ));
    }
    let number = |key: &str| {
        query[key].parse::<u64>().map_err(|_| {
            Error::new(
                "invalid_request",
                "Issue reference numbers must be integers",
            )
        })
    };
    Ok(Some(
        json!({"instance_id":query["instance_id"],"repository_id":number("repository_id")?,"issue_number":number("issue_number")?}),
    ))
}
async fn session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Api<Json<Value>> {
    let _snapshot = state.relay.lock.lock().unwrap();
    let actor = actor(&state, &headers)?;
    let mut value = state.relay.visible(&actor, "sessions", &id)?;
    value["work"] = state.relay.visible(
        &actor,
        "work_records",
        value["work_id"].as_str().unwrap_or(""),
    )?;
    value["connection"] = state
        .relay
        .store
        .get("devices", value["device_id"].as_str().unwrap_or(""))?
        .map(|d| {
            if d["online"] == true {
                "online"
            } else {
                "offline"
            }
        })
        .map(|s| json!(s))
        .unwrap_or(json!("unknown"));
    value["permission_flags"] = json!({"read":true,"steer":state.relay.permitted(&actor,field(&value,"project_id")?,"steer")?,"interrupt":state.relay.permitted(&actor,field(&value,"project_id")?,"interrupt")?,"comment":state.relay.permitted(&actor,field(&value,"project_id")?,"comment")?});
    value["as_of_cursor"] = json!(state.relay.store.cursor()?.to_string());
    Ok(Json(value))
}
async fn events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Api<Json<Value>> {
    let _snapshot = state.relay.lock.lock().unwrap();
    let before = query
        .get("before_sequence")
        .map(|value| devmesh_protocol::sequence(value))
        .transpose()?;
    let mut events = state.relay.events(&actor(&state, &headers)?, &id)?;
    if let Some(before) = before {
        events.retain(|event| event["sequence"].as_str().unwrap().parse::<u64>().unwrap() < before);
    }
    if let Some(after) = query.get("after_sequence") {
        let after = devmesh_protocol::sequence(after)?;
        events.retain(|event| event["sequence"].as_str().unwrap().parse::<u64>().unwrap() > after);
    }
    match query.get("order").map(String::as_str).unwrap_or("asc") {
        "desc" => events.reverse(),
        "asc" => {}
        _ => return Err(Error::new("invalid_request", "Event order must be asc or desc").into()),
    }
    Ok(Json(page(events, &query, state.relay.store.cursor()?)?))
}
async fn commands(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Api<Json<Value>> {
    Ok(Json(page(
        state
            .relay
            .command_history(&actor(&state, &headers)?, &id)?,
        &query,
        state.relay.store.cursor()?,
    )?))
}
async fn command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Api<Response> {
    writable(&headers)?;
    let key = headers
        .get("Idempotency-Key")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| Error::new("invalid_request", "Idempotency-Key required"))?;
    let command = state
        .relay
        .command(&actor(&state, &headers)?, &id, key, body)?;
    if command["state"] == "pending"
        && command["expires_at"]
            .as_str()
            .is_some_and(|s| s > devmesh_protocol::now().as_str())
    {
        let device = field(&command, "device_id")?;
        let connection = state
            .connections
            .lock()
            .unwrap()
            .get(device)
            .cloned()
            .ok_or_else(|| Error::new("offline", "Device connection unavailable"))?;
        state
            .relay
            .mark_dispatched(field(&command, "command_id")?)?;
        connection
            .output
            .try_send(serde_json::to_string(&crate::commands::dispatch_frame(&command)).unwrap())
            .map_err(|_| Error::new("unavailable", "Device control queue unavailable"))?;
    }
    Ok((StatusCode::ACCEPTED, Json(command)).into_response())
}
async fn comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Api<Json<Value>> {
    writable(&headers)?;
    Ok(Json(state.relay.comment(
        &actor(&state, &headers)?,
        &id,
        field(&body, "message")?,
        body["work_id"].as_str(),
    )?))
}
async fn devices(State(state): State<AppState>, headers: HeaderMap) -> Api<Json<Value>> {
    let actor = actor(&state, &headers)?;
    Ok(Json(
        json!({"items":state.relay.store.list("devices")?.into_iter().filter(|d|d["owner"]==actor.user_id).collect::<Vec<_>>()}),
    ))
}
async fn subscriptions(State(state): State<AppState>, headers: HeaderMap) -> Api<Json<Value>> {
    let actor = actor(&state, &headers)?;
    Ok(Json(
        state
            .relay
            .store
            .get("subscriptions", &actor.user_id)?
            .unwrap_or(json!({"items":[]})),
    ))
}
async fn save_subscriptions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Api<Json<Value>> {
    writable(&headers)?;
    let actor = actor(&state, &headers)?;
    state.relay.store.commit(
        &[Change::put("subscriptions", actor.user_id, body.clone())],
        &[],
    )?;
    Ok(Json(body))
}
async fn admin_create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(collection): Path<String>,
    Json(body): Json<Value>,
) -> Api<Json<Value>> {
    writable(&headers)?;
    let actor = actor(&state, &headers)?;
    state.relay.admin(&actor)?;
    let value = match collection.as_str() {
        "projects" => state.relay.create_project(&actor, body)?,
        "users" => serde_json::to_value(state.relay.create_user(
            &actor,
            field(&body, "username")?,
            field(&body, "password")?,
        )?)
        .unwrap(),
        "keys" => {
            let user = body["user_id"].as_str().unwrap_or(&actor.user_id);
            if state.relay.store.get("users", user)?.is_none() {
                return Err(Error::new("not_found", "User missing").into());
            }
            let token = format!("dm_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
            let device = Uuid::new_v4().to_string();
            use sha2::{Digest, Sha256};
            let hash = format!("{:x}", Sha256::digest(token.as_bytes()));
            state.relay.store.commit(&[Change::put("api_keys",&hash,json!({"id":hash,"user_id":user,"device_id":device,"scopes":body.get("scopes").cloned().unwrap_or(json!(["read","ingest","catalog:write"])),"revoked":false})),Change::put("devices",&device,json!({"id":device,"device_id":device,"owner":user,"name":body["name"],"online":false}))],&[])?;
            json!({"key_id":hash,"key":token,"device_id":device})
        }
        _ => return Err(Error::new("not_found", "Admin collection missing").into()),
    };
    Ok(Json(value))
}
async fn admin_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(collection): Path<String>,
) -> Api<Json<Value>> {
    let actor = actor(&state, &headers)?;
    state.relay.admin(&actor)?;
    let table = match collection.as_str() {
        "users" => "users",
        "devices" => "devices",
        "projects" => "projects",
        "keys" => "api_keys",
        "audit" => "audit_logs",
        _ => return Err(Error::new("not_found", "Collection missing").into()),
    };
    let mut records = state.relay.store.list(table)?;
    for record in &mut records {
        if let Some(object) = record.as_object_mut() {
            for key in ["password_hash", "salt"] {
                object.remove(key);
            }
        }
    }
    Ok(Json(json!({"items":records})))
}
async fn member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Api<Json<Value>> {
    writable(&headers)?;
    let permissions = body["permissions"]
        .as_array()
        .ok_or_else(|| Error::new("invalid_request", "Permissions array required"))?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    state.relay.set_member(
        &actor(&state, &headers)?,
        &id,
        field(&body, "user_id")?,
        permissions,
    )?;
    Ok(Json(json!({"updated":true})))
}
async fn revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Api<Json<Value>> {
    writable(&headers)?;
    let actor = actor(&state, &headers)?;
    state.relay.admin(&actor)?;
    let mut key = state
        .relay
        .store
        .get("api_keys", &id)?
        .ok_or_else(|| Error::new("not_found", "Key missing"))?;
    key["revoked"] = json!(true);
    state
        .relay
        .store
        .commit(&[Change::put("api_keys", id, key.clone())], &[])?;
    if let Some(device) = key["device_id"].as_str() {
        state.relay.disconnect_device(device)?;
        if let Some(connection) = state.connections.lock().unwrap().remove(device) {
            connection.stop.notify_one();
        }
    }
    Ok(Json(json!({"revoked":true})))
}
async fn revoke_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Api<Json<Value>> {
    writable(&headers)?;
    let actor = actor(&state, &headers)?;
    state.relay.admin(&actor)?;
    let keys = state.relay.store.list("api_keys")?;
    let changes = keys
        .into_iter()
        .filter(|k| k["device_id"] == id)
        .filter_map(|mut key| {
            key["revoked"] = json!(true);
            let id = key["id"].as_str()?.to_owned();
            Some(Change::put("api_keys", id, key))
        })
        .collect::<Vec<_>>();
    state.relay.store.commit(&changes, &[])?;
    state.relay.disconnect_device(&id)?;
    if let Some(connection) = state.connections.lock().unwrap().remove(&id) {
        connection.stop.notify_one();
    }
    Ok(Json(json!({"revoked":true})))
}
async fn remove_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Api<Json<Value>> {
    writable(&headers)?;
    state.relay.remove_device(&actor(&state, &headers)?, &id)?;
    Ok(Json(json!({"removed":true})))
}
fn page(
    items: Vec<Value>,
    query: &HashMap<String, String>,
    cursor: u64,
) -> devmesh_protocol::Result<Value> {
    let offset = query
        .get("cursor")
        .map(|s| s.parse::<usize>())
        .transpose()
        .map_err(|_| Error::new("invalid_request", "Invalid pagination cursor"))?
        .unwrap_or(0);
    let limit = query
        .get("limit")
        .map(|s| s.parse::<usize>())
        .transpose()
        .map_err(|_| Error::new("invalid_request", "Invalid limit"))?
        .unwrap_or(50)
        .min(200);
    let total = items.len();
    Ok(
        json!({"items":items.into_iter().skip(offset).take(limit).collect::<Vec<_>>(),"next_cursor":if offset+limit<total{Some((offset+limit).to_string())}else{None},"as_of_cursor":cursor.to_string()}),
    )
}
async fn feed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Api<Response> {
    stream_feed(state, headers, query, None).await
}
async fn session_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Api<Response> {
    stream_feed(state, headers, query, Some(id)).await
}
async fn stream_feed(
    state: AppState,
    headers: HeaderMap,
    query: HashMap<String, String>,
    session: Option<String>,
) -> Api<Response> {
    let credential = token(&headers)?;
    let actor = state.relay.authenticate(&credential)?;
    let project = if let Some(id) = &session {
        Some(field(&state.relay.visible(&actor, "sessions", id)?, "project_id")?.to_owned())
    } else {
        None
    };
    let cursor = headers
        .get("Last-Event-ID")
        .and_then(|h| h.to_str().ok())
        .map(str::to_owned)
        .or_else(|| query.get("cursor").cloned())
        .unwrap_or("0".into())
        .parse::<u64>()
        .map_err(|_| Error::new("invalid_request", "Invalid SSE cursor"))?;
    let stream = stream::unfold(
        (
            state,
            credential,
            cursor,
            VecDeque::<Value>::new(),
            session,
            project,
        ),
        |(state, key, mut cursor, mut pending, session, project)| async move {
            loop {
                let actor = state.relay.authenticate(&key).ok()?;
                if let Some(project) = &project {
                    if !state.relay.permitted(&actor, project, "read").ok()? {
                        return None;
                    }
                }
                if let Some(item) = pending.pop_front() {
                    if !state
                        .relay
                        .permitted(&actor, item["project_id"].as_str().unwrap_or(""), "read")
                        .ok()?
                    {
                        continue;
                    }
                    let event = Event::default()
                        .id(item["cursor"].as_str().unwrap_or("0"))
                        .event(item["kind"].as_str().unwrap_or("session.event"))
                        .json_data(item.clone())
                        .ok()?;
                    return Some((
                        Ok::<_, Infallible>(event),
                        (state, key, cursor, pending, session, project),
                    ));
                }
                for item in state.relay.store.feed(cursor).ok()? {
                    cursor = item["cursor"].as_str()?.parse().ok()?;
                    if session.as_ref().is_none_or(|s| {
                        item["payload"]["session_id"] == *s
                            || item["payload"]["target"]["session_id"] == *s
                    }) {
                        pending.push_back(item);
                    }
                }
                if pending.is_empty() {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        },
    );
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response())
}
async fn static_file(State(state): State<AppState>, request: Request) -> Response {
    let path = request.uri().path().trim_start_matches('/');
    if path.starts_with("api/") || path.split('/').any(|p| p == "..") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let candidate = state
        .web_dir
        .join(if path.is_empty() { "index.html" } else { path });
    let candidate = if tokio::fs::try_exists(&candidate).await.unwrap_or(false) {
        candidate
    } else {
        state.web_dir.join("index.html")
    };
    match tokio::fs::read(&candidate).await {
        Ok(bytes) => {
            let mime = match candidate.extension().and_then(|s| s.to_str()) {
                Some("js") => "text/javascript",
                Some("css") => "text/css",
                Some("png") => "image/png",
                _ => "text/html; charset=utf-8",
            };
            ([(header::CONTENT_TYPE, mime)], bytes).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn websocket(State(state): State<AppState>, mut request: Request) -> Api<Response> {
    let credential = token(request.headers())?;
    let actor = state.relay.authenticate(&credential)?;
    if actor.device_id.is_none() {
        return Err(Error::new("permission_denied", "Agent device Key required").into());
    }
    let key = request
        .headers()
        .get("Sec-WebSocket-Key")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| Error::new("invalid_request", "WebSocket key required"))?
        .to_owned();
    let upgrade = hyper::upgrade::on(&mut request);
    tokio::spawn(async move {
        if let Ok(upgraded) = upgrade.await {
            let socket =
                devmesh_protocol::socket::Socket::new(hyper_util::rt::TokioIo::new(upgraded), true);
            let _ = crate::connection::run(state, credential, actor, socket).await;
        }
    });
    Ok(Response::builder()
        .status(101)
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header(
            "Sec-WebSocket-Accept",
            devmesh_protocol::socket::accept_key(&key),
        )
        .body(Body::empty())
        .unwrap())
}
