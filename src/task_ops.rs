use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};
use std::path::Path;

use crate::config::ArchiveConfig;
use crate::date::{resolve_operation_target_date, today_local};
use crate::validation::validate_core;

pub fn mutate_with_validation(frontmatter: &Map<String, Value>, strict: bool) -> Result<Value> {
    let payload = Map::from_iter([(
        "frontmatter".to_string(),
        Value::Object(frontmatter.clone()),
    )]);
    let result = validate_core(&payload, strict);
    if result.get("hasErrors").and_then(Value::as_bool) == Some(true) {
        bail!(
            "validation:{}",
            result
                .get("errorCodes")
                .and_then(Value::as_array)
                .and_then(|arr| arr.first())
                .and_then(Value::as_str)
                .unwrap_or("invalid")
        );
    }
    Ok(json!({ "value": "accepted" }))
}

pub fn apply_patch(
    original: &Map<String, Value>,
    patch: &Map<String, Value>,
) -> (bool, Map<String, Value>) {
    let mut merged = original.clone();
    for (key, value) in patch {
        merged.insert(key.clone(), value.clone());
    }
    let changed = &merged != original;
    (changed, merged)
}

pub fn atomic_write(
    original: &Map<String, Value>,
    patch: &Map<String, Value>,
    simulate_failure_after_write: bool,
) -> Value {
    let (_, merged) = apply_patch(original, patch);
    json!({
        "committed": !simulate_failure_after_write,
        "persisted": if simulate_failure_after_write {
            Value::Object(original.clone())
        } else {
            Value::Object(merged)
        }
    })
}

pub fn complete_nonrecurring(
    frontmatter: &Map<String, Value>,
    completed_status: &str,
    explicit_date: Option<&str>,
) -> Result<Map<String, Value>> {
    let mut after = frontmatter.clone();
    let completion_date = match explicit_date {
        Some(_) => resolve_operation_target_date(explicit_date, None, None)?,
        None => today_local(),
    };
    after.insert("status".into(), Value::String(completed_status.to_string()));
    after.insert("completedDate".into(), Value::String(completion_date));
    Ok(after)
}

pub fn uncomplete_nonrecurring(
    frontmatter: &Map<String, Value>,
    default_status: &str,
    clear_completed_date: bool,
) -> Map<String, Value> {
    let mut after = frontmatter.clone();
    after.insert("status".into(), Value::String(default_status.to_string()));
    if clear_completed_date {
        after.insert("completedDate".into(), Value::Null);
    }
    after
}

pub fn check_idempotency(operation: &str, first: &Value, second: &Value) -> Result<Value> {
    let idempotent = match operation {
        "complete_nonrecurring" => {
            let second_obj = second
                .as_object()
                .ok_or_else(|| anyhow!("second must be an object"))?;
            let completed_status = second_obj
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("done");
            let completed_date = second_obj.get("completedDate").and_then(Value::as_str);
            let rerun = complete_nonrecurring(second_obj, completed_status, completed_date)?;
            Value::Object(rerun) == second.clone()
        }
        "create" => first.is_null() && second.is_object(),
        _ => false,
    };
    Ok(json!({ "idempotent": idempotent }))
}

pub fn apply_title_update(
    old_path: &str,
    frontmatter: &Map<String, Value>,
    new_title: &str,
    title_storage: &str,
) -> (String, bool, Map<String, Value>) {
    let mut updated = frontmatter.clone();
    updated.insert("title".into(), Value::String(new_title.to_string()));
    if title_storage == "filename" {
        let new_path = rename_task_path(old_path, new_title);
        let renamed = new_path != old_path;
        (new_path, renamed, updated)
    } else {
        (old_path.to_string(), false, updated)
    }
}

pub fn apply_explicit_rename(
    to_path: &str,
    frontmatter: &Map<String, Value>,
    title_storage: &str,
    update_references: bool,
) -> Value {
    let mut updated = frontmatter.clone();
    if title_storage == "filename" {
        updated.insert("title".into(), Value::String(title_from_path(to_path)));
    }
    json!({
        "path": to_path,
        "referencesUpdated": update_references,
        "frontmatter": updated,
    })
}

pub fn is_archived(frontmatter: &Map<String, Value>, path: &str, archive: &ArchiveConfig) -> bool {
    frontmatter
        .get(&archive.field)
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || frontmatter
            .get("tags")
            .map(|value| tags_contain_archive_tag(value, &archive.tag))
            .unwrap_or(false)
        || is_path_in_folder(path, &archive.folder)
}

pub fn ensure_archive_markers(frontmatter: &mut Map<String, Value>, archive: &ArchiveConfig) {
    frontmatter.insert(archive.field.clone(), Value::Bool(true));
    let mut tags = frontmatter.get("tags").map(read_tags).unwrap_or_default();
    if !archive.tag.trim().is_empty()
        && !tags.iter().any(|tag| {
            tag.eq_ignore_ascii_case(&archive.tag)
                || tag.eq_ignore_ascii_case(&format!("#{}", archive.tag))
        })
    {
        tags.push(archive.tag.clone());
    }
    frontmatter.insert(
        "tags".into(),
        Value::Array(tags.into_iter().map(Value::String).collect()),
    );
}

pub fn clear_archive_markers(frontmatter: &mut Map<String, Value>, archive: &ArchiveConfig) {
    frontmatter.insert(archive.field.clone(), Value::Bool(false));
    let tags: Vec<String> = frontmatter
        .get("tags")
        .map(read_tags)
        .unwrap_or_default()
        .into_iter()
        .filter(|tag| {
            !tag.eq_ignore_ascii_case(&archive.tag)
                && !tag.eq_ignore_ascii_case(&format!("#{}", archive.tag))
        })
        .collect();
    frontmatter.insert(
        "tags".into(),
        Value::Array(tags.into_iter().map(Value::String).collect()),
    );
}

pub fn archive_apply(
    frontmatter: &Map<String, Value>,
    archive: &ArchiveConfig,
    mode: &str,
) -> Value {
    if mode == "delete" {
        json!({ "deleted": true })
    } else {
        let mut updated = frontmatter.clone();
        ensure_archive_markers(&mut updated, archive);
        json!({ "deleted": false, "frontmatter": updated })
    }
}

pub fn ensure_delete_allowed(
    check_backlinks: bool,
    force: bool,
    broken_links: &[Value],
) -> Result<()> {
    if check_backlinks && !force && !broken_links.is_empty() {
        bail!("backlink requires force");
    }
    Ok(())
}

fn tags_contain_archive_tag(value: &Value, archive_tag: &str) -> bool {
    let archive_tag = archive_tag.trim();
    if archive_tag.is_empty() {
        return false;
    }
    let with_hash = format!("#{archive_tag}");
    match value {
        Value::Array(values) => values.iter().filter_map(Value::as_str).any(|tag| {
            tag.eq_ignore_ascii_case(archive_tag) || tag.eq_ignore_ascii_case(&with_hash)
        }),
        Value::String(value) => value.split(',').map(str::trim).any(|tag| {
            tag.eq_ignore_ascii_case(archive_tag) || tag.eq_ignore_ascii_case(&with_hash)
        }),
        _ => false,
    }
}

fn read_tags(value: &Value) -> Vec<String> {
    match value {
        Value::Array(values) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Value::String(value) => value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn is_path_in_folder(path: &str, folder: &str) -> bool {
    let folder = folder.trim_matches('/').replace('\\', "/");
    let path = path.trim_start_matches('/').replace('\\', "/");
    !folder.is_empty() && (path == folder || path.starts_with(&format!("{folder}/")))
}

fn rename_task_path(old_path: &str, title: &str) -> String {
    let stem = slugify(title);
    let old = Path::new(old_path);
    let parent = old.parent().map(|path| path.to_string_lossy().to_string());
    match parent {
        Some(parent) if !parent.is_empty() => format!("{parent}/{stem}.md"),
        _ => format!("{stem}.md"),
    }
}

fn title_from_path(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default()
        .replace(['-', '_'], " ")
        .trim()
        .to_string()
}

fn slugify(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_preserves_unknown_fields() {
        let original = json!({"title":"X","status":"open","vendor":"ZX-42"})
            .as_object()
            .unwrap()
            .clone();
        let patch = json!({"status":"done"}).as_object().unwrap().clone();
        let (changed, merged) = apply_patch(&original, &patch);
        assert!(changed);
        assert_eq!(merged.get("vendor").and_then(Value::as_str), Some("ZX-42"));
    }

    #[test]
    fn complete_nonrecurring_is_idempotent_against_completed_state() {
        let second = json!({"status":"done","completedDate":"2026-02-20"})
            .as_object()
            .unwrap()
            .clone();
        let result = check_idempotency(
            "complete_nonrecurring",
            &Value::Null,
            &Value::Object(second),
        )
        .unwrap();
        assert_eq!(
            result.get("idempotent").and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn title_update_renames_when_title_storage_is_filename() {
        let frontmatter = json!({"title":"Old"}).as_object().unwrap().clone();
        let (path, renamed, frontmatter) =
            apply_title_update("tasks/old.md", &frontmatter, "New Title", "filename");
        assert_eq!(path, "tasks/new-title.md");
        assert!(renamed);
        assert_eq!(
            frontmatter.get("title").and_then(Value::as_str),
            Some("New Title")
        );
    }

    #[test]
    fn explicit_rename_in_filename_mode_syncs_title() {
        let frontmatter = json!({"title":"Old","id":"abc"})
            .as_object()
            .unwrap()
            .clone();
        let result = apply_explicit_rename("tasks/New-Title.md", &frontmatter, "filename", true);
        assert_eq!(
            result.get("path").and_then(Value::as_str),
            Some("tasks/New-Title.md")
        );
        assert_eq!(
            result
                .get("frontmatter")
                .and_then(Value::as_object)
                .and_then(|obj| obj.get("title"))
                .and_then(Value::as_str),
            Some("New Title")
        );
        assert_eq!(
            result
                .get("frontmatter")
                .and_then(Value::as_object)
                .and_then(|obj| obj.get("id"))
                .and_then(Value::as_str),
            Some("abc")
        );
    }
}
