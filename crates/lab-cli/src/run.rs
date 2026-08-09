//! Live execution of an emitted Hamilton STAR run package.
//!
//! `lab run <dir>` replays the reviewed `lab.star-run.v0` documents a build
//! produced: every frame is validated through the driver crate before
//! anything moves, the machine runs the documented setup choreography
//! first, and any firmware error retracts the channels and aborts with the
//! failed step named. The dry run prints the full step table and touches no
//! hardware.

use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lab_compiler::runfmt::{STAR_RUN_FORMAT, StarRunDocument};
use lab_hamilton_star::{InitializeOptions, RawCommand, Star};
use serde::Deserialize;

use crate::Output;

pub(crate) use lab_compiler::runfmt::ManualStep;

/// One `lab.star-run.v0` document, loaded and frame-validated.
pub(crate) struct LoadedRun {
    pub id: String,
    pub title: String,
    pub steps: Vec<LoadedStep>,
    pub manual_after: Vec<ManualStep>,
}

pub(crate) struct LoadedStep {
    pub command: RawCommand,
    pub description: String,
}

/// The manifest fields the runner reads: run order and the bench's
/// initialize options.
#[derive(Deserialize)]
struct ManifestSummary {
    target: String,
    runs: Vec<ManifestRun>,
    deck: ManifestDeck,
}

#[derive(Deserialize)]
struct ManifestRun {
    id: String,
}

#[derive(Deserialize)]
struct ManifestDeck {
    #[serde(default)]
    run: ManifestRunOptions,
}

#[derive(Deserialize, Default)]
struct ManifestRunOptions {
    #[serde(default)]
    autoload_park_track: Option<u32>,
}

/// Loads a run directory: the automation manifest names the run order, and
/// every document's frames must parse before anything is reported ready.
pub(crate) fn load_run_directory(directory: &Path) -> Result<(Vec<LoadedRun>, Option<u32>)> {
    let manifest_path = directory.join("automation_manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path).with_context(|| {
        format!(
            "no automation manifest at {}; point lab run at a directory produced by `lab build` for a hamilton.star target",
            manifest_path.display()
        )
    })?;
    let manifest: ManifestSummary =
        serde_json::from_str(&manifest_text).context("failed to parse the automation manifest")?;
    if manifest.target != "hamilton.star" {
        bail!(
            "this package was compiled for '{}'; lab run executes hamilton.star run documents only",
            manifest.target
        );
    }

    let mut runs = Vec::new();
    for run in &manifest.runs {
        let path = directory.join(format!("{}.star.json", run.id));
        let text = fs::read_to_string(&path)
            .with_context(|| format!("missing run document {}", path.display()))?;
        let document: StarRunDocument = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        if document.format != STAR_RUN_FORMAT {
            bail!(
                "{} declares format '{}'; this runner speaks {STAR_RUN_FORMAT}",
                path.display(),
                document.format
            );
        }
        let steps = document
            .steps
            .iter()
            .map(|step| {
                RawCommand::parse(&step.frame)
                    .map(|command| LoadedStep {
                        command,
                        description: step.description.clone(),
                    })
                    .with_context(|| format!("{} carries an unreplayable frame", path.display()))
            })
            .collect::<Result<Vec<_>>>()?;
        runs.push(LoadedRun {
            id: document.run,
            title: document.title,
            steps,
            manual_after: document.manual_after,
        });
    }
    Ok((runs, manifest.deck.run.autoload_park_track))
}

/// The outcome of replaying one package.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RunOutcome {
    Completed {
        steps: usize,
    },
    /// A firmware error stopped the run; the channels were retracted and
    /// physical state stands at the named step.
    Aborted {
        run_id: String,
        step_index: usize,
        error: String,
    },
}

/// Replays loaded runs over an open session. `pause` is called between
/// runs with the manual-step text and must return `true` to continue —
/// the operator confirms the bench matches before more motion.
pub(crate) fn execute_runs(
    star: &Star,
    runs: &[LoadedRun],
    pause: &mut dyn FnMut(&str) -> bool,
    narrate: &mut dyn FnMut(&str),
) -> Result<RunOutcome> {
    let mut executed = 0usize;
    for (index, run) in runs.iter().enumerate() {
        narrate(&format!(
            "run {}: {} ({} steps)",
            index + 1,
            run.title,
            run.steps.len()
        ));
        for (step_index, step) in run.steps.iter().enumerate() {
            narrate(&format!("  [{:>3}] {}", step_index + 1, step.description));
            if let Err(error) = star.execute_raw(&step.command) {
                // Any failure leaves the machine mid-motion: retract to
                // Z-safety before handing control back.
                let retract = RawCommand::parse("C0ZA")
                    .expect("the retract frame is a constant well-formed frame");
                let _ = star.execute_raw(&retract);
                return Ok(RunOutcome::Aborted {
                    run_id: run.id.clone(),
                    step_index,
                    error: error.to_string(),
                });
            }
            executed += 1;
        }
        for manual in &run.manual_after {
            let prompt = format!("{}: {}", manual.title, manual.instructions);
            if !pause(&prompt) {
                bail!("run stopped by the operator after '{}'", run.id);
            }
        }
    }
    Ok(RunOutcome::Completed { steps: executed })
}

/// The `lab run` command.
pub(crate) fn run(directory: PathBuf, dry_run: bool, yes: bool, output: &Output) -> Result<()> {
    let (runs, autoload_park_track) = load_run_directory(&directory)?;
    let total_steps: usize = runs.iter().map(|run| run.steps.len()).sum();

    if dry_run {
        let mut human = format!(
            "dry run: {} run document(s), {} frames, all validated\n",
            runs.len(),
            total_steps
        );
        for run in &runs {
            human.push_str(&format!("\n{} — {}\n", run.id, run.title));
            for (index, step) in run.steps.iter().enumerate() {
                human.push_str(&format!(
                    "  [{:>3}] {:<4} {}\n        {}\n",
                    index + 1,
                    step.command.code(),
                    step.description,
                    step.command.frame(),
                ));
            }
            for manual in &run.manual_after {
                human.push_str(&format!(
                    "  then by hand — {}: {}\n",
                    manual.title, manual.instructions
                ));
            }
        }
        return output.success(
            "dry-run",
            serde_json::json!({
                "runs": runs.len(),
                "steps": total_steps,
            }),
            human,
        );
    }

    println!(
        "about to execute {} run document(s), {} frames, on the first Hamilton STAR on USB",
        runs.len(),
        total_steps
    );
    if !yes && !confirm("proceed? The machine will move. [y/N] ")? {
        bail!("run cancelled before any motion");
    }

    let star = Star::open_usb().context(
        "no Hamilton STAR answered on USB; use --dry-run to review the package without hardware",
    )?;
    println!("connected; running the setup choreography");
    star.initialize(InitializeOptions {
        autoload_park_track,
        ..InitializeOptions::default()
    })
    .context("the setup choreography failed; the machine is not in a known state")?;

    let mut pause = |prompt: &str| {
        println!("\nby hand — {prompt}");
        confirm("done, and the bench matches the plan? Continue [y/N] ").unwrap_or(false)
    };
    let mut narrate = |line: &str| println!("{line}");
    match execute_runs(&star, &runs, &mut pause, &mut narrate)? {
        RunOutcome::Completed { steps } => output.success(
            "run",
            serde_json::json!({ "steps": steps }),
            format!("completed {steps} machine steps"),
        ),
        RunOutcome::Aborted {
            run_id,
            step_index,
            error,
        } => {
            bail!(
                "firmware error at {run_id} step {}: {error}; channels were retracted to Z-safety — resolve the bench and re-run from this run document",
                step_index + 1
            )
        }
    }
}

fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

/// Test-only session construction over an arbitrary transport, so the
/// replay loop is exercised without hardware.
#[cfg(test)]
pub(crate) fn star_over(
    transport: std::sync::Arc<dyn lab_hamilton_star::Transport>,
) -> Result<Star> {
    Ok(Star::new(transport)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lab_hamilton_star::{MockTransport, Transport};
    use std::sync::Arc;

    fn loaded(frames: &[(&str, &str)]) -> LoadedRun {
        LoadedRun {
            id: "test_run".into(),
            title: "test".into(),
            steps: frames
                .iter()
                .map(|(frame, description)| LoadedStep {
                    command: RawCommand::parse(frame).expect("test frames are well-formed"),
                    description: description.to_string(),
                })
                .collect(),
            manual_after: Vec::new(),
        }
    }

    #[test]
    fn a_scripted_run_replays_every_frame_in_order() {
        let transport = Arc::new(MockTransport::new());
        transport.set_responder(|command| {
            let id = command.get(6..10).unwrap_or("0000").to_string();
            vec![format!("{}id{id}er00/00", &command[..4])]
        });
        let star = star_over(transport.clone() as Arc<dyn Transport>).expect("mock opens");
        let runs = vec![loaded(&[
            ("C0TTtt00tf1tl0519tv03600tg2tu0", "define the small tip"),
            ("C0ZA", "retract"),
        ])];
        let outcome = execute_runs(&star, &runs, &mut |_| true, &mut |_| {})
            .expect("the scripted run completes");
        assert_eq!(outcome, RunOutcome::Completed { steps: 2 });
        let written = transport.written();
        assert_eq!(written.len(), 2, "both frames reached the wire in order");
        assert!(
            written[0].starts_with("C0TTid") && written[0].ends_with("tt00tf1tl0519tv03600tg2tu0"),
            "the tip definition went first with the session's id spliced in: {}",
            written[0]
        );
    }

    #[test]
    fn a_firmware_error_retracts_and_reports_the_failed_step() {
        let transport = Arc::new(MockTransport::new());
        transport.set_responder(|command| {
            let id = command.get(6..10).unwrap_or("0000").to_string();
            if &command[2..4] == "TP" {
                // The firmware refuses the pickup: a tip is already fitted.
                vec![format!("C0TPid{id}er07/00")]
            } else {
                vec![format!("{}id{id}er00/00", &command[..4])]
            }
        });
        let star = star_over(transport.clone() as Arc<dyn Transport>).expect("mock opens");
        let runs = vec![loaded(&[
            ("C0ZA", "retract"),
            (
                "C0TPxp01179 01179 00000&yp2418 2328 0000&tm1 1 0&tt01tp2244tz2164th2450td0",
                "pick up tips",
            ),
            ("C0ZA", "never reached"),
        ])];
        let outcome = execute_runs(&star, &runs, &mut |_| true, &mut |_| {})
            .expect("an abort is an outcome, not a runner failure");
        let RunOutcome::Aborted {
            run_id,
            step_index,
            error,
        } = outcome
        else {
            panic!("the firmware error aborts the run");
        };
        assert_eq!(run_id, "test_run");
        assert_eq!(step_index, 1, "the pickup was the second step");
        assert!(
            error.contains("already fitted"),
            "the typed firmware meaning survives into the report: {error}"
        );
        let written = transport.written();
        assert!(
            written
                .last()
                .expect("frames were written")
                .starts_with("C0ZAid"),
            "the runner's last act is the Z-safety retract"
        );
    }

    #[test]
    fn an_operator_decline_stops_between_runs() {
        let transport = Arc::new(MockTransport::new());
        transport.set_responder(|command| {
            let id = command.get(6..10).unwrap_or("0000").to_string();
            vec![format!("{}id{id}er00/00", &command[..4])]
        });
        let star = star_over(transport.clone() as Arc<dyn Transport>).expect("mock opens");
        let mut first = loaded(&[("C0ZA", "retract")]);
        first.manual_after.push(ManualStep {
            title: "thermocycle".into(),
            instructions: "off-deck".into(),
        });
        let second = loaded(&[("C0ZA", "never reached")]);
        let error = execute_runs(&star, &[first, second], &mut |_| false, &mut |_| {})
            .expect_err("declining the manual step stops the program");
        assert!(
            error.to_string().contains("stopped by the operator"),
            "the stop names its cause: {error}"
        );
        assert_eq!(
            transport.written().len(),
            1,
            "nothing after the declined manual step reached the wire"
        );
    }
}
