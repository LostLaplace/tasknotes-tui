use anyhow::Result;
use clap::Parser;
use tasknotes_tui::spec_ops;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    stdio: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.stdio {
        return spec_ops::run_stdio();
    }
    Ok(())
}
