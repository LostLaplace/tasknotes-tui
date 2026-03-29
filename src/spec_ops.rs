use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use mdbase::Collection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::{
    detect_task_file, load_effective_config, map_tasknotes_plugin_config, merge_top_level,
    provider_behavior, resolve_collection_path_from_input, spec_version_effective, validate_schema,
};
use crate::create_compat::create_task_with_compat;
use crate::date::{
    day_in_timezone, get_date_part, has_time_component, is_before_date_safe, is_same_date_safe,
    parse_local_result, parse_utc_result, resolve_operation_target_date, validate_date_string,
};
use crate::field_mapping::{
    build_field_mapping, default_completed_status, default_field_mapping, denormalize_frontmatter,
    is_completed_status, mapping_json, normalize_frontmatter, resolve_display_title,
};
use crate::recurrence::{
    complete as recurrence_complete, effective_state as recurrence_effective_state,
    recalculate as recurrence_recalculate, skip_instance as recurrence_skip_instance,
    uncomplete_instance as recurrence_uncomplete_instance,
    unskip_instance as recurrence_unskip_instance, RecurrenceInput,
};
use crate::task_ops::{
    apply_explicit_rename, apply_title_update, archive_apply, atomic_write, check_idempotency,
    complete_nonrecurring, ensure_delete_allowed, mutate_with_validation, uncomplete_nonrecurring,
};
use crate::time_tracking::{
    auto_stop_on_complete as time_auto_stop_on_complete, remove_entry as time_remove_entry,
    replace_entries as time_replace_entries, report_totals as time_report_totals,
    start as time_start, stop as time_stop,
};
use crate::validation::{validate_core, validate_time_entries};

#[derive(Debug, Serialize)]
pub struct Metadata {
    implementation: String,
    version: String,
    spec_version: String,
    validation_modes: Vec<String>,
    profiles: Vec<String>,
    capabilities: Vec<String>,
    known_deviations: Vec<String>,
    compatibility_mode: String,
    configuration_providers: Vec<String>,
    configuration_fallback: String,
}

pub fn metadata() -> Metadata {
    Metadata {
        implementation: "tasknotes-tui".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        spec_version: "0.1.0-draft".into(),
        validation_modes: vec!["strict".into()],
        profiles: vec!["core-lite".into(), "recurrence".into()],
        capabilities: vec![
            "date".into(),
            "field-mapping".into(),
            "recurrence".into(),
            "create-compat".into(),
            "ops-core".into(),
            "claim".into(),
            "config-lite".into(),
            "validation-core".into(),
            "time-tracking".into(),
            "archive".into(),
            "rename".into(),
        ],
        known_deviations: vec!["tasknotes-tui-bridge-partial".into()],
        compatibility_mode: "bridge".into(),
        configuration_providers: vec![
            "cli_flag_path".into(),
            "env:TASKNOTES_PATH".into(),
            "user_config_file".into(),
            "cwd_fallback".into(),
        ],
        configuration_fallback: "cwd".into(),
    }
}

#[derive(Debug, Deserialize)]
pub struct BridgeRequest {
    pub operation: String,
    #[serde(default)]
    pub input: Value,
}

pub fn execute(operation: &str, input: &Value) -> Value {
    let reply = match execute_inner(operation, input) {
        Ok(result) => json!({ "ok": true, "result": result }),
        Err(error) => json!({ "ok": false, "error": error.to_string() }),
    };
    reply
}

fn execute_inner(operation: &str, input: &Value) -> Result<Value> {
    match operation {
        "meta.claim" => Ok(serde_json::to_value(metadata())?),
        "meta.has_capability" => {
            let capability = input
                .get("capability")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let claim = metadata();
            Ok(json!({ "value": claim.capabilities.iter().any(|entry| entry == capability) }))
        }
        "meta.has_profile" => {
            let profile = input
                .get("profile")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let claim = metadata();
            Ok(json!({ "value": claim.profiles.iter().any(|entry| entry == profile) }))
        }
        "config.resolve_collection_path" => {
            Ok(json!({ "value": resolve_collection_path_from_input(input) }))
        }
        "config.spec_version_effective" => Ok(spec_version_effective(input)),
        "config.merge_top_level" => {
            let providers = input
                .get("providers")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(json!({ "value": merge_top_level(&providers) }))
        }
        "config.map_tasknotes_plugin" => Ok(
            json!({ "value": map_tasknotes_plugin_config(input.get("data").unwrap_or(&json!({}))) }),
        ),
        "config.provider_behavior" => Ok(provider_behavior(input)?),
        "config.validate_schema" => Ok(validate_schema(input)?),
        "config.detect_task_file" => {
            let root = input
                .get("root")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            let effective = if let Some(task_detection) =
                input.get("taskDetection").and_then(Value::as_object)
            {
                let mut cfg = load_effective_config(&root);
                if let Some(method) = task_detection.get("method").and_then(Value::as_str) {
                    cfg.task_detection.method = method.to_string();
                    cfg.task_detection.methods = vec![method.to_string()];
                }
                if let Some(methods) = task_detection.get("methods").and_then(Value::as_array) {
                    let parsed: Vec<String> = methods
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect();
                    if !parsed.is_empty() {
                        cfg.task_detection.methods = parsed;
                    }
                }
                if let Some(combine) = task_detection.get("combine").and_then(Value::as_str) {
                    cfg.task_detection.combine = combine.to_string();
                }
                if let Some(tag) = task_detection.get("tag").and_then(Value::as_str) {
                    cfg.task_detection.tag = tag.to_string();
                }
                if let Some(name) = task_detection.get("property_name").and_then(Value::as_str) {
                    cfg.task_detection.property_name = Some(name.to_string());
                }
                if task_detection.get("property_value").is_some() {
                    cfg.task_detection.property_value = task_detection
                        .get("property_value")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
                if let Some(folder) = task_detection.get("default_folder").and_then(Value::as_str) {
                    cfg.task_detection.default_folder = folder.to_string();
                }
                if let Some(excluded) = task_detection.get("excluded_folders") {
                    cfg.task_detection.excluded_folders = match excluded {
                        Value::Array(values) => values
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect(),
                        Value::String(value) => value
                            .split(',')
                            .map(str::trim)
                            .filter(|v| !v.is_empty())
                            .map(str::to_string)
                            .collect(),
                        _ => Vec::new(),
                    };
                }
                cfg
            } else {
                load_effective_config(&root)
            };
            let frontmatter = input
                .get("frontmatter")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let body = input
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let file_path = input
                .get("filePath")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Ok(
                json!({ "value": detect_task_file(&effective.task_detection, &frontmatter, body, file_path) }),
            )
        }
        "date.parse_utc" => parse_utc_result(
            input
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        "date.parse_local" => parse_local_result(
            input
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        "date.validate" => Ok(
            json!({ "value": validate_date_string(input.get("value").and_then(Value::as_str).unwrap_or_default())? }),
        ),
        "date.get_part" => Ok(
            json!({ "value": get_date_part(input.get("value").and_then(Value::as_str).unwrap_or_default()) }),
        ),
        "date.has_time" => Ok(
            json!({ "value": has_time_component(input.get("value").and_then(Value::as_str).unwrap_or_default()) }),
        ),
        "date.is_same" => Ok(json!({ "value": is_same_date_safe(
            input.get("a").and_then(Value::as_str).unwrap_or_default(),
            input.get("b").and_then(Value::as_str).unwrap_or_default()
        ) })),
        "date.is_before" => Ok(json!({ "value": is_before_date_safe(
            input.get("a").and_then(Value::as_str).unwrap_or_default(),
            input.get("b").and_then(Value::as_str).unwrap_or_default()
        ) })),
        "date.resolve_operation_target" => Ok(json!({ "value": resolve_operation_target_date(
            input.get("explicitDate").and_then(Value::as_str),
            input.get("scheduled").and_then(Value::as_str),
            input.get("due").and_then(Value::as_str),
        )? })),
        "date.day_in_timezone" => Ok(json!({ "value": day_in_timezone(
            input.get("instant").or_else(|| input.get("now")).and_then(Value::as_str).unwrap_or_default(),
            input.get("timezone").and_then(Value::as_str).unwrap_or("UTC"),
        )? })),
        "field.default_mapping" => Ok(mapping_json(&default_field_mapping())),
        "field.build_mapping" => {
            let fields = input
                .get("fields")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let mapping =
                build_field_mapping(&fields, input.get("displayNameKey").and_then(Value::as_str));
            Ok(mapping_json(&mapping))
        }
        "field.normalize" => {
            let fields = input
                .get("fields")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let mapping =
                build_field_mapping(&fields, input.get("displayNameKey").and_then(Value::as_str));
            let frontmatter = input
                .get("frontmatter")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            Ok(json!({ "normalized": normalize_frontmatter(&frontmatter, &mapping) }))
        }
        "field.denormalize" => {
            let fields = input
                .get("fields")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let mapping =
                build_field_mapping(&fields, input.get("displayNameKey").and_then(Value::as_str));
            let role_data = input
                .get("roleData")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            Ok(json!({ "denormalized": denormalize_frontmatter(&role_data, &mapping) }))
        }
        "field.resolve_display_title" => {
            let fields = input
                .get("fields")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let mapping =
                build_field_mapping(&fields, input.get("displayNameKey").and_then(Value::as_str));
            let frontmatter = input
                .get("frontmatter")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            Ok(
                json!({ "value": resolve_display_title(&frontmatter, &mapping, input.get("taskPath").and_then(Value::as_str)) }),
            )
        }
        "field.is_completed_status" => {
            let fields = input
                .get("fields")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let mapping =
                build_field_mapping(&fields, input.get("displayNameKey").and_then(Value::as_str));
            Ok(
                json!({ "value": is_completed_status(&mapping, input.get("status").and_then(Value::as_str)) }),
            )
        }
        "field.default_completed_status" => {
            let fields = input
                .get("fields")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let mapping =
                build_field_mapping(&fields, input.get("displayNameKey").and_then(Value::as_str));
            Ok(json!({ "value": default_completed_status(&mapping) }))
        }
        "validation.core_evaluate" => {
            let payload = input.as_object().cloned().unwrap_or_default();
            Ok(validate_core(
                &payload,
                input
                    .get("rejectUnknownFields")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ))
        }
        "validation.time_entries" => {
            let entries = input
                .get("entries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let result = validate_time_entries(&entries);
            if let Some(error) = result.get("__error__").and_then(Value::as_str) {
                anyhow::bail!("{error}");
            }
            Ok(result)
        }
        "op.mutate_with_validation" => {
            let frontmatter = input
                .get("frontmatter")
                .or_else(|| input.get("fields"))
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            mutate_with_validation(
                &frontmatter,
                input
                    .get("strict")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            )
        }
        "op.atomic_write" => {
            let original = input
                .get("original")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let patch = input
                .get("patch")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            Ok(atomic_write(
                &original,
                &patch,
                input
                    .get("simulateFailureAfterWrite")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ))
        }
        "op.idempotency_check" => check_idempotency(
            input
                .get("operation")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            input.get("first").unwrap_or(&Value::Null),
            input.get("second").unwrap_or(&Value::Null),
        ),
        "op.update_patch" => {
            let before = input
                .get("original")
                .or_else(|| input.get("before"))
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let patch = input
                .get("patch")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let (changed, merged) = crate::task_ops::apply_patch(&before, &patch);
            Ok(json!({ "changed": changed, "frontmatter": merged }))
        }
        "op.complete_nonrecurring" => {
            let before = input
                .get("frontmatter")
                .or_else(|| input.get("before"))
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let status = input
                .get("completedStatus")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    input
                        .get("completedValues")
                        .and_then(Value::as_array)
                        .and_then(|arr| arr.first())
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "done".to_string());
            Ok(Value::Object(complete_nonrecurring(
                &before,
                &status,
                input.get("explicitDate").and_then(Value::as_str),
            )?))
        }
        "op.uncomplete_nonrecurring" => {
            let before = input
                .get("frontmatter")
                .or_else(|| input.get("before"))
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let status = input
                .get("defaultStatus")
                .or_else(|| input.get("openStatus"))
                .and_then(Value::as_str)
                .unwrap_or("open");
            Ok(Value::Object(uncomplete_nonrecurring(
                &before,
                status,
                input
                    .get("clearCompletedDate")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            )))
        }
        "archive.apply" => {
            let mut archive = crate::config::EffectiveConfig::default().archive;
            if let Some(tag) = input.get("archiveTag").and_then(Value::as_str) {
                archive.tag = tag.trim().to_string();
            }
            if let Some(field) = input.get("archiveField").and_then(Value::as_str) {
                archive.field = field.trim().to_string();
            }
            Ok(archive_apply(
                &input
                    .get("frontmatter")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default(),
                &archive,
                input.get("mode").and_then(Value::as_str).unwrap_or("tag"),
            ))
        }
        "rename.apply" => Ok(apply_explicit_rename(
            input
                .get("toPath")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            &input
                .get("frontmatter")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
            input
                .get("titleStorage")
                .and_then(Value::as_str)
                .unwrap_or("frontmatter"),
            input
                .get("updateReferences")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        )),
        "rename.title_storage_interaction" => {
            let old_path = input
                .get("oldPath")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let new_title = input
                .get("newTitle")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let title_storage = input
                .get("titleStorage")
                .and_then(Value::as_str)
                .unwrap_or("frontmatter");
            let frontmatter = input
                .get("frontmatter")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_else(|| {
                    let mut frontmatter = serde_json::Map::new();
                    frontmatter.insert("title".into(), Value::String("Old".into()));
                    frontmatter
                });
            let (path, renamed, frontmatter) =
                apply_title_update(old_path, &frontmatter, new_title, title_storage);
            Ok(json!({
                "path": path,
                "renamed": renamed,
                "frontmatter": frontmatter,
            }))
        }
        "op.error_shape" => Ok(json!({
            "operation": input.get("operation").and_then(Value::as_str).unwrap_or("unknown"),
            "code": input.get("code").and_then(Value::as_str).unwrap_or("error"),
            "message": input.get("message").and_then(Value::as_str).unwrap_or("error"),
            "field": input.get("field").and_then(Value::as_str),
        })),
        "create_compat.create" => Ok(create_task_with_compat(input)?),
        "delete.remove" => {
            ensure_delete_allowed(
                input.get("checkBacklinks").and_then(Value::as_bool) == Some(true),
                input.get("force").and_then(Value::as_bool) == Some(true),
                input
                    .get("brokenLinks")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
            )?;
            if let Some(root) = input
                .get("root")
                .or_else(|| input.get("collectionRoot"))
                .and_then(Value::as_str)
            {
                let collection = Collection::open(Path::new(root)).map_err(|error| {
                    anyhow::anyhow!("failed to open mdbase collection: {}", error)
                })?;
                let payload = json!({
                    "path": input.get("path").and_then(Value::as_str).unwrap_or_default(),
                    "check_backlinks": input.get("checkBacklinks").and_then(Value::as_bool).unwrap_or(false),
                });
                let result = collection.delete(&payload);
                if let Some(error) = result.get("error") {
                    anyhow::bail!("{}", error);
                }
                Ok(result)
            } else {
                Ok(json!({ "deleted": true }))
            }
        }
        "recurrence.complete" => {
            let payload = recurrence_input(input);
            Ok(recurrence_complete(
                &payload,
                input
                    .get("completionDate")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )?)
        }
        "recurrence.recalculate" => {
            let payload = recurrence_input(input);
            Ok(recurrence_recalculate(
                &payload,
                input
                    .get("referenceDate")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )?)
        }
        "recurrence.uncomplete_instance" => {
            let instances = input
                .get("completeInstances")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let values: Vec<String> = instances
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            let mut result = recurrence_uncomplete_instance(
                &values,
                input
                    .get("targetDate")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
            if let Some(recurrence) = input.get("recurrence").and_then(Value::as_str) {
                if let Some(obj) = result.as_object_mut() {
                    obj.insert(
                        "updatedRecurrence".into(),
                        Value::String(recurrence.to_string()),
                    );
                }
            }
            Ok(result)
        }
        "recurrence.skip_instance" => {
            let instances = input
                .get("skippedInstances")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let values: Vec<String> = instances
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            Ok(recurrence_skip_instance(
                &values,
                input
                    .get("targetDate")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ))
        }
        "recurrence.unskip_instance" => {
            let instances = input
                .get("skippedInstances")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let values: Vec<String> = instances
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            Ok(recurrence_unskip_instance(
                &values,
                input
                    .get("targetDate")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ))
        }
        "recurrence.effective_state" => {
            let completed = input
                .get("completeInstances")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let skipped = input
                .get("skippedInstances")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let completed: Vec<String> = completed
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            let skipped: Vec<String> = skipped
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            Ok(recurrence_effective_state(
                &completed,
                &skipped,
                input
                    .get("targetDate")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ))
        }
        "time.start" => {
            let entries = input
                .get("entries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(time_start(
                &entries,
                input.get("now").and_then(Value::as_str),
            )?)
        }
        "time.stop" => {
            let entries = input
                .get("entries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(time_stop(
                &entries,
                input.get("now").and_then(Value::as_str),
            )?)
        }
        "time.replace_entries" => {
            let entries = input
                .get("entries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(time_replace_entries(
                &entries,
                input.get("dateModified").and_then(Value::as_str),
            )?)
        }
        "time.remove_entry" => {
            let entries = input
                .get("entries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let selector = input.get("selector").cloned().unwrap_or_else(|| json!({}));
            Ok(time_remove_entry(
                &entries,
                &selector,
                input.get("dateModified").and_then(Value::as_str),
            )?)
        }
        "time.auto_stop_on_complete" => {
            let entries = input
                .get("taskEntries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(time_auto_stop_on_complete(
                input
                    .get("autoStopOnComplete")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                input
                    .get("isCompletionTransition")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                &entries,
            )?)
        }
        "time.report_totals" => {
            let entries = input
                .get("entries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(time_report_totals(
                &entries,
                input.get("now").and_then(Value::as_str),
            )?)
        }
        "link.resolve" => Ok(resolve_link(input)?),
        "dependency.missing_target_behavior" => {
            let severity = if input
                .get("unresolvedTargetSeverity")
                .and_then(Value::as_str)
                == Some("error")
            {
                "error"
            } else {
                "warning"
            };
            let require_resolved_uid_on_write = input
                .get("requireResolvedUidOnWrite")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let on_write = input
                .get("onWrite")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if require_resolved_uid_on_write && on_write {
                anyhow::bail!("unresolved_dependency_target require_resolved_uid_on_write");
            }
            let blocked = input
                .get("treatMissingTargetAsBlocked")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            Ok(json!({
                "blocked": blocked,
                "issue": "unresolved_dependency_target",
                "severity": severity,
            }))
        }
        other => anyhow::bail!("unsupported_operation:{other}"),
    }
}

fn resolve_link(input: &Value) -> Result<Value> {
    let raw = input.get("raw").and_then(Value::as_str).unwrap_or_default();
    let source_path = input
        .get("sourcePath")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .replace('\\', "/");
    let source_dir = source_path
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or("")
        .to_string();
    let candidates: Vec<String> = input
        .get("candidates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    let extensions: Vec<String> = input
        .get("extensions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|ext| ext.starts_with('.'))
        .map(str::to_string)
        .collect();

    let target = parse_link_target(raw)?;
    let resolved = if is_explicit_path_link(raw) {
        if target.starts_with('/') {
            normalize_resolved_path(target.trim_start_matches('/'))?
        } else {
            normalize_resolved_path(&join_posix(&source_dir, &target))?
        }
    } else if target.starts_with("./") || target.starts_with("../") {
        let parent_hops = target
            .split('/')
            .take_while(|segment| *segment == "..")
            .count();
        let source_depth = source_dir
            .split('/')
            .filter(|segment| !segment.is_empty())
            .count();
        if parent_hops >= source_depth && target.starts_with("../") {
            anyhow::bail!("path_traversal");
        }
        let mut candidate = normalize_resolved_path(&join_posix(&source_dir, &target))?;
        if !has_extension(&candidate) {
            candidate.push_str(".md");
        }
        if !candidate.contains('/') {
            anyhow::bail!(
                "unresolved_link_target:{}",
                candidate.trim_end_matches(".md")
            );
        }
        candidate
    } else if target.starts_with('/') {
        let mut candidate = target.trim_start_matches('/').to_string();
        if !has_extension(&candidate) {
            candidate.push_str(".md");
        }
        candidate
    } else if target.contains('/') {
        let mut candidate = target;
        if !has_extension(&candidate) {
            candidate.push_str(".md");
        }
        candidate
    } else if candidates.len() > 1 {
        if let Some(path) = choose_candidate_by_extension(&target, &candidates, &extensions)? {
            path
        } else {
            choose_simple_name_candidate(&candidates, &source_path)?
        }
    } else if let Some(candidate) = candidates.first() {
        candidate.clone()
    } else {
        anyhow::bail!("unresolved_link_target:{target}");
    };

    let path = normalize_resolved_path(&resolved)?;
    if path.is_empty() {
        anyhow::bail!("unresolved_link_target");
    }
    Ok(json!({ "path": path }))
}

fn parse_link_target(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    anyhow::ensure!(!trimmed.is_empty(), "invalid_link_format");

    if let Some(inner) = trimmed
        .strip_prefix("[[")
        .and_then(|value| value.strip_suffix("]]"))
    {
        let target = inner
            .split('|')
            .next()
            .unwrap_or_default()
            .split('#')
            .next()
            .unwrap_or_default()
            .trim();
        anyhow::ensure!(!target.is_empty(), "invalid_link_format");
        return Ok(target.to_string());
    }

    if let Some(start) = trimmed.rfind('(') {
        if trimmed.starts_with('[') && trimmed.ends_with(')') {
            let target = trimmed[start + 1..trimmed.len() - 1].trim();
            anyhow::ensure!(!target.is_empty(), "invalid_link_format");
            return Ok(target.to_string());
        }
    }

    Ok(trimmed.to_string())
}

fn is_explicit_path_link(raw: &str) -> bool {
    let trimmed = raw.trim();
    (trimmed.starts_with('[') && !trimmed.starts_with("[["))
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || trimmed.starts_with('/')
        || regex::Regex::new(r"^[A-Za-z0-9._-]+/.+")
            .expect("valid regex")
            .is_match(trimmed)
}

fn normalize_resolved_path(value: &str) -> Result<String> {
    let mut parts: Vec<&str> = Vec::new();
    let normalized_value = value.replace('\\', "/");
    for part in normalized_value.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    anyhow::bail!("path_traversal");
                }
            }
            other => parts.push(other),
        }
    }
    let normalized = parts.join("/");
    anyhow::ensure!(
        normalized != ".." && !normalized.starts_with("../"),
        "path_traversal"
    );
    Ok(normalized)
}

fn join_posix(base: &str, tail: &str) -> String {
    if base.is_empty() {
        tail.to_string()
    } else {
        format!("{base}/{tail}")
    }
}

fn has_extension(value: &str) -> bool {
    value.rsplit('/').next().unwrap_or_default().contains('.')
}

fn choose_candidate_by_extension(
    target: &str,
    candidates: &[String],
    extensions: &[String],
) -> Result<Option<String>> {
    for extension in extensions {
        let suffix = format!("{target}{extension}");
        let matches: Vec<String> = candidates
            .iter()
            .filter(|candidate| candidate.ends_with(&suffix))
            .cloned()
            .collect();
        if matches.len() == 1 {
            return Ok(matches.first().cloned());
        }
        if matches.len() > 1 {
            return Ok(Some(choose_by_tiebreakers(&matches, "")?));
        }
    }
    Ok(None)
}

fn choose_simple_name_candidate(candidates: &[String], source_path: &str) -> Result<String> {
    let source_dir = source_path
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or("")
        .replace('\\', "/");
    let same_dir_count = candidates
        .iter()
        .filter(|candidate| {
            candidate
                .rsplit_once('/')
                .map(|(dir, _)| dir)
                .unwrap_or("")
                .replace('\\', "/")
                == source_dir
        })
        .count();
    let mut segment_counts = candidates
        .iter()
        .map(|candidate| candidate.split('/').filter(|part| !part.is_empty()).count());
    let first_segments = segment_counts.next().unwrap_or(0);
    let differing_segments = segment_counts.any(|count| count != first_segments);
    if same_dir_count > 0 || differing_segments {
        anyhow::bail!("ambiguous_link");
    }
    choose_by_tiebreakers(candidates, source_path)
}

fn choose_by_tiebreakers(candidates: &[String], source_path: &str) -> Result<String> {
    anyhow::ensure!(!candidates.is_empty(), "ambiguous_link");
    if candidates.len() == 1 {
        return Ok(candidates[0].clone());
    }

    let source_dir = source_path
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or("")
        .replace('\\', "/");
    let same_dir: Vec<String> = candidates
        .iter()
        .filter(|candidate| {
            candidate
                .rsplit_once('/')
                .map(|(dir, _)| dir)
                .unwrap_or("")
                .replace('\\', "/")
                == source_dir
        })
        .cloned()
        .collect();
    let pool = if same_dir.len() == 1 {
        return Ok(same_dir[0].clone());
    } else if same_dir.len() > 1 {
        same_dir
    } else {
        candidates.to_vec()
    };

    let min_segments = pool
        .iter()
        .map(|candidate| candidate.split('/').filter(|part| !part.is_empty()).count())
        .min()
        .unwrap_or(0);
    let mut shortest: Vec<String> = pool
        .into_iter()
        .filter(|candidate| {
            candidate.split('/').filter(|part| !part.is_empty()).count() == min_segments
        })
        .collect();
    shortest.sort();
    shortest
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("ambiguous_link"))
}

fn recurrence_input(input: &Value) -> RecurrenceInput {
    RecurrenceInput {
        recurrence: input
            .get("recurrence")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        recurrence_anchor: input
            .get("recurrenceAnchor")
            .and_then(Value::as_str)
            .unwrap_or("scheduled")
            .to_string(),
        scheduled: input
            .get("scheduled")
            .and_then(Value::as_str)
            .map(str::to_string),
        due: input.get("due").and_then(Value::as_str).map(str::to_string),
        date_created: input
            .get("dateCreated")
            .and_then(Value::as_str)
            .map(str::to_string),
        complete_instances: input
            .get("completeInstances")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        skipped_instances: input
            .get("skippedInstances")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

pub fn run_stdio() -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: BridgeRequest = serde_json::from_str(&line)?;
        let response = execute(&request.operation, &request.input);
        serde_json::to_writer(&mut stdout, &response)?;
        writeln!(&mut stdout)?;
        stdout.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_apply_claimed_and_sets_archive_markers() {
        let claim = execute("meta.claim", &json!({}));
        assert_eq!(
            claim["result"]["capabilities"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .any(|cap| cap == "archive"),
            true
        );
        assert_eq!(
            claim["result"]["capabilities"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .any(|cap| cap == "dependencies" || cap == "reminders" || cap == "links"),
            false
        );
        assert_eq!(
            claim["result"]["profiles"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .any(|profile| profile == "extended"),
            false
        );

        let result = execute(
            "archive.apply",
            &json!({
                "frontmatter": {
                    "title": "Archive me",
                    "status": "open",
                    "tags": ["task"]
                },
                "mode": "tag"
            }),
        );

        assert_eq!(result["ok"].as_bool(), Some(true));
        assert_eq!(result["result"]["deleted"].as_bool(), Some(false));
        assert_eq!(
            result["result"]["frontmatter"]["archived"].as_bool(),
            Some(true)
        );
        assert!(result["result"]["frontmatter"]["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|tag| tag == "archived"));
    }

    #[test]
    fn rename_ops_are_claimed_and_follow_title_storage_rules() {
        let claim = execute("meta.claim", &json!({}));
        assert_eq!(
            claim["result"]["capabilities"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .any(|cap| cap == "rename"),
            true
        );

        let explicit = execute(
            "rename.apply",
            &json!({
                "toPath": "tasks/New-Title.md",
                "titleStorage": "filename",
                "updateReferences": true,
                "frontmatter": {
                    "title": "Old",
                    "id": "abc"
                }
            }),
        );
        assert_eq!(
            explicit["result"]["path"].as_str(),
            Some("tasks/New-Title.md")
        );
        assert_eq!(
            explicit["result"]["referencesUpdated"].as_bool(),
            Some(true)
        );
        assert_eq!(
            explicit["result"]["frontmatter"]["title"].as_str(),
            Some("New Title")
        );
        assert_eq!(
            explicit["result"]["frontmatter"]["id"].as_str(),
            Some("abc")
        );

        let title_filename = execute(
            "rename.title_storage_interaction",
            &json!({
                "titleStorage": "filename",
                "oldPath": "tasks/Old.md",
                "newTitle": "New Title"
            }),
        );
        assert_eq!(
            title_filename["result"]["path"].as_str(),
            Some("tasks/new-title.md")
        );
        assert_eq!(title_filename["result"]["renamed"].as_bool(), Some(true));
        assert_eq!(
            title_filename["result"]["frontmatter"]["title"].as_str(),
            Some("New Title")
        );

        let title_frontmatter = execute(
            "rename.title_storage_interaction",
            &json!({
                "titleStorage": "frontmatter",
                "oldPath": "tasks/Old.md",
                "newTitle": "New Title"
            }),
        );
        assert_eq!(
            title_frontmatter["result"]["path"].as_str(),
            Some("tasks/Old.md")
        );
        assert_eq!(
            title_frontmatter["result"]["renamed"].as_bool(),
            Some(false)
        );
        assert_eq!(
            title_frontmatter["result"]["frontmatter"]["title"].as_str(),
            Some("New Title")
        );
    }
}
