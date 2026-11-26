use regex::Regex;
use serde_json::{json, Value};
use std::sync::LazyLock;
static SECRETS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:[a-z0-9_]*(?:api_key|token|password|secret)[a-z0-9_]*\s*[:=]\s*\S+|bearer\s+\S+|sk-[a-z0-9_-]+|(?:postgres|postgresql|mysql)://\S+)").unwrap()
});
static HOMES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:/Users/|/home/|~/)[^\s`]+|[?&](?:token|key|secret)=[^&\s]+").unwrap()
});

pub fn sanitize(text: &str, author: &str) -> Value {
    let mut fenced = false;
    let mut omitted = false;
    let mut kept = Vec::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            fenced = !fenced;
            omitted = true;
            if fenced {
                kept.push("[代码片段已省略]");
            }
            continue;
        }
        if !fenced {
            kept.push(line);
        }
    }
    let joined = kept.join("\n");
    let redacted = SECRETS.replace_all(&joined, "[已脱敏]");
    let redacted = HOMES
        .replace_all(&redacted, "[路径/参数已省略]")
        .into_owned();
    let filtered = redacted != joined;
    let truncated = redacted.len() > 16384;
    let mut end = redacted.len().min(16384);
    while !redacted.is_char_boundary(end) {
        end -= 1;
    }
    json!({"message_id":uuid::Uuid::new_v4(),"author_kind":author,"text":&redacted[..end],"content_policy":{"policy_id":"issue-share-v1","redacted":filtered,"truncated":truncated,"omitted":omitted,"original_bytes":text.len()}})
}
