use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default)]
    pub keybinds: KeybindConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindConfig {
    #[serde(default = "default_binding_command_palette")]
    pub command_palette: String,
    #[serde(default = "default_binding_quit")]
    pub quit: String,
    #[serde(default = "default_binding_next_task")]
    pub next_task: String,
    #[serde(default = "default_binding_prev_task")]
    pub prev_task: String,
    #[serde(default = "default_binding_refresh")]
    pub refresh: String,
    #[serde(default = "default_binding_search")]
    pub search: String,
    #[serde(default = "default_binding_create")]
    pub create_task: String,
    #[serde(default = "default_binding_toggle_complete")]
    pub toggle_complete: String,
    #[serde(default = "default_binding_toggle_time_tracking")]
    pub toggle_time_tracking: String,
    #[serde(default = "default_binding_toggle_skip")]
    pub toggle_skip_recurring: String,
    #[serde(default = "default_binding_toggle_archive")]
    pub toggle_archive: String,
    #[serde(default = "default_binding_edit_title")]
    pub edit_title: String,
    #[serde(default = "default_binding_open_in_editor")]
    pub open_in_editor: String,
    #[serde(default = "default_binding_edit_due")]
    pub edit_due: String,
    #[serde(default = "default_binding_edit_scheduled")]
    pub edit_scheduled: String,
    #[serde(default = "default_binding_edit_priority")]
    pub edit_priority: String,
    #[serde(default = "default_binding_edit_status")]
    pub edit_status: String,
    #[serde(default = "default_binding_edit_recurrence")]
    pub edit_recurrence: String,
    #[serde(default = "default_binding_edit_recurrence_anchor")]
    pub edit_recurrence_anchor: String,
    #[serde(default = "default_binding_focus_prev_day")]
    pub focus_prev_day: String,
    #[serde(default = "default_binding_focus_next_day")]
    pub focus_next_day: String,
    #[serde(default = "default_binding_focus_prev_week")]
    pub focus_prev_week: String,
    #[serde(default = "default_binding_focus_next_week")]
    pub focus_next_week: String,
    #[serde(default = "default_binding_focus_today")]
    pub focus_today: String,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            keybinds: KeybindConfig::default(),
            views: default_views(),
        }
    }
}

impl Default for KeybindConfig {
    fn default() -> Self {
        Self {
            command_palette: default_binding_command_palette(),
            quit: default_binding_quit(),
            next_task: default_binding_next_task(),
            prev_task: default_binding_prev_task(),
            refresh: default_binding_refresh(),
            search: default_binding_search(),
            create_task: default_binding_create(),
            toggle_complete: default_binding_toggle_complete(),
            toggle_time_tracking: default_binding_toggle_time_tracking(),
            toggle_skip_recurring: default_binding_toggle_skip(),
            toggle_archive: default_binding_toggle_archive(),
            edit_title: default_binding_edit_title(),
            open_in_editor: default_binding_open_in_editor(),
            edit_due: default_binding_edit_due(),
            edit_scheduled: default_binding_edit_scheduled(),
            edit_priority: default_binding_edit_priority(),
            edit_status: default_binding_edit_status(),
            edit_recurrence: default_binding_edit_recurrence(),
            edit_recurrence_anchor: default_binding_edit_recurrence_anchor(),
            focus_prev_day: default_binding_focus_prev_day(),
            focus_next_day: default_binding_focus_next_day(),
            focus_prev_week: default_binding_focus_prev_week(),
            focus_next_week: default_binding_focus_next_week(),
            focus_today: default_binding_focus_today(),
        }
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
    ])
}

fn default_binding_command_palette() -> String {
    "ctrl-p".into()
}
fn default_binding_quit() -> String {
    "q".into()
}
fn default_binding_next_task() -> String {
    "j".into()
}
fn default_binding_prev_task() -> String {
    "k".into()
}
fn default_binding_refresh() -> String {
    "r".into()
}
fn default_binding_search() -> String {
    "/".into()
}
fn default_binding_create() -> String {
    "n".into()
}
fn default_binding_toggle_complete() -> String {
    "x".into()
}
fn default_binding_toggle_time_tracking() -> String {
    "shift-t".into()
}
fn default_binding_toggle_skip() -> String {
    "shift-s".into()
}
fn default_binding_toggle_archive() -> String {
    "z".into()
}
fn default_binding_edit_title() -> String {
    "e".into()
}
fn default_binding_open_in_editor() -> String {
    "i".into()
}
fn default_binding_edit_due() -> String {
    "d".into()
}
fn default_binding_edit_scheduled() -> String {
    "s".into()
}
fn default_binding_edit_priority() -> String {
    "p".into()
}
fn default_binding_edit_status() -> String {
    "t".into()
}
fn default_binding_edit_recurrence() -> String {
    "shift-r".into()
}
fn default_binding_edit_recurrence_anchor() -> String {
    "shift-a".into()
}
fn default_binding_focus_prev_day() -> String {
    "left".into()
}
fn default_binding_focus_next_day() -> String {
    "right".into()
}
fn default_binding_focus_prev_week() -> String {
    "pageup".into()
}
fn default_binding_focus_next_week() -> String {
    "pagedown".into()
}
fn default_binding_focus_today() -> String {
    "g".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_custom_views_and_keybinds() {
        let config: TuiConfig = serde_yaml::from_str(
            r#"
keybinds:
  create_task: "a"
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

        assert_eq!(config.keybinds.create_task, "a");
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

        assert_eq!(config.keybinds.command_palette, "ctrl-p");
        assert_eq!(config.views.get(&1).unwrap().label, "Open");
        assert!(config.views.contains_key(&6));
    }
}
