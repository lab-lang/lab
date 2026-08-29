//! Projection of facility allocation into the reviewed generic execution-plan format.

use std::collections::{BTreeMap, BTreeSet};

use lab_language::{CheckedExpression, TypedExpression};
use lab_runfmt::{
    EXECUTION_PLAN_FORMAT, ExecutionAdapterBinding, ExecutionInventoryReference,
    ExecutionLoweringBundle, ExecutionMaterialBinding, ExecutionParameterBinding,
    ExecutionParameterValue, ExecutionPlanAction, ExecutionPlanDocument, ExecutionPlanNode,
    ExecutionRequirementBinding, ReviewedRunDocument,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{
    AllocationScalarValue, FACILITY_ALLOCATION_SCHEMA_VERSION, FacilityAllocation,
    ParameterRelation,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPlanOptions {
    /// Package-relative copy of the exact inventory graph reviewed with the plan.
    pub inventory_document: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<ExecutionMaterialBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<lab_runfmt::ExecutionMaterialOutput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub movements: Vec<PlannedMaterialMove>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub reviewed_documents: BTreeMap<String, ReviewedRunDocument>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lowerings: Vec<ExecutionLoweringBundle>,
}

impl Default for ExecutionPlanOptions {
    fn default() -> Self {
        Self {
            inventory_document: "inventory-source.ttl".to_owned(),
            materials: Vec::new(),
            outputs: Vec::new(),
            movements: Vec::new(),
            reviewed_documents: BTreeMap::new(),
            lowerings: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedMaterialMove {
    pub id: String,
    pub material: String,
    pub from: String,
    pub to: String,
    pub instructions: String,
    pub after_requirement: String,
    pub before_requirement: String,
}

pub fn build_execution_plan(
    allocation: &FacilityAllocation,
    mut options: ExecutionPlanOptions,
) -> Result<ExecutionPlanDocument, ExecutionPlanBuildError> {
    if allocation.schema_version != FACILITY_ALLOCATION_SCHEMA_VERSION {
        return Err(ExecutionPlanBuildError::WrongAllocationSchema {
            found: allocation.schema_version.clone(),
        });
    }
    let mut requirement_nodes = BTreeMap::new();
    let mut requirements = Vec::new();
    let mut nodes = Vec::new();
    let mut previous = None;
    for (index, selected) in allocation.allocations.iter().enumerate() {
        let id = format!("execute-{:04}", index + 1);
        requirement_nodes.insert(selected.requirement_instance.clone(), id.clone());
        let document = options
            .reviewed_documents
            .remove(&selected.requirement_instance);
        if let Some(document) = &document {
            let Some(adapter) = selected.adapter.as_ref() else {
                return Err(ExecutionPlanBuildError::DocumentWithoutAdapter {
                    requirement: selected.requirement_instance.clone(),
                });
            };
            if !adapter.emitted_run_formats.contains(&document.format) {
                return Err(ExecutionPlanBuildError::UnsupportedDocumentFormat {
                    requirement: selected.requirement_instance.clone(),
                    driver: adapter.driver.clone(),
                    format: document.format.clone(),
                    supported: render_set(&adapter.emitted_run_formats),
                });
            }
        }
        requirements.push(ExecutionRequirementBinding {
            requirement_instance: selected.requirement_instance.clone(),
            requirement_template: selected.requirement_template.clone(),
            capability_kind: selected.capability_kind.clone(),
            offering: selected.offering.clone(),
            asset: selected.asset.clone(),
            minimum_qualification: selected.minimum_qualification.clone(),
            observed_qualification: selected.observed_qualification.clone(),
            control_mode: selected.control_mode.clone(),
            parameters: selected
                .parameters
                .iter()
                .map(|parameter| {
                    Ok(ExecutionParameterBinding {
                        argument: parameter.argument.clone(),
                        property_kind: parameter.property_kind.clone(),
                        relation: match parameter.relation {
                            ParameterRelation::Exact => "exact".to_owned(),
                            ParameterRelation::AtLeast => "at_least".to_owned(),
                            ParameterRelation::AtMost => "at_most".to_owned(),
                        },
                        required: requirement_value(&parameter.required)?,
                        required_unit: parameter.required_unit.clone(),
                        offering_parameter: parameter.offering_parameter.clone(),
                        observed: observed_value(&parameter.observed),
                        observed_unit: parameter.observed_unit.clone(),
                    })
                })
                .collect::<Result<Vec<_>, ExecutionPlanBuildError>>()?,
            adapter: selected
                .adapter
                .as_ref()
                .map(|adapter| {
                    let profile_path = adapter.profile_path.to_str().ok_or_else(|| {
                        ExecutionPlanBuildError::NonUtf8ProfilePath {
                            driver: adapter.driver.clone(),
                        }
                    })?;
                    Ok(ExecutionAdapterBinding {
                        driver: adapter.driver.clone(),
                        profile_path: profile_path.to_owned(),
                        profile_sha256: adapter.profile_sha256.clone(),
                    })
                })
                .transpose()?,
        });
        nodes.push(ExecutionPlanNode {
            id: id.clone(),
            after: previous.into_iter().collect(),
            action: ExecutionPlanAction::Execute {
                requirement: selected.requirement_instance.clone(),
                document,
            },
        });
        previous = Some(id);
    }
    if let Some(requirement) = options.reviewed_documents.keys().next() {
        return Err(ExecutionPlanBuildError::UnknownDocumentRequirement {
            requirement: requirement.clone(),
        });
    }

    for movement in options.movements {
        let after = requirement_nodes
            .get(&movement.after_requirement)
            .ok_or_else(|| ExecutionPlanBuildError::UnknownMovementRequirement {
                movement: movement.id.clone(),
                requirement: movement.after_requirement.clone(),
            })?
            .clone();
        let before = requirement_nodes
            .get(&movement.before_requirement)
            .ok_or_else(|| ExecutionPlanBuildError::UnknownMovementRequirement {
                movement: movement.id.clone(),
                requirement: movement.before_requirement.clone(),
            })?
            .clone();
        let before_node = nodes
            .iter_mut()
            .find(|node| node.id == before)
            .expect("requirement node map points into the node list");
        if !before_node.after.contains(&movement.id) {
            before_node.after.push(movement.id.clone());
            before_node.after.sort();
        }
        nodes.push(ExecutionPlanNode {
            id: movement.id,
            after: vec![after],
            action: ExecutionPlanAction::MoveMaterial {
                material: movement.material,
                from: movement.from,
                to: movement.to,
                instructions: movement.instructions,
            },
        });
    }

    let plan = ExecutionPlanDocument {
        format: EXECUTION_PLAN_FORMAT.to_owned(),
        inventory: ExecutionInventoryReference {
            document: options.inventory_document,
            source_sha256: allocation.inventory_sha256.clone(),
            facility: allocation.facility.clone(),
        },
        requirements,
        materials: options.materials,
        outputs: options.outputs,
        lowerings: options.lowerings,
        nodes,
    };
    plan.validate()
        .map_err(ExecutionPlanBuildError::InvalidPlan)?;
    Ok(plan)
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExecutionPlanBuildError {
    #[error(
        "facility allocation declares schema `{found}`, expected `{FACILITY_ALLOCATION_SCHEMA_VERSION}`"
    )]
    WrongAllocationSchema { found: String },
    #[error("requirement `{requirement}` has a reviewed run document but no allocated adapter")]
    DocumentWithoutAdapter { requirement: String },
    #[error(
        "adapter `{driver}` for requirement `{requirement}` does not emit `{format}`; supported formats: {supported}"
    )]
    UnsupportedDocumentFormat {
        requirement: String,
        driver: String,
        format: String,
        supported: String,
    },
    #[error("reviewed run document references unknown requirement `{requirement}`")]
    UnknownDocumentRequirement { requirement: String },
    #[error("adapter `{driver}` has a non-UTF-8 profile path")]
    NonUtf8ProfilePath { driver: String },
    #[error("material movement `{movement}` references unknown requirement `{requirement}`")]
    UnknownMovementRequirement {
        movement: String,
        requirement: String,
    },
    #[error("requirement parameter uses a dynamic value that cannot enter a reviewed plan")]
    DynamicParameter,
    #[error("constructed execution plan is invalid: {0}")]
    InvalidPlan(String),
}

fn requirement_value(
    value: &TypedExpression,
) -> Result<ExecutionParameterValue, ExecutionPlanBuildError> {
    match &value.value {
        CheckedExpression::Integer { value } => {
            Ok(ExecutionParameterValue::Integer(value.to_string()))
        }
        CheckedExpression::Decimal { text } => Ok(ExecutionParameterValue::Real(text.clone())),
        CheckedExpression::String { value } => Ok(ExecutionParameterValue::Text(value.clone())),
        CheckedExpression::Quantity { magnitude, .. } => Ok(if magnitude.parse::<i128>().is_ok() {
            ExecutionParameterValue::Integer(magnitude.clone())
        } else {
            ExecutionParameterValue::Real(magnitude.clone())
        }),
        CheckedExpression::Unary { operator, operand } if operator == "negate" => {
            match requirement_value(operand)? {
                ExecutionParameterValue::Integer(value) => {
                    Ok(ExecutionParameterValue::Integer(format!("-{value}")))
                }
                ExecutionParameterValue::Real(value) => {
                    Ok(ExecutionParameterValue::Real(format!("-{value}")))
                }
                ExecutionParameterValue::Text(_)
                | ExecutionParameterValue::Boolean(_)
                | ExecutionParameterValue::Iri(_) => Err(ExecutionPlanBuildError::DynamicParameter),
            }
        }
        CheckedExpression::Reference { .. }
        | CheckedExpression::List { .. }
        | CheckedExpression::Call { .. }
        | CheckedExpression::Construct { .. }
        | CheckedExpression::Field { .. }
        | CheckedExpression::Unary { .. }
        | CheckedExpression::Binary { .. } => Err(ExecutionPlanBuildError::DynamicParameter),
    }
}

fn observed_value(value: &AllocationScalarValue) -> ExecutionParameterValue {
    match value {
        AllocationScalarValue::Text { value } => ExecutionParameterValue::Text(value.clone()),
        AllocationScalarValue::Integer { value } => ExecutionParameterValue::Integer(value.clone()),
        AllocationScalarValue::Real { value } => ExecutionParameterValue::Real(value.clone()),
        AllocationScalarValue::Boolean { value } => ExecutionParameterValue::Boolean(*value),
        AllocationScalarValue::Iri { value } => ExecutionParameterValue::Iri(value.clone()),
    }
}

fn render_set(values: &BTreeSet<String>) -> String {
    if values.is_empty() {
        "none".to_owned()
    } else {
        values.iter().cloned().collect::<Vec<_>>().join(", ")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use lab_runfmt::ExecutionMaterialBinding;

    use crate::planning::{AllocatedAdapter, RequirementAllocation};

    use super::*;

    fn allocation() -> FacilityAllocation {
        FacilityAllocation {
            schema_version: FACILITY_ALLOCATION_SCHEMA_VERSION.to_owned(),
            inventory_sha256: "a".repeat(64),
            facility: "https://example.org/facility".to_owned(),
            requirements_schema_version: "lab.capability-requirements.v2".to_owned(),
            instances_schema_version: "lab.capability-requirement-instances.v2".to_owned(),
            allocations: ["liquid", "read"]
                .into_iter()
                .enumerate()
                .map(|(index, name)| RequirementAllocation {
                    requirement_instance: format!("example::main/{name}"),
                    requirement_template: format!("example::main::{name}"),
                    capability_kind: format!("https://example.org/capability/{name}"),
                    minimum_qualification: "https://sbol.io/ns/facility#Plannable".to_owned(),
                    accepted_control_modes: BTreeSet::new(),
                    offering: format!("https://example.org/{name}/offering"),
                    asset: format!("https://example.org/{name}"),
                    observed_qualification: "https://sbol.io/ns/facility#Executable".to_owned(),
                    control_mode: "https://sbol.io/ns/facility#ReviewedFileControl".to_owned(),
                    parameters: Vec::new(),
                    adapter: Some(AllocatedAdapter {
                        driver: format!("example.{name}"),
                        profile_path: PathBuf::from(format!("adapters/{name}.toml")),
                        profile_sha256: if index == 0 { "b" } else { "c" }.repeat(64),
                        features: BTreeSet::new(),
                        accepted_run_formats: BTreeSet::new(),
                        emitted_run_formats: [format!("example.{name}.v1")].into_iter().collect(),
                    }),
                    rejected_candidates: Vec::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn projects_exact_bindings_and_explicit_material_movement_into_a_valid_dag() {
        let allocation = allocation();
        let material = ExecutionMaterialBinding {
            id: "plate".to_owned(),
            component: "https://example.org/design".to_owned(),
            material_lot: "https://example.org/lot".to_owned(),
        };
        let plan = build_execution_plan(
            &allocation,
            ExecutionPlanOptions {
                inventory_document: "inventory-source.ttl".to_owned(),
                materials: vec![material],
                outputs: Vec::new(),
                movements: vec![PlannedMaterialMove {
                    id: "move-plate".to_owned(),
                    material: "plate".to_owned(),
                    from: "https://example.org/liquid".to_owned(),
                    to: "https://example.org/read".to_owned(),
                    instructions: "Move the plate to the reader.".to_owned(),
                    after_requirement: "example::main/liquid".to_owned(),
                    before_requirement: "example::main/read".to_owned(),
                }],
                reviewed_documents: BTreeMap::new(),
                lowerings: Vec::new(),
            },
        )
        .unwrap();

        assert_eq!(plan.requirements.len(), 2);
        assert_eq!(plan.materials.len(), 1);
        assert_eq!(plan.nodes.len(), 3);
        assert!(matches!(
            plan.nodes[2].action,
            ExecutionPlanAction::MoveMaterial { .. }
        ));
        assert!(plan.nodes[1].after.contains(&"move-plate".to_owned()));
        plan.validate().unwrap();
    }

    #[test]
    fn reviewed_child_documents_must_match_the_frozen_adapter_contract() {
        let allocation = allocation();
        let documents = [(
            "example::main/liquid".to_owned(),
            ReviewedRunDocument {
                path: "runs/liquid.json".to_owned(),
                format: "wrong.format".to_owned(),
                sha256: "d".repeat(64),
            },
        )]
        .into_iter()
        .collect();

        let error = build_execution_plan(
            &allocation,
            ExecutionPlanOptions {
                inventory_document: "inventory-source.ttl".to_owned(),
                reviewed_documents: documents,
                ..ExecutionPlanOptions::default()
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ExecutionPlanBuildError::UnsupportedDocumentFormat { .. }
        ));
    }
}
