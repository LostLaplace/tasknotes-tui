use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use tasknotes_tui::{app::App, repository::TaskRepository, ui};

#[derive(Parser)]
struct Cli {
    #[arg(short = 'C', long = "root", default_value = ".")]
    root: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo = TaskRepository::open(cli.root)?;
    let app = App::new(repo)?;
    ui::run(app)
}
