use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TitleConfig {
    pub storage: String,
    pub filename_format: String,
    pub custom_filename_template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusConfig {
    pub values: Vec<String>,
    pub default: String,
    pub completed_values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultsConfig {
    pub status: String,
    pub priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDetectionConfig {
    pub method: String,
    pub methods: Vec<String>,
    pub combine: String,
    pub tag: String,
    pub property_name: Option<String>,
    pub property_value: Option<String>,
    pub default_folder: String,
    pub excluded_folders: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveConfig {
    pub move_on_archive: bool,
    pub folder: String,
    pub tag: String,
    pub field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveConfig {
    pub spec_version: String,
    pub mapping: BTreeMap<String, String>,
    pub title: TitleConfig,
    pub status: StatusConfig,
    pub defaults: DefaultsConfig,
    pub task_detection: TaskDetectionConfig,
    pub archive: ArchiveConfig,
}

impl Default for EffectiveConfig {
    fn default() -> Self {
        let mapping = BTreeMap::from([
            ("title".into(), "title".into()),
            ("status".into(), "status".into()),
            ("completed_date".into(), "completedDate".into()),
            ("date_created".into(), "dateCreated".into()),
            ("date_modified".into(), "dateModified".into()),
            ("priority".into(), "priority".into()),
            ("due".into(), "due".into()),
            ("scheduled".into(), "scheduled".into()),
            ("projects".into(), "projects".into()),
            ("recurrence".into(), "recurrence".into()),
            ("recurrence_anchor".into(), "recurrenceAnchor".into()),
            ("complete_instances".into(), "completeInstances".into()),
            ("skipped_instances".into(), "skippedInstances".into()),
            ("time_entries".into(), "timeEntries".into()),
        ]);

        Self {
            spec_version: "0.1.0-draft".into(),
            mapping,
            title: TitleConfig {
                storage: "frontmatter".into(),
                filename_format: "title".into(),
                custom_filename_template: None,
            },
            status: StatusConfig {
                values: vec![
                    "open".into(),
                    "in-progress".into(),
                    "done".into(),
                    "cancelled".into(),
                ],
                default: "open".into(),
                completed_values: vec!["done".into(), "cancelled".into()],
            },
            defaults: DefaultsConfig {
                status: "open".into(),
                priority: "normal".into(),
            },
            task_detection: TaskDetectionConfig {
                method: "tag".into(),
                methods: vec!["tag".into()],
                combine: "or".into(),
                tag: "task".into(),
                property_name: None,
                property_value: None,
                default_folder: "TaskNotes/Tasks".into(),
                excluded_folders: Vec::new(),
            },
            archive: ArchiveConfig {
                move_on_archive: false,
                folder: "TaskNotes/Archive".into(),
                tag: "archived".into(),
                field: "archived".into(),
            },
        }
    }
}

pub fn resolve_collection_path_from_input(input: &Value) -> PathBuf {
    let payload = input.as_object().cloned().unwrap_or_default();
    let chosen = ["flagPath", "envPath", "persistedPath", "cwd"]
        .iter()
        .filter_map(|key| payload.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("TASKNOTES_PATH")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    if chosen.is_absolute() {
        chosen
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(chosen)
    }
}

pub fn merge_top_level(providers: &[Value]) -> Value {
    let mut out = Map::new();
    for provider in providers {
        for (key, value) in provider
            .as_object()
            .into_iter()
            .flat_map(|object| object.iter())
        {
            out.insert(key.clone(), value.clone());
        }
    }
    Value::Object(out)
}

fn map_tasknotes_role_to_spec_role(role: &str) -> String {
    match role {
        "completedDate" => "completed_date".into(),
        "dateCreated" => "date_created".into(),
        "dateModified" => "date_modified".into(),
        "recurrenceAnchor" => "recurrence_anchor".into(),
        "completeInstances" => "complete_instances".into(),
        "skippedInstances" => "skipped_instances".into(),
        "timeEntries" => "time_entries".into(),
        "timeEstimate" => "time_estimate".into(),
        "blockedBy" => "blocked_by".into(),
        other => camel_to_snake(other),
    }
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

pub fn map_tasknotes_plugin_config(data: &Value) -> Value {
    let source = data.as_object().cloned().unwrap_or_default();
    let mut out = Map::new();

    if let Some(mapping_obj) = source.get("fieldMapping").and_then(Value::as_object) {
        let mut mapping = Map::new();
        for (role, field_name) in mapping_obj {
            if let Some(field_name) = field_name.as_str().filter(|v| !v.trim().is_empty()) {
                mapping.insert(
                    map_tasknotes_role_to_spec_role(role),
                    Value::String(field_name.to_string()),
                );
            }
        }
        if !mapping.is_empty() {
            out.insert("mapping".into(), Value::Object(mapping));
        }
    }

    if source.contains_key("storeTitleInFilename")
        || source.contains_key("taskFilenameFormat")
        || source.contains_key("customFilenameTemplate")
    {
        let mut title = Map::new();
        if let Some(value) = source.get("storeTitleInFilename").and_then(Value::as_bool) {
            title.insert(
                "storage".into(),
                Value::String(if value { "filename" } else { "frontmatter" }.into()),
            );
        }
        if let Some(value) = source.get("taskFilenameFormat").and_then(Value::as_str) {
            title.insert("filename_format".into(), Value::String(value.to_string()));
        }
        if let Some(value) = source.get("customFilenameTemplate").and_then(Value::as_str) {
            title.insert(
                "custom_filename_template".into(),
                Value::String(value.to_string()),
            );
        }
        out.insert("title".into(), Value::Object(title));
    }

    if let Some(defaults) = source
        .get("taskCreationDefaults")
        .and_then(Value::as_object)
    {
        let mut templating = Map::new();
        if let Some(value) = defaults.get("useBodyTemplate").and_then(Value::as_bool) {
            templating.insert("enabled".into(), Value::Bool(value));
        }
        if let Some(value) = defaults.get("bodyTemplate").and_then(Value::as_str) {
            templating.insert("template_path".into(), Value::String(value.to_string()));
        }
        out.insert("templating".into(), Value::Object(templating));
    }

    if let Some(statuses) = source.get("customStatuses").and_then(Value::as_array) {
        let values: Vec<Value> = statuses
            .iter()
            .filter_map(|entry| entry.get("value").and_then(Value::as_str))
            .map(|v| Value::String(v.to_string()))
            .collect();
        let completed: Vec<Value> = statuses
            .iter()
            .filter(|entry| entry.get("isCompleted").and_then(Value::as_bool) == Some(true))
            .filter_map(|entry| entry.get("value").and_then(Value::as_str))
            .map(|v| Value::String(v.to_string()))
            .collect();

        let mut status = Map::new();
        if !values.is_empty() {
            status.insert("values".into(), Value::Array(values));
        }
        if let Some(default) = source.get("defaultTaskStatus").and_then(Value::as_str) {
            status.insert("default".into(), Value::String(default.to_string()));
        }
        if !completed.is_empty() {
            status.insert("completed_values".into(), Value::Array(completed));
        }
        out.insert("status".into(), Value::Object(status));
    }

    let mut defaults = Map::new();
    if let Some(value) = source.get("defaultTaskStatus").and_then(Value::as_str) {
        defaults.insert("status".into(), Value::String(value.to_string()));
    }
    if let Some(value) = source.get("defaultTaskPriority").and_then(Value::as_str) {
        defaults.insert("priority".into(), Value::String(value.to_string()));
    }
    if !defaults.is_empty() {
        out.insert("defaults".into(), Value::Object(defaults));
    }

    if let Some(method) = source
        .get("taskIdentificationMethod")
        .and_then(Value::as_str)
    {
        let mut detection = Map::new();
        detection.insert("method".into(), Value::String(method.to_string()));
        if let Some(value) = source.get("taskTag").and_then(Value::as_str) {
            detection.insert("tag".into(), Value::String(value.to_string()));
        }
        if let Some(value) = source.get("taskPropertyName").and_then(Value::as_str) {
            detection.insert("property_name".into(), Value::String(value.to_string()));
        }
        if let Some(value) = source.get("taskPropertyValue").and_then(Value::as_str) {
            detection.insert("property_value".into(), Value::String(value.to_string()));
        }
        if let Some(value) = source.get("tasksFolder").and_then(Value::as_str) {
            detection.insert("default_folder".into(), Value::String(value.to_string()));
        }
        if let Some(value) = source.get("excludedFolders") {
            detection.insert("excluded_folders".into(), value.clone());
        }
        out.insert("task_detection".into(), Value::Object(detection));
    }

    if source.contains_key("autoStopTimeTrackingOnComplete")
        || source.contains_key("autoStopTimeTrackingNotification")
    {
        let mut time_tracking = Map::new();
        if let Some(value) = source
            .get("autoStopTimeTrackingOnComplete")
            .and_then(Value::as_bool)
        {
            time_tracking.insert("auto_stop_on_complete".into(), Value::Bool(value));
        }
        if let Some(value) = source
            .get("autoStopTimeTrackingNotification")
            .and_then(Value::as_bool)
        {
            time_tracking.insert("auto_stop_notification".into(), Value::Bool(value));
        }
        out.insert("time_tracking".into(), Value::Object(time_tracking));
    }

    if source.contains_key("tasksFolder") || source.contains_key("excludedFolders") {
        let mut task_detection = out
            .get("task_detection")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some(value) = source.get("tasksFolder").and_then(Value::as_str) {
            task_detection.insert("default_folder".into(), Value::String(value.to_string()));
        }
        if let Some(value) = source.get("excludedFolders") {
            task_detection.insert("excluded_folders".into(), value.clone());
        }
        if !task_detection.is_empty() {
            out.insert("task_detection".into(), Value::Object(task_detection));
        }
    }

    if source.contains_key("moveArchivedTasks") || source.contains_key("archiveFolder") {
        let mut archive = Map::new();
        if let Some(value) = source.get("moveArchivedTasks").and_then(Value::as_bool) {
            archive.insert("move_on_archive".into(), Value::Bool(value));
        }
        if let Some(value) = source.get("archiveFolder").and_then(Value::as_str) {
            archive.insert("folder".into(), Value::String(value.to_string()));
        }
        out.insert("archive".into(), Value::Object(archive));
    }

    if let Some(value) = source
        .get("useFrontmatterMarkdownLinks")
        .and_then(Value::as_bool)
    {
        out.insert("links".into(), json!({ "use_markdown_format": value }));
    }

    Value::Object(out)
}

pub fn spec_version_effective(input: &Value) -> Value {
    let provider = input
        .get("providerSpecVersion")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let target = input
        .get("targetSpecVersion")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("0.1.0-draft");

    match provider {
        Some(provider) => json!({ "value": provider, "synthesized": false }),
        None => json!({ "value": target, "synthesized": true }),
    }
}

pub fn provider_behavior(input: &Value) -> anyhow::Result<Value> {
    let mode = input
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("strict");
    let providers_readable = input
        .get("providersReadable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let has_required_keys = input
        .get("hasRequiredKeys")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    anyhow::ensure!(
        mode == "strict" || mode == "permissive",
        "configuration mode unsupported"
    );
    anyhow::ensure!(
        mode != "strict" || (providers_readable && has_required_keys),
        "strict configuration requires providers readable and required effective keys"
    );

    Ok(json!({ "value": "accepted" }))
}

pub fn validate_schema(input: &Value) -> anyhow::Result<Value> {
    let kind = input
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let value = input
        .get("value")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    match kind {
        "validation" => {
            if let Some(mode) = value.get("mode").and_then(Value::as_str) {
                anyhow::ensure!(
                    mode == "strict" || mode == "permissive",
                    "validation.mode unsupported"
                );
            }
            if let Some(flag) = value.get("reject_unknown_fields") {
                anyhow::ensure!(
                    flag.is_boolean(),
                    "validation.reject_unknown_fields invalid"
                );
            }
        }
        "title" => {
            if let Some(storage) = value.get("storage").and_then(Value::as_str) {
                anyhow::ensure!(
                    storage == "filename" || storage == "frontmatter",
                    "title.storage invalid"
                );
            }
            if let Some(format) = value.get("filename_format").and_then(Value::as_str) {
                anyhow::ensure!(
                    format == "slug" || format == "custom",
                    "title.filename_format invalid"
                );
                if format == "custom" {
                    anyhow::ensure!(
                        value
                            .get("custom_filename_template")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|entry| !entry.is_empty())
                            .is_some(),
                        "title.custom_filename_template missing"
                    );
                }
            }
        }
        "templating" => {
            if let Some(enabled) = value.get("enabled") {
                anyhow::ensure!(enabled.is_boolean(), "templating.enabled invalid");
                if enabled.as_bool() == Some(true) {
                    anyhow::ensure!(
                        value
                            .get("template_path")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|entry| !entry.is_empty())
                            .is_some(),
                        "templating.template_path missing"
                    );
                }
            }
            if let Some(mode) = value.get("failure_mode").and_then(Value::as_str) {
                anyhow::ensure!(
                    mode == "warning_fallback" || mode == "error_abort",
                    "templating.failure_mode invalid"
                );
            }
            if let Some(policy) = value.get("unknown_variable_policy").and_then(Value::as_str) {
                anyhow::ensure!(
                    policy == "preserve" || policy == "error" || policy == "empty",
                    "templating.unknown_variable_policy invalid"
                );
            }
        }
        "reminders" => {
            if let Some(time) = value.get("date_only_anchor_time").and_then(Value::as_str) {
                let pattern = regex::Regex::new(r"^([01]\d|2[0-3]):[0-5]\d$").expect("valid regex");
                anyhow::ensure!(
                    pattern.is_match(time),
                    "reminders.date_only_anchor_time invalid"
                );
            }
            if let Some(flag) = value.get("apply_defaults_when_explicit") {
                anyhow::ensure!(
                    flag.is_boolean(),
                    "reminders.apply_defaults_when_explicit invalid"
                );
            }
        }
        "time_tracking" => {
            if let Some(flag) = value.get("auto_stop_on_complete") {
                anyhow::ensure!(
                    flag.is_boolean(),
                    "time_tracking.auto_stop_on_complete invalid"
                );
            }
            if let Some(flag) = value.get("auto_stop_notification") {
                anyhow::ensure!(
                    flag.is_boolean(),
                    "time_tracking.auto_stop_notification invalid"
                );
            }
        }
        "archive" => {
            if let Some(flag) = value.get("move_on_archive") {
                anyhow::ensure!(flag.is_boolean(), "archive.move_on_archive invalid");
            }
            if let Some(folder) = value.get("folder").and_then(Value::as_str) {
                anyhow::ensure!(!folder.trim().is_empty(), "archive.folder invalid");
            }
            if let Some(tag) = value.get("tag").and_then(Value::as_str) {
                anyhow::ensure!(!tag.trim().is_empty(), "archive.tag invalid");
            }
            if let Some(field) = value.get("field").and_then(Value::as_str) {
                anyhow::ensure!(!field.trim().is_empty(), "archive.field invalid");
            }
        }
        "status" => {
            let values = value
                .get("values")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            anyhow::ensure!(values.iter().all(Value::is_string), "status.values invalid");
            let values: Vec<String> = values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            if let Some(default) = value.get("default").and_then(Value::as_str) {
                anyhow::ensure!(
                    values.is_empty() || values.iter().any(|entry| entry == default),
                    "status.default must be one of status.values"
                );
            }
            if let Some(completed_values) = value.get("completed_values") {
                let array = completed_values
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("status.completed_values non-empty"))?;
                anyhow::ensure!(!array.is_empty(), "status.completed_values non-empty");
                anyhow::ensure!(
                    array.iter().all(Value::is_string),
                    "status.completed_values invalid"
                );
                anyhow::ensure!(
                    values.is_empty()
                        || array
                            .iter()
                            .filter_map(Value::as_str)
                            .all(|entry| values.iter().any(|value| value == entry)),
                    "status.completed_values must be in status.values"
                );
            }
        }
        "task_detection" => {
            if let Some(combine) = value.get("combine").and_then(Value::as_str) {
                anyhow::ensure!(
                    combine == "and" || combine == "or",
                    "task_detection.combine invalid"
                );
            }
        }
        "dependencies" => {
            if let Some(kind) = value.get("default_reltype").and_then(Value::as_str) {
                anyhow::ensure!(
                    matches!(
                        kind,
                        "FINISHTOSTART" | "STARTTOSTART" | "FINISHTOFINISH" | "STARTTOFINISH"
                    ),
                    "dependencies.default_reltype invalid"
                );
            }
            if let Some(severity) = value
                .get("unresolved_target_severity")
                .and_then(Value::as_str)
            {
                anyhow::ensure!(
                    severity == "warning" || severity == "error",
                    "dependencies.unresolved_target_severity invalid"
                );
            }
        }
        "links" => {
            if let Some(extensions) = value.get("extensions") {
                let array = extensions
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("links.extensions invalid"))?;
                anyhow::ensure!(
                    array.iter().all(Value::is_string),
                    "links.extensions invalid"
                );
            }
            if let Some(severity) = value
                .get("unresolved_default_severity")
                .and_then(Value::as_str)
            {
                anyhow::ensure!(
                    severity == "warning" || severity == "error",
                    "links.unresolved_default_severity invalid"
                );
            }
        }
        _ => anyhow::bail!("config kind unsupported:{kind}"),
    }

    Ok(json!({ "value": "valid" }))
}

pub fn normalize_effective_config(config_value: Value) -> EffectiveConfig {
    let mut config = EffectiveConfig::default();
    let obj = config_value.as_object().cloned().unwrap_or_default();

    if let Some(spec_version) = obj.get("spec_version").and_then(Value::as_str) {
        config.spec_version = spec_version.to_string();
    }
    if let Some(mapping) = obj.get("mapping").and_then(Value::as_object) {
        for (key, value) in mapping {
            if let Some(value) = value.as_str() {
                config.mapping.insert(key.clone(), value.to_string());
            }
        }
    }
    if let Some(title) = obj.get("title").and_then(Value::as_object) {
        if let Some(storage) = title.get("storage").and_then(Value::as_str) {
            config.title.storage = storage.to_string();
        }
        if let Some(format) = title.get("filename_format").and_then(Value::as_str) {
            config.title.filename_format = format.to_string();
        }
        config.title.custom_filename_template = title
            .get("custom_filename_template")
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    if let Some(status) = obj.get("status").and_then(Value::as_object) {
        if let Some(values) = status.get("values").and_then(Value::as_array) {
            config.status.values = values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
        }
        if let Some(default) = status.get("default").and_then(Value::as_str) {
            config.status.default = default.to_string();
        }
        if let Some(completed_values) = status.get("completed_values").and_then(Value::as_array) {
            config.status.completed_values = completed_values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
        }
    }
    if let Some(defaults) = obj.get("defaults").and_then(Value::as_object) {
        if let Some(value) = defaults.get("status").and_then(Value::as_str) {
            config.defaults.status = value.to_string();
        }
        if let Some(value) = defaults.get("priority").and_then(Value::as_str) {
            config.defaults.priority = value.to_string();
        }
    }
    if let Some(detection) = obj.get("task_detection").and_then(Value::as_object) {
        if let Some(method) = detection.get("method").and_then(Value::as_str) {
            config.task_detection.method = method.to_string();
            config.task_detection.methods = vec![method.to_string()];
        }
        if let Some(methods) = detection.get("methods").and_then(Value::as_array) {
            let parsed: Vec<String> = methods
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            if !parsed.is_empty() {
                config.task_detection.methods = parsed;
            }
        }
        if let Some(combine) = detection.get("combine").and_then(Value::as_str) {
            config.task_detection.combine = combine.to_string();
        }
        if let Some(tag) = detection.get("tag").and_then(Value::as_str) {
            config.task_detection.tag = tag.to_string();
        }
        config.task_detection.property_name = detection
            .get("property_name")
            .and_then(Value::as_str)
            .map(str::to_string);
        config.task_detection.property_value = detection
            .get("property_value")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(folder) = detection.get("default_folder").and_then(Value::as_str) {
            config.task_detection.default_folder = folder.to_string();
        }
        if let Some(excluded) = detection.get("excluded_folders") {
            config.task_detection.excluded_folders = match excluded {
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
    }
    if let Some(archive) = obj.get("archive").and_then(Value::as_object) {
        if let Some(value) = archive.get("move_on_archive").and_then(Value::as_bool) {
            config.archive.move_on_archive = value;
        }
        if let Some(value) = archive.get("folder").and_then(Value::as_str) {
            config.archive.folder = value.to_string();
        }
        if let Some(value) = archive.get("tag").and_then(Value::as_str) {
            config.archive.tag = value.to_string();
        }
        if let Some(value) = archive.get("field").and_then(Value::as_str) {
            config.archive.field = value.to_string();
        }
    }

    config
}

pub fn load_effective_config(root: &Path) -> EffectiveConfig {
    let mut value = json!({});
    let plugin_path = root.join(".obsidian/plugins/tasknotes/data.json");
    if let Ok(content) = fs::read_to_string(plugin_path) {
        if let Ok(json_value) = serde_json::from_str::<Value>(&content) {
            value = merge_top_level(&[value.clone(), map_tasknotes_plugin_config(&json_value)]);
        }
    }

    let tasknotes_yaml = root.join("tasknotes.yaml");
    if let Ok(content) = fs::read_to_string(tasknotes_yaml) {
        if let Ok(yaml_value) = serde_yaml::from_str::<Value>(&content) {
            value = merge_top_level(&[value.clone(), yaml_value]);
        }
    }

    normalize_effective_config(value)
}

fn normalize_hashtag_value(value: &str) -> String {
    value.trim().trim_start_matches('#').to_ascii_lowercase()
}

fn strip_code_spans_and_fences(markdown: &str) -> String {
    let fenced = regex::Regex::new(r"```[\s\S]*?```").expect("valid regex");
    let inline = regex::Regex::new(r"`[^`]*`").expect("valid regex");
    let without_fences = fenced.replace_all(markdown, " ");
    inline.replace_all(&without_fences, " ").to_string()
}

pub fn detect_task_file(
    config: &TaskDetectionConfig,
    frontmatter: &Map<String, Value>,
    body: &str,
    file_path: &str,
) -> bool {
    let normalized_path = file_path.trim_start_matches('/').replace('\\', "/");
    if config.excluded_folders.iter().any(|folder| {
        normalized_path == *folder || normalized_path.starts_with(&format!("{}/", folder))
    }) {
        return false;
    }

    let property_matches = || {
        let Some(name) = config.property_name.as_ref() else {
            return false;
        };
        let Some(value) = frontmatter.get(name) else {
            return false;
        };
        match config.property_value.as_deref() {
            Some(expected) if !expected.is_empty() => value
                .as_str()
                .map(|actual| actual == expected)
                .unwrap_or(false),
            _ => true,
        }
    };

    let tag_matches = || {
        let normalized_tag = normalize_hashtag_value(&config.tag);
        let frontmatter_hit = match frontmatter.get("tags") {
            Some(Value::Array(tags)) => tags
                .iter()
                .filter_map(Value::as_str)
                .any(|entry| normalize_hashtag_value(entry) == normalized_tag),
            Some(Value::String(tag)) => normalize_hashtag_value(tag) == normalized_tag,
            _ => false,
        };
        if frontmatter_hit {
            return true;
        }
        let body = strip_code_spans_and_fences(body);
        let hashtag =
            regex::Regex::new(r"(^|[^\w])#([A-Za-z0-9][A-Za-z0-9/_-]*)").expect("valid regex");
        let matched = hashtag
            .captures_iter(&body)
            .filter_map(|caps| caps.get(2))
            .any(|m| m.as_str().eq_ignore_ascii_case(&normalized_tag));
        matched
    };

    let methods = if config.methods.is_empty() {
        vec![config.method.as_str()]
    } else {
        config.methods.iter().map(String::as_str).collect()
    };
    let results: Vec<bool> = methods
        .into_iter()
        .map(|method| match method {
            "property" => property_matches(),
            _ => tag_matches(),
        })
        .collect();

    if config.combine.eq_ignore_ascii_case("and") {
        results.into_iter().all(|value| value)
    } else {
        results.into_iter().any(|value| value)
    }
}
