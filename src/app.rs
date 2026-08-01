use anyhow::Result;
use std::collections::BTreeMap;

use crate::date::{apply_day_offset, apply_month_offset, get_date_part, today_local};
use crate::repository::{
    build_project_summaries, task_matches_project, ProjectSummary, TaskDraft, TaskFilter,
    TaskRecord, TaskRepository,
};
use crate::tui_config::{KeyCommand, TuiConfig, ViewConfig, ViewFilter, ViewSort};
use crate::urgency::compute_urgency;
use crate::view_query::{compile_view_filters, CompiledViewFilter, ViewEvalSupport};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    None,
    CommandPalette,
    Search,
    PickCreateDue,
    PickCreateScheduled,
    PickEditDue,
    PickEditScheduled,
    TextCreateDue,
    TextCreateScheduled,
    TextEditDue,
    TextEditScheduled,
    PickCreatePriority,
    PickCreateStatus,
    PickCreateProject,
    PickEditPriority,
    PickEditStatus,
    QuickCreateTitle,
    AddNextActionTitle,
    CreateTitle,
    CreateDetails,
    CreatePriority,
    CreateStatus,
    CreateProject,
    CreateRecurrence,
    CreateRecurrenceAnchor,
    ConfirmDelete,
    EditTitle,
    EditPriority,
    EditStatus,
    EditRecurrence,
    EditRecurrenceAnchor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveProject {
    pub path: String,
    pub title: String,
}

/// Which project the Projects view is currently drilled into, if any. Deliberately
/// separate from `ActiveProject`: that mechanism is resolved-path-only (via view slot
/// 7's expression filter) and can't represent a phantom project, whereas this key
/// matches the same way `ProjectSummary::key` does, resolved or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDrilldown {
    pub key: String,
    pub display_title: String,
}

pub struct App {
    pub repo: TaskRepository,
    pub tui_config: TuiConfig,
    pub compiled_views: BTreeMap<u8, CompiledViewFilter>,
    pub view_eval_support: Option<ViewEvalSupport>,
    pub all_tasks: Vec<TaskRecord>,
    pub tasks: Vec<TaskRecord>,
    pub selected: usize,
    pub current_view_slot: u8,
    pub status: String,
    pub search_query: String,
    pub input_mode: InputMode,
    pub input_value: String,
    pub draft: TaskDraft,
    pub palette_selected: usize,
    pub focus_date: String,
    pub calendar_tasks: Vec<TaskRecord>,
    pub picker_date: String,
    pub picker_has_value: bool,
    pub picker_options: Vec<String>,
    pub picker_option_index: usize,
    pub pending_open_editor: bool,
    pub active_project: Option<ActiveProject>,
    pub project_rows: Vec<ProjectSummary>,
    pub project_selected: usize,
    pub project_drilldown: Option<ProjectDrilldown>,
    /// The target project's `projects:` link text, set by `begin_add_next_action` and
    /// consumed when `InputMode::AddNextActionTitle` is submitted.
    pub add_next_action_project: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCommand {
    CreateTask,
    QuickCreateTask,
    Search,
    Refresh,
    DeleteTask,
    ToggleComplete,
    ToggleArchive,
    ToggleTimeTracking,
    ToggleRecurringSkip,
    ViewSlot(u8),
    EditTitle,
    OpenInEditor,
    EditDue,
    EditScheduled,
    EditPriority,
    EditStatus,
    EditRecurrence,
    EditRecurrenceAnchor,
    SetActiveProject,
    ClearActiveProject,
    EnterProject,
    LeaveProject,
    AddNextAction,
}

pub struct PaletteItem {
    pub command: PaletteCommand,
    pub title: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub hotkey: Option<String>,
}

fn static_palette_items() -> Vec<PaletteItem> {
    vec![
        PaletteItem {
            command: PaletteCommand::CreateTask,
            title: "Create task".into(),
            aliases: vec!["new".into(), "n".into(), "add".into()],
            description: "Start the multi-step task creation flow".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::QuickCreateTask,
            title: "Quick create".into(),
            aliases: vec!["quick".into(), "capture".into(), "c".into()],
            description: "Create a task for the focused date from a title only".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::SetActiveProject,
            title: "Set active project".into(),
            aliases: vec![
                "project".into(),
                "activate project".into(),
                "shift-p".into(),
            ],
            description: "Treat the selected task as the active project context".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::ClearActiveProject,
            title: "Clear active project".into(),
            aliases: vec!["clear project".into(), "unset project".into()],
            description: "Clear the current active project context".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::EnterProject,
            title: "Open project".into(),
            aliases: vec!["drill in".into(), "enter".into()],
            description: "Show the selected project's tasks (Projects view)".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::LeaveProject,
            title: "Back to projects".into(),
            aliases: vec!["leave project".into(), "esc".into()],
            description: "Return to the project summary list (Projects view)".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::AddNextAction,
            title: "Add next action".into(),
            aliases: vec!["next action".into(), "N".into()],
            description: "Create a task linked to the highlighted project (Projects view)"
                .into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::Search,
            title: "Search tasks".into(),
            aliases: vec!["find".into(), "/".into()],
            description: "Open live task search".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::Refresh,
            title: "Refresh list".into(),
            aliases: vec!["reload".into(), "r".into()],
            description: "Reload tasks from disk".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::DeleteTask,
            title: "Delete task".into(),
            aliases: vec!["delete".into(), "remove".into(), "trash".into()],
            description: "Delete the selected task after confirmation".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::ToggleComplete,
            title: "Toggle completion".into(),
            aliases: vec!["complete".into(), "done".into(), "x".into()],
            description: "Complete or reopen the selected task".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::ToggleArchive,
            title: "Toggle archive".into(),
            aliases: vec!["archive".into(), "unarchive".into(), "z".into()],
            description: "Archive or restore the selected task".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::ToggleTimeTracking,
            title: "Toggle time tracking".into(),
            aliases: vec!["track".into(), "timer".into(), "T".into()],
            description: "Start or stop time tracking on the selected task".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::ToggleRecurringSkip,
            title: "Toggle recurring skip today".into(),
            aliases: vec!["skip".into(), "recurring".into(), "S".into()],
            description: "Skip or unskip today's recurring instance".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::EditTitle,
            title: "Edit title".into(),
            aliases: vec!["rename".into(), "e".into()],
            description: "Edit the selected task title".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::OpenInEditor,
            title: "Open in editor".into(),
            aliases: vec!["edit".into(), "body".into(), "notes".into(), "i".into()],
            description: "Open the selected task in $EDITOR".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::EditDue,
            title: "Edit due date".into(),
            aliases: vec!["due".into(), "d".into()],
            description: "Edit the selected task due date".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::EditScheduled,
            title: "Edit scheduled date".into(),
            aliases: vec!["scheduled".into(), "s".into()],
            description: "Edit the selected task scheduled date".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::EditPriority,
            title: "Edit priority".into(),
            aliases: vec!["priority".into(), "p".into()],
            description: "Edit the selected task priority".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::EditStatus,
            title: "Edit status".into(),
            aliases: vec!["status".into(), "t".into()],
            description: "Edit the selected task status".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::EditRecurrence,
            title: "Edit recurrence rule".into(),
            aliases: vec!["recurrence".into(), "rrule".into(), "R".into()],
            description: "Edit the selected task recurrence rule".into(),
            hotkey: None,
        },
        PaletteItem {
            command: PaletteCommand::EditRecurrenceAnchor,
            title: "Edit recurrence anchor".into(),
            aliases: vec!["anchor".into(), "A".into()],
            description: "Edit the selected task recurrence anchor".into(),
            hotkey: None,
        },
    ]
}

impl App {
    pub fn new(repo: TaskRepository, tui_config: TuiConfig) -> Result<Self> {
        let initial_view_slot = tui_config.views.keys().next().copied().unwrap_or(1);
        let mut app = Self {
            repo,
            compiled_views: compile_view_filters(&tui_config.views),
            tui_config,
            view_eval_support: None,
            all_tasks: Vec::new(),
            tasks: Vec::new(),
            selected: 0,
            current_view_slot: initial_view_slot,
            status: String::new(),
            search_query: String::new(),
            input_mode: InputMode::None,
            input_value: String::new(),
            draft: TaskDraft::default(),
            palette_selected: 0,
            focus_date: today_local(),
            calendar_tasks: Vec::new(),
            picker_date: today_local(),
            picker_has_value: true,
            picker_options: Vec::new(),
            picker_option_index: 0,
            pending_open_editor: false,
            active_project: None,
            project_rows: Vec::new(),
            project_selected: 0,
            project_drilldown: None,
            add_next_action_project: None,
        };
        app.reload_from_disk()?;
        Ok(app)
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.apply_filters();
        Ok(())
    }

    pub fn reload_from_disk(&mut self) -> Result<()> {
        self.all_tasks = self.repo.list_tasks(TaskFilter::All, &self.focus_date)?;
        self.view_eval_support = Some(self.repo.build_view_eval_support()?);
        self.calendar_tasks = self.all_tasks.clone();
        self.apply_filters();
        Ok(())
    }

    fn apply_filters(&mut self) {
        if self.is_projects_view() {
            self.apply_project_filters();
            return;
        }
        self.tasks = self
            .all_tasks
            .iter()
            .filter(|task| self.matches_filter(task))
            .filter(|task| self.matches_search(task))
            .cloned()
            .collect();
        if matches!(self.current_view().map(|view| view.sort), Some(ViewSort::Urgency)) {
            let today = today_local();
            let urgency = &self.tui_config.urgency;
            self.tasks.sort_by(|a, b| {
                compute_urgency(b, &today, urgency)
                    .partial_cmp(&compute_urgency(a, &today, urgency))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        if self.selected >= self.tasks.len() && !self.tasks.is_empty() {
            self.selected = self.tasks.len() - 1;
        } else if self.tasks.is_empty() {
            self.selected = 0;
        }
        if let Some(error) = self.current_view_error() {
            self.status = format!("View {} invalid: {}", self.current_view_slot, error);
            return;
        }
        self.status = if self.search_query.trim().is_empty() {
            if matches!(
                self.current_view().map(|view| &view.filter),
                Some(ViewFilter::Date | ViewFilter::Overdue | ViewFilter::Expression { .. })
            ) {
                format!("{} tasks for {}", self.tasks.len(), self.focus_date)
            } else {
                format!("{} tasks", self.tasks.len())
            }
        } else {
            if matches!(
                self.current_view().map(|view| &view.filter),
                Some(ViewFilter::Date | ViewFilter::Overdue)
            ) {
                format!(
                    "{} tasks for {} matching '{}'",
                    self.tasks.len(),
                    self.focus_date,
                    self.search_query
                )
            } else {
                format!(
                    "{} tasks matching '{}'",
                    self.tasks.len(),
                    self.search_query
                )
            }
        };
    }

    pub fn selected_task(&self) -> Option<&TaskRecord> {
        self.tasks.get(self.selected)
    }

    /// Whether the current view slot is configured as `kind: projects`.
    pub fn is_projects_view(&self) -> bool {
        matches!(
            self.current_view().map(|view| &view.filter),
            Some(ViewFilter::Projects)
        )
    }

    /// Whether the Projects view is showing the project summary list (as opposed to
    /// being drilled into one project's tasks).
    pub fn is_showing_project_list(&self) -> bool {
        self.is_projects_view() && self.project_drilldown.is_none()
    }

    pub fn selected_project_row(&self) -> Option<&ProjectSummary> {
        self.project_rows.get(self.project_selected)
    }

    fn matches_project_search(&self, row: &ProjectSummary) -> bool {
        let needle = self.search_query.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return true;
        }
        row.display_title.to_ascii_lowercase().contains(&needle)
    }

    fn apply_project_filters(&mut self) {
        self.tasks.clear();
        let Some(support) = self.view_eval_support.clone() else {
            self.project_rows.clear();
            self.status = "Projects unavailable: still loading".to_string();
            return;
        };
        if let Some(drilldown) = self.project_drilldown.clone() {
            self.project_rows.clear();
            self.tasks = self
                .all_tasks
                .iter()
                .filter(|task| task_matches_project(task, &drilldown.key, &support.all_files))
                .filter(|task| self.matches_search(task))
                .cloned()
                .collect();
            if self.selected >= self.tasks.len() && !self.tasks.is_empty() {
                self.selected = self.tasks.len() - 1;
            } else if self.tasks.is_empty() {
                self.selected = 0;
            }
            self.status = format!("{} tasks in {}", self.tasks.len(), drilldown.display_title);
        } else {
            self.project_rows = build_project_summaries(
                &self.all_tasks,
                &support.all_files,
                &self.repo.field_mapping,
                &self.repo.config.archive,
                &self.tui_config.next_action_statuses,
            )
            .into_iter()
            .filter(|row| self.matches_project_search(row))
            .collect();
            if self.project_selected >= self.project_rows.len() && !self.project_rows.is_empty() {
                self.project_selected = self.project_rows.len() - 1;
            } else if self.project_rows.is_empty() {
                self.project_selected = 0;
            }
            self.status = format!("{} projects", self.project_rows.len());
        }
    }

    /// Drills into the currently selected project row's tasks. No-op unless the
    /// Projects view is showing the summary list.
    pub fn enter_selected_project(&mut self) {
        if !self.is_showing_project_list() {
            return;
        }
        let Some(row) = self.selected_project_row() else {
            return;
        };
        self.project_drilldown = Some(ProjectDrilldown {
            key: row.key.clone(),
            display_title: row.display_title.clone(),
        });
        self.selected = 0;
        self.apply_filters();
    }

    /// Backs out of a project drilldown to the summary list. No-op unless currently
    /// drilled in. Leaves `project_selected` untouched so the list cursor position
    /// survives the round trip.
    pub fn exit_project_drilldown(&mut self) {
        if self.project_drilldown.is_none() {
            return;
        }
        self.project_drilldown = None;
        self.apply_filters();
    }

    /// Starts a title-only quick-create prompt for a new task linked to the highlighted
    /// project, pre-set to the first configured `next_action_statuses` value. No-op
    /// unless the Projects view is showing the summary list, or if no next-action
    /// status is configured (there'd be nothing meaningful to set).
    pub fn begin_add_next_action(&mut self) {
        if !self.is_showing_project_list() {
            return;
        }
        if self.tui_config.next_action_statuses.is_empty() {
            self.status = "Configure next_action_statuses to use this".to_string();
            return;
        }
        let Some((link_text, display_title)) = self
            .selected_project_row()
            .map(|row| (row.link_text.clone(), row.display_title.clone()))
        else {
            return;
        };
        self.add_next_action_project = Some(link_text);
        self.input_mode = InputMode::AddNextActionTitle;
        self.input_value.clear();
        self.status = format!("Add next action for {display_title}: enter title");
    }

    pub fn next(&mut self) {
        if self.is_showing_project_list() {
            if self.project_rows.is_empty() {
                return;
            }
            self.project_selected = (self.project_selected + 1).min(self.project_rows.len() - 1);
            return;
        }
        if self.tasks.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.tasks.len() - 1);
    }

    pub fn previous(&mut self) {
        if self.is_showing_project_list() {
            if self.project_selected > 0 {
                self.project_selected -= 1;
            }
            return;
        }
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn activate_view_slot(&mut self, slot: u8) -> Result<()> {
        if self.tui_config.views.contains_key(&slot) {
            self.current_view_slot = slot;
            if !self.is_projects_view() {
                self.project_drilldown = None;
            }
            self.refresh()?;
            if let Some(error) = self.current_view_error() {
                self.status = format!("View {} invalid: {}", slot, error);
            } else {
                self.status = format!(
                    "View {}: {}",
                    slot,
                    self.current_view()
                        .map(|view| view.label.as_str())
                        .unwrap_or("Unknown")
                );
            }
        }
        Ok(())
    }

    pub fn move_focus_date(&mut self, offset_days: i64) -> Result<()> {
        if let Some(next) = apply_day_offset(&self.focus_date, offset_days) {
            self.focus_date = next;
            self.refresh()?;
        }
        Ok(())
    }

    pub fn reset_focus_date(&mut self) -> Result<()> {
        self.focus_date = today_local();
        self.refresh()
    }

    pub fn toggle_selected(&mut self) -> Result<()> {
        if let Some(task) = self.selected_task().cloned() {
            let updated = self.repo.toggle_complete(&task)?;
            self.replace_cached_task(&task.path, updated);
            self.status = format!("Updated {}", task.title);
        }
        Ok(())
    }

    pub fn toggle_selected_archive(&mut self) -> Result<()> {
        if let Some(task) = self.selected_task().cloned() {
            let was_archived =
                crate::repository::is_archived_task(&task, &self.repo.config.archive);
            let updated = self.repo.toggle_archive(&task)?;
            self.replace_cached_task(&task.path, updated);
            self.status = if was_archived {
                format!("Restored {}", task.title)
            } else {
                format!("Archived {}", task.title)
            };
        }
        Ok(())
    }

    pub fn toggle_selected_time_tracking(&mut self) -> Result<()> {
        if let Some(task) = self.selected_task().cloned() {
            let updated = self.repo.toggle_time_tracking(&task)?;
            self.replace_cached_task(&task.path, updated);
            self.status = if task.has_active_time_entry {
                format!("Stopped tracking {}", task.title)
            } else {
                format!("Started tracking {}", task.title)
            };
        }
        Ok(())
    }

    pub fn skip_selected_today(&mut self) -> Result<()> {
        if let Some(task) = self.selected_task().cloned() {
            let updated = self.repo.toggle_skip_today(&task)?;
            self.replace_cached_task(&task.path, updated);
            self.status = format!("Updated recurring state for {}", task.title);
        }
        Ok(())
    }

    pub fn begin_search(&mut self) {
        self.input_mode = InputMode::Search;
        self.input_value = self.search_query.clone();
        self.status = "Search tasks".to_string();
    }

    pub fn begin_delete(&mut self) {
        if let Some(title) = self.selected_task().map(|task| task.title.clone()) {
            self.input_mode = InputMode::ConfirmDelete;
            self.input_value.clear();
            self.status = format!("Delete {}: Enter confirms, Esc cancels", title);
        }
    }

    fn replace_cached_task(&mut self, old_path: &str, updated: TaskRecord) {
        let updated_path = updated.path.clone();
        if self
            .active_project
            .as_ref()
            .is_some_and(|project| project.path == old_path)
        {
            self.active_project = Some(ActiveProject {
                path: updated.path.clone(),
                title: updated.title.clone(),
            });
        }
        if let Some(existing) = self.all_tasks.iter_mut().find(|task| task.path == old_path) {
            *existing = updated;
        } else {
            self.all_tasks.push(updated);
        }
        self.calendar_tasks = self.all_tasks.clone();
        self.apply_filters();
        if let Some(index) = self.tasks.iter().position(|task| task.path == updated_path) {
            self.selected = index;
        }
    }

    fn append_cached_task(&mut self, task: TaskRecord) {
        let task_path = task.path.clone();
        self.all_tasks.push(task);
        self.calendar_tasks = self.all_tasks.clone();
        self.apply_filters();
        if let Some(index) = self.tasks.iter().position(|entry| entry.path == task_path) {
            self.selected = index;
        }
    }

    fn remove_cached_task(&mut self, path: &str) {
        if self
            .active_project
            .as_ref()
            .is_some_and(|project| project.path == path)
        {
            self.active_project = None;
        }
        self.all_tasks.retain(|task| task.path != path);
        self.calendar_tasks = self.all_tasks.clone();
        self.apply_filters();
    }

    pub fn refresh_selected_task(&mut self) -> Result<()> {
        let Some(task) = self.selected_task().cloned() else {
            return Ok(());
        };
        match self.repo.read_task(&task.path) {
            Ok(updated) => {
                self.replace_cached_task(&task.path, updated);
                Ok(())
            }
            Err(_) => self.reload_from_disk(),
        }
    }

    pub fn begin_command_palette(&mut self) {
        self.input_mode = InputMode::CommandPalette;
        self.input_value.clear();
        self.palette_selected = 0;
        self.status = "Command palette".to_string();
    }

    pub fn begin_create(&mut self) {
        self.draft = TaskDraft::default();
        self.input_mode = InputMode::CreateTitle;
        self.input_value.clear();
        self.status = "New task: enter title".to_string();
    }

    pub fn begin_quick_create(&mut self) {
        self.input_mode = InputMode::QuickCreateTitle;
        self.input_value.clear();
        self.status = if let Some(project) = self.active_project.as_ref() {
            format!("Quick create in project {}: enter title", project.title)
        } else {
            "Quick create: enter title".to_string()
        };
    }

    pub fn set_selected_as_active_project(&mut self) -> Result<()> {
        let Some(task) = self.selected_task().cloned() else {
            self.status = "No task selected".to_string();
            return Ok(());
        };
        if self
            .active_project
            .as_ref()
            .is_some_and(|project| project.path == task.path)
        {
            self.active_project = None;
            self.status = format!("Cleared active project {}", task.title);
            self.apply_filters();
            return Ok(());
        }
        self.active_project = Some(ActiveProject {
            path: task.path.clone(),
            title: task.title.clone(),
        });
        self.status = format!("Active project set to {}", task.title);
        self.apply_filters();
        Ok(())
    }

    pub fn clear_active_project(&mut self) {
        self.active_project = None;
        self.status = "Cleared active project".to_string();
        self.apply_filters();
    }

    pub fn begin_edit_title(&mut self) {
        if let Some(task) = self.selected_task().cloned() {
            self.input_mode = InputMode::EditTitle;
            self.input_value = task.title;
            self.status = "Edit title".to_string();
        }
    }

    pub fn begin_edit_due(&mut self) {
        if let Some(task) = self.selected_task().cloned() {
            self.begin_date_picker(InputMode::PickEditDue, task.due.as_deref());
            self.status =
                "Edit due date: arrows move, H/L month, t today, c clear, / type".to_string();
        }
    }

    pub fn begin_edit_scheduled(&mut self) {
        if let Some(task) = self.selected_task().cloned() {
            self.begin_date_picker(InputMode::PickEditScheduled, task.scheduled.as_deref());
            self.status =
                "Edit scheduled date: arrows move, H/L month, t today, c clear, / type".to_string();
        }
    }

    pub fn begin_edit_priority(&mut self) {
        if let Some(task) = self.selected_task().cloned() {
            let options = self.repo.config.priority.values.clone();
            self.begin_option_picker(InputMode::PickEditPriority, options, task.priority.as_deref());
            self.status = "Edit priority: arrows move, enter saves, / type".to_string();
        }
    }

    pub fn begin_edit_status(&mut self) {
        if let Some(task) = self.selected_task().cloned() {
            let options = self.repo.config.status.values.clone();
            self.begin_option_picker(InputMode::PickEditStatus, options, Some(task.status.as_str()));
            self.status = "Edit status: arrows move, enter saves, / type".to_string();
        }
    }

    /// Sets the selected task's status to `value` directly, without opening the
    /// free-text edit prompt. Used by `KeyCommand::SetStatus` bindings.
    pub fn set_selected_status(&mut self, value: &str) -> Result<()> {
        if let Some(task) = self.selected_task().cloned() {
            let value = option_from_input(value);
            let updated = self
                .repo
                .update_scalar_field(&task, "status", value.as_deref())?;
            self.replace_cached_task(&task.path, updated);
            self.status = format!(
                "Set status to {} for {}",
                value.as_deref().unwrap_or("(none)"),
                task.title
            );
        }
        Ok(())
    }

    pub fn begin_edit_recurrence(&mut self) {
        if let Some(task) = self.selected_task().cloned() {
            self.input_mode = InputMode::EditRecurrence;
            self.input_value = task
                .normalized_frontmatter
                .get("recurrence")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            self.status = "Edit recurrence rule (RRULE..., blank clears)".to_string();
        }
    }

    pub fn begin_edit_recurrence_anchor(&mut self) {
        if let Some(task) = self.selected_task().cloned() {
            self.input_mode = InputMode::EditRecurrenceAnchor;
            self.input_value = task
                .normalized_frontmatter
                .get("recurrenceAnchor")
                .and_then(|value| value.as_str())
                .unwrap_or("scheduled")
                .to_string();
            self.status = "Edit recurrence anchor (scheduled or completion)".to_string();
        }
    }

    pub fn push_input_char(&mut self, ch: char) -> Result<()> {
        self.input_value.push(ch);
        match self.input_mode {
            InputMode::Search => {
                self.search_query = self.input_value.clone();
                self.apply_filters();
            }
            InputMode::CommandPalette => {
                self.palette_selected = 0;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn backspace_input(&mut self) -> Result<()> {
        self.input_value.pop();
        match self.input_mode {
            InputMode::Search => {
                self.search_query = self.input_value.clone();
                self.apply_filters();
            }
            InputMode::CommandPalette => {
                self.palette_selected = 0;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn next_palette_item(&mut self) {
        let count = self.filtered_palette_items().len();
        if count == 0 {
            self.palette_selected = 0;
            return;
        }
        self.palette_selected = (self.palette_selected + 1).min(count - 1);
    }

    pub fn previous_palette_item(&mut self) {
        if self.palette_selected > 0 {
            self.palette_selected -= 1;
        }
    }

    pub fn cancel_input(&mut self) -> Result<()> {
        self.input_mode = InputMode::None;
        self.input_value.clear();
        self.status = "Cancelled".to_string();
        if self.search_query.is_empty() {
            self.apply_filters();
        }
        Ok(())
    }

    pub fn submit_input(&mut self) -> Result<()> {
        match self.input_mode {
            InputMode::None => {}
            InputMode::CommandPalette => {
                let Some(item) = self
                    .filtered_palette_items()
                    .get(self.palette_selected)
                    .map(|item| item.command)
                else {
                    self.status = "No matching commands".to_string();
                    return Ok(());
                };
                self.input_mode = InputMode::None;
                self.input_value.clear();
                self.palette_selected = 0;
                self.run_palette_command(item)?;
            }
            InputMode::Search => {
                self.search_query = self.input_value.trim().to_string();
                self.input_mode = InputMode::None;
                self.apply_filters();
            }
            InputMode::PickCreateDue => {
                self.draft.due = self.current_picker_value();
                let initial = self.draft.scheduled.clone();
                self.begin_date_picker(InputMode::PickCreateScheduled, initial.as_deref());
                self.status =
                    "New task: scheduled date, arrows move, H/L month, t today, c clear, / type"
                        .to_string();
            }
            InputMode::PickCreateScheduled => {
                self.draft.scheduled = self.current_picker_value();
                self.begin_create_priority_picker();
                self.status = "New task: priority, arrows move, enter saves, / type".to_string();
            }
            InputMode::PickEditDue => {
                if let Some(task) = self.selected_task().cloned() {
                    let value = self.current_picker_value();
                    let updated = self
                        .repo
                        .update_date_field(&task, "due", value.as_deref())?;
                    self.replace_cached_task(&task.path, updated);
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.status = format!("Updated due date for {}", task.title);
                }
            }
            InputMode::PickEditScheduled => {
                if let Some(task) = self.selected_task().cloned() {
                    let value = self.current_picker_value();
                    let updated =
                        self.repo
                            .update_date_field(&task, "scheduled", value.as_deref())?;
                    self.replace_cached_task(&task.path, updated);
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.status = format!("Updated scheduled date for {}", task.title);
                }
            }
            InputMode::PickCreatePriority => {
                self.draft.priority = self.current_picker_option();
                self.begin_create_status_picker();
                self.status = "New task: status, arrows move, enter saves, / type".to_string();
            }
            InputMode::PickCreateStatus => {
                self.draft.status = self.current_picker_option();
                self.input_mode = InputMode::CreateRecurrence;
                self.input_value.clear();
                self.status =
                    "New task: recurrence rule (RRULE..., blank for non-recurring)".to_string();
            }
            InputMode::PickEditPriority => {
                if let Some(task) = self.selected_task().cloned() {
                    let value = self.current_picker_option();
                    let updated =
                        self.repo
                            .update_scalar_field(&task, "priority", value.as_deref())?;
                    self.replace_cached_task(&task.path, updated);
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.status = format!("Updated priority for {}", task.title);
                }
            }
            InputMode::PickEditStatus => {
                if let Some(task) = self.selected_task().cloned() {
                    let value = self.current_picker_option();
                    let updated =
                        self.repo
                            .update_scalar_field(&task, "status", value.as_deref())?;
                    self.replace_cached_task(&task.path, updated);
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.status = format!("Updated status for {}", task.title);
                }
            }
            InputMode::TextCreateDue => {
                self.draft.due = option_from_input(&self.input_value);
                let initial = self.draft.scheduled.clone();
                self.begin_date_picker(InputMode::PickCreateScheduled, initial.as_deref());
                self.status =
                    "New task: scheduled date, arrows move, H/L month, t today, c clear, / type"
                        .to_string();
            }
            InputMode::TextCreateScheduled => {
                self.draft.scheduled = option_from_input(&self.input_value);
                self.begin_create_priority_picker();
                self.status = "New task: priority, arrows move, enter saves, / type".to_string();
            }
            InputMode::TextEditDue => {
                if let Some(task) = self.selected_task().cloned() {
                    let value = option_from_input(&self.input_value);
                    let updated = self
                        .repo
                        .update_date_field(&task, "due", value.as_deref())?;
                    self.replace_cached_task(&task.path, updated);
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.status = format!("Updated due date for {}", task.title);
                }
            }
            InputMode::TextEditScheduled => {
                if let Some(task) = self.selected_task().cloned() {
                    let value = option_from_input(&self.input_value);
                    let updated =
                        self.repo
                            .update_date_field(&task, "scheduled", value.as_deref())?;
                    self.replace_cached_task(&task.path, updated);
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.status = format!("Updated scheduled date for {}", task.title);
                }
            }
            InputMode::QuickCreateTitle => {
                let title = self.input_value.trim().to_string();
                if title.is_empty() {
                    self.status = "Title must not be empty".to_string();
                } else {
                    let created = self.repo.create_task_from_draft(&TaskDraft {
                        title: title.clone(),
                        details: String::new(),
                        due: None,
                        scheduled: None,
                        priority: None,
                        status: None,
                        recurrence: None,
                        recurrence_anchor: None,
                        projects: self.active_project_links()?,
                    })?;
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.append_cached_task(created);
                    self.status = format!("Quick created {}", title);
                }
            }
            InputMode::AddNextActionTitle => {
                let title = self.input_value.trim().to_string();
                if title.is_empty() {
                    self.status = "Title must not be empty".to_string();
                } else {
                    let project_link = self.add_next_action_project.take();
                    let status = self.tui_config.next_action_statuses.first().cloned();
                    let created = self.repo.create_task_from_draft(&TaskDraft {
                        title: title.clone(),
                        details: String::new(),
                        due: None,
                        scheduled: None,
                        priority: None,
                        status,
                        recurrence: None,
                        recurrence_anchor: None,
                        projects: project_link.into_iter().collect(),
                    })?;
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.append_cached_task(created);
                    self.status = format!("Added next action {}", title);
                }
            }
            InputMode::CreateTitle => {
                let title = self.input_value.trim();
                if title.is_empty() {
                    self.status = "Title must not be empty".to_string();
                } else {
                    self.draft.title = title.to_string();
                    self.input_mode = InputMode::CreateDetails;
                    self.input_value.clear();
                    self.status = "New task: enter details".to_string();
                }
            }
            InputMode::CreateDetails => {
                self.draft.details = self.input_value.clone();
                self.begin_create_project_picker();
                self.status =
                    "New task: project, arrows move, enter saves, / new project".to_string();
            }
            InputMode::PickCreateProject => {
                let selected = self.current_picker_option();
                self.draft.projects = self.resolve_project_option(selected.as_deref());
                let initial = self.draft.due.clone();
                self.begin_date_picker(InputMode::PickCreateDue, initial.as_deref());
                self.status =
                    "New task: due date, arrows move, H/L month, t today, c clear, / type"
                        .to_string();
            }
            InputMode::CreateProject => {
                let title = self.input_value.trim();
                self.draft.projects = if title.is_empty() {
                    Vec::new()
                } else {
                    vec![format!("[[{title}]]")]
                };
                let initial = self.draft.due.clone();
                self.begin_date_picker(InputMode::PickCreateDue, initial.as_deref());
                self.status =
                    "New task: due date, arrows move, H/L month, t today, c clear, / type"
                        .to_string();
            }
            InputMode::CreatePriority => {
                self.draft.priority = option_from_input(&self.input_value);
                self.begin_create_status_picker();
                self.status = "New task: status, arrows move, enter saves, / type".to_string();
            }
            InputMode::CreateStatus => {
                self.draft.status = option_from_input(&self.input_value);
                self.input_mode = InputMode::CreateRecurrence;
                self.input_value.clear();
                self.status =
                    "New task: recurrence rule (RRULE..., blank for non-recurring)".to_string();
            }
            InputMode::CreateRecurrence => {
                self.draft.recurrence = option_from_input(&self.input_value);
                if self.draft.recurrence.is_some() {
                    self.input_mode = InputMode::CreateRecurrenceAnchor;
                    self.input_value = "scheduled".to_string();
                    self.status =
                        "New task: recurrence anchor (scheduled or completion)".to_string();
                } else {
                    if self.draft.projects.is_empty() {
                        self.draft.projects = self.active_project_links()?;
                    }
                    let title = self.draft.title.clone();
                    let created = self.repo.create_task_from_draft(&self.draft)?;
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.append_cached_task(created);
                    self.status = format!("Created {}", title);
                }
            }
            InputMode::CreateRecurrenceAnchor => {
                self.draft.recurrence_anchor = option_from_input(&self.input_value);
                if self.draft.projects.is_empty() {
                    self.draft.projects = self.active_project_links()?;
                }
                let title = self.draft.title.clone();
                let created = self.repo.create_task_from_draft(&self.draft)?;
                self.input_mode = InputMode::None;
                self.input_value.clear();
                self.append_cached_task(created);
                self.status = format!("Created {}", title);
            }
            InputMode::ConfirmDelete => {
                if let Some(task) = self.selected_task().cloned() {
                    self.repo.delete_task(&task)?;
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.remove_cached_task(&task.path);
                    self.status = format!("Deleted {}", task.title);
                }
            }
            InputMode::EditTitle => {
                if let Some(task) = self.selected_task().cloned() {
                    let title = self.input_value.trim().to_string();
                    let updated = self.repo.update_title(&task, &title)?;
                    self.replace_cached_task(&task.path, updated);
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.status = format!("Renamed {}", title);
                }
            }
            InputMode::EditPriority => {
                if let Some(task) = self.selected_task().cloned() {
                    let value = option_from_input(&self.input_value);
                    let updated =
                        self.repo
                            .update_scalar_field(&task, "priority", value.as_deref())?;
                    self.replace_cached_task(&task.path, updated);
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.status = format!("Updated priority for {}", task.title);
                }
            }
            InputMode::EditStatus => {
                if let Some(task) = self.selected_task().cloned() {
                    let value = option_from_input(&self.input_value);
                    let updated =
                        self.repo
                            .update_scalar_field(&task, "status", value.as_deref())?;
                    self.replace_cached_task(&task.path, updated);
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.status = format!("Updated status for {}", task.title);
                }
            }
            InputMode::EditRecurrence => {
                if let Some(task) = self.selected_task().cloned() {
                    let value = option_from_input(&self.input_value);
                    let mut updated =
                        self.repo
                            .update_scalar_field(&task, "recurrence", value.as_deref())?;
                    if value.is_none() {
                        updated = self
                            .repo
                            .update_scalar_field(&task, "recurrenceAnchor", None)?;
                    }
                    self.replace_cached_task(&task.path, updated);
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.status = format!("Updated recurrence for {}", task.title);
                }
            }
            InputMode::EditRecurrenceAnchor => {
                if let Some(task) = self.selected_task().cloned() {
                    let value = option_from_input(&self.input_value);
                    let updated = self.repo.update_scalar_field(
                        &task,
                        "recurrenceAnchor",
                        value.as_deref(),
                    )?;
                    self.replace_cached_task(&task.path, updated);
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.status = format!("Updated recurrence anchor for {}", task.title);
                }
            }
        }
        Ok(())
    }

    pub fn input_prompt(&self) -> Option<&'static str> {
        Some(match self.input_mode {
            InputMode::None => return None,
            InputMode::CommandPalette => "Command palette",
            InputMode::Search => "Search",
            InputMode::PickCreateDue => "Pick due date",
            InputMode::PickCreateScheduled => "Pick scheduled date",
            InputMode::PickEditDue => "Pick due date",
            InputMode::PickEditScheduled => "Pick scheduled date",
            InputMode::TextCreateDue => "Type due date",
            InputMode::TextCreateScheduled => "Type scheduled date",
            InputMode::TextEditDue => "Type due date",
            InputMode::TextEditScheduled => "Type scheduled date",
            InputMode::PickCreatePriority => "Pick priority",
            InputMode::PickCreateStatus => "Pick status",
            InputMode::PickCreateProject => "Pick project",
            InputMode::PickEditPriority => "Pick priority",
            InputMode::PickEditStatus => "Pick status",
            InputMode::QuickCreateTitle => "Quick create",
            InputMode::AddNextActionTitle => "Add next action",
            InputMode::CreateTitle => "New title",
            InputMode::CreateDetails => "New details",
            InputMode::CreatePriority => "New priority",
            InputMode::CreateStatus => "New status",
            InputMode::CreateProject => "New project",
            InputMode::CreateRecurrence => "New recurrence",
            InputMode::CreateRecurrenceAnchor => "New recurrence anchor",
            InputMode::ConfirmDelete => "Confirm delete",
            InputMode::EditTitle => "Edit title",
            InputMode::EditPriority => "Edit priority",
            InputMode::EditStatus => "Edit status",
            InputMode::EditRecurrence => "Edit recurrence",
            InputMode::EditRecurrenceAnchor => "Edit recurrence anchor",
        })
    }

    pub fn mode_label(&self) -> String {
        if let Some((step, total, label)) = self.create_progress() {
            return format!("Create {step}/{total}: {label}");
        }
        if self.input_mode == InputMode::None {
            if self.is_showing_project_list() {
                return "Projects".to_string();
            }
            if let Some(drilldown) = self.project_drilldown.as_ref() {
                return format!("Project: {}", drilldown.display_title);
            }
        }
        match self.input_mode {
            InputMode::None => "Browse".to_string(),
            InputMode::CommandPalette => "Command Palette".to_string(),
            InputMode::Search => "Search".to_string(),
            InputMode::PickCreateDue | InputMode::PickEditDue => "Date Picker: Due".to_string(),
            InputMode::PickCreateScheduled | InputMode::PickEditScheduled => {
                "Date Picker: Scheduled".to_string()
            }
            InputMode::TextCreateDue | InputMode::TextEditDue => "Type Due Date".to_string(),
            InputMode::TextCreateScheduled | InputMode::TextEditScheduled => {
                "Type Scheduled Date".to_string()
            }
            InputMode::PickCreatePriority | InputMode::PickEditPriority => {
                "Priority Picker".to_string()
            }
            InputMode::PickCreateStatus | InputMode::PickEditStatus => {
                "Status Picker".to_string()
            }
            InputMode::PickCreateProject => "Project Picker".to_string(),
            InputMode::QuickCreateTitle => "Quick Create".to_string(),
            InputMode::AddNextActionTitle => "Add Next Action".to_string(),
            InputMode::ConfirmDelete => "Confirm Delete".to_string(),
            InputMode::EditTitle => "Edit Title".to_string(),
            InputMode::EditPriority => "Edit Priority".to_string(),
            InputMode::EditStatus => "Edit Status".to_string(),
            InputMode::EditRecurrence => "Edit Recurrence".to_string(),
            InputMode::EditRecurrenceAnchor => "Edit Recurrence Anchor".to_string(),
            InputMode::CreateTitle
            | InputMode::CreateDetails
            | InputMode::CreatePriority
            | InputMode::CreateStatus
            | InputMode::CreateProject
            | InputMode::CreateRecurrence
            | InputMode::CreateRecurrenceAnchor => "Create".to_string(),
        }
    }

    pub fn create_progress(&self) -> Option<(usize, usize, &'static str)> {
        let progress = match self.input_mode {
            InputMode::CreateTitle => (1, 8, "Title"),
            InputMode::CreateDetails => (2, 8, "Details"),
            InputMode::PickCreateProject | InputMode::CreateProject => (3, 8, "Project"),
            InputMode::PickCreateDue | InputMode::TextCreateDue => (4, 8, "Due"),
            InputMode::PickCreateScheduled | InputMode::TextCreateScheduled => (5, 8, "Scheduled"),
            InputMode::PickCreatePriority | InputMode::CreatePriority => (6, 8, "Priority"),
            InputMode::PickCreateStatus | InputMode::CreateStatus => (7, 8, "Status"),
            InputMode::CreateRecurrence => (8, 8, "Recurrence"),
            InputMode::CreateRecurrenceAnchor => (8, 8, "Anchor"),
            _ => return None,
        };
        Some(progress)
    }

    pub fn selected_position_label(&self) -> String {
        if self.is_showing_project_list() {
            return if self.project_rows.is_empty() {
                "0/0".to_string()
            } else {
                format!("{}/{}", self.project_selected + 1, self.project_rows.len())
            };
        }
        if self.tasks.is_empty() {
            "0/0".to_string()
        } else {
            format!("{}/{}", self.selected + 1, self.tasks.len())
        }
    }

    pub fn active_project_title(&self) -> Option<&str> {
        self.active_project
            .as_ref()
            .map(|project| project.title.as_str())
    }

    pub fn active_project_path(&self) -> Option<&str> {
        self.active_project
            .as_ref()
            .map(|project| project.path.as_str())
    }

    pub fn active_project_links(&self) -> Result<Vec<String>> {
        self.active_project
            .as_ref()
            .map(|project| {
                self.repo
                    .read_task(&project.path)
                    .and_then(|task| self.repo.canonical_project_link_for_task(&task))
                    .map(|link| vec![link])
            })
            .transpose()
            .map(|links| links.unwrap_or_default())
    }

    pub fn contextual_shortcuts(&self) -> Vec<(String, String)> {
        if self.input_mode == InputMode::None && self.is_showing_project_list() {
            return vec![
                (
                    "Move".to_string(),
                    self.binding_label(KeyCommand::NextTask, "j/k"),
                ),
                ("Views".to_string(), "1-9 switch".to_string()),
                (
                    "Open".to_string(),
                    self.binding_label(KeyCommand::EnterProject, "enter"),
                ),
                (
                    "Next action".to_string(),
                    self.binding_label(KeyCommand::AddNextAction, "N"),
                ),
            ];
        }
        if self.input_mode == InputMode::None && self.project_drilldown.is_some() {
            return vec![
                (
                    "Move".to_string(),
                    self.binding_label(KeyCommand::NextTask, "j/k"),
                ),
                (
                    "Back".to_string(),
                    self.binding_label(KeyCommand::LeaveProject, "esc"),
                ),
                (
                    "Actions".to_string(),
                    format!(
                        "{} complete, {} archive, {} palette",
                        self.binding_label(KeyCommand::ToggleComplete, "x"),
                        self.binding_label(KeyCommand::ToggleArchive, "z"),
                        self.binding_label(KeyCommand::CommandPalette, "ctrl-p"),
                    ),
                ),
            ];
        }
        match self.input_mode {
            InputMode::None => vec![
                (
                    "Move".to_string(),
                    self.binding_label(KeyCommand::NextTask, "j/k"),
                ),
                ("Views".to_string(), "1-9 switch".to_string()),
                (
                    "Date".to_string(),
                    self.binding_label(KeyCommand::FocusToday, "h/l, pgup/pgdn, g"),
                ),
                (
                    "Actions".to_string(),
                    format!(
                        "{} complete, {} archive, {} project, {} palette",
                        self.binding_label(KeyCommand::ToggleComplete, "x"),
                        self.binding_label(KeyCommand::ToggleArchive, "z"),
                        self.binding_label(KeyCommand::SetActiveProject, "P"),
                        self.binding_label(KeyCommand::CommandPalette, "ctrl-p"),
                    ),
                ),
            ],
            InputMode::CommandPalette => vec![
                ("Move".to_string(), "up/down".to_string()),
                ("Apply".to_string(), "enter".to_string()),
                ("Cancel".to_string(), "esc".to_string()),
            ],
            InputMode::Search => vec![
                ("Apply".to_string(), "enter".to_string()),
                ("Cancel".to_string(), "esc".to_string()),
                (
                    "Scope".to_string(),
                    "title, body, path, priority".to_string(),
                ),
            ],
            InputMode::PickCreateDue
            | InputMode::PickCreateScheduled
            | InputMode::PickEditDue
            | InputMode::PickEditScheduled => vec![
                ("Move".to_string(), "arrows or hjkl".to_string()),
                ("Month".to_string(), "H/L".to_string()),
                ("Other".to_string(), "t today, c clear, / type".to_string()),
            ],
            InputMode::TextCreateDue
            | InputMode::TextCreateScheduled
            | InputMode::TextEditDue
            | InputMode::TextEditScheduled => vec![
                ("Format".to_string(), "YYYY-MM-DD".to_string()),
                ("Clear".to_string(), "blank".to_string()),
                ("Cancel".to_string(), "esc".to_string()),
            ],
            InputMode::PickCreatePriority
            | InputMode::PickCreateStatus
            | InputMode::PickEditPriority
            | InputMode::PickEditStatus => vec![
                ("Move".to_string(), "up/down or j/k".to_string()),
                ("Apply".to_string(), "enter".to_string()),
                ("Other".to_string(), "/ type custom value".to_string()),
            ],
            _ => vec![
                ("Submit".to_string(), "enter".to_string()),
                ("Cancel".to_string(), "esc".to_string()),
            ],
        }
    }

    pub fn is_input_active(&self) -> bool {
        self.input_mode != InputMode::None
    }

    pub fn is_palette_active(&self) -> bool {
        self.input_mode == InputMode::CommandPalette
    }

    pub fn is_date_picker_active(&self) -> bool {
        matches!(
            self.input_mode,
            InputMode::PickCreateDue
                | InputMode::PickCreateScheduled
                | InputMode::PickEditDue
                | InputMode::PickEditScheduled
        )
    }

    pub fn is_option_picker_active(&self) -> bool {
        matches!(
            self.input_mode,
            InputMode::PickCreatePriority
                | InputMode::PickCreateStatus
                | InputMode::PickCreateProject
                | InputMode::PickEditPriority
                | InputMode::PickEditStatus
        )
    }

    pub fn move_picker_day(&mut self, offset_days: i64) {
        if let Some(next) = apply_day_offset(&self.picker_date, offset_days) {
            self.picker_date = next;
            self.picker_has_value = true;
        }
    }

    pub fn move_picker_month(&mut self, offset_months: i32) {
        if let Some(next) = apply_month_offset(&self.picker_date, offset_months) {
            self.picker_date = next;
            self.picker_has_value = true;
        }
    }

    pub fn clear_picker_value(&mut self) {
        self.picker_has_value = false;
    }

    pub fn set_picker_today(&mut self) {
        self.picker_date = today_local();
        self.picker_has_value = true;
    }

    pub fn switch_picker_to_text(&mut self) {
        self.input_value = if self.picker_has_value {
            self.picker_date.clone()
        } else {
            String::new()
        };
        self.input_mode = match self.input_mode {
            InputMode::PickCreateDue => InputMode::TextCreateDue,
            InputMode::PickCreateScheduled => InputMode::TextCreateScheduled,
            InputMode::PickEditDue => InputMode::TextEditDue,
            InputMode::PickEditScheduled => InputMode::TextEditScheduled,
            other => other,
        };
        self.status = "Type date as YYYY-MM-DD, blank clears".to_string();
    }

    pub fn request_open_in_editor(&mut self) {
        self.pending_open_editor = true;
        self.status = "Opening selected task in $EDITOR".to_string();
    }

    pub fn take_open_in_editor_request(&mut self) -> bool {
        let pending = self.pending_open_editor;
        self.pending_open_editor = false;
        pending
    }

    pub fn selected_task_absolute_path(&self) -> Option<std::path::PathBuf> {
        self.selected_task()
            .map(|task| self.repo.absolute_task_path(task))
    }

    pub fn filtered_palette_items(&self) -> Vec<PaletteItem> {
        let query = self.input_value.trim().to_ascii_lowercase();
        let mut items: Vec<PaletteItem> = self
            .palette_items()
            .into_iter()
            .filter(|item| query.is_empty() || palette_matches(item, &query))
            .collect();
        items.sort_by_key(|item| palette_rank(item, &query));
        items
    }

    fn run_palette_command(&mut self, command: PaletteCommand) -> Result<()> {
        match command {
            PaletteCommand::CreateTask => self.begin_create(),
            PaletteCommand::QuickCreateTask => self.begin_quick_create(),
            PaletteCommand::Search => self.begin_search(),
            PaletteCommand::Refresh => {
                self.reload_from_disk()?;
                self.status = "Refreshed task list".to_string();
            }
            PaletteCommand::DeleteTask => self.begin_delete(),
            PaletteCommand::ToggleComplete => self.toggle_selected()?,
            PaletteCommand::ToggleArchive => self.toggle_selected_archive()?,
            PaletteCommand::ToggleTimeTracking => self.toggle_selected_time_tracking()?,
            PaletteCommand::ToggleRecurringSkip => self.skip_selected_today()?,
            PaletteCommand::ViewSlot(slot) => self.activate_view_slot(slot)?,
            PaletteCommand::EditTitle => self.begin_edit_title(),
            PaletteCommand::OpenInEditor => {
                self.request_open_in_editor();
            }
            PaletteCommand::EditDue => self.begin_edit_due(),
            PaletteCommand::EditScheduled => self.begin_edit_scheduled(),
            PaletteCommand::EditPriority => self.begin_edit_priority(),
            PaletteCommand::EditStatus => self.begin_edit_status(),
            PaletteCommand::EditRecurrence => self.begin_edit_recurrence(),
            PaletteCommand::EditRecurrenceAnchor => self.begin_edit_recurrence_anchor(),
            PaletteCommand::SetActiveProject => self.set_selected_as_active_project()?,
            PaletteCommand::ClearActiveProject => self.clear_active_project(),
            PaletteCommand::EnterProject => self.enter_selected_project(),
            PaletteCommand::LeaveProject => self.exit_project_drilldown(),
            PaletteCommand::AddNextAction => self.begin_add_next_action(),
        }
        Ok(())
    }
}

impl App {
    fn matches_filter(&self, task: &TaskRecord) -> bool {
        let Some(compiled) = self.compiled_views.get(&self.current_view_slot) else {
            return true;
        };
        let Some(support) = self.view_eval_support.as_ref() else {
            return false;
        };
        compiled.matches(
            task,
            &self.focus_date,
            &self.repo.field_mapping,
            &self.repo.config.archive,
            support,
            self.active_project.as_ref(),
            &self.tui_config.urgency,
        )
    }

    fn matches_search(&self, task: &TaskRecord) -> bool {
        let needle = self.search_query.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return true;
        }
        task.title.to_ascii_lowercase().contains(&needle)
            || task.path.to_ascii_lowercase().contains(&needle)
            || task.body.to_ascii_lowercase().contains(&needle)
            || task
                .priority
                .as_deref()
                .map(|value| value.to_ascii_lowercase().contains(&needle))
                .unwrap_or(false)
    }

    fn begin_date_picker(&mut self, mode: InputMode, initial: Option<&str>) {
        self.input_mode = mode;
        self.input_value.clear();
        // No existing value: start cleared rather than silently pre-filled with today,
        // so pressing Enter without touching the picker leaves the field unset. The
        // cursor still starts on today so `t`/arrow keys/Enter-after-moving are all one
        // keystroke away from picking it.
        self.picker_date = initial.map(get_date_part).unwrap_or_else(today_local);
        self.picker_has_value = initial.is_some();
    }

    fn current_picker_value(&self) -> Option<String> {
        if self.picker_has_value {
            Some(self.picker_date.clone())
        } else {
            None
        }
    }

    fn begin_option_picker(&mut self, mode: InputMode, options: Vec<String>, current: Option<&str>) {
        self.input_mode = mode;
        self.input_value.clear();
        let index = current
            .and_then(|value| options.iter().position(|option| option == value))
            .unwrap_or(0);
        self.picker_options = options;
        self.picker_option_index = index;
    }

    pub fn move_picker_option(&mut self, offset: isize) {
        if self.picker_options.is_empty() {
            return;
        }
        let len = self.picker_options.len() as isize;
        let mut next = self.picker_option_index as isize + offset;
        next = ((next % len) + len) % len;
        self.picker_option_index = next as usize;
    }

    fn current_picker_option(&self) -> Option<String> {
        self.picker_options.get(self.picker_option_index).cloned()
    }

    const NO_PROJECT_OPTION: &'static str = "(none)";

    fn project_summaries(&self) -> Vec<ProjectSummary> {
        let Some(support) = self.view_eval_support.as_ref() else {
            return Vec::new();
        };
        build_project_summaries(
            &self.all_tasks,
            &support.all_files,
            &self.repo.field_mapping,
            &self.repo.config.archive,
            &self.tui_config.next_action_statuses,
        )
    }

    fn begin_create_project_picker(&mut self) {
        let rows = self.project_summaries();
        let mut options = vec![Self::NO_PROJECT_OPTION.to_string()];
        options.extend(rows.into_iter().map(|row| row.display_title));
        self.begin_option_picker(InputMode::PickCreateProject, options, Some(Self::NO_PROJECT_OPTION));
    }

    /// Resolves a project picker's selected label (an existing project's display title,
    /// or the "(none)" sentinel) back to `projects:` link text. A label that doesn't
    /// match any known project (only reachable via free-text entry) is treated as the
    /// title of a brand-new phantom project.
    fn resolve_project_option(&self, label: Option<&str>) -> Vec<String> {
        match label {
            None => Vec::new(),
            Some(label) if label == Self::NO_PROJECT_OPTION => Vec::new(),
            Some(title) => self
                .project_summaries()
                .into_iter()
                .find(|row| row.display_title == title)
                .map(|row| vec![row.link_text])
                .unwrap_or_else(|| vec![format!("[[{title}]]")]),
        }
    }

    fn begin_create_priority_picker(&mut self) {
        let options = self.repo.config.priority.values.clone();
        let default = self.repo.config.defaults.priority.clone();
        self.begin_option_picker(InputMode::PickCreatePriority, options, Some(&default));
    }

    fn begin_create_status_picker(&mut self) {
        let options = self.repo.config.status.values.clone();
        let default = self.repo.config.defaults.status.clone();
        self.begin_option_picker(InputMode::PickCreateStatus, options, Some(&default));
    }

    pub fn switch_option_picker_to_text(&mut self) {
        let is_project_picker = self.input_mode == InputMode::PickCreateProject;
        self.input_value = if is_project_picker {
            // Prefilling with "(none)" or an existing project's title isn't useful here
            // — free-text entry on the project step is for typing a brand-new project.
            String::new()
        } else {
            self.current_picker_option().unwrap_or_default()
        };
        self.input_mode = match self.input_mode {
            InputMode::PickCreatePriority => InputMode::CreatePriority,
            InputMode::PickCreateStatus => InputMode::CreateStatus,
            InputMode::PickCreateProject => InputMode::CreateProject,
            InputMode::PickEditPriority => InputMode::EditPriority,
            InputMode::PickEditStatus => InputMode::EditStatus,
            other => other,
        };
        self.status = if is_project_picker {
            "Type new project title, blank = no project".to_string()
        } else {
            "Type value, blank clears".to_string()
        };
    }

    pub fn current_view(&self) -> Option<&ViewConfig> {
        self.tui_config.views.get(&self.current_view_slot)
    }

    pub fn available_view_slots(&self) -> Vec<u8> {
        self.tui_config.views.keys().copied().collect()
    }

    fn current_view_error(&self) -> Option<String> {
        self.compiled_views
            .get(&self.current_view_slot)
            .and_then(CompiledViewFilter::error_message)
    }

    fn palette_items(&self) -> Vec<PaletteItem> {
        let mut items = static_palette_items();
        for item in &mut items {
            item.hotkey = self.command_hotkey(item.command);
        }
        items.extend(
            self.tui_config
                .views
                .iter()
                .map(|(slot, view)| PaletteItem {
                    command: PaletteCommand::ViewSlot(*slot),
                    title: format!("Switch to view {}: {}", slot, view.label),
                    aliases: vec![
                        slot.to_string(),
                        view.label.to_ascii_lowercase(),
                        view_filter_name(&view.filter).to_string(),
                    ],
                    description: format!("Activate configured view slot {}", slot),
                    hotkey: Some(slot.to_string()),
                }),
        );
        items
    }

    fn command_hotkey(&self, command: PaletteCommand) -> Option<String> {
        let key_command = match command {
            PaletteCommand::CreateTask => KeyCommand::CreateTask,
            PaletteCommand::QuickCreateTask => KeyCommand::QuickCreateTask,
            PaletteCommand::Search => KeyCommand::Search,
            PaletteCommand::Refresh => KeyCommand::Refresh,
            PaletteCommand::ToggleComplete => KeyCommand::ToggleComplete,
            PaletteCommand::ToggleArchive => KeyCommand::ToggleArchive,
            PaletteCommand::ToggleTimeTracking => KeyCommand::ToggleTimeTracking,
            PaletteCommand::ToggleRecurringSkip => KeyCommand::ToggleSkipRecurring,
            PaletteCommand::DeleteTask => return None,
            PaletteCommand::ViewSlot(_) => return None,
            PaletteCommand::EditTitle => KeyCommand::EditTitle,
            PaletteCommand::OpenInEditor => KeyCommand::OpenInEditor,
            PaletteCommand::EditDue => KeyCommand::EditDue,
            PaletteCommand::EditScheduled => KeyCommand::EditScheduled,
            PaletteCommand::EditPriority => KeyCommand::EditPriority,
            PaletteCommand::EditStatus => KeyCommand::EditStatus,
            PaletteCommand::EditRecurrence => KeyCommand::EditRecurrence,
            PaletteCommand::EditRecurrenceAnchor => KeyCommand::EditRecurrenceAnchor,
            PaletteCommand::SetActiveProject => KeyCommand::SetActiveProject,
            PaletteCommand::ClearActiveProject => return None,
            PaletteCommand::EnterProject => KeyCommand::EnterProject,
            PaletteCommand::LeaveProject => KeyCommand::LeaveProject,
            PaletteCommand::AddNextAction => KeyCommand::AddNextAction,
        };
        let bindings = self.tui_config.bindings_for_command(key_command);
        (!bindings.is_empty()).then(|| bindings.join(", "))
    }

    fn binding_label(&self, command: KeyCommand, fallback: &str) -> String {
        let bindings = self.tui_config.bindings_for_command(command);
        if bindings.is_empty() {
            fallback.to_string()
        } else {
            bindings.join(", ")
        }
    }
}

fn option_from_input(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn palette_matches(item: &PaletteItem, query: &str) -> bool {
    let title = item.title.to_ascii_lowercase();
    let description = item.description.to_ascii_lowercase();
    if title.contains(query) || description.contains(query) {
        return true;
    }
    item.aliases
        .iter()
        .any(|alias| alias.to_ascii_lowercase().contains(query))
        || fuzzy_match(&title, query)
}

fn palette_rank(item: &PaletteItem, query: &str) -> (usize, usize, String) {
    if query.is_empty() {
        return (2, 0, item.title.clone());
    }
    let title = item.title.to_ascii_lowercase();
    if title.starts_with(query) {
        return (0, title.len(), item.title.clone());
    }
    if item
        .aliases
        .iter()
        .any(|alias| alias.to_ascii_lowercase().starts_with(query))
    {
        return (1, title.len(), item.title.clone());
    }
    (2, title.len(), item.title.clone())
}

fn view_filter_name(filter: &ViewFilter) -> &str {
    match filter {
        ViewFilter::All => "all",
        ViewFilter::Open => "open",
        ViewFilter::Date => "date",
        ViewFilter::Overdue => "overdue",
        ViewFilter::Tracked => "tracked",
        ViewFilter::Archived => "archived",
        ViewFilter::Status { .. } => "status",
        ViewFilter::Expression { .. } => "expression",
        ViewFilter::Projects => "projects",
    }
}

fn fuzzy_match(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let mut chars = needle.chars();
    let mut current = chars.next();
    for ch in haystack.chars() {
        if Some(ch) == current {
            current = chars.next();
            if current.is_none() {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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
  scheduled:
    type: date
  projects:
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
    fn active_project_drives_project_view_and_quick_create() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());

        let repo = TaskRepository::open(tmp.path()).unwrap();
        let project = repo
            .create_task_from_draft(&TaskDraft {
                title: "Plan release".into(),
                details: "Project container".into(),
                due: None,
                scheduled: Some("2026-03-29".into()),
                priority: Some("high".into()),
                status: Some("doing".into()),
                recurrence: None,
                recurrence_anchor: None,
                projects: vec![],
            })
            .unwrap();
        repo.create_task_from_draft(&TaskDraft {
            title: "Write changelog".into(),
            details: String::new(),
            due: None,
            scheduled: Some("2026-03-30".into()),
            priority: None,
            status: Some("open".into()),
            recurrence: None,
            recurrence_anchor: None,
            projects: vec!["[[Plan release]]".into()],
        })
        .unwrap();

        let mut app = App::new(repo, TuiConfig::default()).unwrap();
        app.selected = app
            .tasks
            .iter()
            .position(|task| task.path == project.path)
            .unwrap();
        app.set_selected_as_active_project().unwrap();
        assert_eq!(app.active_project_title(), Some("Plan release"));

        app.activate_view_slot(7).unwrap();
        assert_eq!(app.tasks.len(), 1);
        assert_eq!(app.tasks[0].title, "Write changelog");

        app.begin_quick_create();
        app.input_value = "Ship release".into();
        app.submit_input().unwrap();

        assert_eq!(app.current_view_slot, 7);
        assert_eq!(app.tasks.len(), 2);
        assert!(app.tasks.iter().any(|task| task.title == "Ship release"));
        let created = app
            .all_tasks
            .iter()
            .find(|task| task.title == "Ship release")
            .unwrap();
        assert_eq!(
            created.normalized_frontmatter.get("projects"),
            Some(&serde_json::json!(["[[Plan release]]"]))
        );
    }

    #[test]
    fn setting_active_project_on_same_task_toggles_it_off() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());

        let repo = TaskRepository::open(tmp.path()).unwrap();
        let project = repo
            .create_task_from_draft(&TaskDraft {
                title: "Plan release".into(),
                details: String::new(),
                due: None,
                scheduled: Some("2026-03-29".into()),
                priority: None,
                status: Some("open".into()),
                recurrence: None,
                recurrence_anchor: None,
                projects: vec![],
            })
            .unwrap();

        let mut app = App::new(repo, TuiConfig::default()).unwrap();
        app.selected = app
            .tasks
            .iter()
            .position(|task| task.path == project.path)
            .unwrap();

        app.set_selected_as_active_project().unwrap();
        assert_eq!(app.active_project_title(), Some("Plan release"));

        app.set_selected_as_active_project().unwrap();
        assert_eq!(app.active_project_title(), None);
    }

    #[test]
    fn set_selected_status_updates_status_without_free_text_prompt() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());

        let repo = TaskRepository::open(tmp.path()).unwrap();
        let task = repo
            .create_task_from_draft(&TaskDraft {
                title: "Triage inbox item".into(),
                details: String::new(),
                due: None,
                scheduled: None,
                priority: None,
                status: Some("inbox".into()),
                recurrence: None,
                recurrence_anchor: None,
                projects: vec![],
            })
            .unwrap();

        let mut config = TuiConfig::default();
        config
            .keybinds
            .insert("b".into(), KeyCommand::SetStatus("next_action".into()));

        let mut app = App::new(repo, config).unwrap();
        app.selected = app
            .tasks
            .iter()
            .position(|t| t.path == task.path)
            .unwrap();

        // Simulate the configured keybind resolving to a `SetStatus` command, then
        // dispatching it the same way `ui.rs` would.
        let key = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE);
        let command = app.tui_config.command_for_key(key);
        assert_eq!(command, Some(KeyCommand::SetStatus("next_action".into())));

        // No input_mode prompt should be involved: status flips immediately.
        assert_eq!(app.input_mode, InputMode::None);
        app.set_selected_status("next_action").unwrap();
        assert_eq!(app.input_mode, InputMode::None);

        let updated = app
            .all_tasks
            .iter()
            .find(|t| t.path == task.path)
            .unwrap();
        assert_eq!(updated.status, "next_action");
    }

    #[test]
    fn edit_status_picker_moves_selection_and_saves_on_submit() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());

        let repo = TaskRepository::open(tmp.path()).unwrap();
        let task = repo
            .create_task_from_draft(&TaskDraft {
                title: "Triage inbox item".into(),
                details: String::new(),
                due: None,
                scheduled: None,
                priority: None,
                status: Some("inbox".into()),
                recurrence: None,
                recurrence_anchor: None,
                projects: vec![],
            })
            .unwrap();

        let config = TuiConfig::default();
        let mut app = App::new(repo, config).unwrap();
        app.repo.config.status.values = vec![
            "none".into(),
            "inbox".into(),
            "next_action".into(),
            "done".into(),
        ];
        app.selected = app
            .tasks
            .iter()
            .position(|t| t.path == task.path)
            .unwrap();

        app.begin_edit_status();
        assert_eq!(app.input_mode, InputMode::PickEditStatus);
        assert_eq!(app.picker_option_index, 1); // "inbox"

        app.move_picker_option(2); // inbox -> next_action -> done
        assert_eq!(app.picker_option_index, 3); // "done"

        app.submit_input().unwrap();
        assert_eq!(app.input_mode, InputMode::None);

        let updated = app
            .all_tasks
            .iter()
            .find(|t| t.path == task.path)
            .unwrap();
        assert_eq!(updated.status, "done");
    }

    #[test]
    fn edit_priority_picker_moves_selection_and_saves_on_submit() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());

        let repo = TaskRepository::open(tmp.path()).unwrap();
        let task = repo
            .create_task_from_draft(&TaskDraft {
                title: "Triage inbox item".into(),
                details: String::new(),
                due: None,
                scheduled: None,
                priority: Some("normal".into()),
                status: None,
                recurrence: None,
                recurrence_anchor: None,
                projects: vec![],
            })
            .unwrap();

        let config = TuiConfig::default();
        let mut app = App::new(repo, config).unwrap();
        app.repo.config.priority.values = vec!["low".into(), "normal".into(), "high".into()];
        app.selected = app
            .tasks
            .iter()
            .position(|t| t.path == task.path)
            .unwrap();

        app.begin_edit_priority();
        assert_eq!(app.input_mode, InputMode::PickEditPriority);
        assert_eq!(app.picker_option_index, 1); // "normal"

        app.move_picker_option(1); // normal -> high
        assert_eq!(app.picker_option_index, 2);

        app.submit_input().unwrap();
        assert_eq!(app.input_mode, InputMode::None);

        let updated = app
            .all_tasks
            .iter()
            .find(|t| t.path == task.path)
            .unwrap();
        assert_eq!(updated.priority.as_deref(), Some("high"));
    }

    fn draft_with_project(title: &str, status: &str, project: &str) -> TaskDraft {
        TaskDraft {
            title: title.into(),
            details: String::new(),
            due: None,
            scheduled: None,
            priority: None,
            status: Some(status.into()),
            recurrence: None,
            recurrence_anchor: None,
            projects: vec![project.into()],
        }
    }

    fn config_with_projects_view() -> TuiConfig {
        let mut config = TuiConfig::default();
        config.views.insert(
            8,
            ViewConfig {
                label: "Projects".into(),
                filter: ViewFilter::Projects,
                sort: ViewSort::Date,
            },
        );
        config
    }

    #[test]
    fn projects_view_lists_rows_and_drilldown_shows_only_linked_tasks() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());
        let repo = TaskRepository::open(tmp.path()).unwrap();
        repo.create_task_from_draft(&TaskDraft {
            title: "Project Alpha".into(),
            details: String::new(),
            due: None,
            scheduled: None,
            priority: None,
            status: Some("open".into()),
            recurrence: None,
            recurrence_anchor: None,
            projects: vec![],
        })
        .unwrap();
        repo.create_task_from_draft(&TaskDraft {
            title: "Child of Alpha".into(),
            details: String::new(),
            due: None,
            scheduled: None,
            priority: None,
            status: Some("open".into()),
            recurrence: None,
            recurrence_anchor: None,
            projects: vec!["[[Project Alpha]]".into()],
        })
        .unwrap();
        repo.create_task_from_draft(&TaskDraft {
            title: "Unrelated task".into(),
            details: String::new(),
            due: None,
            scheduled: None,
            priority: None,
            status: Some("open".into()),
            recurrence: None,
            recurrence_anchor: None,
            projects: vec![],
        })
        .unwrap();

        let mut app = App::new(repo, config_with_projects_view()).unwrap();
        app.activate_view_slot(8).unwrap();
        assert!(app.is_showing_project_list());
        assert_eq!(app.tasks.len(), 0);
        assert_eq!(app.project_rows.len(), 1);
        assert_eq!(app.project_rows[0].display_title, "Project Alpha");

        app.enter_selected_project();
        assert!(!app.is_showing_project_list());
        assert_eq!(app.tasks.len(), 1);
        assert_eq!(app.tasks[0].title, "Child of Alpha");

        app.exit_project_drilldown();
        assert!(app.is_showing_project_list());
        assert_eq!(app.project_rows.len(), 1);
    }

    #[test]
    fn projects_view_next_previous_move_project_selection_not_task_selection() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());
        let repo = TaskRepository::open(tmp.path()).unwrap();
        repo.create_task_from_draft(&draft_with_project(
            "Child A",
            "open",
            "[[Alpha]]",
        ))
        .unwrap();
        repo.create_task_from_draft(&draft_with_project(
            "Child B",
            "open",
            "[[Beta]]",
        ))
        .unwrap();

        let mut app = App::new(repo, config_with_projects_view()).unwrap();
        app.activate_view_slot(8).unwrap();
        assert_eq!(app.project_rows.len(), 2);

        app.next();
        assert_eq!(app.project_selected, 1);
        app.previous();
        assert_eq!(app.project_selected, 0);
    }

    #[test]
    fn projects_view_search_filters_rows_by_title() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());
        let repo = TaskRepository::open(tmp.path()).unwrap();
        repo.create_task_from_draft(&draft_with_project(
            "Child A",
            "open",
            "[[Alpha]]",
        ))
        .unwrap();
        repo.create_task_from_draft(&draft_with_project(
            "Child B",
            "open",
            "[[Beta]]",
        ))
        .unwrap();

        let mut app = App::new(repo, config_with_projects_view()).unwrap();
        app.activate_view_slot(8).unwrap();
        app.search_query = "alpha".into();
        app.refresh().unwrap();

        assert_eq!(app.project_rows.len(), 1);
        assert_eq!(app.project_rows[0].display_title, "Alpha");
    }

    #[test]
    fn add_next_action_creates_task_linked_to_highlighted_project() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());
        let repo = TaskRepository::open(tmp.path()).unwrap();
        repo.create_task_from_draft(&draft_with_project(
            "Existing child",
            "open",
            "[[Alpha]]",
        ))
        .unwrap();

        let mut config = config_with_projects_view();
        config.next_action_statuses = vec!["next_action".into()];
        let mut app = App::new(repo, config).unwrap();
        app.activate_view_slot(8).unwrap();
        assert_eq!(app.project_rows.len(), 1);
        assert!(!app.project_rows[0].has_next_action);

        app.begin_add_next_action();
        assert_eq!(app.input_mode, InputMode::AddNextActionTitle);
        app.input_value = "Call vendor".into();
        app.submit_input().unwrap();

        assert_eq!(app.input_mode, InputMode::None);
        let created = app
            .all_tasks
            .iter()
            .find(|task| task.title == "Call vendor")
            .unwrap();
        assert_eq!(created.status, "next_action");
        assert_eq!(
            created.normalized_frontmatter.get("projects"),
            Some(&serde_json::json!(["[[Alpha]]"]))
        );
        assert!(app.project_rows[0].has_next_action);
    }

    #[test]
    fn add_next_action_is_noop_without_next_action_statuses_configured() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());
        let repo = TaskRepository::open(tmp.path()).unwrap();
        repo.create_task_from_draft(&draft_with_project("Existing child", "open", "[[Alpha]]"))
            .unwrap();

        let mut app = App::new(repo, config_with_projects_view()).unwrap();
        app.activate_view_slot(8).unwrap();

        app.begin_add_next_action();
        assert_eq!(app.input_mode, InputMode::None);
        assert!(app.status.contains("next_action_statuses"));
    }

    #[test]
    fn create_flow_project_picker_links_selected_existing_project() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());
        let repo = TaskRepository::open(tmp.path()).unwrap();
        repo.create_task_from_draft(&draft_with_project("Existing child", "open", "[[Alpha]]"))
            .unwrap();

        let mut app = App::new(repo, TuiConfig::default()).unwrap();
        app.begin_create();
        app.input_value = "New task".into();
        app.submit_input().unwrap();
        assert_eq!(app.input_mode, InputMode::CreateDetails);

        app.input_value.clear();
        app.submit_input().unwrap();
        assert_eq!(app.input_mode, InputMode::PickCreateProject);
        assert!(app.picker_options.contains(&"(none)".to_string()));
        assert!(app.picker_options.contains(&"Alpha".to_string()));

        app.picker_option_index = app
            .picker_options
            .iter()
            .position(|option| option == "Alpha")
            .unwrap();
        app.submit_input().unwrap();
        assert_eq!(app.input_mode, InputMode::PickCreateDue);
        assert_eq!(app.draft.projects, vec!["[[Alpha]]".to_string()]);

        // Fast-forward through the remaining steps with defaults.
        app.submit_input().unwrap(); // due -> scheduled
        app.submit_input().unwrap(); // scheduled -> priority
        app.submit_input().unwrap(); // priority -> status
        app.submit_input().unwrap(); // status -> recurrence
        app.input_value.clear();
        app.submit_input().unwrap(); // blank recurrence -> create

        assert_eq!(app.input_mode, InputMode::None);
        let created = app
            .all_tasks
            .iter()
            .find(|task| task.title == "New task")
            .unwrap();
        assert_eq!(
            created.normalized_frontmatter.get("projects"),
            Some(&serde_json::json!(["[[Alpha]]"]))
        );
    }

    #[test]
    fn create_flow_project_picker_free_text_creates_phantom_project() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());
        let repo = TaskRepository::open(tmp.path()).unwrap();

        let mut app = App::new(repo, TuiConfig::default()).unwrap();
        app.begin_create();
        app.input_value = "New task".into();
        app.submit_input().unwrap();
        app.input_value.clear();
        app.submit_input().unwrap();
        assert_eq!(app.input_mode, InputMode::PickCreateProject);

        app.switch_option_picker_to_text();
        assert_eq!(app.input_mode, InputMode::CreateProject);
        assert_eq!(app.input_value, "");

        app.input_value = "Brand New Project".into();
        app.submit_input().unwrap();
        assert_eq!(app.input_mode, InputMode::PickCreateDue);
        assert_eq!(
            app.draft.projects,
            vec!["[[Brand New Project]]".to_string()]
        );
    }

    #[test]
    fn create_flow_defaults_to_active_project_when_picker_left_unset() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());
        let repo = TaskRepository::open(tmp.path()).unwrap();
        let project = repo
            .create_task_from_draft(&TaskDraft {
                title: "Plan release".into(),
                details: String::new(),
                due: None,
                scheduled: None,
                priority: None,
                status: Some("open".into()),
                recurrence: None,
                recurrence_anchor: None,
                projects: vec![],
            })
            .unwrap();

        let mut app = App::new(repo, TuiConfig::default()).unwrap();
        app.selected = app
            .tasks
            .iter()
            .position(|task| task.path == project.path)
            .unwrap();
        app.set_selected_as_active_project().unwrap();

        app.begin_create();
        app.input_value = "Ship release".into();
        app.submit_input().unwrap();
        app.input_value.clear();
        app.submit_input().unwrap();
        assert_eq!(app.input_mode, InputMode::PickCreateProject);
        // Leave the picker at its "(none)" default and move on.
        app.submit_input().unwrap();
        assert_eq!(app.input_mode, InputMode::PickCreateDue);

        app.submit_input().unwrap(); // due -> scheduled
        app.submit_input().unwrap(); // scheduled -> priority
        app.submit_input().unwrap(); // priority -> status
        app.submit_input().unwrap(); // status -> recurrence
        app.input_value.clear();
        app.submit_input().unwrap(); // blank recurrence -> create

        let created = app
            .all_tasks
            .iter()
            .find(|task| task.title == "Ship release")
            .unwrap();
        assert_eq!(
            created.normalized_frontmatter.get("projects"),
            Some(&serde_json::json!(["[[Plan release]]"]))
        );
    }

    #[test]
    fn editing_an_unset_date_starts_cleared_and_submitting_leaves_it_unset() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());
        let repo = TaskRepository::open(tmp.path()).unwrap();
        repo.create_task("No dates yet").unwrap();

        let mut app = App::new(repo, TuiConfig::default()).unwrap();
        app.selected = 0;

        app.begin_edit_due();
        assert_eq!(app.input_mode, InputMode::PickEditDue);
        assert!(!app.picker_has_value);

        app.submit_input().unwrap();
        let updated = app.all_tasks.iter().find(|t| t.title == "No dates yet").unwrap();
        assert_eq!(updated.due, None);
    }

    #[test]
    fn editing_an_already_set_date_still_starts_prefilled() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());
        let repo = TaskRepository::open(tmp.path()).unwrap();
        repo.create_task_from_draft(&TaskDraft {
            title: "Has a due date".into(),
            details: String::new(),
            due: Some("2026-05-01".into()),
            scheduled: None,
            priority: None,
            status: Some("open".into()),
            recurrence: None,
            recurrence_anchor: None,
            projects: vec![],
        })
        .unwrap();

        let mut app = App::new(repo, TuiConfig::default()).unwrap();
        app.selected = 0;

        app.begin_edit_due();
        assert!(app.picker_has_value);
        assert_eq!(app.picker_date, "2026-05-01");

        app.submit_input().unwrap();
        let updated = app
            .all_tasks
            .iter()
            .find(|t| t.title == "Has a due date")
            .unwrap();
        assert_eq!(updated.due.as_deref(), Some("2026-05-01"));
    }

    #[test]
    fn create_flow_due_and_scheduled_steps_start_cleared() {
        let tmp = tempdir().unwrap();
        write_collection(tmp.path());
        let repo = TaskRepository::open(tmp.path()).unwrap();

        let mut app = App::new(repo, TuiConfig::default()).unwrap();
        app.begin_create();
        app.input_value = "New task".into();
        app.submit_input().unwrap();
        app.input_value.clear();
        app.submit_input().unwrap();
        assert_eq!(app.input_mode, InputMode::PickCreateProject);
        app.submit_input().unwrap();
        assert_eq!(app.input_mode, InputMode::PickCreateDue);
        assert!(!app.picker_has_value);

        app.submit_input().unwrap();
        assert_eq!(app.input_mode, InputMode::PickCreateScheduled);
        assert!(!app.picker_has_value);
        assert_eq!(app.draft.due, None);
    }
}
