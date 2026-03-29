use serde_json::{json, Map, Value};

use crate::config::EffectiveConfig;

fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            last_dash = false;
            ch.to_ascii_lowercase()
        } else if !last_dash {
            last_dash = true;
            '-'
        } else {
            continue;
        };
        out.push(mapped);
    }
    out.trim_matches('-').to_string()
}

pub fn create_task_with_compat(input: &Map<String, Value>, config: &EffectiveConfig) -> Value {
    let title = input
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled task");
    let status = input
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or(&config.defaults.status);
    let priority = input
        .get("priority")
        .and_then(Value::as_str)
        .unwrap_or(&config.defaults.priority);
    let path = input
        .get("path")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            let basename = slugify(title);
            format!(
                "{}/{}.md",
                config.task_detection.default_folder.trim_matches('/'),
                basename
            )
        });

    json!({
        "path": path,
        "frontmatter": {
            "title": title,
            "status": status,
            "priority": priority,
        },
        "body": input.get("details").cloned().unwrap_or(Value::String(String::new())),
    })
}
