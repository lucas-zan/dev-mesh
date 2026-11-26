use devmesh_agent::delivery::validate_delivery;
use serde_json::json;

fn report() -> serde_json::Value {
    json!({"candidate_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","evidence":[{"evidence_id":"test-1","kind":"test","candidate_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","outcome":"pass","exit_code":0,"environment":"local","acceptance_ids":["AC-01"]},{"evidence_id":"pr-1","kind":"pr","candidate_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","outcome":"pass","pr":{"number":4,"head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}],"acceptance_results":[{"id":"AC-01","outcome":"pass","evidence_ids":["test-1"]}]})
}
#[test]
fn passing_candidate_requires_matching_tests_and_acceptance_results() {
    let meta = json!({"required_role":"backend","acceptance_criteria":[{"id":"AC-01","text":"upload works"}]});
    let mut value = report();
    assert!(validate_delivery(&meta, &value).is_ok());
    value["acceptance_results"][0]["outcome"] = json!("skipped");
    assert!(validate_delivery(&meta, &value).is_err());
    value = report();
    value["evidence"][0]["candidate_sha"] = json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    assert_eq!(
        validate_delivery(&meta, &value).unwrap_err().code,
        "stale_evidence"
    );
}
#[test]
fn mutable_artifact_and_nonexistent_evidence_cannot_complete_product_work() {
    let meta = json!({"required_role":"product","acceptance_criteria":[{"id":"AC-01"}]});
    let mut value = report();
    value["evidence"] = json!([{"evidence_id":"test-1","kind":"artifact","artifact":{"url":"https://example.invalid/latest"},"outcome":"pass"}]);
    assert!(validate_delivery(&meta, &value).is_err());
    value = report();
    value["acceptance_results"][0]["evidence_ids"] = json!(["unknown"]);
    assert!(validate_delivery(&meta, &value).is_err());
}

#[tokio::test]
async fn immutable_artifact_hash_is_verified_against_gitea_content() {
    use devmesh_agent::gitea::{GiteaClient, GiteaTransport, Repo, TaskService};
    use devmesh_protocol::Result;
    use serde_json::Value;
    use std::sync::Arc;
    struct Content;
    #[async_trait::async_trait]
    impl GiteaTransport for Content {
        async fn request(
            &self,
            _: reqwest::Method,
            path: &str,
            _: Option<&Value>,
        ) -> Result<Value> {
            if path.starts_with("repositories/") {
                return Ok(json!({"owner":{"login":"org"},"name":"repo"}));
            }
            assert!(path.contains("?ref=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
            Ok(json!({"type":"file","encoding":"base64","content":"Y29udHJhY3Q="}))
        }
    }
    use sha2::{Digest, Sha256};
    let repo = Repo {
        instance_id: uuid::Uuid::new_v4(),
        repository_id: 42,
        owner: "org".into(),
        name: "repo".into(),
    };
    let mut artifact = json!({"kind":"git_file","repository":{"instance_id":repo.instance_id,"repository_id":42},"commit_sha":"a".repeat(40),"path":"api/contract.md","sha256":format!("{:x}",Sha256::digest(b"contract"))});
    let service = TaskService::new(GiteaClient::with_transport(Arc::new(Content)), repo);
    service.verify_artifact(&artifact).await.unwrap();
    artifact["sha256"] = json!("b".repeat(64));
    assert_eq!(
        service.verify_artifact(&artifact).await.unwrap_err().code,
        "stale_evidence"
    );
}

#[test]
fn pr_candidate_must_match_the_configured_target_and_tested_sha() {
    use devmesh_agent::delivery::validate_pr_candidate;
    let reference = json!({"head_sha":"a".repeat(40),"target_branch":"main"});
    let mut pr =
        json!({"head":{"sha":"a".repeat(40)},"base":{"ref":"main"},"state":"open","draft":false});
    assert!(validate_pr_candidate(&pr, &reference, &"a".repeat(40), "main").is_ok());
    pr["base"]["ref"] = json!("other");
    assert!(validate_pr_candidate(&pr, &reference, &"a".repeat(40), "main").is_err());
    pr["base"]["ref"] = json!("main");
    pr["head"]["sha"] = json!("b".repeat(40));
    assert!(validate_pr_candidate(&pr, &reference, &"a".repeat(40), "main").is_err());
}

#[test]
fn qa_delivery_requires_observed_tests_as_well_as_a_report() {
    let meta = json!({"required_role":"qa","acceptance_criteria":[{"id":"AC-01"}]});
    let mut value = report();
    value["evidence"] = json!([{"evidence_id":"report","kind":"artifact","candidate_sha":value["candidate_sha"],"outcome":"pass","artifact":{"kind":"git_file","repository":{"repository_id":42},"commit_sha":"a".repeat(40),"path":"qa.md","sha256":"b".repeat(64)}}]);
    value["acceptance_results"][0]["evidence_ids"] = json!(["report"]);
    assert!(validate_delivery(&meta, &value).is_err());
}
