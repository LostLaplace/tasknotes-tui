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
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    DefaultTerminal, Frame,
};

use crate::app::App;
use crate::date::{get_date_part, today_local};
use crate::repository::is_archived_task;

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

    let keys = &app.tui_config.keybinds;
    if binding_matches(key, &keys.quit) {
        return Ok(true);
    } else if binding_matches(key, &keys.focus_prev_day) {
        app.move_focus_date(-1)?;
    } else if binding_matches(key, &keys.focus_next_day) {
        app.move_focus_date(1)?;
    } else if binding_matches(key, &keys.focus_prev_week) {
        app.move_focus_date(-7)?;
    } else if binding_matches(key, &keys.focus_next_week) {
        app.move_focus_date(7)?;
    } else if binding_matches(key, &keys.focus_today) {
        app.reset_focus_date()?;
    } else if binding_matches(key, &keys.next_task) {
        app.next();
    } else if binding_matches(key, &keys.prev_task) {
        app.previous();
    } else if binding_matches(key, &keys.refresh) {
        app.refresh()?;
    } else if binding_matches(key, &keys.toggle_complete) {
        app.toggle_selected()?;
    } else if binding_matches(key, &keys.toggle_archive) {
        app.toggle_selected_archive()?;
    } else if binding_matches(key, &keys.toggle_skip_recurring) {
        app.skip_selected_today()?;
    } else if let Some(slot) = numeric_view_slot(key) {
        app.activate_view_slot(slot)?;
    } else if binding_matches(key, &keys.create_task) {
        app.begin_create();
    } else if binding_matches(key, &keys.search) {
        app.begin_search();
    } else if binding_matches(key, &keys.command_palette) {
        app.begin_command_palette();
    } else if binding_matches(key, &keys.edit_title) {
        app.begin_edit_title();
    } else if binding_matches(key, &keys.open_in_editor) {
        app.request_open_in_editor();
    } else if binding_matches(key, &keys.edit_due) {
        app.begin_edit_due();
    } else if binding_matches(key, &keys.edit_scheduled) {
        app.begin_edit_scheduled();
    } else if binding_matches(key, &keys.toggle_time_tracking) {
        app.toggle_selected_time_tracking()?;
    } else if binding_matches(key, &keys.edit_priority) {
        app.begin_edit_priority();
    } else if binding_matches(key, &keys.edit_status) {
        app.begin_edit_status();
    } else if binding_matches(key, &keys.edit_recurrence) {
        app.begin_edit_recurrence();
    } else if binding_matches(key, &keys.edit_recurrence_anchor) {
        app.begin_edit_recurrence_anchor();
    }
    Ok(false)
}

fn draw(frame: &mut Frame<'_>, app: &App) {
    let has_input = app.input_prompt().is_some();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if has_input {
            vec![
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
                Constraint::Length(2),
            ]
        } else {
            vec![
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(2),
            ]
        })
        .split(frame.area());

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(layout[1]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(10), Constraint::Min(10)])
        .split(body[1]);

    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            "TaskNotes TUI",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
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
            format!("Date: {}", app.focus_date),
            Style::default().fg(Color::Magenta),
        ),
        Span::raw("  "),
        Span::styled(
            format!(
                "Search: {}",
                if app.search_query.is_empty() {
                    "off"
                } else {
                    app.search_query.as_str()
                }
            ),
            Style::default().fg(Color::Green),
        ),
    ]))
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(title, layout[0]);

    let items: Vec<ListItem> = app
        .tasks
        .iter()
        .map(|task| {
            let is_archived = is_archived_task(task, &app.repo.config.archive);
            let status_style = if is_archived {
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM)
            } else if task.status == "done" || task.status == "cancelled" {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Cyan)
            };
            let mut line = vec![Span::styled(format!("{:<12}", task.status), status_style)];
            if is_archived {
                line.push(Span::styled(" A ", Style::default().fg(Color::DarkGray)));
            }
            if task
                .normalized_frontmatter
                .get("recurrence")
                .and_then(|value| value.as_str())
                .is_some()
            {
                line.push(Span::styled(" R ", Style::default().fg(Color::Yellow)));
            }
            if task.has_active_time_entry {
                line.push(Span::styled(" * ", Style::default().fg(Color::Green)));
            }
            if let Some(due) = task.scheduled.as_deref().or(task.due.as_deref()) {
                line.push(Span::styled(
                    format!(" {} ", due),
                    Style::default().fg(Color::Magenta),
                ));
            }
            line.push(Span::raw(task.title.clone()));
            ListItem::new(Line::from(line))
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().title("Tasks").borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );
    let mut state = ListState::default();
    if !app.tasks.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, body[0], &mut state);

    frame.render_widget(
        draw_calendar(&app.focus_date, &app.calendar_tasks),
        right[0],
    );

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
        Paragraph::new(format!(
            "Path: {}\nStatus: {}\nArchived: {}\nPriority: {}\nScheduled: {}\nDue: {}\nTracking: {}\nRecurring: {}\nRecurrence anchor: {}\nCompleted instances: {}\nSkipped instances: {}\n\n{}",
            task.path,
            task.status,
            archived,
            task.priority.clone().unwrap_or_default(),
            task.scheduled.clone().unwrap_or_default(),
            task.due.clone().unwrap_or_default(),
            if task.has_active_time_entry { "active" } else { "inactive" },
            recurring.unwrap_or(""),
            recurrence_anchor,
            complete_instances,
            skipped_instances,
            task.body
        ))
    } else {
        Paragraph::new("No tasks")
    }
    .block(Block::default().title("Details").borders(Borders::ALL))
    .wrap(Wrap { trim: false });
    frame.render_widget(details, right[1]);

    if let Some(prompt) = app.input_prompt() {
        let input = Paragraph::new(app.input_value.clone()).block(
            Block::default()
                .title(format!("{prompt} (Enter submit, Esc cancel)"))
                .borders(Borders::ALL),
        );
        frame.render_widget(input, layout[2]);
    }

    let status = Paragraph::new(app.status.clone()).block(Block::default().borders(Borders::ALL));
    if has_input {
        frame.render_widget(status, layout[3]);
    } else {
        frame.render_widget(status, layout[2]);
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
        .block(Block::default().title("Calendar").borders(Borders::ALL))
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
            .title("Command Palette")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Black)),
        area,
    );
    frame.render_widget(
        Paragraph::new(app.input_value.clone())
            .alignment(Alignment::Left)
            .block(Block::default().title("Query").borders(Borders::ALL)),
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
                "Ctrl-P open  Up/Down move  Enter run  Esc cancel\nEntries show their normal hotkey in [brackets]. Views: {}",
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
            .title("Date Picker")
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
        Paragraph::new(current)
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
            "Arrows move  H/L month  t today  c clear  / type ISO  Enter save  Esc cancel",
        )
        .block(Block::default().title("Help").borders(Borders::ALL))
        .wrap(Wrap { trim: false }),
        layout[2],
    );
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

fn binding_matches(key: KeyEvent, binding: &str) -> bool {
    let normalized = binding.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "enter" => key.code == KeyCode::Enter,
        "esc" | "escape" => key.code == KeyCode::Esc,
        "left" => key.code == KeyCode::Left,
        "right" => key.code == KeyCode::Right,
        "up" => key.code == KeyCode::Up,
        "down" => key.code == KeyCode::Down,
        "pageup" => key.code == KeyCode::PageUp,
        "pagedown" => key.code == KeyCode::PageDown,
        "space" => key.code == KeyCode::Char(' '),
        _ if normalized.starts_with("ctrl-") => {
            let expected = normalized.trim_start_matches("ctrl-");
            matches!(key.code, KeyCode::Char(ch) if expected == ch.to_string())
                && key.modifiers.contains(KeyModifiers::CONTROL)
        }
        _ if normalized.starts_with("shift-") => {
            let expected = normalized.trim_start_matches("shift-");
            matches!(key.code, KeyCode::Char(ch) if expected == ch.to_ascii_lowercase().to_string())
                && key.modifiers.contains(KeyModifiers::SHIFT)
        }
        _ if normalized.len() == 1 => {
            matches!(key.code, KeyCode::Char(ch) if normalized == ch.to_ascii_lowercase().to_string())
        }
        _ => false,
    }
}
