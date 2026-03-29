use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use mdbase::Collection;
use serde_json::{json, Map, Value};

use crate::config::{detect_task_file, load_effective_config, EffectiveConfig};
use crate::date::{get_date_part, is_before_date_safe, today_local};
use crate::field_mapping::{
    default_completed_status, denormalize_frontmatter, field_mapping_from_config,
    is_completed_status, normalize_frontmatter, resolve_display_title, FieldMapping,
};
use crate::recurrence::{complete as complete_recurring, RecurrenceInput};
use crate::time_tracking;

#[derive(Debug, Clone)]
pub struct TaskRecord {
    pub path: String,
    pub title: String,
    pub status: String,
    pub priority: Option<String>,
    pub due: Option<String>,
    pub scheduled: Option<String>,
    pub time_entries: Vec<Value>,
    pub has_active_time_entry: bool,
    pub body: String,
    pub normalized_frontmatter: Map<String, Value>,
    pub raw_frontmatter: Map<String, Value>,
}

#[derive(Debug, Clone, Default)]
pub struct TaskDraft {
    pub title: String,
    pub details: String,
    pub due: Option<String>,
    pub scheduled: Option<String>,
    pub priority: Option<String>,
    pub status: Option<String>,
    pub recurrence: Option<String>,
    pub recurrence_anchor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskFilter {
    All,
    Open,
    Today,
    Overdue,
    Tracked,
}

pub struct TaskRepository {
    root: PathBuf,
    pub config: EffectiveConfig,
    pub field_mapping: FieldMapping,
}

impl TaskRepository {
    pub fn open(root: impl AsRef<Path>) -> anyhow::Result<Self> {
        let root = root.as_ref().to_path_buf();
        let config = load_effective_config(&root);
        let _collection = Collection::open(&root)
            .map_err(|error| anyhow!("failed to open mdbase collection: {}", error))?;
        let field_mapping = field_mapping_from_config(&config);
        Ok(Self {
            root,
            config,
            field_mapping,
        })
    }

    fn collection(&self) -> anyhow::Result<Collection> {
        Collection::open(&self.root)
            .map_err(|error| anyhow!("failed to open mdbase collection: {}", error))
    }

    pub fn absolute_task_path(&self, task: &TaskRecord) -> PathBuf {
        self.root.join(&task.path)
    }

    pub fn list_tasks(&self, filter: TaskFilter, reference_date: &str) -> anyhow::Result<Vec<TaskRecord>> {
        let collection = self.collection()?;
        let query = collection.query(&json!({
            "query": {
                "types": ["task"],
                "include_body": true,
            }
        }));
        let results = query
            .get("results")
            .and_then(Value::as_array)
            .context("mdbase query did not return results")?;

        let mut tasks = Vec::new();
        for entry in results {
            let path = entry
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let raw = entry
                .get("frontmatter")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let body = entry
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if !detect_task_file(&self.config.task_detection, &raw, &body, &path) {
                continue;
            }
            let normalized = normalize_frontmatter(&raw, &self.field_mapping);
            let title = resolve_display_title(&normalized, &self.field_mapping, Some(&path))
                .unwrap_or_else(|| path.clone());
            let status = normalized
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or(&self.config.defaults.status)
                .to_string();
            let task = TaskRecord {
                path,
                title,
                status,
                priority: normalized
                    .get("priority")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                due: normalized
                    .get("due")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                scheduled: normalized
                    .get("scheduled")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                time_entries: normalized
                    .get("timeEntries")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
                has_active_time_entry: normalized
                    .get("timeEntries")
                    .and_then(Value::as_array)
                    .and_then(|entries| time_tracking::has_active_entry(entries).ok())
                    .unwrap_or(false),
                body,
                normalized_frontmatter: normalized,
                raw_frontmatter: raw,
            };
            if matches_filter(&task, filter, &self.field_mapping, reference_date) {
                tasks.push(task);
            }
        }

        tasks.sort_by(task_sort_key);
        Ok(tasks)
    }

    pub fn search_tasks(
        &self,
        filter: TaskFilter,
        query: Option<&str>,
        reference_date: &str,
    ) -> anyhow::Result<Vec<TaskRecord>> {
        let tasks = self.list_tasks(filter, reference_date)?;
        let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(tasks);
        };
        let needle = query.to_ascii_lowercase();
        Ok(tasks
            .into_iter()
            .filter(|task| {
                task.title.to_ascii_lowercase().contains(&needle)
                    || task.path.to_ascii_lowercase().contains(&needle)
                    || task.body.to_ascii_lowercase().contains(&needle)
                    || task
                        .priority
                        .as_deref()
                        .map(|value| value.to_ascii_lowercase().contains(&needle))
                        .unwrap_or(false)
            })
            .collect())
    }

    pub fn read_task(&self, path: &str) -> anyhow::Result<TaskRecord> {
        let collection = self.collection()?;
        let result = collection.read(&json!({ "path": path }));
        let raw = result
            .get("frontmatter")
            .and_then(Value::as_object)
            .cloned()
            .context("read missing frontmatter")?;
        let body = result
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let normalized = normalize_frontmatter(&raw, &self.field_mapping);
        Ok(TaskRecord {
            path: path.to_string(),
            title: resolve_display_title(&normalized, &self.field_mapping, Some(path))
                .unwrap_or_else(|| path.to_string()),
            status: normalized
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or(&self.config.defaults.status)
                .to_string(),
            priority: normalized
                .get("priority")
                .and_then(Value::as_str)
                .map(str::to_string),
            due: normalized
                .get("due")
                .and_then(Value::as_str)
                .map(str::to_string),
            scheduled: normalized
                .get("scheduled")
                .and_then(Value::as_str)
                .map(str::to_string),
            time_entries: normalized
                .get("timeEntries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            has_active_time_entry: normalized
                .get("timeEntries")
                .and_then(Value::as_array)
                .and_then(|entries| time_tracking::has_active_entry(entries).ok())
                .unwrap_or(false),
            body,
            normalized_frontmatter: normalized,
            raw_frontmatter: raw,
        })
    }

    pub fn toggle_time_tracking(&self, task: &TaskRecord) -> anyhow::Result<()> {
        let mut normalized = self.read_task(&task.path)?.normalized_frontmatter;
        let entries = normalized
            .get("timeEntries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let result = if time_tracking::has_active_entry(&entries).unwrap_or(false) {
            time_tracking::stop(&entries, None)?
        } else {
            time_tracking::start(&entries, None)?
        };
        if let Some(entries) = result.get("value") {
            normalized.insert("timeEntries".into(), entries.clone());
        }
        if let Some(date_modified) = result.get("dateModified") {
            normalized.insert("dateModified".into(), date_modified.clone());
        }
        self.write_task_update(task, normalized)
    }

    pub fn toggle_complete(&self, task: &TaskRecord) -> anyhow::Result<()> {
        let collection = self.collection()?;
        let mut normalized = task.normalized_frontmatter.clone();
        let is_recurring = normalized
            .get("recurrence")
            .and_then(Value::as_str)
            .is_some();

        if is_recurring {
            let input = RecurrenceInput {
                recurrence: normalized
                    .get("recurrence")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                recurrence_anchor: normalized
                    .get("recurrenceAnchor")
                    .and_then(Value::as_str)
                    .unwrap_or("scheduled")
                    .to_string(),
                scheduled: normalized
                    .get("scheduled")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                due: normalized
                    .get("due")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                date_created: normalized
                    .get("dateCreated")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                complete_instances: normalized
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
                skipped_instances: normalized
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
            };
            let completion = complete_recurring(&input, &today_local())?;
            if let Some(value) = completion.get("completeInstances") {
                normalized.insert("completeInstances".into(), value.clone());
            }
            if let Some(value) = completion.get("skippedInstances") {
                normalized.insert("skippedInstances".into(), value.clone());
            }
            if let Some(value) = completion.get("nextScheduled") {
                normalized.insert("scheduled".into(), value.clone());
            }
            if let Some(value) = completion.get("nextDue") {
                normalized.insert("due".into(), value.clone());
            }
        } else if is_completed_status(&self.field_mapping, Some(&task.status)) {
            normalized.insert(
                "status".into(),
                Value::String(self.config.defaults.status.clone()),
            );
            normalized.remove("completedDate");
        } else {
            normalized.insert(
                "status".into(),
                Value::String(default_completed_status(&self.field_mapping)),
            );
            normalized.insert("completedDate".into(), Value::String(today_local()));
        }

        normalized.insert(
            "dateModified".into(),
            Value::String(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        );
        let fields = denormalize_frontmatter(&normalized, &self.field_mapping);
        let result = collection.update(&json!({ "path": task.path, "fields": fields }));
        if let Some(error) = result.get("error") {
            return Err(anyhow!("update failed: {}", error));
        }
        Ok(())
    }

    pub fn toggle_skip_today(&self, task: &TaskRecord) -> anyhow::Result<()> {
        let mut normalized = task.normalized_frontmatter.clone();
        let is_recurring = normalized
            .get("recurrence")
            .and_then(Value::as_str)
            .is_some();
        anyhow::ensure!(is_recurring, "task is not recurring");

        let today = today_local();
        let mut skipped_instances: Vec<String> = normalized
            .get("skippedInstances")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if skipped_instances
            .iter()
            .any(|value| get_date_part(value) == today)
        {
            skipped_instances.retain(|value| get_date_part(value) != today);
        } else {
            skipped_instances.push(today.clone());
            if let Some(completed) = normalized
                .get_mut("completeInstances")
                .and_then(Value::as_array_mut)
            {
                completed
                    .retain(|value| value.as_str().map(get_date_part).unwrap_or_default() != today);
            }
        }
        normalized.insert(
            "skippedInstances".into(),
            Value::Array(
                skipped_instances
                    .into_iter()
                    .map(Value::String)
                    .collect::<Vec<_>>(),
            ),
        );
        self.write_task_update(task, normalized)
    }

    pub fn create_task(&self, title: &str) -> anyhow::Result<()> {
        self.create_task_from_draft(&TaskDraft {
            title: title.to_string(),
            ..TaskDraft::default()
        })
    }

    pub fn create_task_from_draft(&self, draft: &TaskDraft) -> anyhow::Result<()> {
        let collection = self.collection()?;
        anyhow::ensure!(!draft.title.trim().is_empty(), "title must not be empty");
        let slug = slugify(&draft.title);
        let path = format!(
            "{}/{}.md",
            self.config.task_detection.default_folder.trim_matches('/'),
            slug
        );
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let mut fields = Map::new();
        fields.insert(
            self.config
                .mapping
                .get("title")
                .cloned()
                .unwrap_or_else(|| "title".into()),
            Value::String(draft.title.trim().to_string()),
        );
        fields.insert(
            self.config
                .mapping
                .get("status")
                .cloned()
                .unwrap_or_else(|| "status".into()),
            Value::String(
                draft
                    .status
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(&self.config.defaults.status)
                    .trim()
                    .to_string(),
            ),
        );
        fields.insert(
            "priority".into(),
            Value::String(
                draft
                    .priority
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(&self.config.defaults.priority)
                    .trim()
                    .to_string(),
            ),
        );
        if let Some(due) = draft.due.as_ref().filter(|value| !value.trim().is_empty()) {
            fields.insert("due".into(), Value::String(due.trim().to_string()));
        }
        if let Some(scheduled) = draft
            .scheduled
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            fields.insert(
                "scheduled".into(),
                Value::String(scheduled.trim().to_string()),
            );
        }
        if let Some(recurrence) = draft
            .recurrence
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            fields.insert(
                "recurrence".into(),
                Value::String(recurrence.trim().to_string()),
            );
            fields.insert(
                "recurrenceAnchor".into(),
                Value::String(
                    draft
                        .recurrence_anchor
                        .as_deref()
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or("scheduled")
                        .trim()
                        .to_string(),
                ),
            );
        }
        fields.insert(
            self.config
                .mapping
                .get("date_created")
                .cloned()
                .unwrap_or_else(|| "dateCreated".into()),
            Value::String(now.clone()),
        );
        fields.insert(
            self.config
                .mapping
                .get("date_modified")
                .cloned()
                .unwrap_or_else(|| "dateModified".into()),
            Value::String(now),
        );
        let result = collection.create(&json!({
            "path": path,
            "type": "task",
            "fields": Value::Object(fields),
            "body": draft.details,
        }));
        if let Some(error) = result.get("error") {
            return Err(anyhow!("create failed: {}", error));
        }
        Ok(())
    }

    pub fn update_title(&self, task: &TaskRecord, title: &str) -> anyhow::Result<()> {
        let title = title.trim();
        anyhow::ensure!(!title.is_empty(), "title must not be empty");
        let collection = self.collection()?;
        let mut normalized = task.normalized_frontmatter.clone();
        normalized.insert("title".into(), Value::String(title.to_string()));

        let old_path = task.path.clone();
        let maybe_new_path = if self.config.title.storage == "filename" {
            Some(rename_task_path(&old_path, title))
        } else {
            None
        };
        let fields = denormalize_frontmatter(&normalized, &self.field_mapping);
        let result = collection.update(&json!({ "path": old_path, "fields": fields }));
        if let Some(error) = result.get("error") {
            return Err(anyhow!("update failed: {}", error));
        }
        if let Some(new_path) = maybe_new_path {
            if new_path != task.path {
                let rename_result = collection.rename(&json!({
                    "from": task.path,
                    "to": new_path,
                    "update_refs": true
                }));
                if let Some(error) = rename_result.get("error") {
                    return Err(anyhow!("rename failed: {}", error));
                }
            }
        }
        Ok(())
    }

    pub fn update_date_field(
        &self,
        task: &TaskRecord,
        field: &str,
        value: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut normalized = self.read_task(&task.path)?.normalized_frontmatter;
        match value.map(str::trim).filter(|value| !value.is_empty()) {
            Some(value) => {
                normalized.insert(field.to_string(), Value::String(value.to_string()));
            }
            None => {
                normalized.remove(field);
            }
        }
        self.write_task_update(task, normalized)
    }

    pub fn update_scalar_field(
        &self,
        task: &TaskRecord,
        field: &str,
        value: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut normalized = self.read_task(&task.path)?.normalized_frontmatter;
        match value.map(str::trim).filter(|value| !value.is_empty()) {
            Some(value) => {
                normalized.insert(field.to_string(), Value::String(value.to_string()));
            }
            None => {
                normalized.remove(field);
            }
        }
        self.write_task_update(task, normalized)
    }

    fn write_task_update(
        &self,
        task: &TaskRecord,
        mut normalized: Map<String, Value>,
    ) -> anyhow::Result<()> {
        let collection = self.collection()?;
        normalized.insert(
            "dateModified".into(),
            Value::String(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        );
        let fields = denormalize_frontmatter(&normalized, &self.field_mapping);
        let result = collection.update(&json!({ "path": task.path, "fields": fields }));
        if let Some(error) = result.get("error") {
            return Err(anyhow!("update failed: {}", error));
        }
        Ok(())
    }
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

fn rename_task_path(old_path: &str, title: &str) -> String {
    let stem = slugify(title);
    let old = Path::new(old_path);
    let parent = old.parent().map(|path| path.to_string_lossy().to_string());
    match parent {
        Some(parent) if !parent.is_empty() => format!("{parent}/{stem}.md"),
        _ => format!("{stem}.md"),
    }
}

fn matches_filter(
    task: &TaskRecord,
    filter: TaskFilter,
    mapping: &FieldMapping,
    reference_date: &str,
) -> bool {
    match filter {
        TaskFilter::All => true,
        TaskFilter::Open => !is_completed_status(mapping, Some(&task.status)),
        TaskFilter::Today => task
            .scheduled
            .as_deref()
            .map(get_date_part)
            .or_else(|| task.due.as_deref().map(get_date_part))
            .map(|value| value == get_date_part(reference_date))
            .unwrap_or(false),
        TaskFilter::Overdue => task
            .due
            .as_deref()
            .map(|due| {
                is_before_date_safe(due, reference_date)
                    && !is_completed_status(mapping, Some(&task.status))
            })
            .unwrap_or(false),
        TaskFilter::Tracked => task.has_active_time_entry,
    }
}

fn task_sort_key(a: &TaskRecord, b: &TaskRecord) -> Ordering {
    let left = a
        .scheduled
        .as_deref()
        .or(a.due.as_deref())
        .map(get_date_part)
        .unwrap_or_else(|| "9999-99-99".to_string());
    let right = b
        .scheduled
        .as_deref()
        .or(b.due.as_deref())
        .map(get_date_part)
        .unwrap_or_else(|| "9999-99-99".to_string());
    left.cmp(&right)
        .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    fn write_collection(root: &Path) {
        fs::write(
            root.join("mdbase.yaml"),
            r#"spec_version: "0.2.1"
settings:
  types_folder: "_types"
  default_validation: "warn"
"#,
        )
        .unwrap();
        fs::create_dir_all(root.join("_types")).unwrap();
        fs::write(
            root.join("tasknotes.yaml"),
            r#"task_detection:
  method: property
  property_name: status
  property_value: ""
"#,
        )
        .unwrap();
        fs::write(
            root.join("_types/task.md"),
            r#"---
name: task
path_pattern: "TaskNotes/Tasks/{title}.md"
fields:
  title:
    type: string
    required: true
  status:
    type: string
  priority:
    type: string
  due:
    type: date
  scheduled:
    type: date
  recurrence:
    type: string
  recurrenceAnchor:
    type: string
  completeInstances:
    type: list
  skippedInstances:
    type: list
  dateCreated:
    type: datetime
  dateModified:
    type: datetime
---
"#,
        )
        .unwrap();
        fs::create_dir_all(root.join("TaskNotes/Tasks")).unwrap();
    }

    #[test]
    fn create_list_and_toggle_task() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());

        let repo = TaskRepository::open(tmp.path()).unwrap();
        repo.create_task("Test Task").unwrap();

        let tasks = repo.list_tasks(TaskFilter::Open, &today_local()).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Test Task");

        repo.toggle_complete(&tasks[0]).unwrap();
        let refreshed = repo.read_task(&tasks[0].path).unwrap();
        let today = today_local();
        assert_eq!(refreshed.status, "done");
        assert_eq!(
            refreshed
                .normalized_frontmatter
                .get("completedDate")
                .and_then(Value::as_str),
            Some(today.as_str())
        );
    }

    #[test]
    fn search_and_edit_task() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());

        let repo = TaskRepository::open(tmp.path()).unwrap();
        repo.create_task_from_draft(&TaskDraft {
            title: "Alpha task".into(),
            details: "original details".into(),
            due: Some("2026-04-01".into()),
            scheduled: None,
            priority: Some("high".into()),
            status: Some("open".into()),
            recurrence: None,
            recurrence_anchor: None,
        })
        .unwrap();

        let tasks = repo
            .search_tasks(TaskFilter::All, Some("alpha"), &today_local())
            .unwrap();
        assert_eq!(tasks.len(), 1);
        repo.update_title(&tasks[0], "Renamed task").unwrap();
        let renamed = repo
            .search_tasks(TaskFilter::All, Some("renamed"), &today_local())
            .unwrap();
        assert_eq!(renamed.len(), 1);
        repo.update_date_field(&renamed[0], "scheduled", Some("2026-04-02"))
            .unwrap();

        let refreshed = repo.read_task(&renamed[0].path).unwrap();
        assert_eq!(refreshed.title, "Renamed task");
        assert_eq!(refreshed.body.trim_end(), "original details");
        assert_eq!(refreshed.scheduled.as_deref(), Some("2026-04-02"));
        assert_eq!(refreshed.priority.as_deref(), Some("high"));
    }

    #[test]
    fn skip_recurring_task_today() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());
        let today = today_local();
        let repo = TaskRepository::open(tmp.path()).unwrap();
        repo.create_task("Recurring").unwrap();
        let task = repo
            .search_tasks(TaskFilter::All, Some("Recurring"), &today_local())
            .unwrap();
        assert_eq!(task.len(), 1);
        repo.update_date_field(&task[0], "scheduled", Some(today.as_str()))
            .unwrap();
        let mut recurring = repo.read_task(&task[0].path).unwrap();
        recurring
            .normalized_frontmatter
            .insert("recurrence".into(), Value::String("FREQ=DAILY".into()));
        recurring.normalized_frontmatter.insert(
            "recurrenceAnchor".into(),
            Value::String("scheduled".into()),
        );
        repo.write_task_update(&recurring, recurring.normalized_frontmatter.clone())
            .unwrap();
        let recurring = repo.read_task(&task[0].path).unwrap();
        repo.toggle_skip_today(&recurring).unwrap();
        let refreshed = repo.read_task(&recurring.path).unwrap();
        let skipped = refreshed
            .normalized_frontmatter
            .get("skippedInstances")
            .and_then(Value::as_array)
            .unwrap();
        assert!(skipped
            .iter()
            .filter_map(Value::as_str)
            .any(|value| value == today));
    }

    #[test]
    fn create_with_recurrence_and_update_scalar_fields() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());

        let repo = TaskRepository::open(tmp.path()).unwrap();
        repo.create_task_from_draft(&TaskDraft {
            title: "Weekly review".into(),
            details: "check backlog".into(),
            due: Some("2026-04-03".into()),
            scheduled: Some("2026-04-01".into()),
            priority: Some("high".into()),
            status: Some("doing".into()),
            recurrence: Some("FREQ=WEEKLY".into()),
            recurrence_anchor: Some("completion".into()),
        })
        .unwrap();

        let task = repo
            .search_tasks(TaskFilter::All, Some("weekly review"), &today_local())
            .unwrap();
        assert_eq!(task.len(), 1);
        assert_eq!(task[0].priority.as_deref(), Some("high"));
        assert_eq!(task[0].status, "doing");
        assert_eq!(
            task[0]
                .normalized_frontmatter
                .get("recurrence")
                .and_then(Value::as_str),
            Some("FREQ=WEEKLY")
        );
        assert_eq!(
            task[0]
                .normalized_frontmatter
                .get("recurrenceAnchor")
                .and_then(Value::as_str),
            Some("completion")
        );

        repo.update_scalar_field(&task[0], "priority", Some("low"))
            .unwrap();
        repo.update_scalar_field(&task[0], "status", Some("done"))
            .unwrap();
        let refreshed = repo.read_task(&task[0].path).unwrap();
        assert_eq!(refreshed.priority.as_deref(), Some("low"));
        assert_eq!(refreshed.status, "done");
    }

    #[test]
    fn today_filter_uses_reference_date() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());

        let repo = TaskRepository::open(tmp.path()).unwrap();
        repo.create_task_from_draft(&TaskDraft {
            title: "Today A".into(),
            details: String::new(),
            due: None,
            scheduled: Some("2026-04-10".into()),
            priority: None,
            status: Some("open".into()),
            recurrence: None,
            recurrence_anchor: None,
        })
        .unwrap();
        repo.create_task_from_draft(&TaskDraft {
            title: "Today B".into(),
            details: String::new(),
            due: None,
            scheduled: Some("2026-04-11".into()),
            priority: None,
            status: Some("open".into()),
            recurrence: None,
            recurrence_anchor: None,
        })
        .unwrap();

        let april_10 = repo
            .list_tasks(TaskFilter::Today, "2026-04-10")
            .unwrap();
        let april_11 = repo
            .list_tasks(TaskFilter::Today, "2026-04-11")
            .unwrap();

        assert_eq!(april_10.len(), 1);
        assert_eq!(april_10[0].title, "Today A");
        assert_eq!(april_11.len(), 1);
        assert_eq!(april_11[0].title, "Today B");
    }

    #[test]
    fn toggle_time_tracking_and_filter_tracked() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());

        let repo = TaskRepository::open(tmp.path()).unwrap();
        repo.create_task("Tracked Task").unwrap();
        let task = repo
            .search_tasks(TaskFilter::All, Some("Tracked Task"), &today_local())
            .unwrap();
        assert_eq!(task.len(), 1);
        assert!(!task[0].has_active_time_entry);

        repo.toggle_time_tracking(&task[0]).unwrap();
        let tracked = repo.read_task(&task[0].path).unwrap();
        assert!(tracked.has_active_time_entry);
        assert_eq!(tracked.time_entries.len(), 1);
        assert_eq!(
            repo.list_tasks(TaskFilter::Tracked, &today_local())
                .unwrap()
                .len(),
            1
        );

        repo.toggle_time_tracking(&tracked).unwrap();
        let stopped = repo.read_task(&task[0].path).unwrap();
        assert!(!stopped.has_active_time_entry);
        assert_eq!(
            repo.list_tasks(TaskFilter::Tracked, &today_local())
                .unwrap()
                .len(),
            0
        );
    }
}
