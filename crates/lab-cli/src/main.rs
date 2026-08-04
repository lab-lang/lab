mod args;
mod commands;
mod output;

use anyhow::Result;
use clap::Parser;

use args::{Cli, Command};
use output::Output;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let output = Output::new(cli.json);
    match cli.command {
        Command::New { path, name } => commands::new_project(path, name, &output),
        Command::Check { path } => commands::check(path, &output),
        Command::Build { path, out_dir } => commands::build(path, out_dir, &output),
        Command::Metadata { path } => commands::metadata(path, &output),
    }
}
