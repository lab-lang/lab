mod commands;
mod run;
mod scene;
mod simulate;
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
    /// Build a package into verified portable module artifacts, and into robot
    /// protocols when a target is named.
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
    /// Simulate an emitted run package on a virtual clock: how long the
    /// work takes, when an operator must be present, and how long each
    /// walk-away window lasts. Touches no hardware and writes no ledger;
    /// the full record lands in a `lab.sim-trace.v0` trace file.
    Simulate {
        /// A run directory produced by `lab build`, workcell wave or
        /// Hamilton STAR package.
        path: PathBuf,
        /// Where to write the trace; defaults to `sim-trace.json` beside
        /// the plan.
        #[arg(long)]
        trace: Option<PathBuf>,
        /// Facility description to simulate against: the plan's stations
        /// must exist there, and its transport times drive the handoffs.
        #[arg(long)]
        facility: Option<PathBuf>,
    },
    /// Render a built run package as a 3D scene: a `lab.scene.v0`
    /// document plus glTF and USD projections of it.
    Scene {
        /// A run directory produced by `lab build`, workcell wave or
        /// Hamilton STAR package.
        path: PathBuf,
        /// Where to write scene files; defaults to the run directory.
        #[arg(long)]
        out_dir: Option<PathBuf>,
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
            no_target,
        } => commands::build(path, out_dir, target, no_target, &output),
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
        Command::Simulate {
            path,
            trace,
            facility,
        } => simulate::simulate(path, trace, facility, &output),
        Command::Scene { path, out_dir } => scene::scene(path, out_dir, &output),
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
    fn parses_update_check_flag() {
        let cli = Cli::try_parse_from(["lab", "update", "--check"]).unwrap();
        assert!(matches!(cli.command, Command::Update { check: true }));

        let cli = Cli::try_parse_from(["lab", "update"]).unwrap();
        assert!(matches!(cli.command, Command::Update { check: false }));
    }
}
