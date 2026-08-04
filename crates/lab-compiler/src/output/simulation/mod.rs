//! Symbolic execution of compiler plans.
//!
//! Simulation checks plan invariants and records the order in which symbolic
//! values would flow through the workflow. It does not simulate biological
//! outcomes or claim that an artifact would pass its acceptance criteria.

mod error;
mod trace;

pub use error::SimulationError;
pub use trace::{SimulationEvent, SimulationTrace};

use crate::ExecutablePlan;

pub fn simulate(plan: &ExecutablePlan) -> Result<SimulationTrace, SimulationError> {
    plan.validate()?;

    let events = plan
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| SimulationEvent {
            sequence: index + 1,
            step_id: step.id.clone(),
            operation: step.operation,
            inputs: step.inputs.clone(),
            outputs: step.outputs.clone(),
        })
        .collect();

    Ok(SimulationTrace {
        artifact: plan.artifact.clone(),
        lab_profile: plan.lab_profile.clone(),
        events,
        acceptance: plan.acceptance.clone(),
    })
}

#[cfg(test)]
mod tests {
    use crate::{OperationKind, PlanError, PlanStep, PlanValue};

    use super::*;

    #[test]
    fn symbolically_executes_a_valid_plan_in_order() {
        let plan = ExecutablePlan {
            artifact: "p_test".into(),
            lab_profile: "test-lab".into(),
            initial_values: vec![],
            steps: vec![PlanStep::new(
                "provision",
                OperationKind::Provision,
                Vec::<String>::new(),
                [PlanValue::material("cells")],
            )],
            acceptance: vec![],
        };

        let trace = simulate(&plan).unwrap();
        assert_eq!(trace.events.len(), 1);
        assert_eq!(trace.events[0].sequence, 1);
        assert_eq!(trace.events[0].step_id, "provision");
        assert_eq!(trace.events[0].outputs[0].name, "cells");
    }

    #[test]
    fn rejects_an_invalid_plan_before_execution() {
        let plan = ExecutablePlan {
            artifact: "p_test".into(),
            lab_profile: "test-lab".into(),
            initial_values: vec![],
            steps: vec![PlanStep::new(
                "purify",
                OperationKind::Purify,
                ["unknown"],
                [PlanValue::material("plasmid")],
            )],
            acceptance: vec![],
        };

        assert!(matches!(
            simulate(&plan),
            Err(SimulationError::InvalidPlan(PlanError::UnknownInput { .. }))
        ));
    }
}
