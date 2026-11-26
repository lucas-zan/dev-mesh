use devmesh_relay::{
    http::{router, AppState},
    MemoryStore, PostgresStore, Relay,
};
use std::{path::PathBuf, sync::Arc};
fn option(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|p| p[0] == name).map(|p| p[1].clone())
}
fn listen_address(args: &[String]) -> String {
    option(args, "--listen").unwrap_or("127.0.0.1:17843".into())
}
#[tokio::main]
async fn main() {
    if let Err(error) = run(std::env::args().skip(1).collect()).await {
        eprintln!(
            "{}",
            serde_json::json!({"error":{"code":error.code,"message":error.message}})
        );
        std::process::exit(1);
    }
}
async fn run(args: Vec<String>) -> devmesh_protocol::Result<()> {
    use devmesh_protocol::Error;
    let command = args.first().map(String::as_str).unwrap_or("serve");
    if matches!(command, "--help" | "help") {
        println!("DevMesh Relay\n\n  migrate\n  bootstrap --username NAME [--password-env DEVMESH_INITIAL_PASSWORD]\n  serve [--listen 127.0.0.1:17843] [--web-dir web/dist]\n\nDATABASE_URL selects PostgreSQL. --ephemeral uses an in-memory development store and requires DEVMESH_INITIAL_PASSWORD.");
        return Ok(());
    }
    let relay = if args.iter().any(|a| a == "--ephemeral") {
        let relay = Relay::new(Arc::new(MemoryStore::default()));
        let password = std::env::var("DEVMESH_INITIAL_PASSWORD").map_err(|_| {
            Error::new(
                "credential_missing",
                "DEVMESH_INITIAL_PASSWORD required for ephemeral development",
            )
        })?;
        relay.bootstrap("admin", &password)?;
        Arc::new(relay)
    } else {
        let url = std::env::var("DATABASE_URL").map_err(|_| {
            Error::new(
                "config_error",
                "DATABASE_URL required; no implicit in-memory fallback",
            )
        })?;
        let store = PostgresStore::connect(url)?;
        store.migrate()?;
        Arc::new(Relay::new(Arc::new(store)))
    };
    if command == "migrate" {
        return Ok(());
    }
    if command == "bootstrap" {
        let username = option(&args, "--username")
            .ok_or_else(|| Error::new("invalid_request", "--username required"))?;
        let name = option(&args, "--password-env").unwrap_or("DEVMESH_INITIAL_PASSWORD".into());
        let password = std::env::var(name)
            .map_err(|_| Error::new("credential_missing", "Initial password reference missing"))?;
        println!(
            "{}",
            serde_json::to_string_pretty(&relay.bootstrap(&username, &password)?).unwrap()
        );
        return Ok(());
    }
    if command != "serve" {
        return Err(Error::new("invalid_request", "Unknown relay command"));
    }
    let address = listen_address(&args);
    let web = option(&args, "--web-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("web/dist"));
    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .map_err(|e| Error::new("listen_failed", e.to_string()))?;
    eprintln!("DevMesh Relay listening on {address}");
    axum::serve(listener, router(AppState::new(relay, web)))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|e| Error::new("server_error", e.to_string()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn relay_uses_17843_by_default_and_keeps_explicit_listen_override() {
        assert_eq!(super::listen_address(&[]), "127.0.0.1:17843");
        assert_eq!(
            super::listen_address(&["--listen".into(), "127.0.0.1:19001".into()]),
            "127.0.0.1:19001"
        );
    }
}
