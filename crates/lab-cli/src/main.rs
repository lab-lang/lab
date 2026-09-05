mod adapters;
mod commands;
mod execution_run;
mod facility_lowering;
mod typeset;
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
    /// Build verified experiment artifacts and specialize through a configured facility.
    Build {
        /// Package directory or any path inside a package.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Artifact directory, relative to the project root unless absolute.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// A program under src/programs/ to build, named by its file. The
        /// build is rooted at that program's main workflow.
        #[arg(long)]
        program: Option<String>,
    },
    /// Write only the reviewed facility plan and its adapter lowerings.
    Plan {
        /// Package directory or any path inside a package.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Plan artifact directory, relative to the project root unless absolute.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// A program under src/programs/ to plan, named by its file. The plan
        /// is rooted at that program's main workflow and written under its own
        /// directory.
        #[arg(long)]
        program: Option<String>,
    },
    /// Discover, validate, and render asset-bound adapter profiles.
    Adapters {
        #[command(subcommand)]
        command: AdaptersCommand,
    },
    /// Execute a reviewed facility plan, or validate and review it with --dry-run.
    Run {
        /// A directory containing plan.execution.json.
        path: PathBuf,
        /// Validate and print the full step table without touching
        /// hardware.
        #[arg(long)]
        dry_run: bool,
        /// Execute through simulation adapters without touching physical hardware.
        #[arg(long, conflicts_with = "dry_run")]
        simulate: bool,
        /// Skip the initial confirmation. Handoffs and manual steps still
        /// require the operator.
        #[arg(long)]
        yes: bool,
        /// Continue the exact reviewed plan from its durable ledger.
        #[arg(long)]
        resume: bool,
        /// Where a networked Asset answers, as ASSET_IRI=ADDRESS (repeatable).
        #[arg(long, value_name = "ASSET_IRI=ADDRESS")]
        asset_endpoint: Vec<String>,
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
            program,
        } => commands::build(path, out_dir, program, &output),
        Command::Plan {
            path,
            out_dir,
            program,
        } => commands::plan(path, out_dir, program, &output),
        Command::Adapters { command } => match command {
            AdaptersCommand::Describe { driver } => adapters::describe(driver, &output),
            AdaptersCommand::Default { driver, name } => adapters::default(driver, name, &output),
            AdaptersCommand::Validate { driver, path } => adapters::validate(driver, path, &output),
            AdaptersCommand::Render { driver, path } => adapters::render(driver, path, &output),
        },
        Command::Run {
            path,
            dry_run,
            simulate,
            yes,
            resume,
            asset_endpoint,
        } => execution_run::run_execution_command(
            path,
            dry_run,
            simulate,
            yes,
            resume,
            asset_endpoint,
            &output,
        ),
        Command::Metadata { path } => commands::metadata(path, &output),
        Command::Update { check } => update::update(check, &output),
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn parses_package_build_options() {
        let cli = Cli::try_parse_from(["lab", "build", "project", "--out-dir", "dist"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Build { path, out_dir, .. }
                if path.as_path() == std::path::Path::new("project")
                    && out_dir.as_deref() == Some(std::path::Path::new("dist"))
        ));
    }

    #[test]
    fn parses_facility_plan_options() {
        let cli = Cli::try_parse_from(["lab", "plan", "project", "--out-dir", "review"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Plan { path, out_dir, .. }
                if path.as_path() == std::path::Path::new("project")
                    && out_dir.as_deref() == Some(std::path::Path::new("review"))
        ));
    }

    #[test]
    fn parses_exact_asset_endpoints_for_facility_runs() {
        let cli = Cli::try_parse_from([
            "lab",
            "run",
            "review",
            "--resume",
            "--asset-endpoint",
            "https://example.org/facility/odtc=192.0.2.1:8080",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Run {
                path,
                resume: true,
                asset_endpoint,
                ..
            } if path.as_path() == std::path::Path::new("review")
                && asset_endpoint == ["https://example.org/facility/odtc=192.0.2.1:8080"]
        ));
    }

    #[test]
    fn parses_explicit_facility_simulation_mode() {
        let cli = Cli::try_parse_from(["lab", "run", "review", "--simulate", "--resume"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Run {
                path,
                simulate: true,
                resume: true,
                ..
            } if path.as_path() == std::path::Path::new("review")
        ));
    }

    #[test]
    fn rejects_the_removed_target_surfaces() {
        assert!(Cli::try_parse_from(["lab", "build", "--target", "opentrons-ot2"]).is_err());
        assert!(Cli::try_parse_from(["lab", "targets", "describe"]).is_err());
    }

    #[test]
    fn accepts_global_json_after_the_subcommand() {
        let cli = Cli::try_parse_from(["lab", "check", "--json"]).unwrap();
        assert!(cli.json);
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
