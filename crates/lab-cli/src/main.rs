mod commands;
mod update;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(
    name = "lab",
    version,
    about = "Design, build, and operate programmable laboratory workflows"
)]
struct Cli {
    /// Emit stable machine-readable command results.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a new Lab project.
    New {
        /// Directory to create.
        path: PathBuf,
        /// Package name; defaults to the directory name.
        #[arg(long)]
        name: Option<String>,
    },
    /// Check one source file or every module in a package.
    Check {
        /// Source file, package directory, or any path inside a package.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Build a package into verified portable module artifacts, and into robot
    /// protocols when a target is named.
    Build {
        /// Package directory or any path inside a package.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Artifact directory, relative to the project root unless absolute.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Target profile to compile for, named by a file under `targets/`.
        #[arg(long)]
        target: Option<String>,
    },
    /// Print resolved package metadata and source-module names.
    Metadata {
        /// Package directory or any path inside a package.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Check for and install a newer lab/labc/lab-opt release.
    Update {
        /// Only report whether an update is available; don't install it.
        #[arg(long)]
        check: bool,
    },
}

struct Output {
    json: bool,
}

impl Output {
    fn new(json: bool) -> Self {
        Self { json }
    }

    fn success<T: Serialize>(&self, status: &'static str, result: T, human: String) -> Result<()> {
        if self.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&MachineResult { status, result })?
            );
        } else {
            println!("{human}");
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct MachineResult<T> {
    status: &'static str,
    result: T,
}

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
        Command::Build {
            path,
            out_dir,
            target,
        } => commands::build(path, out_dir, target, &output),
        Command::Metadata { path } => commands::metadata(path, &output),
        Command::Update { check } => update::update(check, &output),
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn parses_package_build_options() {
        let cli = Cli::try_parse_from([
            "lab",
            "build",
            "project",
            "--out-dir",
            "dist",
            "--target",
            "bench-ot2",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Build { path, out_dir, target }
                if path.as_path() == std::path::Path::new("project")
                    && out_dir.as_deref() == Some(std::path::Path::new("dist"))
                    && target.as_deref() == Some("bench-ot2")
        ));
    }

    #[test]
    fn accepts_global_json_after_the_subcommand() {
        let cli = Cli::try_parse_from(["lab", "check", "--json"]).unwrap();
        assert!(cli.json);
    }

    #[test]
    fn parses_update_check_flag() {
        let cli = Cli::try_parse_from(["lab", "update", "--check"]).unwrap();
        assert!(matches!(cli.command, Command::Update { check: true }));

        let cli = Cli::try_parse_from(["lab", "update"]).unwrap();
        assert!(matches!(cli.command, Command::Update { check: false }));
    }
}
