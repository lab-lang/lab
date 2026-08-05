//! Simulation of backend-neutral execution graphs.
//!
//! The initial engine models symbolic material and evidence flow. Its public
//! state and trace boundary is intentionally independent of any robot so that
//! timing, containers, liquids, devices, and backend adapters can grow here.

mod error;
mod execution;
mod trace;

pub use error::SimulationError;
pub use execution::{ExecutionDependency, ExecutionGraph, ExecutionOperation};
pub use trace::{LabState, SimulatedValue, SimulationEvent, SimulationTrace};

use crate::ValueKind;

pub fn simulate(graph: &ExecutionGraph) -> Result<SimulationTrace, SimulationError> {
    let mut state = LabState::default();
    for value in &graph.initial_values {
        state.values.insert(
            value.name.clone(),
            SimulatedValue {
                kind: value.kind,
                available: true,
                consumed: false,
            },
        );
    }

    let mut events = Vec::new();
    for (index, operation) in graph.operations.iter().enumerate() {
        for input in &operation.inputs {
            let value =
                state
                    .values
                    .get_mut(input)
                    .ok_or_else(|| SimulationError::UnavailableValue {
                        operation: operation.id.clone(),
                        value: input.clone(),
                    })?;
            if !value.available {
                return Err(SimulationError::UnavailableValue {
                    operation: operation.id.clone(),
                    value: input.clone(),
                });
            }
            if value.kind == ValueKind::Material {
                value.available = false;
                value.consumed = true;
            }
        }
        for output in &operation.outputs {
            state.values.insert(
                output.name.clone(),
                SimulatedValue {
                    kind: output.kind,
                    available: true,
                    consumed: false,
                },
            );
        }
        events.push(SimulationEvent {
            sequence: index + 1,
            step_id: operation.id.clone(),
            operation: operation.operation,
            inputs: operation.inputs.clone(),
            outputs: operation.outputs.clone(),
        });
    }

    Ok(SimulationTrace {
        protocol: graph.protocol.clone(),
        environment: graph.environment.clone(),
        events,
        final_state: state,
        acceptance: graph.acceptance.clone(),
    })
}

#[cfg(test)]
mod tests {
    use crate::{ExecutionGraph, OperationKind, PlanStep, PlanValue, ProtocolPlan};

    use crate::simulation::*;

    #[test]
    fn symbolically_executes_a_valid_plan_in_order() {
        let plan = ProtocolPlan {
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

        let graph = ExecutionGraph::from_protocol_plan(&plan).unwrap();
        let trace = simulate(&graph).unwrap();
        assert_eq!(trace.events.len(), 1);
        assert_eq!(trace.events[0].sequence, 1);
        assert_eq!(trace.events[0].step_id, "provision");
        assert_eq!(trace.events[0].outputs[0].name, "cells");
    }

    #[test]
    fn rejects_an_invalid_plan_before_execution() {
        let graph = ExecutionGraph {
            protocol: "p_test".into(),
            environment: "test-lab".into(),
            initial_values: vec![],
            operations: vec![crate::ExecutionOperation {
                id: "purify".into(),
                operation: OperationKind::Purify,
                inputs: vec!["unknown".into()],
                outputs: vec![PlanValue::material("plasmid")],
                parameters: Default::default(),
            }],
            dependencies: vec![],
            acceptance: vec![],
        };

        assert_eq!(
            simulate(&graph),
            Err(SimulationError::UnavailableValue {
                operation: "purify".into(),
                value: "unknown".into(),
            })
        );
    }
}
