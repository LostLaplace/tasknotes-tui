use anyhow::Result;

use crate::date::{
    apply_day_offset, apply_month_offset, get_date_part, is_before_date_safe, today_local,
};
use crate::field_mapping::is_completed_status;
use crate::repository::{TaskDraft, TaskFilter, TaskRecord, TaskRepository};

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
    pub all_tasks: Vec<TaskRecord>,
    pub tasks: Vec<TaskRecord>,
    pub selected: usize,
    pub filter: TaskFilter,
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
    ToggleTimeTracking,
    ToggleRecurringSkip,
    FilterOpen,
    FilterToday,
    FilterOverdue,
    FilterAll,
    FilterTracked,
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
    pub title: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
}

const PALETTE_ITEMS: &[PaletteItem] = &[
    PaletteItem {
        command: PaletteCommand::CreateTask,
        title: "Create task",
        aliases: &["new", "n", "add"],
        description: "Start the multi-step task creation flow",
    },
    PaletteItem {
        command: PaletteCommand::Search,
        title: "Search tasks",
        aliases: &["find", "/"],
        description: "Open live task search",
    },
    PaletteItem {
        command: PaletteCommand::Refresh,
        title: "Refresh list",
        aliases: &["reload", "r"],
        description: "Reload tasks from disk",
    },
    PaletteItem {
        command: PaletteCommand::ToggleComplete,
        title: "Toggle completion",
        aliases: &["complete", "done", "x"],
        description: "Complete or reopen the selected task",
    },
    PaletteItem {
        command: PaletteCommand::ToggleTimeTracking,
        title: "Toggle time tracking",
        aliases: &["track", "timer", "T"],
        description: "Start or stop time tracking on the selected task",
    },
    PaletteItem {
        command: PaletteCommand::ToggleRecurringSkip,
        title: "Toggle recurring skip today",
        aliases: &["skip", "recurring", "S"],
        description: "Skip or unskip today's recurring instance",
    },
    PaletteItem {
        command: PaletteCommand::FilterOpen,
        title: "Filter: Open",
        aliases: &["open", "1"],
        description: "Show open tasks",
    },
    PaletteItem {
        command: PaletteCommand::FilterToday,
        title: "Filter: Today",
        aliases: &["today", "2"],
        description: "Show tasks due or scheduled today",
    },
    PaletteItem {
        command: PaletteCommand::FilterOverdue,
        title: "Filter: Overdue",
        aliases: &["overdue", "3"],
        description: "Show overdue tasks",
    },
    PaletteItem {
        command: PaletteCommand::FilterAll,
        title: "Filter: All",
        aliases: &["all", "4"],
        description: "Show all tasks",
    },
    PaletteItem {
        command: PaletteCommand::FilterTracked,
        title: "Filter: Tracked",
        aliases: &["tracked", "active", "5"],
        description: "Show tasks with an active time entry",
    },
    PaletteItem {
        command: PaletteCommand::EditTitle,
        title: "Edit title",
        aliases: &["rename", "e"],
        description: "Edit the selected task title",
    },
    PaletteItem {
        command: PaletteCommand::OpenInEditor,
        title: "Open in editor",
        aliases: &["edit", "body", "notes", "i"],
        description: "Open the selected task in $EDITOR",
    },
    PaletteItem {
        command: PaletteCommand::EditDue,
        title: "Edit due date",
        aliases: &["due", "d"],
        description: "Edit the selected task due date",
    },
    PaletteItem {
        command: PaletteCommand::EditScheduled,
        title: "Edit scheduled date",
        aliases: &["scheduled", "s"],
        description: "Edit the selected task scheduled date",
    },
    PaletteItem {
        command: PaletteCommand::EditPriority,
        title: "Edit priority",
        aliases: &["priority", "p"],
        description: "Edit the selected task priority",
    },
    PaletteItem {
        command: PaletteCommand::EditStatus,
        title: "Edit status",
        aliases: &["status", "t"],
        description: "Edit the selected task status",
    },
    PaletteItem {
        command: PaletteCommand::EditRecurrence,
        title: "Edit recurrence rule",
        aliases: &["recurrence", "rrule", "R"],
        description: "Edit the selected task recurrence rule",
    },
    PaletteItem {
        command: PaletteCommand::EditRecurrenceAnchor,
        title: "Edit recurrence anchor",
        aliases: &["anchor", "A"],
        description: "Edit the selected task recurrence anchor",
    },
];

impl App {
    pub fn new(repo: TaskRepository) -> Result<Self> {
        let mut app = Self {
            repo,
            all_tasks: Vec::new(),
            tasks: Vec::new(),
            selected: 0,
            filter: TaskFilter::Open,
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
        self.status = if self.search_query.trim().is_empty() {
            if matches!(self.filter, TaskFilter::Today | TaskFilter::Overdue) {
                format!("{} tasks for {}", self.tasks.len(), self.focus_date)
            } else {
                format!("{} tasks", self.tasks.len())
            }
        } else {
            if matches!(self.filter, TaskFilter::Today | TaskFilter::Overdue) {
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

    pub fn set_filter(&mut self, filter: TaskFilter) -> Result<()> {
        self.filter = filter;
        if filter == TaskFilter::Today {
            self.focus_date = today_local();
        }
        self.refresh()
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
            self.repo.toggle_complete(&task)?;
            self.reload_from_disk()?;
            self.status = format!("Updated {}", task.title);
        }
        Ok(())
    }

    pub fn toggle_selected_time_tracking(&mut self) -> Result<()> {
        if let Some(task) = self.selected_task().cloned() {
            self.repo.toggle_time_tracking(&task)?;
            self.reload_from_disk()?;
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
            self.repo.toggle_skip_today(&task)?;
            self.reload_from_disk()?;
            self.status = format!("Updated recurring state for {}", task.title);
        }
        Ok(())
    }

    pub fn begin_search(&mut self) {
        self.input_mode = InputMode::Search;
        self.input_value = self.search_query.clone();
        self.status = "Search tasks".to_string();
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
            self.status = "Edit scheduled date: arrows move, H/L month, t today, c clear, / type"
                .to_string();
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
                    self.repo.update_date_field(&task, "due", value.as_deref())?;
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.reload_from_disk()?;
                    self.status = format!("Updated due date for {}", task.title);
                }
            }
            InputMode::PickEditScheduled => {
                if let Some(task) = self.selected_task().cloned() {
                    let value = self.current_picker_value();
                    self.repo
                        .update_date_field(&task, "scheduled", value.as_deref())?;
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.reload_from_disk()?;
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
                    self.repo.update_date_field(&task, "due", value.as_deref())?;
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.reload_from_disk()?;
                    self.status = format!("Updated due date for {}", task.title);
                }
            }
            InputMode::TextEditScheduled => {
                if let Some(task) = self.selected_task().cloned() {
                    let value = option_from_input(&self.input_value);
                    self.repo
                        .update_date_field(&task, "scheduled", value.as_deref())?;
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.reload_from_disk()?;
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
                    self.repo.create_task_from_draft(&self.draft)?;
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.reload_from_disk()?;
                    self.status = format!("Created {}", title);
                }
            }
            InputMode::CreateRecurrenceAnchor => {
                self.draft.recurrence_anchor = option_from_input(&self.input_value);
                let title = self.draft.title.clone();
                self.repo.create_task_from_draft(&self.draft)?;
                self.input_mode = InputMode::None;
                self.input_value.clear();
                self.reload_from_disk()?;
                self.status = format!("Created {}", title);
            }
            InputMode::EditTitle => {
                if let Some(task) = self.selected_task().cloned() {
                    let title = self.input_value.trim().to_string();
                    self.repo.update_title(&task, &title)?;
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.reload_from_disk()?;
                    self.status = format!("Renamed {}", title);
                }
            }
            InputMode::EditPriority => {
                if let Some(task) = self.selected_task().cloned() {
                    let value = option_from_input(&self.input_value);
                    self.repo
                        .update_scalar_field(&task, "priority", value.as_deref())?;
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.reload_from_disk()?;
                    self.status = format!("Updated priority for {}", task.title);
                }
            }
            InputMode::EditStatus => {
                if let Some(task) = self.selected_task().cloned() {
                    let value = option_from_input(&self.input_value);
                    self.repo
                        .update_scalar_field(&task, "status", value.as_deref())?;
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.reload_from_disk()?;
                    self.status = format!("Updated status for {}", task.title);
                }
            }
            InputMode::EditRecurrence => {
                if let Some(task) = self.selected_task().cloned() {
                    let value = option_from_input(&self.input_value);
                    self.repo
                        .update_scalar_field(&task, "recurrence", value.as_deref())?;
                    if value.is_none() {
                        self.repo.update_scalar_field(&task, "recurrenceAnchor", None)?;
                    }
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.reload_from_disk()?;
                    self.status = format!("Updated recurrence for {}", task.title);
                }
            }
            InputMode::EditRecurrenceAnchor => {
                if let Some(task) = self.selected_task().cloned() {
                    let value = option_from_input(&self.input_value);
                    self.repo
                        .update_scalar_field(&task, "recurrenceAnchor", value.as_deref())?;
                    self.input_mode = InputMode::None;
                    self.input_value.clear();
                    self.reload_from_disk()?;
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

    pub fn filtered_palette_items(&self) -> Vec<&'static PaletteItem> {
        let query = self.input_value.trim().to_ascii_lowercase();
        let mut items: Vec<&PaletteItem> = PALETTE_ITEMS
            .iter()
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
            PaletteCommand::ToggleTimeTracking => self.toggle_selected_time_tracking()?,
            PaletteCommand::ToggleRecurringSkip => self.skip_selected_today()?,
            PaletteCommand::FilterOpen => self.set_filter(TaskFilter::Open)?,
            PaletteCommand::FilterToday => self.set_filter(TaskFilter::Today)?,
            PaletteCommand::FilterOverdue => self.set_filter(TaskFilter::Overdue)?,
            PaletteCommand::FilterAll => self.set_filter(TaskFilter::All)?,
            PaletteCommand::FilterTracked => self.set_filter(TaskFilter::Tracked)?,
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
        match self.filter {
            TaskFilter::All => true,
            TaskFilter::Open => !is_completed_status(&self.repo.field_mapping, Some(&task.status)),
            TaskFilter::Today => task
                .scheduled
                .as_deref()
                .map(get_date_part)
                .or_else(|| task.due.as_deref().map(get_date_part))
                .map(|value| value == self.focus_date)
                .unwrap_or(false),
            TaskFilter::Overdue => task
                .due
                .as_deref()
                .map(|due| {
                    is_before_date_safe(due, &self.focus_date)
                        && !is_completed_status(&self.repo.field_mapping, Some(&task.status))
                })
                .unwrap_or(false),
            TaskFilter::Tracked => task.has_active_time_entry,
        }
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
        self.picker_has_value = initial.is_some();
    }

    fn current_picker_value(&self) -> Option<String> {
        if self.picker_has_value {
            Some(self.picker_date.clone())
        } else {
            None
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

fn palette_rank(item: &PaletteItem, query: &str) -> (usize, usize, &'static str) {
    if query.is_empty() {
        return (2, 0, item.title);
    }
    let title = item.title.to_ascii_lowercase();
    if title.starts_with(query) {
        return (0, title.len(), item.title);
    }
    if item
        .aliases
        .iter()
        .any(|alias| alias.to_ascii_lowercase().starts_with(query))
    {
        return (1, title.len(), item.title);
    }
    (2, title.len(), item.title)
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
