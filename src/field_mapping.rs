use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::config::EffectiveConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldRole {
    Title,
    Status,
    Priority,
    Due,
    Scheduled,
    CompletedDate,
    Tags,
    Contexts,
    Projects,
    TimeEstimate,
    DateCreated,
    DateModified,
    Recurrence,
    RecurrenceAnchor,
    CompleteInstances,
    SkippedInstances,
    TimeEntries,
}

impl FieldRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Status => "status",
            Self::Priority => "priority",
            Self::Due => "due",
            Self::Scheduled => "scheduled",
            Self::CompletedDate => "completedDate",
            Self::Tags => "tags",
            Self::Contexts => "contexts",
            Self::Projects => "projects",
            Self::TimeEstimate => "timeEstimate",
            Self::DateCreated => "dateCreated",
            Self::DateModified => "dateModified",
            Self::Recurrence => "recurrence",
            Self::RecurrenceAnchor => "recurrenceAnchor",
            Self::CompleteInstances => "completeInstances",
            Self::SkippedInstances => "skippedInstances",
            Self::TimeEntries => "timeEntries",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "title" => Self::Title,
            "status" => Self::Status,
            "priority" => Self::Priority,
            "due" => Self::Due,
            "scheduled" => Self::Scheduled,
            "completedDate" => Self::CompletedDate,
            "tags" => Self::Tags,
            "contexts" => Self::Contexts,
            "projects" => Self::Projects,
            "timeEstimate" => Self::TimeEstimate,
            "dateCreated" => Self::DateCreated,
            "dateModified" => Self::DateModified,
            "recurrence" => Self::Recurrence,
            "recurrenceAnchor" => Self::RecurrenceAnchor,
            "completeInstances" => Self::CompleteInstances,
            "skippedInstances" => Self::SkippedInstances,
            "timeEntries" => Self::TimeEntries,
            _ => return None,
        })
    }
}

const ALL_ROLES: [FieldRole; 17] = [
    FieldRole::Title,
    FieldRole::Status,
    FieldRole::Priority,
    FieldRole::Due,
    FieldRole::Scheduled,
    FieldRole::CompletedDate,
    FieldRole::Tags,
    FieldRole::Contexts,
    FieldRole::Projects,
    FieldRole::TimeEstimate,
    FieldRole::DateCreated,
    FieldRole::DateModified,
    FieldRole::Recurrence,
    FieldRole::RecurrenceAnchor,
    FieldRole::CompleteInstances,
    FieldRole::SkippedInstances,
    FieldRole::TimeEntries,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMapping {
    #[serde(rename = "roleToField")]
    pub role_to_field: BTreeMap<String, String>,
    #[serde(rename = "fieldToRole")]
    pub field_to_role: BTreeMap<String, String>,
    #[serde(rename = "displayNameKey")]
    pub display_name_key: String,
    #[serde(rename = "completedStatuses")]
    pub completed_statuses: Vec<String>,
}

pub fn default_field_mapping() -> FieldMapping {
    let mut role_to_field = BTreeMap::new();
    let mut field_to_role = BTreeMap::new();
    for role in ALL_ROLES {
        role_to_field.insert(role.as_str().to_string(), role.as_str().to_string());
        field_to_role.insert(role.as_str().to_string(), role.as_str().to_string());
    }
    FieldMapping {
        role_to_field,
        field_to_role,
        display_name_key: "title".to_string(),
        completed_statuses: vec!["done".to_string(), "cancelled".to_string()],
    }
}

pub fn build_field_mapping(
    fields: &Map<String, Value>,
    display_name_key: Option<&str>,
) -> FieldMapping {
    let mut mapping = default_field_mapping();
    let mut assigned = BTreeSet::new();

    for (field_name, def) in fields {
        if let Some(role) = def
            .get("tn_role")
            .and_then(Value::as_str)
            .and_then(FieldRole::parse)
        {
            if assigned.insert(role.as_str().to_string()) {
                mapping
                    .role_to_field
                    .insert(role.as_str().to_string(), field_name.clone());
                mapping
                    .field_to_role
                    .insert(field_name.clone(), role.as_str().to_string());
            }
        }
    }

    for role in ALL_ROLES {
        let role_key = role.as_str().to_string();
        if !assigned.contains(&role_key) && fields.contains_key(role.as_str()) {
            mapping
                .role_to_field
                .insert(role_key.clone(), role_key.clone());
            mapping.field_to_role.insert(role_key.clone(), role_key);
        }
    }

    mapping.completed_statuses = infer_completed_statuses(
        fields,
        mapping
            .role_to_field
            .get("status")
            .map(String::as_str)
            .unwrap_or("status"),
    );
    if let Some(key) = display_name_key.filter(|v| !v.trim().is_empty()) {
        mapping.display_name_key = key.to_string();
    } else if let Some(title_field) = mapping.role_to_field.get("title") {
        mapping.display_name_key = title_field.clone();
    }
    mapping
}

pub fn field_mapping_from_config(config: &EffectiveConfig) -> FieldMapping {
    let mut mapping = default_field_mapping();
    for (role, field) in &config.mapping {
        let camel_role = match role.as_str() {
            "completed_date" => "completedDate",
            "date_created" => "dateCreated",
            "date_modified" => "dateModified",
            "recurrence_anchor" => "recurrenceAnchor",
            "complete_instances" => "completeInstances",
            "skipped_instances" => "skippedInstances",
            "time_estimate" => "timeEstimate",
            "time_entries" => "timeEntries",
            other => other,
        };
        mapping
            .role_to_field
            .insert(camel_role.to_string(), field.to_string());
        mapping
            .field_to_role
            .insert(field.to_string(), camel_role.to_string());
    }
    mapping.completed_statuses = config.status.completed_values.clone();
    mapping
}

fn infer_completed_statuses(fields: &Map<String, Value>, status_field_name: &str) -> Vec<String> {
    let Some(status_def) = fields.get(status_field_name) else {
        return vec!["done".into(), "cancelled".into()];
    };

    if let Some(values) = status_def
        .get("tn_completed_values")
        .and_then(Value::as_array)
    {
        let explicit: Vec<String> = values
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .collect();
        if !explicit.is_empty() {
            return explicit;
        }
    }

    if let Some(values) = status_def.get("values").and_then(Value::as_array) {
        let inferred: Vec<String> = values
            .iter()
            .filter_map(Value::as_str)
            .filter(|value| {
                let lower = value.to_ascii_lowercase();
                lower.contains("done")
                    || lower.contains("complete")
                    || lower.contains("cancel")
                    || lower.contains("finish")
            })
            .map(str::to_string)
            .collect();
        if !inferred.is_empty() {
            return inferred;
        }
    }

    vec!["done".into(), "cancelled".into()]
}

pub fn normalize_frontmatter(
    raw: &Map<String, Value>,
    mapping: &FieldMapping,
) -> Map<String, Value> {
    let mut out = Map::new();
    for (key, value) in raw {
        let role = mapping
            .field_to_role
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.clone());
        out.insert(role, value.clone());
    }
    out
}

pub fn denormalize_frontmatter(
    role_data: &Map<String, Value>,
    mapping: &FieldMapping,
) -> Map<String, Value> {
    let mut out = Map::new();
    for (key, value) in role_data {
        let mapped = mapping
            .role_to_field
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.clone());
        out.insert(mapped, value.clone());
    }
    out
}

pub fn resolve_display_title(
    frontmatter: &Map<String, Value>,
    mapping: &FieldMapping,
    task_path: Option<&str>,
) -> Option<String> {
    for key in [&mapping.display_name_key, "title"] {
        if let Some(value) = frontmatter
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            return Some(value.to_string());
        }
    }

    task_path
        .and_then(|path| std::path::Path::new(path).file_stem())
        .and_then(|stem| stem.to_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

pub fn is_completed_status(mapping: &FieldMapping, status: Option<&str>) -> bool {
    status
        .map(|value| {
            mapping
                .completed_statuses
                .iter()
                .any(|candidate| candidate == value)
        })
        .unwrap_or(false)
}

pub fn default_completed_status(mapping: &FieldMapping) -> String {
    mapping
        .completed_statuses
        .first()
        .cloned()
        .unwrap_or_else(|| "done".to_string())
}

pub fn mapping_json(mapping: &FieldMapping) -> Value {
    json!(mapping)
}
