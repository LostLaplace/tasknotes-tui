use serde_json::{json, Map, Value};

use crate::date::{
    get_date_part, has_time_component, is_before_date_safe, parse_date_to_utc,
};
use crate::field_mapping::{
    build_field_mapping, is_completed_status, normalize_frontmatter, resolve_display_title,
};

pub fn validate_core(input: &Map<String, Value>, reject_unknown_fields: bool) -> Value {
    let fields = input
        .get("fields")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let frontmatter = input
        .get("frontmatter")
        .or_else(|| input.get("fields"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let display_name_key = input.get("displayNameKey").and_then(Value::as_str);
    let task_path = input.get("taskPath").and_then(Value::as_str);
    let mapping = build_field_mapping(&fields, display_name_key);
    let normalized = normalize_frontmatter(&frontmatter, &mapping);
    let mut issues = Vec::new();

    for role in ["status", "dateCreated", "dateModified"] {
        if is_blank(normalized.get(role)) {
            add_issue(
                &mut issues,
                "missing_required",
                "error",
                field_name(&mapping.role_to_field, role),
                format!("missing required {}", camel_to_snake(role)),
            );
        }
    }

    if resolve_display_title(&frontmatter, &mapping, task_path)
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
    {
        add_issue(
            &mut issues,
            "unresolvable_title",
            "error",
            field_name(&mapping.role_to_field, "title"),
            "title could not be resolved".to_string(),
        );
    }

    for role in [
        "status",
        "due",
        "scheduled",
        "completedDate",
        "dateCreated",
        "dateModified",
    ] {
        let Some(value) = normalized.get(role) else {
            continue;
        };
        if value.is_null() || value == "" {
            continue;
        }
        if !value.is_string() {
            add_issue(
                &mut issues,
                "invalid_type",
                "error",
                field_name(&mapping.role_to_field, role),
                format!("expected string for {role}"),
            );
        }
    }

    for role in ["tags", "contexts", "projects"] {
        let Some(value) = normalized.get(role) else {
            continue;
        };
        if !value.is_array() {
            add_issue(
                &mut issues,
                "invalid_type",
                "error",
                field_name(&mapping.role_to_field, role),
                format!("expected array for {role}"),
            );
        }
    }

    if let Some(value) = normalized.get("timeEntries") {
        if !value.is_array() {
            add_issue(
                &mut issues,
                "invalid_type",
                "error",
                field_name(&mapping.role_to_field, "timeEntries"),
                "expected array for timeEntries".to_string(),
            );
        } else if let Some(entries) = value.as_array() {
            if let Err(error) = validate_time_entries_impl(entries) {
            add_issue(
                &mut issues,
                &error,
                "error",
                field_name(&mapping.role_to_field, "timeEntries"),
                error.clone(),
            );
        }
    }
    }

    for role in ["due", "scheduled", "completedDate", "dateCreated", "dateModified"] {
        let Some(value) = normalized.get(role).and_then(Value::as_str) else {
            continue;
        };
        if value.trim().is_empty() {
            continue;
        }
        if parse_date_to_utc(value).is_err() {
            add_issue(
                &mut issues,
                "invalid_date_value",
                "error",
                field_name(&mapping.role_to_field, role),
                format!("invalid date value for {role}"),
            );
        }
    }

    if normalized
        .get("status")
        .and_then(Value::as_str)
        .map(|status| is_completed_status(&mapping, Some(status)))
        .unwrap_or(false)
        && is_blank(normalized.get("completedDate"))
    {
        add_issue(
            &mut issues,
            "missing_required",
            "error",
            field_name(&mapping.role_to_field, "completedDate"),
            "completed_date is required for completed status".to_string(),
        );
    }

    if let (Some(created), Some(modified)) = (
        normalized.get("dateCreated").and_then(Value::as_str),
        normalized.get("dateModified").and_then(Value::as_str),
    ) {
        let created_day = get_date_part(created);
        let modified_day = get_date_part(modified);
        let created_ts = chrono::DateTime::parse_from_rfc3339(created)
            .ok()
            .map(|value| value.timestamp());
        let modified_ts = chrono::DateTime::parse_from_rfc3339(modified)
            .ok()
            .map(|value| value.timestamp());
        let modified_before = if has_time_component(created) && has_time_component(modified) {
            match (created_ts, modified_ts) {
                (Some(created_ts), Some(modified_ts)) => modified_ts < created_ts,
                _ => is_before_date_safe(&modified_day, &created_day),
            }
        } else {
            is_before_date_safe(&modified_day, &created_day)
        };
        if modified_before {
            add_issue(
                &mut issues,
                "date_modified_before_created",
                "error",
                field_name(&mapping.role_to_field, "dateModified"),
                "date_modified must be >= date_created".to_string(),
            );
        }
    }

    for key in frontmatter.keys() {
        let known = mapping.field_to_role.contains_key(key) || mapping.role_to_field.contains_key(key);
        if !known {
            add_issue(
                &mut issues,
                "unknown_field",
                if reject_unknown_fields { "error" } else { "info" },
                key.clone(),
                "field is not mapped to a known semantic role".to_string(),
            );
        }
    }

    let error_codes = codes_by_severity(&issues, "error");
    let warning_codes = codes_by_severity(&issues, "warning");
    let info_codes = codes_by_severity(&issues, "info");
    let all_codes = {
        let mut values = Vec::new();
        for issue in &issues {
            if let Some(code) = issue.get("code").and_then(Value::as_str) {
                if !values.iter().any(|entry| entry == code) {
                    values.push(code.to_string());
                }
            }
        }
        values
    };

    json!({
        "hasErrors": !error_codes.is_empty(),
        "issues": issues,
        "errorCodes": error_codes,
        "warningCodes": warning_codes,
        "infoCodes": info_codes,
        "allCodes": all_codes,
    })
}

pub fn validate_time_entries(entries: &[Value]) -> Value {
    match validate_time_entries_impl(entries) {
        Ok(()) => json!({ "value": "valid" }),
        Err(error) => json!({ "__error__": error }),
    }
}

fn validate_time_entries_impl(entries: &[Value]) -> Result<(), String> {
    let mut active_count = 0;
    for entry in entries {
        let object = entry
            .as_object()
            .ok_or_else(|| "invalid_time_entry".to_string())?;
        let start = object
            .get("startTime")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "missing_time_entry_start".to_string())?;
        let start = chrono::DateTime::parse_from_rfc3339(start)
            .map_err(|_| "invalid_time_entry_start".to_string())?;

        match object.get("endTime") {
            None | Some(Value::Null) => active_count += 1,
            Some(Value::String(end)) if end.trim().is_empty() => active_count += 1,
            Some(Value::String(end)) => {
                let end = chrono::DateTime::parse_from_rfc3339(end)
                    .map_err(|_| "invalid_time_entry_end".to_string())?;
                if end < start {
                    return Err("invalid_time_range".to_string());
                }
            }
            Some(_) => return Err("invalid_time_entry_end".to_string()),
        }
    }

    if active_count > 1 {
        return Err("multiple_active_time_entries".to_string());
    }
    Ok(())
}

fn add_issue(
    issues: &mut Vec<Value>,
    code: &str,
    severity: &str,
    field: String,
    message: String,
) {
    issues.push(json!({
        "code": code,
        "severity": severity,
        "field": field,
        "message": message,
    }));
}

fn is_blank(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::String(text)) => text.trim().is_empty(),
        _ => false,
    }
}

fn field_name(mapping: &std::collections::BTreeMap<String, String>, role: &str) -> String {
    mapping
        .get(role)
        .cloned()
        .unwrap_or_else(|| role.to_string())
}

fn camel_to_snake(value: &str) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if ch.is_uppercase() && index > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

fn codes_by_severity(issues: &[Value], severity: &str) -> Vec<String> {
    issues
        .iter()
        .filter(|issue| issue.get("severity").and_then(Value::as_str) == Some(severity))
        .filter_map(|issue| issue.get("code").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}
