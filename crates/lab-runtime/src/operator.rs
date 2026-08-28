//! The operator port: every confirmation in a run flows through one
//! interface, so the live runner asks a human at the terminal while tests
//! can supply a deterministic answer.

use std::io::{BufRead, Write};

use anyhow::Result;

/// What a confirmation is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmKind {
    /// The gate before any motion starts.
    PreRun,
    /// A material or labware movement between exact facility locations or Assets.
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

/// An operator that always answers the same way, for tests and programmatic
/// callers.
pub struct AutoOperator {
    pub answer: bool,
}

impl Operator for AutoOperator {
    fn confirm(&mut self, _kind: ConfirmKind, _prompt: &str) -> Result<bool> {
        Ok(self.answer)
    }
}
