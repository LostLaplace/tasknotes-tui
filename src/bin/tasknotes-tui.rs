use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tasknotes_tui::{
    app::App,
    repository::TaskRepository,
    tui_config::{default_config_yaml, load_tui_config},
    ui,
};

#[derive(Parser)]
struct Cli {
    #[arg(short = 'C', long = "root", default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    PrintDefaultConfig,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        None => {
            let tui_config = load_tui_config(&cli.root);
            let repo = TaskRepository::open(&cli.root)?;
            let app = App::new(repo, tui_config)?;
            ui::run(app)
        }
        Some(Command::PrintDefaultConfig) => {
            print!("{}", default_config_yaml());
            Ok(())
        }
    }
}
