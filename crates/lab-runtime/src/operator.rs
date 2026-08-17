//! The operator port: every confirmation in a run flows through one
//! interface, so the live runner asks a human at the terminal and the
//! simulator answers for a modeled one.

use std::io::{BufRead, Write};

use anyhow::Result;

/// What a confirmation is for. The live operator sees the same prompt
/// either way; a simulated operator charges different time for different
/// kinds of step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmKind {
    /// The gate before any motion starts.
    PreRun,
    /// A labware movement between stations.
    Handoff,
    /// A by-hand step that is not a movement.
    Manual,
}

pub trait Operator {
    fn confirm(&mut self, kind: ConfirmKind, prompt: &str) -> Result<bool>;
}

/// The terminal operator: prints the prompt and reads one line. `y`, `Y`,
/// and `yes` confirm; anything else declines.
pub struct StdinOperator;

impl Operator for StdinOperator {
    fn confirm(&mut self, _kind: ConfirmKind, prompt: &str) -> Result<bool> {
        print!("{prompt}");
        std::io::stdout().flush()?;
        let mut answer = String::new();
        std::io::stdin().lock().read_line(&mut answer)?;
        Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
    }
}

/// An operator that always answers the same way. The simulator confirms
/// every step with it; tests decline with it.
pub struct AutoOperator {
    pub answer: bool,
}

impl Operator for AutoOperator {
    fn confirm(&mut self, _kind: ConfirmKind, _prompt: &str) -> Result<bool> {
        Ok(self.answer)
    }
}
