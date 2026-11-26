use devmesh_agent::installer::{install_bundle, rollback, uninstall, BundleFile, BundleManifest};
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::tempdir;

fn bundle(root: &std::path::Path, version: &str, content: &str) {
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::write(root.join("bin/devmesh"), content).unwrap();
    let manifest = BundleManifest {
        version: version.into(),
        files: vec![BundleFile {
            path: "bin/devmesh".into(),
            sha256: format!("{:x}", Sha256::digest(content.as_bytes())),
            executable: true,
        }],
    };
    std::fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
}
#[test]
fn upgrade_and_rollback_preserve_user_configuration_and_data() {
    let dir = tempdir().unwrap();
    let prefix = dir.path().join("installed");
    let one = dir.path().join("one");
    let two = dir.path().join("two");
    bundle(&one, "0.1.0", "one");
    bundle(&two, "0.2.0", "two");
    install_bundle(&prefix, &one).unwrap();
    std::fs::write(
        prefix.join("config.json"),
        json!({"custom":"keep me"}).to_string(),
    )
    .unwrap();
    install_bundle(&prefix, &two).unwrap();
    assert_eq!(
        std::fs::read_to_string(prefix.join("current/bin/devmesh")).unwrap(),
        "two"
    );
    rollback(&prefix, "0.1.0").unwrap();
    assert_eq!(
        std::fs::read_to_string(prefix.join("current/bin/devmesh")).unwrap(),
        "one"
    );
    assert!(std::fs::read_to_string(prefix.join("config.json"))
        .unwrap()
        .contains("keep me"));
    std::fs::write(prefix.join("releases/0.1.0/user-file"), "private").unwrap();
    uninstall(&prefix).unwrap();
    assert!(prefix.join("config.json").exists());
    assert!(prefix.join("releases/0.1.0/user-file").exists());
    assert!(!prefix.join("current").exists());
}
#[test]
fn invalid_checksum_does_not_change_installed_version() {
    let dir = tempdir().unwrap();
    let prefix = dir.path().join("installed");
    let source = dir.path().join("source");
    bundle(&source, "0.1.0", "one");
    install_bundle(&prefix, &source).unwrap();
    std::fs::write(source.join("bin/devmesh"), "tampered").unwrap();
    assert_eq!(
        install_bundle(&prefix, &source).unwrap_err().code,
        "checksum_mismatch"
    );
    assert_eq!(
        std::fs::read_to_string(prefix.join("current/bin/devmesh")).unwrap(),
        "one"
    );
}

#[test]
fn failed_runtime_install_restores_the_previous_current_release() {
    use devmesh_agent::installer::restore_current;
    let dir = tempdir().unwrap();
    let prefix = dir.path().join("installed");
    let one = dir.path().join("one");
    let two = dir.path().join("two");
    bundle(&one, "0.1.0", "one");
    bundle(&two, "0.2.0", "two");
    install_bundle(&prefix, &one).unwrap();
    install_bundle(&prefix, &two).unwrap();
    restore_current(&prefix, Some("0.1.0")).unwrap();
    assert_eq!(
        std::fs::read_to_string(prefix.join("current/bin/devmesh")).unwrap(),
        "one"
    );
    restore_current(&prefix, None).unwrap();
    assert!(!prefix.join("current").exists());
}
