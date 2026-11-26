use devmesh_agent::gitea::{decode_mcp, official_operation};
use reqwest::Method;
use serde_json::json;
#[test]
fn mcp_mapping_keeps_cas_on_narrow_rest_and_maps_controlled_comments() {
    let read = official_operation(&Method::GET, "repos/team/api/issues/42", None).unwrap();
    assert_eq!(read.0, "issue_read");
    assert_eq!(read.1["issue_number"], 42);
    assert_eq!(read.1["method"], "get");
    assert!(official_operation(
        &Method::PATCH,
        "repos/team/api/issues/42",
        Some(&json!({"content_version":1}))
    )
    .is_none());
    let comment = official_operation(
        &Method::POST,
        "repos/team/api/issues/42/comments",
        Some(&json!({"body":"evidence"})),
    )
    .unwrap();
    assert_eq!(comment.0, "issue_write");
    assert_eq!(comment.1["method"], "add_comment");
    assert_eq!(
        decode_mcp(json!({"content":[{"type":"text","text":"{\"id\":42}"}]})).unwrap()["id"],
        42
    );
    assert!(
        decode_mcp(json!({"isError":true,"content":[{"type":"text","text":"denied"}]})).is_err()
    );
}
