//! The `lab run` command for a Hamilton STAR package: terminal presentation
//! over the runtime's loader and replay loop.

use std::path::PathBuf;

use anyhow::{Result, bail};
use lab_runtime::operator::{ConfirmKind, Operator, StdinOperator};
use lab_runtime::star::{RunOutcome, execute_runs, load_run_directory, render_dry_run};

use crate::Output;

pub(crate) fn run(directory: PathBuf, dry_run: bool, yes: bool, output: &Output) -> Result<()> {
    let (runs, autoload_park_track) = load_run_directory(&directory)?;
    let total_steps: usize = runs.iter().map(|run| run.steps.len()).sum();

    if dry_run {
        return output.success(
            "dry-run",
            serde_json::json!({
                "runs": runs.len(),
                "steps": total_steps,
            }),
            render_dry_run(&runs),
        );
    }

    println!(
        "about to execute {} run document(s), {} frames, on the first Hamilton STAR on USB",
        runs.len(),
        total_steps
    );
    let mut operator = StdinOperator;
    if !yes
        && !operator.confirm(
            ConfirmKind::PreRun,
            "proceed? The machine will move. [y/N] ",
        )?
    {
        bail!("run cancelled before any motion");
    }

    let star = lab_runtime::star::open_usb_star(autoload_park_track)?;
    println!("connected; the setup choreography has run");

    let mut pause = |prompt: &str| {
        println!("\nby hand — {prompt}");
        StdinOperator
            .confirm(
                ConfirmKind::Manual,
                "done, and the bench matches the plan? Continue [y/N] ",
            )
            .unwrap_or(false)
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
