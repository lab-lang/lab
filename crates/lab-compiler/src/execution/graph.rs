use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{AcceptanceObligation, OperationKind, PlanError, PlanValue, ProtocolPlan};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionGraph {
    pub protocol: String,
    pub environment: String,
    pub initial_values: Vec<PlanValue>,
    pub operations: Vec<ExecutionOperation>,
    pub dependencies: Vec<ExecutionDependency>,
    pub acceptance: Vec<AcceptanceObligation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionOperation {
    pub id: String,
    pub operation: OperationKind,
    pub inputs: Vec<String>,
    pub outputs: Vec<PlanValue>,
    pub parameters: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionDependency {
    pub producer: String,
    pub consumer: String,
    pub value: String,
}

impl ExecutionGraph {
    pub fn from_protocol_plan(plan: &ProtocolPlan) -> Result<Self, PlanError> {
        plan.validate()?;

        let mut producers = BTreeMap::<String, String>::new();
        for step in &plan.steps {
            for output in &step.outputs {
                producers.insert(output.name.clone(), step.id.clone());
            }
        }

        let dependencies = plan
            .steps
            .iter()
            .flat_map(|step| {
                step.inputs.iter().filter_map(|input| {
                    producers.get(input).map(|producer| ExecutionDependency {
                        producer: producer.clone(),
                        consumer: step.id.clone(),
                        value: input.clone(),
                    })
                })
            })
            .collect();

        Ok(Self {
            protocol: plan.artifact.clone(),
            environment: plan.lab_profile.clone(),
            initial_values: plan.initial_values.clone(),
            operations: plan
                .steps
                .iter()
                .map(|step| ExecutionOperation {
                    id: step.id.clone(),
                    operation: step.operation,
                    inputs: step.inputs.clone(),
                    outputs: step.outputs.clone(),
                    parameters: step.parameters.clone(),
                })
                .collect(),
            dependencies,
            acceptance: plan.acceptance.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{OperationKind, PlanStep, PlanValue, ProtocolPlan};

    use super::*;

    #[test]
    fn records_data_dependencies_without_backend_allocations() {
        let plan = ProtocolPlan {
            artifact: "p_test".into(),
            lab_profile: "test-lab".into(),
            initial_values: vec![PlanValue::material("source")],
            steps: vec![
                PlanStep::new(
                    "purify",
                    OperationKind::Purify,
                    ["source"],
                    [PlanValue::material("purified")],
                ),
                PlanStep::new(
                    "sample",
                    OperationKind::Sample,
                    ["purified"],
                    [PlanValue::material("aliquot")],
                ),
            ],
            acceptance: vec![],
        };

        let graph = ExecutionGraph::from_protocol_plan(&plan).unwrap();
        assert_eq!(graph.operations.len(), 2);
        assert_eq!(
            graph.dependencies,
            vec![ExecutionDependency {
                producer: "purify".into(),
                consumer: "sample".into(),
                value: "purified".into(),
            }]
        );
    }
}
