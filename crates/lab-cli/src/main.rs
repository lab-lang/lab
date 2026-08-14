mod commands;
mod flow;
mod render;
mod run;
mod scene;
mod simulate;
mod stamp;
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
        /// A run directory (workcell wave or Hamilton STAR package), or a
        /// package directory whose built default target is simulated wave
        /// by wave. Defaults to the current directory.
        #[arg(default_value = ".")]
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
        /// A run directory, or a package directory whose built default
        /// target is rendered wave by wave. Defaults to the current
        /// directory.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Where to write scene files; defaults to the run directory.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Facility description that lays out the room: station positions,
        /// the room shell, and real meshes from its assets directory.
        #[arg(long)]
        facility: Option<PathBuf>,
        /// Animate the USD layer from this package's sim-trace.json, so
        /// USD tools play the simulated run on their timeline.
        #[arg(long)]
        animated: bool,
    },
    /// Render the simulated run as photographic frames (and a movie when
    /// ffmpeg is present) through a headless Blender.
    Render {
        /// A run directory or package directory. The simulation and the
        /// animated scene regenerate first, so this one command is the
        /// whole flow. Defaults to the current directory.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Camera preset.
        #[arg(long, default_value = "dolly")]
        camera: String,
        /// Simulated seconds per wall-clock second of footage.
        #[arg(long, default_value_t = 600.0)]
        speedup: f64,
        /// Frames per second of footage.
        #[arg(long, default_value_t = 24)]
        fps: u32,
        /// `preview` (fast EEVEE) or `final` (path-traced Cycles).
        #[arg(long, default_value = "preview")]
        quality: String,
        /// Render one frame at this simulated second instead of the run.
        #[arg(long)]
        still: Option<f64>,
        /// Environment .hdr/.exr for lighting; the built-in sky otherwise.
        #[arg(long)]
        hdri: Option<PathBuf>,
        /// The Blender executable; found on PATH or LAB_BLENDER otherwise.
        #[arg(long)]
        blender: Option<PathBuf>,
        /// Where to write frames; defaults to `renders/` beside the scene.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Facility description; defaults to the package's facility.toml
        /// or its manifest's [build] facility pointer.
        #[arg(long)]
        facility: Option<PathBuf>,
        /// Blender processes per wave, each rendering a slice of the frame
        /// range. Previews default to every core; path-traced finals to
        /// one process, which already saturates the GPU.
        #[arg(long)]
        jobs: Option<usize>,
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
        Command::Scene {
            path,
            out_dir,
            facility,
            animated,
        } => scene::scene(path, out_dir, facility, animated, &output),
        Command::Render {
            path,
            camera,
            speedup,
            fps,
            quality,
            still,
            hdri,
            blender,
            out_dir,
            facility,
            jobs,
        } => render::render(
            path,
            render::RenderOptions {
                camera,
                speedup,
                fps,
                quality,
                still,
                hdri,
                blender,
                out_dir,
                facility,
                jobs,
            },
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
