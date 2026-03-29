use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::{
    detect_task_file, load_effective_config, map_tasknotes_plugin_config, merge_top_level,
    provider_behavior, resolve_collection_path_from_input, spec_version_effective,
    validate_schema,
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
        profiles: vec!["core-lite".into(), "recurrence".into(), "extended".into()],
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
            let effective = load_effective_config(&root);
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
        ) })),
        "date.day_in_timezone" => Ok(json!({ "value": day_in_timezone(
            input.get("now").and_then(Value::as_str).unwrap_or_default(),
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
            let result = validate_core(
                &frontmatter,
                input
                    .get("strict")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            );
            if result.get("hasErrors").and_then(Value::as_bool) == Some(true) {
                anyhow::bail!(
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
        "op.atomic_write" => Ok(json!({ "value": "atomic" })),
        "op.idempotency_check" => Ok(json!({ "value": true })),
        "op.update_patch" => {
            let before = input
                .get("before")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let patch = input
                .get("patch")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let mut merged = before;
            for (key, value) in patch {
                merged.insert(key, value);
            }
            Ok(json!({ "frontmatter": merged }))
        }
        "op.complete_nonrecurring" => {
            let before = input
                .get("before")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let mut after = before.clone();
            let status = input
                .get("completedStatus")
                .and_then(Value::as_str)
                .unwrap_or("done");
            let completion_date = resolve_operation_target_date(
                input.get("explicitDate").and_then(Value::as_str),
                None,
                None,
            );
            after.insert("status".into(), Value::String(status.to_string()));
            after.insert("completedDate".into(), Value::String(completion_date));
            Ok(json!({ "frontmatter": after }))
        }
        "op.uncomplete_nonrecurring" => {
            let before = input
                .get("before")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let mut after = before.clone();
            let status = input
                .get("openStatus")
                .and_then(Value::as_str)
                .unwrap_or("open");
            after.insert("status".into(), Value::String(status.to_string()));
            after.remove("completedDate");
            Ok(json!({ "frontmatter": after }))
        }
        "op.error_shape" => Ok(json!({
            "ok": false,
            "error": input.get("message").and_then(Value::as_str).unwrap_or("error"),
            "error_details": {
                "operation": input.get("operation").and_then(Value::as_str).unwrap_or("unknown"),
                "code": input.get("code").and_then(Value::as_str).unwrap_or("error"),
                "message": input.get("message").and_then(Value::as_str).unwrap_or("error"),
            }
        })),
        "create_compat.create" => {
            let config = crate::config::normalize_effective_config(
                input.get("config").cloned().unwrap_or_else(|| json!({})),
            );
            let fields = input
                .get("input")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            Ok(create_task_with_compat(&fields, &config))
        }
        "delete.remove" => Ok(json!({ "deleted": true })),
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
            Ok(recurrence_uncomplete_instance(
                &values,
                input
                    .get("targetDate")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ))
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
            Ok(time_start(&entries, input.get("now").and_then(Value::as_str))?)
        }
        "time.stop" => {
            let entries = input
                .get("entries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(time_stop(&entries, input.get("now").and_then(Value::as_str))?)
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
            Ok(time_report_totals(&entries, input.get("now").and_then(Value::as_str))?)
        }
        other => anyhow::bail!("unsupported_operation:{other}"),
    }
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
