use anyhow::Result;
use std::collections::BTreeMap;

use crate::date::{apply_day_offset, apply_month_offset, get_date_part, today_local};
use crate::repository::{TaskDraft, TaskFilter, TaskRecord, TaskRepository};
use crate::tui_config::{TuiConfig, ViewConfig, ViewFilter};
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
    CreateTitle,
    CreateDetails,
    CreatePriority,
    CreateStatus,
    CreateRecurrence,
    CreateRecurrenceAnchor,
    EditTitle,
    EditPriority,
    EditStatus,
    EditRecurrence,
    EditRecurrenceAnchor,
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
    pub pending_open_editor: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCommand {
    CreateTask,
    Search,
    Refresh,
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
            pending_open_editor: false,
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
        self.tasks = self
            .all_tasks
            .iter()
            .filter(|task| self.matches_filter(task))
            .filter(|task| self.matches_search(task))
            .cloned()
            .collect();
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

    pub fn next(&mut self) {
        if self.tasks.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.tasks.len() - 1);
    }

    pub fn previous(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn activate_view_slot(&mut self, slot: u8) -> Result<()> {
        if self.tui_config.views.contains_key(&slot) {
            self.current_view_slot = slot;
            if matches!(
                self.current_view().map(|view| &view.filter),
                Some(ViewFilter::Date)
            ) {
                self.focus_date = today_local();
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

    fn replace_cached_task(&mut self, old_path: &str, updated: TaskRecord) {
        let updated_path = updated.path.clone();
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
            self.input_mode = InputMode::EditPriority;
            self.input_value = task.priority.unwrap_or_default();
            self.status = "Edit priority (blank clears)".to_string();
        }
    }

    pub fn begin_edit_status(&mut self) {
        if let Some(task) = self.selected_task().cloned() {
            self.input_mode = InputMode::EditStatus;
            self.input_value = task.status;
            self.status = "Edit status".to_string();
        }
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
                self.input_mode = InputMode::CreatePriority;
                self.input_value = self.repo.config.defaults.priority.clone();
                self.status = "New task: priority".to_string();
            }
            InputMode::PickEditDue => {
                if let Some(task) = self.selected_task().cloned() {
                    let value = self.current_picker_value();
                    let updated = self.repo
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
                    let updated = self.repo
                        .update_date_field(&task, "scheduled", value.as_deref())?;
                    self.replace_cached_task(&task.path, updated);
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.status = format!("Updated scheduled date for {}", task.title);
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
                self.input_mode = InputMode::CreatePriority;
                self.input_value = self.repo.config.defaults.priority.clone();
                self.status = "New task: priority".to_string();
            }
            InputMode::TextEditDue => {
                if let Some(task) = self.selected_task().cloned() {
                    let value = option_from_input(&self.input_value);
                    let updated = self.repo
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
                    let updated = self.repo
                        .update_date_field(&task, "scheduled", value.as_deref())?;
                    self.replace_cached_task(&task.path, updated);
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.status = format!("Updated scheduled date for {}", task.title);
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
                let initial = self.draft.due.clone();
                self.begin_date_picker(InputMode::PickCreateDue, initial.as_deref());
                self.status =
                    "New task: due date, arrows move, H/L month, t today, c clear, / type"
                        .to_string();
            }
            InputMode::CreatePriority => {
                self.draft.priority = option_from_input(&self.input_value);
                self.input_mode = InputMode::CreateStatus;
                self.input_value = self.repo.config.defaults.status.clone();
                self.status = "New task: status".to_string();
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
                let title = self.draft.title.clone();
                let created = self.repo.create_task_from_draft(&self.draft)?;
                self.input_mode = InputMode::None;
                self.input_value.clear();
                self.append_cached_task(created);
                self.status = format!("Created {}", title);
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
                    let updated = self.repo
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
                    let updated = self.repo
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
                    let mut updated = self.repo
                        .update_scalar_field(&task, "recurrence", value.as_deref())?;
                    if value.is_none() {
                        updated = self.repo
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
                    let updated = self.repo
                        .update_scalar_field(&task, "recurrenceAnchor", value.as_deref())?;
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
            InputMode::CreateTitle => "New title",
            InputMode::CreateDetails => "New details",
            InputMode::CreatePriority => "New priority",
            InputMode::CreateStatus => "New status",
            InputMode::CreateRecurrence => "New recurrence",
            InputMode::CreateRecurrenceAnchor => "New recurrence anchor",
            InputMode::EditTitle => "Edit title",
            InputMode::EditPriority => "Edit priority",
            InputMode::EditStatus => "Edit status",
            InputMode::EditRecurrence => "Edit recurrence",
            InputMode::EditRecurrenceAnchor => "Edit recurrence anchor",
        })
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
            PaletteCommand::Search => self.begin_search(),
            PaletteCommand::Refresh => {
                self.reload_from_disk()?;
                self.status = "Refreshed task list".to_string();
            }
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
        self.picker_date = initial.map(get_date_part).unwrap_or_else(today_local);
        self.picker_has_value = true;
    }

    fn current_picker_value(&self) -> Option<String> {
        if self.picker_has_value {
            Some(self.picker_date.clone())
        } else {
            None
        }
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
        let keys = &self.tui_config.keybinds;
        let value = match command {
            PaletteCommand::CreateTask => &keys.create_task,
            PaletteCommand::Search => &keys.search,
            PaletteCommand::Refresh => &keys.refresh,
            PaletteCommand::ToggleComplete => &keys.toggle_complete,
            PaletteCommand::ToggleArchive => &keys.toggle_archive,
            PaletteCommand::ToggleTimeTracking => &keys.toggle_time_tracking,
            PaletteCommand::ToggleRecurringSkip => &keys.toggle_skip_recurring,
            PaletteCommand::ViewSlot(_) => return None,
            PaletteCommand::EditTitle => &keys.edit_title,
            PaletteCommand::OpenInEditor => &keys.open_in_editor,
            PaletteCommand::EditDue => &keys.edit_due,
            PaletteCommand::EditScheduled => &keys.edit_scheduled,
            PaletteCommand::EditPriority => &keys.edit_priority,
            PaletteCommand::EditStatus => &keys.edit_status,
            PaletteCommand::EditRecurrence => &keys.edit_recurrence,
            PaletteCommand::EditRecurrenceAnchor => &keys.edit_recurrence_anchor,
        };
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
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
