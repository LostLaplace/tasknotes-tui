use std::io;
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::Result;
use chrono::{Datelike, NaiveDate, Weekday};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap},
    DefaultTerminal, Frame,
};

use crate::app::App;
use crate::date::{get_date_part, today_local};
use crate::repository::is_archived_task;
use crate::tui_config::KeyCommand;

pub fn run(mut app: App) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = ratatui::init();
    let result = run_loop(&mut terminal, &mut app);
    ratatui::restore();
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    result
}

fn run_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, app))?;
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if handle_key(app, key)? {
                    break;
                }
                if app.take_open_in_editor_request() {
                    open_selected_in_editor(terminal, app)?;
                }
            }
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    if app.is_input_active() {
        if app.is_date_picker_active() {
            match key.code {
                KeyCode::Esc => app.cancel_input()?,
                KeyCode::Enter => app.submit_input()?,
                KeyCode::Left | KeyCode::Char('h') => app.move_picker_day(-1),
                KeyCode::Right | KeyCode::Char('l') => app.move_picker_day(1),
                KeyCode::Up | KeyCode::Char('k') => app.move_picker_day(-7),
                KeyCode::Down | KeyCode::Char('j') => app.move_picker_day(7),
                KeyCode::Char('H') => app.move_picker_month(-1),
                KeyCode::Char('L') => app.move_picker_month(1),
                KeyCode::Char('t') => app.set_picker_today(),
                KeyCode::Char('c') => app.clear_picker_value(),
                KeyCode::Char('/') => app.switch_picker_to_text(),
                _ => {}
            }
            return Ok(false);
        }
        match key.code {
            KeyCode::Esc => app.cancel_input()?,
            KeyCode::Enter => app.submit_input()?,
            KeyCode::Down if app.is_palette_active() => app.next_palette_item(),
            KeyCode::Up if app.is_palette_active() => app.previous_palette_item(),
            KeyCode::Backspace => app.backspace_input()?,
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.push_input_char(ch)?
            }
            _ => {}
        }
        return Ok(false);
    }

    if let Some(slot) = numeric_view_slot(key) {
        app.activate_view_slot(slot)?;
    } else if let Some(command) = app.tui_config.command_for_key(key) {
        match command {
            KeyCommand::Quit => return Ok(true),
            KeyCommand::FocusPrevDay => app.move_focus_date(-1)?,
            KeyCommand::FocusNextDay => app.move_focus_date(1)?,
            KeyCommand::FocusPrevWeek => app.move_focus_date(-7)?,
            KeyCommand::FocusNextWeek => app.move_focus_date(7)?,
            KeyCommand::FocusToday => app.reset_focus_date()?,
            KeyCommand::NextTask => app.next(),
            KeyCommand::PrevTask => app.previous(),
            KeyCommand::Refresh => app.refresh()?,
            KeyCommand::ToggleComplete => app.toggle_selected()?,
            KeyCommand::ToggleArchive => app.toggle_selected_archive()?,
            KeyCommand::ToggleSkipRecurring => app.skip_selected_today()?,
            KeyCommand::CreateTask => app.begin_create(),
            KeyCommand::QuickCreateTask => app.begin_quick_create(),
            KeyCommand::Search => app.begin_search(),
            KeyCommand::CommandPalette => app.begin_command_palette(),
            KeyCommand::EditTitle => app.begin_edit_title(),
            KeyCommand::OpenInEditor => app.request_open_in_editor(),
            KeyCommand::EditDue => app.begin_edit_due(),
            KeyCommand::EditScheduled => app.begin_edit_scheduled(),
            KeyCommand::ToggleTimeTracking => app.toggle_selected_time_tracking()?,
            KeyCommand::EditPriority => app.begin_edit_priority(),
            KeyCommand::EditStatus => app.begin_edit_status(),
            KeyCommand::EditRecurrence => app.begin_edit_recurrence(),
            KeyCommand::EditRecurrenceAnchor => app.begin_edit_recurrence_anchor(),
        }
    }
    Ok(false)
}

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let has_input = app.input_prompt().is_some();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if has_input {
            vec![
                Constraint::Length(10),
                Constraint::Min(10),
                Constraint::Length(4),
            ]
        } else {
            vec![Constraint::Length(10), Constraint::Min(10)]
        })
        .split(frame.area());

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
        .split(layout[1]);
    let support = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(25), Constraint::Min(10)])
        .split(layout[0]);
    let support_left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Length(6)])
        .split(support[1]);

    let title = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                "TaskNotes TUI",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(app.mode_label(), Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled(
                format!(
                    "View {}: {}",
                    app.current_view_slot,
                    app.current_view()
                        .map(|view| view.label.as_str())
                        .unwrap_or("Unknown")
                ),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw("  "),
            Span::styled(
                format!("Focus {}", app.focus_date),
                Style::default().fg(Color::Magenta),
            ),
            Span::raw("  "),
            Span::styled(
                format!("Selection {}", app.selected_position_label()),
                Style::default().fg(Color::Blue),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!(
                    "Search {}",
                    if app.search_query.is_empty() {
                        "off".to_string()
                    } else {
                        format!("\"{}\"", app.search_query)
                    }
                ),
                Style::default().fg(Color::Green),
            ),
            Span::raw("  "),
            Span::styled(app.status.as_str(), Style::default().fg(Color::DarkGray)),
        ]),
    ])
    .block(Block::default().title("State").borders(Borders::ALL));
    frame.render_widget(title, support_left[0]);

    let help = Paragraph::new(vec![
        Line::from(shortcut_line(app)),
        Line::from(secondary_footer_line(app)),
    ])
    .block(Block::default().title("Help").borders(Borders::ALL))
    .wrap(Wrap { trim: false });
    frame.render_widget(help, support_left[1]);

    frame.render_widget(
        draw_calendar(&app.focus_date, &app.calendar_tasks),
        support[0],
    );

    let items: Vec<ListItem> = app
        .tasks
        .iter()
        .enumerate()
        .map(|(index, task)| {
            let is_archived = is_archived_task(task, &app.repo.config.archive);
            let is_selected = index == app.selected;
            let title_style = if is_archived {
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM)
            } else {
                Style::default()
            };
            let mut primary = vec![Span::styled(task.title.clone(), title_style)];
            let inline_meta = compact_task_meta(task, is_archived);
            if !inline_meta.is_empty() {
                primary.push(Span::raw("  "));
                primary.extend(inline_meta);
            }

            let mut lines = vec![Line::from(primary)];
            if is_selected {
                let secondary = selected_task_meta(task, is_archived);
                if !secondary.is_empty() {
                    lines.push(Line::from(secondary));
                }
            }
            ListItem::new(lines)
        })
        .collect();
    let list = List::new(items)
        .block(
            Block::default()
                .title(format!("Tasks ({})", app.tasks.len()))
                .borders(Borders::ALL),
        )
        .scroll_padding(2)
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(28, 48, 74))
                .add_modifier(Modifier::BOLD),
        );
    let mut state = ListState::default();
    if !app.tasks.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, body[0], &mut state);

    let details = if let Some(task) = app.selected_task() {
        let recurring = task
            .normalized_frontmatter
            .get("recurrence")
            .and_then(|value| value.as_str());
        let recurrence_anchor = task
            .normalized_frontmatter
            .get("recurrenceAnchor")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let complete_instances = task
            .normalized_frontmatter
            .get("completeInstances")
            .and_then(|value| value.as_array())
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str())
                    .take(5)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let skipped_instances = task
            .normalized_frontmatter
            .get("skippedInstances")
            .and_then(|value| value.as_array())
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str())
                    .take(5)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let archived = if is_archived_task(task, &app.repo.config.archive) {
            "yes"
        } else {
            "no"
        };
        let mut lines = vec![
            Line::from(Span::styled(
                task.title.as_str(),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            detail_line("Path", &task.path),
            detail_line("Status", &task.status),
            detail_line("Archived", archived),
        ];
        if let Some(priority) = task.priority.as_deref().filter(|value| !value.is_empty()) {
            lines.push(detail_line("Priority", priority));
        }
        if let Some(scheduled) = task.scheduled.as_deref().filter(|value| !value.is_empty()) {
            lines.push(detail_line("Scheduled", &get_date_part(scheduled)));
        }
        if let Some(due) = task.due.as_deref().filter(|value| !value.is_empty()) {
            lines.push(detail_line("Due", &get_date_part(due)));
        }
        lines.push(detail_line(
            "Tracking",
            if task.has_active_time_entry {
                "active"
            } else {
                "inactive"
            },
        ));
        if let Some(value) = recurring.filter(|value| !value.is_empty()) {
            lines.push(detail_line("Recurrence", value));
        }
        if !recurrence_anchor.is_empty() {
            lines.push(detail_line("Recurrence anchor", recurrence_anchor));
        }
        if !complete_instances.is_empty() {
            lines.push(detail_line("Completed", &complete_instances));
        }
        if !skipped_instances.is_empty() {
            lines.push(detail_line("Skipped", &skipped_instances));
        }

        let body_preview = if task.body.trim().is_empty() {
            vec![Line::from(Span::styled(
                "No notes",
                Style::default().fg(Color::DarkGray),
            ))]
        } else {
            task.body
                .lines()
                .map(|line| Line::from(line.to_string()))
                .collect::<Vec<_>>()
        };
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Notes",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        lines.extend(body_preview);

        Paragraph::new(lines)
    } else {
        Paragraph::new(vec![
            Line::from(Span::styled(
                "No tasks in this view",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("Try another view, clear search, or create a task."),
        ])
    }
    .block(Block::default().title("Details").borders(Borders::ALL))
    .wrap(Wrap { trim: false });
    frame.render_widget(details, body[1]);

    if let Some(prompt) = app.input_prompt() {
        let title = if let Some((step, total, label)) = app.create_progress() {
            format!("{prompt}  [{step}/{total}: {label}]")
        } else if matches!(app.input_mode, crate::app::InputMode::ConfirmDelete) {
            format!("{prompt}  [Enter confirm | Esc cancel]")
        } else {
            format!("{prompt}  [Enter submit | Esc cancel]")
        };
        let mut lines = if matches!(app.input_mode, crate::app::InputMode::ConfirmDelete) {
            let task_title = app
                .selected_task()
                .map(|task| task.title.clone())
                .unwrap_or_else(|| "selected task".to_string());
            vec![
                Line::from(format!("Delete {task_title}?")),
                Line::from(Span::styled(
                    "This removes the task file from the vault.",
                    Style::default().fg(Color::DarkGray),
                )),
            ]
        } else {
            vec![Line::from(app.input_value.clone())]
        };
        if matches!(app.input_mode, crate::app::InputMode::CreateDetails) {
            lines.push(Line::from(Span::styled(
                "Task body or notes. Leave blank to skip.",
                Style::default().fg(Color::DarkGray),
            )));
        }
        let input =
            Paragraph::new(lines).block(Block::default().title(title).borders(Borders::ALL));
        frame.render_widget(input, layout[2]);
    }

    if app.is_palette_active() {
        draw_command_palette(frame, app);
    }
    if app.is_date_picker_active() {
        draw_date_picker(frame, app);
    }
}

fn draw_calendar(
    focus_date: &str,
    calendar_tasks: &[crate::repository::TaskRecord],
) -> Paragraph<'static> {
    let focus = NaiveDate::parse_from_str(focus_date, "%Y-%m-%d")
        .ok()
        .unwrap_or_else(|| {
            NaiveDate::parse_from_str(&today_local(), "%Y-%m-%d").expect("valid local day")
        });
    let first = focus.with_day(1).unwrap_or(focus);
    let month_name = focus.format("%B %Y").to_string();
    let start_offset = weekday_index(first.weekday()) as usize;
    let next_month = if focus.month() == 12 {
        NaiveDate::from_ymd_opt(focus.year() + 1, 1, 1).expect("valid next month")
    } else {
        NaiveDate::from_ymd_opt(focus.year(), focus.month() + 1, 1).expect("valid next month")
    };
    let days_in_month = next_month.pred_opt().map(|date| date.day()).unwrap_or(30);

    let mut lines = vec![Line::from(month_name), Line::from("Mo Tu We Th Fr Sa Su")];

    let mut day = 1u32;
    for week in 0..6 {
        let mut spans = Vec::new();
        for weekday in 0..7 {
            let cell = week * 7 + weekday;
            if cell < start_offset || day > days_in_month {
                spans.push(Span::raw("   "));
                continue;
            }
            let date =
                NaiveDate::from_ymd_opt(focus.year(), focus.month(), day).expect("valid date");
            let ymd = date.format("%Y-%m-%d").to_string();
            let has_tasks = calendar_tasks.iter().any(|task| {
                task.scheduled
                    .as_deref()
                    .map(get_date_part)
                    .or_else(|| task.due.as_deref().map(get_date_part))
                    .map(|value| value == ymd)
                    .unwrap_or(false)
            });
            let mut label = format!("{day:>2}");
            if has_tasks {
                label.push('*');
            } else {
                label.push(' ');
            }
            let style = if ymd == focus_date {
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else if ymd == today_local() {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if has_tasks {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };
            spans.push(Span::styled(label, style));
            day += 1;
        }
        lines.push(Line::from(spans));
        if day > days_in_month {
            break;
        }
    }

    Paragraph::new(lines)
        .block(
            Block::default()
                .title("Calendar")
                .borders(Borders::ALL)
                .padding(Padding::left(1)),
        )
        .wrap(Wrap { trim: false })
}

fn weekday_index(weekday: Weekday) -> u32 {
    match weekday {
        Weekday::Mon => 0,
        Weekday::Tue => 1,
        Weekday::Wed => 2,
        Weekday::Thu => 3,
        Weekday::Fri => 4,
        Weekday::Sat => 5,
        Weekday::Sun => 6,
    }
}

fn draw_command_palette(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(70, 60, frame.area());
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(area);

    let items = app.filtered_palette_items();
    let list_items: Vec<ListItem> = items
        .iter()
        .map(|item| {
            let mut spans = vec![Span::styled(
                item.title.as_str(),
                Style::default().add_modifier(Modifier::BOLD),
            )];
            if let Some(hotkey) = item.hotkey.as_deref() {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    format!("[{hotkey}]"),
                    Style::default().fg(Color::Yellow),
                ));
            }
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                item.description.as_str(),
                Style::default().fg(Color::DarkGray),
            ));
            ListItem::new(Line::from(spans))
        })
        .collect();

    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(app.palette_selected.min(items.len() - 1)));
    }

    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .title(format!("Command Palette ({})", items.len()))
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Black)),
        area,
    );
    frame.render_widget(
        Paragraph::new(app.input_value.clone())
            .alignment(Alignment::Left)
            .block(
                Block::default()
                    .title("Query (type to filter commands)")
                    .borders(Borders::ALL),
            ),
        layout[0],
    );
    frame.render_stateful_widget(
        List::new(list_items)
            .block(Block::default().title("Commands").borders(Borders::ALL))
            .highlight_style(
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        layout[1],
        &mut state,
    );
    frame.render_widget(
        Paragraph::new(
            format!(
                "Enter run  Esc cancel  Up/Down move\nBrackets show direct hotkeys. Views: {}. Delete stays palette-only.",
                palette_view_help(app)
            ),
        )
        .wrap(Wrap { trim: false })
        .block(Block::default().title("Help").borders(Borders::ALL)),
        layout[2],
    );
}

fn palette_view_help(app: &App) -> String {
    let labels: Vec<String> = app
        .available_view_slots()
        .into_iter()
        .filter_map(|slot| {
            app.tui_config
                .views
                .get(&slot)
                .map(|view| format!("{slot}:{}", view.label))
        })
        .collect();
    if labels.is_empty() {
        "none configured".to_string()
    } else {
        labels.join("  ")
    }
}

fn open_selected_in_editor(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    let Some(path) = app.selected_task_absolute_path() else {
        app.status = "No task selected".to_string();
        return Ok(());
    };

    let editor = std::env::var("VISUAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("EDITOR")
                .ok()
                .filter(|value| !value.trim().is_empty())
        });
    let Some(editor) = editor else {
        app.status = "Set $EDITOR or $VISUAL to open tasks".to_string();
        return Ok(());
    };

    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or_default();
    if program.is_empty() {
        app.status = "Invalid $EDITOR".to_string();
        return Ok(());
    }

    ratatui::restore();
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;

    let status = Command::new(program)
        .args(parts)
        .arg(&path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    terminal.clear()?;
    terminal.autoresize()?;

    match status {
        Ok(exit) => {
            app.refresh_selected_task()?;
            if exit.success() {
                app.status = format!("Opened {}", path.display());
            } else {
                app.status = format!("Editor exited with status {}", exit);
            }
        }
        Err(error) => {
            app.status = format!("Failed to launch editor: {}", error);
        }
    }

    Ok(())
}

fn draw_date_picker(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(52, 56, frame.area());
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(10),
            Constraint::Length(3),
        ])
        .split(area);

    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .title(app.input_prompt().unwrap_or("Date Picker"))
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Black)),
        area,
    );

    let current = if app.picker_has_value {
        app.picker_date.as_str()
    } else {
        "(clear)"
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                current,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "Enter saves the highlighted value",
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .block(Block::default().title("Selected").borders(Borders::ALL))
        .alignment(Alignment::Center),
        layout[0],
    );

    frame.render_widget(
        draw_calendar(&app.picker_date, &app.calendar_tasks),
        layout[1],
    );

    frame.render_widget(
        Paragraph::new(
            "Move arrows or hjkl  H/L month  t today\nc clear  / type ISO date  Enter save  Esc cancel",
        )
        .block(Block::default().title("Help").borders(Borders::ALL))
        .wrap(Wrap { trim: false }),
        layout[2],
    );
}

fn detail_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(value.to_string()),
    ])
}

fn shortcut_line(app: &App) -> Line<'static> {
    let spans: Vec<Span<'static>> = app
        .contextual_shortcuts()
        .into_iter()
        .enumerate()
        .flat_map(|(index, (label, value))| {
            let mut parts = Vec::new();
            if index > 0 {
                parts.push(Span::raw("  "));
            }
            parts.push(Span::styled(
                format!("{label}: "),
                Style::default().fg(Color::DarkGray),
            ));
            parts.push(Span::styled(value, Style::default().fg(Color::Cyan)));
            parts
        })
        .collect();
    Line::from(spans)
}

fn secondary_footer_line(app: &App) -> Line<'static> {
    if let Some((step, total, label)) = app.create_progress() {
        Line::from(vec![
            Span::styled("Create flow: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("step {step}/{total} on {label}"),
                Style::default().fg(Color::Yellow),
            ),
        ])
    } else if app.search_query.is_empty() {
        Line::from(vec![
            Span::styled("Tip: ", Style::default().fg(Color::DarkGray)),
            Span::raw("use "),
            Span::styled("Ctrl-P", Style::default().fg(Color::Yellow)),
            Span::raw(" for commands or "),
            Span::styled("/", Style::default().fg(Color::Yellow)),
            Span::raw(" to search."),
        ])
    } else {
        Line::from(vec![
            Span::styled("Search active: ", Style::default().fg(Color::DarkGray)),
            Span::raw("results are filtered live across title, notes, path, and priority."),
        ])
    }
}

fn compact_task_meta(
    task: &crate::repository::TaskRecord,
    is_archived: bool,
) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(
        task.status.clone(),
        compact_status_style(task.status.as_str(), is_archived),
    )];
    if let Some(date_span) = task
        .due
        .as_deref()
        .map(|due| {
            Span::styled(
                format!("due {}", get_date_part(due)),
                Style::default().fg(Color::LightRed),
            )
        })
        .or_else(|| {
            task.scheduled.as_deref().map(|scheduled| {
                Span::styled(
                    get_date_part(scheduled),
                    Style::default().fg(Color::Magenta),
                )
            })
        })
    {
        spans.push(Span::raw("  "));
        spans.push(date_span);
    }
    if task.has_active_time_entry {
        spans.push(Span::raw("  "));
        spans.push(Span::styled("*", Style::default().fg(Color::Green)));
    }
    if task
        .normalized_frontmatter
        .get("recurrence")
        .and_then(|value| value.as_str())
        .is_some()
    {
        spans.push(Span::raw("  "));
        spans.push(Span::styled("R", Style::default().fg(Color::Yellow)));
    }
    spans
}

fn selected_task_meta(
    task: &crate::repository::TaskRecord,
    is_archived: bool,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut push_value = |label: &str, value: String, style: Style| {
        if !spans.is_empty() {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            format!("{label}:"),
            Style::default().fg(Color::DarkGray),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(value, style));
    };

    if let Some(priority) = task.priority.as_deref().filter(|value| !value.is_empty()) {
        push_value(
            "priority",
            priority.to_string(),
            Style::default().fg(Color::Yellow),
        );
    }
    if let Some(scheduled) = task.scheduled.as_deref().filter(|value| !value.is_empty()) {
        push_value(
            "scheduled",
            get_date_part(scheduled),
            Style::default().fg(Color::Magenta),
        );
    }
    if let Some(due) = task.due.as_deref().filter(|value| !value.is_empty()) {
        push_value(
            "due",
            get_date_part(due),
            Style::default().fg(Color::LightRed),
        );
    }
    if is_archived {
        push_value(
            "archive",
            "yes".to_string(),
            Style::default().fg(Color::DarkGray),
        );
    }
    if task.has_active_time_entry {
        push_value(
            "tracking",
            "active".to_string(),
            Style::default().fg(Color::Green),
        );
    }
    if let Some(recurrence) = task
        .normalized_frontmatter
        .get("recurrence")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
    {
        push_value(
            "recurs",
            recurrence.to_string(),
            Style::default().fg(Color::Yellow),
        );
    }
    spans
}

fn compact_status_style(status: &str, is_archived: bool) -> Style {
    if is_archived {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM)
    } else if status == "done" || status == "cancelled" {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Cyan)
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn numeric_view_slot(key: KeyEvent) -> Option<u8> {
    match key.code {
        KeyCode::Char(ch) if ('1'..='9').contains(&ch) => Some(ch as u8 - b'0'),
        _ => None,
    }
}
