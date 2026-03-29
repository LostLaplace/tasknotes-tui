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
use crate::repository::TaskFilter;

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
                    open_selected_in_editor(app)?;
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

    match key.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('h') | KeyCode::Left => app.move_focus_date(-1)?,
        KeyCode::Char('l') | KeyCode::Right => app.move_focus_date(1)?,
        KeyCode::PageUp => app.move_focus_date(-7)?,
        KeyCode::PageDown => app.move_focus_date(7)?,
        KeyCode::Char('g') => app.reset_focus_date()?,
        KeyCode::Char('j') | KeyCode::Down => app.next(),
        KeyCode::Char('k') | KeyCode::Up => app.previous(),
        KeyCode::Char('r') => app.refresh()?,
        KeyCode::Char('x') | KeyCode::Char(' ') => app.toggle_selected()?,
        KeyCode::Char('S') => app.skip_selected_today()?,
        KeyCode::Char('1') => app.set_filter(TaskFilter::Open)?,
        KeyCode::Char('2') => app.set_filter(TaskFilter::Today)?,
        KeyCode::Char('3') => app.set_filter(TaskFilter::Overdue)?,
        KeyCode::Char('4') => app.set_filter(TaskFilter::All)?,
        KeyCode::Char('5') => app.set_filter(TaskFilter::Tracked)?,
        KeyCode::Char('n') => app.begin_create(),
        KeyCode::Char('/') => app.begin_search(),
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.begin_command_palette()
        }
        KeyCode::Char('e') => app.begin_edit_title(),
        KeyCode::Char('i') => app.request_open_in_editor(),
        KeyCode::Char('d') => app.begin_edit_due(),
        KeyCode::Char('s') => app.begin_edit_scheduled(),
        KeyCode::Char('T') => app.toggle_selected_time_tracking()?,
        KeyCode::Char('p') => app.begin_edit_priority(),
        KeyCode::Char('t') => app.begin_edit_status(),
        KeyCode::Char('R') => app.begin_edit_recurrence(),
        KeyCode::Char('A') => app.begin_edit_recurrence_anchor(),
        _ => {}
    }
    Ok(false)
}

fn draw(frame: &mut Frame<'_>, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
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
            format!("Filter: {}", filter_label(app.filter)),
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
            let mut line = vec![Span::styled(
                format!("{:<12}", task.status),
                Style::default().fg(if task.status == "done" || task.status == "cancelled" {
                    Color::Green
                } else {
                    Color::Cyan
                }),
            )];
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

    frame.render_widget(draw_calendar(&app.focus_date, &app.calendar_tasks), right[0]);

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
        Paragraph::new(format!(
            "Path: {}\nStatus: {}\nPriority: {}\nScheduled: {}\nDue: {}\nTracking: {}\nRecurring: {}\nRecurrence anchor: {}\nCompleted instances: {}\nSkipped instances: {}\n\n{}",
            task.path,
            task.status,
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

    let input = if let Some(prompt) = app.input_prompt() {
        Paragraph::new(app.input_value.clone()).block(
            Block::default()
                .title(format!("{prompt} (Enter submit, Esc cancel)"))
                .borders(Borders::ALL),
        )
    } else {
        Paragraph::new("Keys: Ctrl-P palette  h/l or <-/-> date  PgUp/PgDn week  g today  1-5 filters  / search  n create  i editor  T track  x toggle  q quit")
            .block(Block::default().borders(Borders::ALL))
    };
    frame.render_widget(input, layout[2]);

    let status = Paragraph::new(app.status.clone()).block(Block::default().borders(Borders::ALL));
    frame.render_widget(status, layout[3]);

    if app.is_palette_active() {
        draw_command_palette(frame, app);
    }
    if app.is_date_picker_active() {
        draw_date_picker(frame, app);
    }
}

fn filter_label(filter: TaskFilter) -> &'static str {
    match filter {
        TaskFilter::All => "All",
        TaskFilter::Open => "Open",
        TaskFilter::Today => "Date",
        TaskFilter::Overdue => "Overdue",
        TaskFilter::Tracked => "Tracked",
    }
}

fn draw_calendar(focus_date: &str, calendar_tasks: &[crate::repository::TaskRecord]) -> Paragraph<'static> {
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

    let mut lines = vec![
        Line::from(month_name),
        Line::from("Mo Tu We Th Fr Sa Su"),
    ];

    let mut day = 1u32;
    for week in 0..6 {
        let mut spans = Vec::new();
        for weekday in 0..7 {
            let cell = week * 7 + weekday;
            if cell < start_offset || day > days_in_month {
                spans.push(Span::raw("   "));
                continue;
            }
            let date = NaiveDate::from_ymd_opt(focus.year(), focus.month(), day).expect("valid date");
            let ymd = date.format("%Y-%m-%d").to_string();
            let has_tasks = calendar_tasks
                .iter()
                .any(|task| {
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
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
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
            ListItem::new(Line::from(vec![
                Span::styled(item.title, Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(item.description, Style::default().fg(Color::DarkGray)),
            ]))
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
        List::new(list_items).block(Block::default().title("Commands").borders(Borders::ALL)).highlight_style(
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
            "Ctrl-P open  Up/Down move  Enter run  Esc cancel\nFilters: open date overdue all tracked  Actions: create editor search refresh toggle track skip",
        )
        .wrap(Wrap { trim: false })
        .block(Block::default().title("Help").borders(Borders::ALL)),
        layout[2],
    );
}

fn open_selected_in_editor(app: &mut App) -> Result<()> {
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

    match status {
        Ok(exit) => {
            app.reload_from_disk()?;
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

    frame.render_widget(draw_calendar(&app.picker_date, &app.calendar_tasks), layout[1]);

    frame.render_widget(
        Paragraph::new("Arrows move  H/L month  t today  c clear  / type ISO  Enter save  Esc cancel")
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
