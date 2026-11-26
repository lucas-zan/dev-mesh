use devmesh_agent::daemon::manual_role;
use serde_json::json;
#[test]
fn manual_work_must_use_issue_role_and_both_local_role_allowlists() {
    let role = manual_role(
        &json!({"required_role":"qa"}),
        &["qa".into()],
        &["qa".into(), "backend".into()],
    )
    .unwrap();
    assert_eq!(role, "qa");
    assert_eq!(
        manual_role(
            &json!({"required_role":"qa"}),
            &["backend".into()],
            &["qa".into()]
        )
        .unwrap_err()
        .code,
        "permission_denied"
    );
}
