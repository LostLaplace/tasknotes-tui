use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};

use crate::field_mapping::{default_field_mapping, FieldMapping};

pub fn create_task_with_compat(input: &Value) -> anyhow::Result<Value> {
    let source = input.as_object().cloned().unwrap_or_default();
    if let Some(code) = source.get("forceCreateError").and_then(Value::as_str) {
        anyhow::bail!(code.to_string());
    }

    let mapping = default_field_mapping();
    let task_type = source
        .get("taskType")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let fields = task_type
        .get("fields")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut frontmatter = source
        .get("frontmatter")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let body = source
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let now = source
        .get("fixedNow")
        .and_then(Value::as_str)
        .and_then(parse_datetime)
        .unwrap_or_else(Utc::now);

    apply_field_defaults(&mut frontmatter, &fields);
    apply_timestamp_defaults(&mut frontmatter, &mapping, &fields, now);
    apply_match_defaults(
        &mut frontmatter,
        task_type.get("match").and_then(Value::as_object),
    );

    let path_result = source
        .get("path")
        .and_then(Value::as_str)
        .map(|value| Ok(value.to_string()))
        .or_else(|| {
            task_type
                .get("path_pattern")
                .and_then(Value::as_str)
                .map(|template| derive_path_from_type(template, &frontmatter, &mapping, now))
        });
    let path = match path_result {
        Some(Ok(path)) => Some(path),
        Some(Err(error)) => anyhow::bail!(error),
        None => None,
    };

    Ok(json!({
        "path": path,
        "frontmatter": frontmatter,
        "body": body,
        "warnings": [],
        "callCount": if path.is_some() { 2 } else { 1 },
    }))
}

fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn apply_timestamp_defaults(
    frontmatter: &mut Map<String, Value>,
    mapping: &FieldMapping,
    fields: &Map<String, Value>,
    now: DateTime<Utc>,
) {
    let now_iso = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let created_field = mapping
        .role_to_field
        .get("dateCreated")
        .cloned()
        .unwrap_or_else(|| "dateCreated".into());
    if fields.contains_key(&created_field) && !has_value(frontmatter.get(&created_field)) {
        frontmatter.insert(created_field, Value::String(now_iso.clone()));
    }

    let modified_field = mapping
        .role_to_field
        .get("dateModified")
        .cloned()
        .unwrap_or_else(|| "dateModified".into());
    if fields.contains_key(&modified_field) && !has_value(frontmatter.get(&modified_field)) {
        frontmatter.insert(modified_field, Value::String(now_iso));
    }
}

fn apply_field_defaults(frontmatter: &mut Map<String, Value>, fields: &Map<String, Value>) {
    for (field_name, field_def) in fields {
        if let Some(default) = field_def.get("default") {
            if !has_value(frontmatter.get(field_name)) {
                frontmatter.insert(field_name.clone(), default.clone());
            }
        }
    }
}

fn apply_match_defaults(
    frontmatter: &mut Map<String, Value>,
    match_obj: Option<&Map<String, Value>>,
) {
    let where_obj = match_obj
        .and_then(|value| value.get("where"))
        .and_then(Value::as_object);
    let Some(where_obj) = where_obj else {
        return;
    };

    for (field, condition) in where_obj {
        match condition {
            Value::Null => {}
            Value::Object(ops) => {
                if let Some(eq) = ops.get("eq") {
                    if !has_value(frontmatter.get(field)) {
                        frontmatter.insert(field.clone(), eq.clone());
                    }
                    continue;
                }
                if let Some(contains) = ops.get("contains") {
                    match frontmatter.get_mut(field) {
                        Some(Value::Array(values)) => {
                            if !values.iter().any(|value| value == contains) {
                                values.push(contains.clone());
                            }
                        }
                        Some(Value::String(value)) => {
                            let needle = contains.as_str().unwrap_or_default();
                            if !value.contains(needle) {
                                *value = format!("{value} {needle}").trim().to_string();
                            }
                        }
                        _ => {
                            frontmatter.insert(field.clone(), Value::Array(vec![contains.clone()]));
                        }
                    }
                    continue;
                }
                if ops.get("exists").and_then(Value::as_bool) == Some(true)
                    && !has_value(frontmatter.get(field))
                {
                    frontmatter.insert(field.clone(), Value::Bool(true));
                }
            }
            other => {
                if !has_value(frontmatter.get(field)) {
                    frontmatter.insert(field.clone(), other.clone());
                }
            }
        }
    }
}

fn derive_path_from_type(
    template: &str,
    frontmatter: &Map<String, Value>,
    mapping: &FieldMapping,
    now: DateTime<Utc>,
) -> anyhow::Result<String> {
    let values = build_template_values(frontmatter, mapping, now);
    let missing = extract_template_keys(template)
        .into_iter()
        .filter(|key| {
            !values
                .get(key)
                .and_then(Value::as_str)
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        anyhow::bail!("missing template values for {}", missing.join(", "));
    }
    let mut rendered = template.to_string();
    for (key, value) in values {
        let value = value.as_str().unwrap_or_default();
        rendered = rendered.replace(&format!("{{{{{key}}}}}"), value);
        rendered = rendered.replace(&format!("{{{key}}}"), value);
    }
    rendered = rendered.replace('{', "").replace('}', "");
    let normalized =
        normalize_relative_path(&rendered).ok_or_else(|| anyhow::anyhow!("path_required"))?;
    Ok(ensure_markdown_ext(&normalized))
}

fn build_template_values(
    frontmatter: &Map<String, Value>,
    mapping: &FieldMapping,
    now: DateTime<Utc>,
) -> Map<String, Value> {
    let title_field = mapping
        .role_to_field
        .get("title")
        .map(String::as_str)
        .unwrap_or("title");
    let priority_field = mapping
        .role_to_field
        .get("priority")
        .map(String::as_str)
        .unwrap_or("priority");
    let status_field = mapping
        .role_to_field
        .get("status")
        .map(String::as_str)
        .unwrap_or("status");
    let due_field = mapping
        .role_to_field
        .get("due")
        .map(String::as_str)
        .unwrap_or("due");
    let scheduled_field = mapping
        .role_to_field
        .get("scheduled")
        .map(String::as_str)
        .unwrap_or("scheduled");

    let raw_title = read_string(frontmatter.get(title_field))
        .or_else(|| read_string(frontmatter.get("title")))
        .unwrap_or_else(|| "task".into());
    let title = sanitize_segment(&raw_title);
    let priority = sanitize_segment(
        &read_string(frontmatter.get(priority_field))
            .or_else(|| read_string(frontmatter.get("priority")))
            .unwrap_or_else(|| "normal".into()),
    );
    let status = sanitize_segment(
        &read_string(frontmatter.get(status_field))
            .or_else(|| read_string(frontmatter.get("status")))
            .unwrap_or_else(|| "open".into()),
    );
    let due_date_raw = read_string(frontmatter.get(due_field))
        .or_else(|| read_string(frontmatter.get("due")))
        .unwrap_or_default();
    let scheduled_date_raw = read_string(frontmatter.get(scheduled_field))
        .or_else(|| read_string(frontmatter.get("scheduled")))
        .unwrap_or_default();
    let today = now.format("%Y-%m-%d").to_string();
    let due_date = if due_date_raw.is_empty() {
        if scheduled_date_raw.is_empty() {
            today.clone()
        } else {
            scheduled_date_raw.clone()
        }
    } else {
        due_date_raw.clone()
    };
    let scheduled_date = if scheduled_date_raw.is_empty() {
        if due_date_raw.is_empty() {
            today.clone()
        } else {
            due_date_raw.clone()
        }
    } else {
        scheduled_date_raw.clone()
    };

    let mut out = Map::new();
    let lower = title.to_lowercase();
    out.insert("title".into(), Value::String(title.clone()));
    out.insert("titleLower".into(), Value::String(lower.clone()));
    out.insert("titleKebab".into(), Value::String(slugify(&raw_title)));
    out.insert("priority".into(), Value::String(priority));
    out.insert(
        "priorityShort".into(),
        Value::String(
            out.get("priority")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .chars()
                .next()
                .map(|c| c.to_string())
                .unwrap_or_default(),
        ),
    );
    out.insert("status".into(), Value::String(status));
    out.insert(
        "statusShort".into(),
        Value::String(
            out.get("status")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .chars()
                .next()
                .map(|c| c.to_string())
                .unwrap_or_default(),
        ),
    );
    out.insert("date".into(), Value::String(today.clone()));
    out.insert("time".into(), Value::String(now.format("%H%M").to_string()));
    out.insert(
        "timestamp".into(),
        Value::String(now.format("%Y%m%d%H%M").to_string()),
    );
    out.insert("year".into(), Value::String(now.format("%Y").to_string()));
    out.insert("week".into(), Value::String(now.format("%V").to_string()));
    out.insert("month".into(), Value::String(now.format("%m").to_string()));
    out.insert("day".into(), Value::String(now.format("%d").to_string()));
    out.insert(
        "monthName".into(),
        Value::String(now.format("%B").to_string()),
    );
    out.insert(
        "monthNameShort".into(),
        Value::String(now.format("%b").to_string()),
    );
    out.insert("due".into(), Value::String(due_date));
    if let Some(raw) =
        read_string(frontmatter.get(due_field)).or_else(|| read_string(frontmatter.get("due")))
    {
        out.insert("dueDate".into(), Value::String(raw));
    }
    out.insert("scheduled".into(), Value::String(scheduled_date));
    if let Some(raw) = read_string(frontmatter.get(scheduled_field))
        .or_else(|| read_string(frontmatter.get("scheduled")))
    {
        out.insert("scheduledDate".into(), Value::String(raw));
    }
    out.insert(
        "shortDate".into(),
        Value::String(now.format("%Y%m%d").to_string()),
    );
    out.insert(
        "zettel".into(),
        Value::String(now.format("%Y%m%d%H%M").to_string()),
    );
    out.insert("titleUpper".into(), Value::String(raw_title.to_uppercase()));
    out.insert(
        "titleSnake".into(),
        Value::String(
            raw_title
                .to_lowercase()
                .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
                .split('_')
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join("_"),
        ),
    );
    out.insert(
        "titleCamel".into(),
        Value::String(to_camel_case(&raw_title, false)),
    );
    out.insert(
        "titlePascal".into(),
        Value::String(to_camel_case(&raw_title, true)),
    );
    out
}

fn extract_template_keys(template: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let start = i + 1 + usize::from(i + 1 < bytes.len() && bytes[i + 1] == b'{');
            let mut end = start;
            while end < bytes.len() && bytes[end] != b'}' {
                end += 1;
            }
            if end < bytes.len() && end > start {
                let key = template[start..end].trim_matches('}').trim().to_string();
                if !key.is_empty() && !keys.contains(&key) {
                    keys.push(key);
                }
                i = end;
            }
        }
        i += 1;
    }
    keys
}

fn has_value(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Null) | None => false,
        Some(Value::String(value)) => !value.trim().is_empty(),
        Some(Value::Array(values)) => !values.is_empty(),
        _ => true,
    }
}

fn read_string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(str::to_string)
}

fn sanitize_segment(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "task".into();
    }
    trimmed
        .chars()
        .map(|ch| if ch == '/' || ch == '\\' { '-' } else { ch })
        .collect::<String>()
}

fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn normalize_relative_path(value: &str) -> Option<String> {
    let normalized = value
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/");
    if normalized.is_empty() || normalized.contains("..") || normalized.contains('\0') {
        None
    } else {
        Some(normalized)
    }
}

fn ensure_markdown_ext(value: &str) -> String {
    if value.ends_with(".md") {
        value.to_string()
    } else {
        format!("{value}.md")
    }
}

fn to_camel_case(value: &str, capitalize_first: bool) -> String {
    let mut out = String::new();
    for (index, part) in value
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .enumerate()
    {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            if index > 0 || capitalize_first {
                out.push(first.to_ascii_uppercase());
            } else {
                out.push(first.to_ascii_lowercase());
            }
            for ch in chars {
                out.push(ch.to_ascii_lowercase());
            }
        }
    }
    out
}
