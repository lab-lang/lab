mod adapters;
mod commands;
mod run;
mod targets;
mod typeset;
mod update;
mod workcell_run;

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
    /// Build a package into verified portable module artifacts, and into
    /// automation protocols when a target is named.
    Build {
        /// Package directory or any path inside a package.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Artifact directory, relative to the project root unless absolute.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Target profile to compile for, named by a file under `targets/`;
        /// defaults to `[build] target` in the manifest.
        #[arg(long)]
        target: Option<String>,
        /// Build portable module IR only, ignoring the manifest's default
        /// target.
        #[arg(long, conflicts_with = "target")]
        no_target: bool,
    },
    /// Bind reachable workflow requirements to one validated facility and write a reviewed plan.
    Plan {
        /// Package directory or any path inside a package.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Plan artifact directory, relative to the project root unless absolute.
        #[arg(long)]
        out_dir: Option<PathBuf>,
    },
    /// Discover, validate, and render compiler-owned target profiles.
    Targets {
        #[command(subcommand)]
        command: TargetsCommand,
    },
    /// Discover, validate, and render asset-bound adapter profiles.
    Adapters {
        #[command(subcommand)]
        command: AdaptersCommand,
    },
    /// Execute an emitted run package — a Hamilton STAR package or a
    /// workcell wave — on the connected stations, or review it with
    /// --dry-run.
    Run {
        /// A run directory produced by `lab build` (the target output
        /// directory, or one wave directory of a dependency build). A
        /// directory holding `plan.workcell.json` runs as a workcell
        /// wave; anything else runs as a Hamilton STAR package.
        path: PathBuf,
        /// Validate and print the full step table without touching
        /// hardware.
        #[arg(long)]
        dry_run: bool,
        /// Skip the initial confirmation. Handoffs and manual steps still
        /// require the operator.
        #[arg(long)]
        yes: bool,
        /// Continue a workcell wave from its ledger, skipping nodes it
        /// records as completed.
        #[arg(long)]
        resume: bool,
        /// Where a networked station answers on this bench, as
        /// NAME=ADDRESS (repeatable). Compiled artifacts never carry
        /// addresses; the bench supplies them at run time.
        #[arg(long = "station", value_name = "NAME=ADDRESS")]
        station: Vec<String>,
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

#[derive(Debug, Subcommand)]
enum TargetsCommand {
    /// Describe every backend and station kind, or one backend in detail.
    Describe {
        /// Concrete `[target] backend` value to describe.
        #[arg(long)]
        backend: Option<String>,
    },
    /// Print the complete reference profile for one backend.
    Default {
        /// Concrete `[target] backend` value.
        backend: String,
        /// Profile filename stem used for validation metadata.
        #[arg(long, default_value = "target")]
        name: String,
    },
    /// Parse and semantically validate a target profile.
    Validate { path: PathBuf },
    /// Validate and print a complete canonical target profile.
    Render { path: PathBuf },
}

#[derive(Debug, Subcommand)]
enum AdaptersCommand {
    /// Describe every adapter implementation, or one driver in detail.
    Describe {
        /// Stable adapter ID such as `hamilton.star`.
        #[arg(long)]
        driver: Option<String>,
    },
    /// Print the complete reference profile for one adapter.
    Default {
        driver: String,
        /// Profile filename stem used for validation metadata.
        #[arg(long, default_value = "adapter")]
        name: String,
    },
    /// Parse and semantically validate an adapter profile against an explicit driver.
    Validate { driver: String, path: PathBuf },
    /// Validate and print a complete canonical adapter profile.
    Render { driver: String, path: PathBuf },
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
            no_target,
        } => commands::build(path, out_dir, target, no_target, &output),
        Command::Plan { path, out_dir } => commands::plan(path, out_dir, &output),
        Command::Targets { command } => match command {
            TargetsCommand::Describe { backend } => targets::describe(backend, &output),
            TargetsCommand::Default { backend, name } => targets::default(backend, name, &output),
            TargetsCommand::Validate { path } => targets::validate(path, &output),
            TargetsCommand::Render { path } => targets::render(path, &output),
        },
        Command::Adapters { command } => match command {
            AdaptersCommand::Describe { driver } => adapters::describe(driver, &output),
            AdaptersCommand::Default { driver, name } => adapters::default(driver, name, &output),
            AdaptersCommand::Validate { driver, path } => adapters::validate(driver, path, &output),
            AdaptersCommand::Render { driver, path } => adapters::render(driver, path, &output),
        },
        Command::Run {
            path,
            dry_run,
            yes,
            resume,
            station,
        } => {
            if workcell_run::is_workcell_directory(&path) {
                workcell_run::run_workcell_command(path, dry_run, yes, resume, station, &output)
            } else if resume || !station.is_empty() {
                anyhow::bail!(
                    "--resume and --station apply to workcell waves; this directory holds a Hamilton STAR package, which re-runs from its documents"
                )
            } else {
                run::run(path, dry_run, yes, &output)
            }
        }
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
            "opentrons-ot2",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Build { path, out_dir, target, no_target }
                if path.as_path() == std::path::Path::new("project")
                    && out_dir.as_deref() == Some(std::path::Path::new("dist"))
                    && target.as_deref() == Some("opentrons-ot2")
                    && !no_target
        ));
    }

    #[test]
    fn parses_facility_plan_options() {
        let cli = Cli::try_parse_from(["lab", "plan", "project", "--out-dir", "review"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Plan { path, out_dir }
                if path.as_path() == std::path::Path::new("project")
                    && out_dir.as_deref() == Some(std::path::Path::new("review"))
        ));
    }

    #[test]
    fn rejects_naming_a_target_and_opting_out_of_one() {
        assert!(
            Cli::try_parse_from(["lab", "build", "--target", "opentrons-ot2", "--no-target"])
                .is_err()
        );
    }

    #[test]
    fn accepts_global_json_after_the_subcommand() {
        let cli = Cli::try_parse_from(["lab", "check", "--json"]).unwrap();
        assert!(cli.json);
    }

    #[test]
    fn parses_target_contract_commands() {
        let cli = Cli::try_parse_from([
            "lab",
            "targets",
            "describe",
            "--backend",
            "hamilton.star",
            "--json",
        ])
        .unwrap();
        assert!(cli.json);
        assert!(matches!(
            cli.command,
            Command::Targets {
                command: TargetsCommand::Describe { backend: Some(backend) }
            } if backend == "hamilton.star"
        ));

        let cli = Cli::try_parse_from(["lab", "targets", "default", "workcell", "--name", "bench"])
            .unwrap();
        assert!(matches!(
            cli.command,
            Command::Targets {
                command: TargetsCommand::Default { backend, name }
            } if backend == "workcell" && name == "bench"
        ));
    }

    #[test]
    fn parses_adapter_contract_commands() {
        let cli = Cli::try_parse_from([
            "lab",
            "adapters",
            "describe",
            "--driver",
            "hamilton.star",
            "--json",
        ])
        .unwrap();
        assert!(cli.json);
        assert!(matches!(
            cli.command,
            Command::Adapters {
                command: AdaptersCommand::Describe {
                    driver: Some(driver)
                }
            } if driver == "hamilton.star"
        ));

        let cli = Cli::try_parse_from([
            "lab",
            "adapters",
            "validate",
            "inheco.odtc",
            "adapters/cycler.toml",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Adapters {
                command: AdaptersCommand::Validate { driver, path }
            } if driver == "inheco.odtc"
                && path.as_path() == std::path::Path::new("adapters/cycler.toml")
        ));
    }

    #[test]
    fn parses_update_check_flag() {
        let cli = Cli::try_parse_from(["lab", "update", "--check"]).unwrap();
        assert!(matches!(cli.command, Command::Update { check: true }));

        let cli = Cli::try_parse_from(["lab", "update"]).unwrap();
        assert!(matches!(cli.command, Command::Update { check: false }));
    }
}
