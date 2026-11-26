#[tokio::main]
async fn main() {
    if let Err(error) = devmesh_agent::cli::run(std::env::args().skip(1).collect()).await {
        eprintln!(
            "{}",
            serde_json::json!({"error":{"code":error.code,"message":error.message,"retryable":false}})
        );
        std::process::exit(1);
    }
}
