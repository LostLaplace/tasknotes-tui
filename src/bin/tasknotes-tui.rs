use std::path::PathBuf;
use std::{fs, path::Path};

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use tasknotes_tui::{
    app::App,
    repository::{TaskDraft, TaskFilter, TaskRepository},
    snapshot,
    tui_config::{default_config_yaml, load_tui_config},
    ui,
};

#[derive(Parser)]
struct Cli {
    #[arg(short = 'C', long = "root", default_value = ".")]
    root: PathBuf,
    #[arg(long = "focus-date")]
    focus_date: Option<String>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    PrintDefaultConfig,
    SeedDemoVault,
    RenderSnapshot {
        #[arg(long, default_value_t = 120)]
        width: u16,
        #[arg(long, default_value_t = 32)]
        height: u16,
        #[arg(long, value_enum, default_value_t = SnapshotFormat::Text)]
        format: SnapshotFormat,
        #[arg(long)]
        view: Option<u8>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        selected: Option<usize>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum SnapshotFormat {
    Text,
    Html,
    Svg,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        None => {
            let tui_config = load_tui_config(&cli.root);
            let repo = TaskRepository::open(&cli.root)?;
            let app = build_app(
                repo,
                tui_config,
                cli.focus_date.as_deref(),
                None,
                None,
                None,
            )?;
            ui::run(app)
        }
        Some(Command::PrintDefaultConfig) => {
            print!("{}", default_config_yaml());
            Ok(())
        }
        Some(Command::SeedDemoVault) => seed_demo_vault(&cli.root),
        Some(Command::RenderSnapshot {
            width,
            height,
            format,
            view,
            search,
            selected,
        }) => {
            let tui_config = load_tui_config(&cli.root);
            let repo = TaskRepository::open(&cli.root)?;
            let app = build_app(
                repo,
                tui_config,
                cli.focus_date.as_deref(),
                view,
                search.as_deref(),
                selected,
            )?;
            match format {
                SnapshotFormat::Text => print!("{}", snapshot::render_text(&app, width, height)?),
                SnapshotFormat::Html => print!("{}", snapshot::render_html(&app, width, height)?),
                SnapshotFormat::Svg => print!("{}", snapshot::render_svg(&app, width, height)?),
            }
            Ok(())
        }
    }
}

fn build_app(
    repo: TaskRepository,
    tui_config: tasknotes_tui::tui_config::TuiConfig,
    focus_date: Option<&str>,
    view: Option<u8>,
    search: Option<&str>,
    selected: Option<usize>,
) -> Result<App> {
    let mut app = App::new(repo, tui_config)?;
    if let Some(view) = view {
        app.activate_view_slot(view)?;
    }
    if let Some(focus_date) = focus_date {
        app.focus_date = focus_date.to_string();
        app.refresh()?;
    }
    if let Some(search) = search {
        app.search_query = search.to_string();
        app.refresh()?;
    }
    if let Some(selected) = selected {
        if !app.tasks.is_empty() {
            app.selected = selected.min(app.tasks.len() - 1);
        }
    }
    Ok(app)
}

fn seed_demo_vault(root: &Path) -> Result<()> {
    let tasks_dir = root.join("TaskNotes/Tasks");
    if tasks_dir.exists() {
        fs::remove_dir_all(&tasks_dir)?;
    }
    fs::create_dir_all(&tasks_dir)?;

    let repo = TaskRepository::open(root)?;
    let drafts = [
        TaskDraft {
            title: "Plan release".into(),
            details: "Finalize scope, assign owners, and confirm the release checklist.".into(),
            due: Some("2026-03-30".into()),
            scheduled: Some("2026-03-29".into()),
            priority: Some("high".into()),
            status: Some("doing".into()),
            recurrence: None,
            recurrence_anchor: None,
        },
        TaskDraft {
            title: "Inbox sweep".into(),
            details: "Triage loose notes and convert anything actionable into tasks.".into(),
            due: None,
            scheduled: Some("2026-03-29".into()),
            priority: Some("normal".into()),
            status: Some("open".into()),
            recurrence: None,
            recurrence_anchor: None,
        },
        TaskDraft {
            title: "Weekly review".into(),
            details: "Review backlog health and promote next actions into the current cycle."
                .into(),
            due: Some("2026-03-29".into()),
            scheduled: Some("2026-03-29".into()),
            priority: Some("normal".into()),
            status: Some("open".into()),
            recurrence: Some("FREQ=WEEKLY".into()),
            recurrence_anchor: Some("scheduled".into()),
        },
        TaskDraft {
            title: "Write changelog".into(),
            details: "Summarize visible changes for the upcoming release notes.".into(),
            due: Some("2026-04-01".into()),
            scheduled: Some("2026-03-30".into()),
            priority: Some("medium".into()),
            status: Some("open".into()),
            recurrence: None,
            recurrence_anchor: None,
        },
        TaskDraft {
            title: "Cleanup archived notes".into(),
            details: "Finished.".into(),
            due: None,
            scheduled: Some("2026-03-28".into()),
            priority: Some("normal".into()),
            status: Some("done".into()),
            recurrence: None,
            recurrence_anchor: None,
        },
    ];

    for draft in drafts {
        repo.create_task_from_draft(&draft)?;
    }

    let tasks = repo.list_tasks(TaskFilter::All, "2026-03-29")?;
    if let Some(task) = tasks.iter().find(|task| task.title == "Plan release") {
        let _ = repo.toggle_time_tracking(task)?;
    }
    Ok(())
}
