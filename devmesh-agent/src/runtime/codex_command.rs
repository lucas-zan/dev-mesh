use serde_json::Value;
use std::path::{Component, Path};

pub(super) fn describe(item: &Value) -> Option<String> {
    let cwd = item["cwd"].as_str().map(Path::new);
    let actions = item["commandActions"].as_array().into_iter().flatten();
    let steps = actions
        .filter_map(|action| describe_action(action, cwd))
        .take(3)
        .collect::<Vec<_>>();
    if !steps.is_empty() {
        return Some(steps.join(" · "));
    }
    Some(shared_text(item["command"].as_str()?))
}

fn describe_action(action: &Value, cwd: Option<&Path>) -> Option<String> {
    let verb = match action["type"].as_str() {
        Some("read") => "读取文件",
        Some("listFiles") => "列出文件",
        Some("search") => "搜索文件",
        _ => return None,
    };
    let detail = action["path"]
        .as_str()
        .and_then(|path| relative_path(path, cwd));
    Some(match (verb, detail) {
        ("读取文件", Some(path)) => format!("读取 {path}"),
        ("列出文件", Some(path)) => format!("列出 {path}"),
        ("搜索文件", Some(path)) => format!("搜索 {path}"),
        _ => verb.into(),
    })
}

fn relative_path<'a>(path: &'a str, cwd: Option<&Path>) -> Option<&'a str> {
    let path = Path::new(path);
    let relative = if path.is_absolute() {
        path.strip_prefix(cwd?).ok()?
    } else {
        path
    };
    let value = relative.to_str()?;
    if value.is_empty()
        || value.len() > 160
        || !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        || !value
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '/' | '.' | '_' | '-'))
    {
        return None;
    }
    Some(value)
}

fn shared_text(text: &str) -> String {
    crate::collector::sanitize(text, "agent")["text"]
        .as_str()
        .unwrap_or("")
        .to_owned()
}

pub(super) fn output(item: &Value) -> Option<(String, Value)> {
    let reads_source = item["commandActions"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|action| matches!(action["type"].as_str(), Some("read" | "search")));
    if reads_source {
        return None;
    }
    let command = item["command"].as_str().unwrap_or("");
    if matches!(
        command.split_whitespace().next(),
        Some("cat" | "sed" | "head" | "tail" | "rg")
    ) {
        return None;
    }
    let raw = item["aggregatedOutput"]
        .as_str()
        .filter(|value| !value.is_empty())?;
    let shared = crate::collector::sanitize(raw, "agent");
    let text = shared["text"].as_str().unwrap_or("");
    let mut end = text.len().min(8192);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut policy = shared["content_policy"].clone();
    policy["truncated"] = serde_json::json!(end < text.len() || policy["truncated"] == true);
    Some((text[..end].to_owned(), policy))
}
