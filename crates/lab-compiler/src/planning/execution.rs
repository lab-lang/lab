//! Projection of facility allocation into the reviewed generic execution-plan format.

use std::collections::{BTreeMap, BTreeSet};

use lab_capability::{ControlMode, ScalarValue};
use lab_method::ProcedureValue;
use lab_runfmt::{
    EXECUTION_PLAN_FORMAT, ExecutionAdapterBinding, ExecutionInventoryReference,
    ExecutionLoweringBundle, ExecutionMaterialBinding, ExecutionParameterBinding,
    ExecutionParameterValue, ExecutionPlanAction, ExecutionPlanDocument, ExecutionPlanNode,
    ExecutionPlanningReference, ExecutionRequirementBinding, ReviewedRunDocument,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{AdapterInvocationPlan, AllocatedProcedureTask, AllocatedRequirementBinding};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPlanOptions {
    /// Package-relative copy of the exact inventory graph reviewed with the plan.
    pub inventory_document: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<ExecutionMaterialBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<lab_runfmt::ExecutionMaterialOutput>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub reviewed_documents: BTreeMap<String, ReviewedRunDocument>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lowerings: Vec<ExecutionLoweringBundle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planning: Option<ExecutionPlanningReference>,
}

impl Default for ExecutionPlanOptions {
    fn default() -> Self {
        Self {
            inventory_document: "inventory-source.ttl".to_owned(),
            materials: Vec::new(),
            outputs: Vec::new(),
            reviewed_documents: BTreeMap::new(),
            lowerings: Vec::new(),
            planning: None,
        }
    }
}

/// Build a reviewed plan from the exact selected Method graph and adapter invocations.
pub fn build_execution_plan_from_invocations(
    invocations: &AdapterInvocationPlan,
    mut options: ExecutionPlanOptions,
) -> Result<ExecutionPlanDocument, ExecutionPlanBuildError> {
    invocations
        .validate()
        .map_err(|error| ExecutionPlanBuildError::InvalidInvocations(error.to_string()))?;
    let planning = options
        .planning
        .take()
        .ok_or(ExecutionPlanBuildError::MissingPlanningReference)?;
    if planning.problem_sha256 != invocations.problem_sha256
        || planning.allocated_lair_sha256 != invocations.allocated_lair_sha256
    {
        return Err(ExecutionPlanBuildError::PlanningReferenceMismatch);
    }
    let expected_methods = invocations
        .methods
        .iter()
        .map(|method| {
            (
                method.choice.as_str(),
                method.source_operation.as_str(),
                method.method.as_str(),
                method
                    .tasks
                    .iter()
                    .map(|task| task.id.as_str())
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    let planned_methods = planning
        .methods
        .iter()
        .map(|method| {
            (
                method.choice.as_str(),
                method.source_operation.as_str(),
                method.method.as_str(),
                method.tasks.iter().map(String::as_str).collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    if planned_methods != expected_methods {
        return Err(ExecutionPlanBuildError::PlanningReferenceMismatch);
    }

    let mut requirements = Vec::new();
    let mut nodes = Vec::new();
    let mut previous = None;
    for method in &invocations.methods {
        for task in &method.tasks {
            for binding in &task.requirements {
                let requirement = binding.id.to_string();
                let document = options.reviewed_documents.remove(&requirement);
                let is_manual = binding.control_mode == ControlMode::Manual.iri();
                if is_manual && document.is_some() {
                    return Err(ExecutionPlanBuildError::ManualRequirementDocument { requirement });
                }
                if is_manual && binding.adapter.is_some() {
                    return Err(ExecutionPlanBuildError::ManualRequirementAdapter { requirement });
                }
                if let Some(document) = &document {
                    let Some(adapter) = binding.adapter.as_ref() else {
                        return Err(ExecutionPlanBuildError::DocumentWithoutAdapter {
                            requirement,
                        });
                    };
                    if !adapter.emitted_run_formats.contains(&document.format) {
                        return Err(ExecutionPlanBuildError::UnsupportedDocumentFormat {
                            requirement,
                            driver: adapter.driver.clone(),
                            format: document.format.clone(),
                            supported: render_set(&adapter.emitted_run_formats),
                        });
                    }
                }
                let execution_binding = ExecutionRequirementBinding {
                    requirement_instance: requirement.clone(),
                    requirement_template: format!("{}::{}", method.method, binding.id),
                    capability_kind: binding.capability_kind.to_string(),
                    offering: binding.offering.clone(),
                    asset: binding.asset.clone(),
                    minimum_qualification: binding.minimum_qualification.iri().to_owned(),
                    observed_qualification: binding.observed_qualification.clone(),
                    control_mode: binding.control_mode.clone(),
                    parameters: binding
                        .parameters
                        .iter()
                        .map(|parameter| ExecutionParameterBinding {
                            argument: parameter.property_kind.to_string(),
                            property_kind: parameter.property_kind.to_string(),
                            relation: parameter.relation.to_string(),
                            required: semantic_value(&parameter.required.value),
                            required_unit: parameter
                                .required
                                .unit
                                .as_ref()
                                .map(ToString::to_string),
                            offering_parameter: parameter.offering_parameter.clone(),
                            observed: semantic_value(&parameter.observed.value),
                            observed_unit: parameter
                                .observed
                                .unit
                                .as_ref()
                                .map(ToString::to_string),
                        })
                        .collect(),
                    adapter: binding
                        .adapter
                        .as_ref()
                        .map(|adapter| ExecutionAdapterBinding {
                            driver: adapter.driver.clone(),
                            profile_path: adapter.profile_path.to_string_lossy().into_owned(),
                            profile_sha256: adapter.profile_sha256.clone(),
                        }),
                };
                requirements.push(execution_binding);
                let (id, action) = if is_manual {
                    (
                        format!("manual-{:04}", nodes.len() + 1),
                        ExecutionPlanAction::Manual {
                            requirement,
                            title: format!("Perform {}", task.operation),
                            instructions: manual_instructions(task, binding),
                        },
                    )
                } else {
                    (
                        format!("execute-{:04}", nodes.len() + 1),
                        ExecutionPlanAction::Execute {
                            requirement,
                            document,
                        },
                    )
                };
                nodes.push(ExecutionPlanNode {
                    id: id.clone(),
                    after: previous.into_iter().collect(),
                    action,
                });
                previous = Some(id);
            }
        }
    }
    if let Some(requirement) = options.reviewed_documents.keys().next() {
        return Err(ExecutionPlanBuildError::UnknownDocumentRequirement {
            requirement: requirement.clone(),
        });
    }
    let plan = ExecutionPlanDocument {
        format: EXECUTION_PLAN_FORMAT.to_owned(),
        inventory: ExecutionInventoryReference {
            document: options.inventory_document,
            source_sha256: invocations.inventory_sha256.clone(),
            facility: invocations.facility.clone(),
        },
        planning: Some(planning),
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
    #[error("adapter invocation plan is invalid: {0}")]
    InvalidInvocations(String),
    #[error("compiler-derived execution planning requires frozen compiler artifacts")]
    MissingPlanningReference,
    #[error("frozen compiler artifacts do not match the adapter invocation plan")]
    PlanningReferenceMismatch,
    #[error("requirement `{requirement}` has a reviewed run document but no allocated adapter")]
    DocumentWithoutAdapter { requirement: String },
    #[error("manual-control requirement `{requirement}` cannot have a reviewed run document")]
    ManualRequirementDocument { requirement: String },
    #[error("manual-control requirement `{requirement}` cannot have a runtime adapter")]
    ManualRequirementAdapter { requirement: String },
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
    #[error("constructed execution plan is invalid: {0}")]
    InvalidPlan(String),
}

fn semantic_value(value: &lab_capability::ScalarValue) -> ExecutionParameterValue {
    match value {
        lab_capability::ScalarValue::Text(value) => ExecutionParameterValue::Text(value.clone()),
        lab_capability::ScalarValue::Integer(value) => {
            ExecutionParameterValue::Integer(value.to_string())
        }
        lab_capability::ScalarValue::Real(value) => {
            ExecutionParameterValue::Real(value.to_string())
        }
        lab_capability::ScalarValue::Boolean(value) => ExecutionParameterValue::Boolean(*value),
        lab_capability::ScalarValue::Iri(value) => ExecutionParameterValue::Iri(value.to_string()),
    }
}

fn manual_instructions(
    task: &AllocatedProcedureTask,
    binding: &AllocatedRequirementBinding,
) -> String {
    let mut instructions = format!(
        "Use CapabilityOffering '{}' on Asset '{}' to perform Procedure operation '{}'. Follow the facility's reviewed local SOP for this operation and confirm completion.",
        binding.offering, binding.asset, task.operation
    );
    if !task.parameters.is_empty() {
        let parameters = task
            .parameters
            .iter()
            .map(|parameter| {
                format!(
                    "{} ({}) = {}",
                    parameter.id,
                    parameter.property_kind,
                    render_procedure_value(&parameter.value)
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        instructions.push_str(" Procedure parameters: ");
        instructions.push_str(&parameters);
        instructions.push('.');
    }
    instructions
}

fn render_procedure_value(value: &ProcedureValue) -> String {
    match value {
        ProcedureValue::Scalar { value } => render_property_value(value),
        ProcedureValue::List { values, .. } => format!(
            "[{}]",
            values
                .iter()
                .map(render_property_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn render_property_value(value: &lab_capability::PropertyValue) -> String {
    let scalar = match &value.value {
        ScalarValue::Text(value) => {
            serde_json::to_string(value).expect("a scalar string is always representable as JSON")
        }
        ScalarValue::Integer(value) => value.to_string(),
        ScalarValue::Real(value) => value.to_string(),
        ScalarValue::Boolean(value) => value.to_string(),
        ScalarValue::Iri(value) => format!("<{value}>"),
    };
    match &value.unit {
        Some(unit) => format!("{scalar} <{unit}>"),
        None => scalar,
    }
}

fn render_set(values: &BTreeSet<String>) -> String {
    if values.is_empty() {
        "none".to_owned()
    } else {
        values.iter().cloned().collect::<Vec<_>>().join(", ")
    }
}
