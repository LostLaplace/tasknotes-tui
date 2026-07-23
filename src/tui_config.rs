use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default = "default_keybinds")]
    pub keybinds: BTreeMap<String, KeyCommand>,
    #[serde(default = "default_views")]
    pub views: BTreeMap<u8, ViewConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewConfig {
    pub label: String,
    #[serde(flatten)]
    pub filter: ViewFilter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ViewFilter {
    All,
    Open,
    Date,
    Overdue,
    Tracked,
    Archived,
    Status {
        value: String,
    },
    Expression {
        #[serde(alias = "expression", alias = "where")]
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeyCommand {
    CommandPalette,
    Quit,
    NextTask,
    PrevTask,
    Refresh,
    Search,
    CreateTask,
    QuickCreateTask,
    ToggleComplete,
    ToggleTimeTracking,
    ToggleSkipRecurring,
    ToggleArchive,
    EditTitle,
    OpenInEditor,
    EditDue,
    EditScheduled,
    EditPriority,
    EditStatus,
    /// Sets the selected task's status directly to a fixed value, bypassing the
    /// free-text prompt used by `EditStatus`. Configured in `tasknotes-tui.yaml` as a
    /// string of the form `set_status:<value>`, e.g. `keybinds: { "1": "set_status:inbox" }`.
    SetStatus(String),
    EditRecurrence,
    EditRecurrenceAnchor,
    SetActiveProject,
    FocusPrevDay,
    FocusNextDay,
    FocusPrevWeek,
    FocusNextWeek,
    FocusToday,
}

impl KeyCommand {
    /// Parses the config string form of a key command. Unit commands use their
    /// snake_case name (e.g. `edit_status`); `SetStatus` uses `set_status:<value>`.
    fn from_config_str(value: &str) -> Option<Self> {
        if let Some(status_value) = value.strip_prefix("set_status:") {
            return Some(KeyCommand::SetStatus(status_value.to_string()));
        }
        Some(match value {
            "command_palette" => KeyCommand::CommandPalette,
            "quit" => KeyCommand::Quit,
            "next_task" => KeyCommand::NextTask,
            "prev_task" => KeyCommand::PrevTask,
            "refresh" => KeyCommand::Refresh,
            "search" => KeyCommand::Search,
            "create_task" => KeyCommand::CreateTask,
            "quick_create_task" => KeyCommand::QuickCreateTask,
            "toggle_complete" => KeyCommand::ToggleComplete,
            "toggle_time_tracking" => KeyCommand::ToggleTimeTracking,
            "toggle_skip_recurring" => KeyCommand::ToggleSkipRecurring,
            "toggle_archive" => KeyCommand::ToggleArchive,
            "edit_title" => KeyCommand::EditTitle,
            "open_in_editor" => KeyCommand::OpenInEditor,
            "edit_due" => KeyCommand::EditDue,
            "edit_scheduled" => KeyCommand::EditScheduled,
            "edit_priority" => KeyCommand::EditPriority,
            "edit_status" => KeyCommand::EditStatus,
            "edit_recurrence" => KeyCommand::EditRecurrence,
            "edit_recurrence_anchor" => KeyCommand::EditRecurrenceAnchor,
            "set_active_project" => KeyCommand::SetActiveProject,
            "focus_prev_day" => KeyCommand::FocusPrevDay,
            "focus_next_day" => KeyCommand::FocusNextDay,
            "focus_prev_week" => KeyCommand::FocusPrevWeek,
            "focus_next_week" => KeyCommand::FocusNextWeek,
            "focus_today" => KeyCommand::FocusToday,
            _ => return None,
        })
    }

    /// Renders the config string form of this command (inverse of `from_config_str`).
    fn to_config_string(&self) -> String {
        match self {
            KeyCommand::SetStatus(value) => format!("set_status:{value}"),
            KeyCommand::CommandPalette => "command_palette".to_string(),
            KeyCommand::Quit => "quit".to_string(),
            KeyCommand::NextTask => "next_task".to_string(),
            KeyCommand::PrevTask => "prev_task".to_string(),
            KeyCommand::Refresh => "refresh".to_string(),
            KeyCommand::Search => "search".to_string(),
            KeyCommand::CreateTask => "create_task".to_string(),
            KeyCommand::QuickCreateTask => "quick_create_task".to_string(),
            KeyCommand::ToggleComplete => "toggle_complete".to_string(),
            KeyCommand::ToggleTimeTracking => "toggle_time_tracking".to_string(),
            KeyCommand::ToggleSkipRecurring => "toggle_skip_recurring".to_string(),
            KeyCommand::ToggleArchive => "toggle_archive".to_string(),
            KeyCommand::EditTitle => "edit_title".to_string(),
            KeyCommand::OpenInEditor => "open_in_editor".to_string(),
            KeyCommand::EditDue => "edit_due".to_string(),
            KeyCommand::EditScheduled => "edit_scheduled".to_string(),
            KeyCommand::EditPriority => "edit_priority".to_string(),
            KeyCommand::EditStatus => "edit_status".to_string(),
            KeyCommand::EditRecurrence => "edit_recurrence".to_string(),
            KeyCommand::EditRecurrenceAnchor => "edit_recurrence_anchor".to_string(),
            KeyCommand::SetActiveProject => "set_active_project".to_string(),
            KeyCommand::FocusPrevDay => "focus_prev_day".to_string(),
            KeyCommand::FocusNextDay => "focus_next_day".to_string(),
            KeyCommand::FocusPrevWeek => "focus_prev_week".to_string(),
            KeyCommand::FocusNextWeek => "focus_next_week".to_string(),
            KeyCommand::FocusToday => "focus_today".to_string(),
        }
    }
}

impl Serialize for KeyCommand {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_config_string())
    }
}

impl<'de> Deserialize<'de> for KeyCommand {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        KeyCommand::from_config_str(&raw)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown key command: {raw}")))
    }
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            keybinds: default_keybinds(),
            views: default_views(),
        }
    }
}

impl TuiConfig {
    pub fn command_for_key(&self, key: KeyEvent) -> Option<KeyCommand> {
        let normalized = normalize_key_event(key)?;
        self.keybinds.get(&normalized).cloned()
    }

    pub fn bindings_for_command(&self, command: KeyCommand) -> Vec<String> {
        self.keybinds
            .iter()
            .filter_map(|(key, value)| (*value == command).then_some(key.clone()))
            .collect()
    }
}

pub fn load_tui_config(root: &Path) -> TuiConfig {
    let path = root.join("tasknotes-tui.yaml");
    let Ok(content) = fs::read_to_string(path) else {
        return TuiConfig::default();
    };
    serde_yaml::from_str::<TuiConfig>(&content).unwrap_or_default()
}

pub fn default_config_yaml() -> String {
    serde_yaml::to_string(&TuiConfig::default())
        .expect("default TUI config should always serialize to YAML")
}

pub fn default_views() -> BTreeMap<u8, ViewConfig> {
    BTreeMap::from([
        (
            1,
            ViewConfig {
                label: "Open".into(),
                filter: ViewFilter::Open,
            },
        ),
        (
            2,
            ViewConfig {
                label: "Date".into(),
                filter: ViewFilter::Date,
            },
        ),
        (
            3,
            ViewConfig {
                label: "Overdue".into(),
                filter: ViewFilter::Overdue,
            },
        ),
        (
            4,
            ViewConfig {
                label: "All".into(),
                filter: ViewFilter::All,
            },
        ),
        (
            5,
            ViewConfig {
                label: "Tracked".into(),
                filter: ViewFilter::Tracked,
            },
        ),
        (
            6,
            ViewConfig {
                label: "Archived".into(),
                filter: ViewFilter::Archived,
            },
        ),
        (
            7,
            ViewConfig {
                label: "Project".into(),
                filter: ViewFilter::Expression {
                    value: "hasActiveProject && projectPaths.contains(activeProjectPath) && path != activeProjectPath".into(),
                },
            },
        ),
    ])
}

pub fn default_keybinds() -> BTreeMap<String, KeyCommand> {
    BTreeMap::from([
        ("ctrl-p".into(), KeyCommand::CommandPalette),
        ("q".into(), KeyCommand::Quit),
        ("j".into(), KeyCommand::NextTask),
        ("down".into(), KeyCommand::NextTask),
        ("k".into(), KeyCommand::PrevTask),
        ("up".into(), KeyCommand::PrevTask),
        ("r".into(), KeyCommand::Refresh),
        ("/".into(), KeyCommand::Search),
        ("n".into(), KeyCommand::CreateTask),
        ("c".into(), KeyCommand::QuickCreateTask),
        ("x".into(), KeyCommand::ToggleComplete),
        ("space".into(), KeyCommand::ToggleComplete),
        ("shift-t".into(), KeyCommand::ToggleTimeTracking),
        ("shift-s".into(), KeyCommand::ToggleSkipRecurring),
        ("z".into(), KeyCommand::ToggleArchive),
        ("e".into(), KeyCommand::EditTitle),
        ("i".into(), KeyCommand::OpenInEditor),
        ("d".into(), KeyCommand::EditDue),
        ("s".into(), KeyCommand::EditScheduled),
        ("p".into(), KeyCommand::EditPriority),
        ("t".into(), KeyCommand::EditStatus),
        ("shift-r".into(), KeyCommand::EditRecurrence),
        ("shift-a".into(), KeyCommand::EditRecurrenceAnchor),
        ("shift-p".into(), KeyCommand::SetActiveProject),
        ("h".into(), KeyCommand::FocusPrevDay),
        ("left".into(), KeyCommand::FocusPrevDay),
        ("l".into(), KeyCommand::FocusNextDay),
        ("right".into(), KeyCommand::FocusNextDay),
        ("pageup".into(), KeyCommand::FocusPrevWeek),
        ("pagedown".into(), KeyCommand::FocusNextWeek),
        ("g".into(), KeyCommand::FocusToday),
    ])
}

fn normalize_key_event(key: KeyEvent) -> Option<String> {
    match key.code {
        KeyCode::Enter => Some("enter".into()),
        KeyCode::Esc => Some("esc".into()),
        KeyCode::Left => Some("left".into()),
        KeyCode::Right => Some("right".into()),
        KeyCode::Up => Some("up".into()),
        KeyCode::Down => Some("down".into()),
        KeyCode::PageUp => Some("pageup".into()),
        KeyCode::PageDown => Some("pagedown".into()),
        KeyCode::Char(' ') => Some("space".into()),
        KeyCode::Char(ch) if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(format!("ctrl-{}", ch.to_ascii_lowercase()))
        }
        KeyCode::Char(ch)
            if key.modifiers.contains(KeyModifiers::SHIFT) && ch.is_ascii_alphabetic() =>
        {
            Some(format!("shift-{}", ch.to_ascii_lowercase()))
        }
        KeyCode::Char(ch) => Some(ch.to_ascii_lowercase().to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_custom_views_and_keybinds() {
        let config: TuiConfig = serde_yaml::from_str(
            r#"
keybinds:
  a: create_task
  left: focus_prev_day
  h: focus_prev_day
views:
  1:
    label: "Inbox"
    kind: "open"
  2:
    label: "Doing"
    kind: "status"
    value: "doing"
  3:
    label: "Tracked Doing"
    kind: "expression"
    expression: "status == \"doing\" && isTracked"
"#,
        )
        .unwrap();

        assert_eq!(config.keybinds.get("a"), Some(&KeyCommand::CreateTask));
        assert_eq!(
            config.bindings_for_command(KeyCommand::FocusPrevDay),
            vec!["h".to_string(), "left".to_string()]
        );
        assert_eq!(config.views.get(&1).unwrap().label, "Inbox");
        match &config.views.get(&2).unwrap().filter {
            ViewFilter::Status { value } => assert_eq!(value, "doing"),
            other => panic!("unexpected filter: {other:?}"),
        }
        match &config.views.get(&3).unwrap().filter {
            ViewFilter::Expression { value } => {
                assert_eq!(value, "status == \"doing\" && isTracked")
            }
            other => panic!("unexpected filter: {other:?}"),
        }
    }

    #[test]
    fn default_config_yaml_round_trips() {
        let yaml = default_config_yaml();
        let config: TuiConfig = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(
            config.keybinds.get("ctrl-p"),
            Some(&KeyCommand::CommandPalette)
        );
        assert_eq!(
            config.bindings_for_command(KeyCommand::FocusPrevDay),
            vec!["h".to_string(), "left".to_string()]
        );
        assert_eq!(config.views.get(&1).unwrap().label, "Open");
        assert!(config.views.contains_key(&6));
    }

    #[test]
    fn deserialize_set_status_keybind() {
        let config: TuiConfig = serde_yaml::from_str(
            r#"
keybinds:
  "1": "set_status:inbox"
  "2": "set_status:next_action"
  t: edit_status
"#,
        )
        .unwrap();

        assert_eq!(
            config.keybinds.get("1"),
            Some(&KeyCommand::SetStatus("inbox".to_string()))
        );
        assert_eq!(
            config.keybinds.get("2"),
            Some(&KeyCommand::SetStatus("next_action".to_string()))
        );
        assert_eq!(config.keybinds.get("t"), Some(&KeyCommand::EditStatus));
        assert_eq!(
            config.bindings_for_command(KeyCommand::SetStatus("next_action".to_string())),
            vec!["2".to_string()]
        );
    }
}
