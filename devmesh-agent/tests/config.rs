use devmesh_agent::config::Config;
use tempfile::tempdir;
#[test]
fn invalid_config_reports_error_without_reading_credentials() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("config.json");
    std::fs::write(&file, r#"{"schema_version":99}"#).unwrap();
    assert!(Config::read(&file).is_err());
}
