//! Projection of facility allocation into the reviewed generic execution-plan format.

use std::collections::{BTreeMap, BTreeSet};

use lab_adapters::AdapterInvocationPlan;
use lab_capability::{ControlMode, ScalarValue};
use lab_compiler::allocation::{
    AllocatedMethod, AllocatedProcedureTask, AllocatedRequirementBinding,
};
use lab_compiler::method::ProcedureValue;
use lab_compiler::planning::{PlanningValueSource, SelectedMaterialSource};
use lab_compiler::procedure::BindingScope;
use lab_runfmt::{
    EXECUTION_PLAN_FORMAT, ExecutionAdapterBinding, ExecutionInventoryReference,
    ExecutionMaterialBinding, ExecutionParameterBinding, ExecutionParameterValue,
    ExecutionPlanAction, ExecutionPlanDocument, ExecutionPlanNode, ExecutionPlanningReference,
    ExecutionRequirementBinding, ReviewedRunDocument,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

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
    if planning.problem_sha256 != invocations.allocated.problem_sha256
        || planning.allocated_lair_sha256 != invocations.allocated_lair_sha256
    {
        return Err(ExecutionPlanBuildError::PlanningReferenceMismatch);
    }
    let expected_methods = invocations
        .allocated
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

    options.materials.extend(
        invocations
            .allocated
            .methods
            .iter()
            .flat_map(|method| &method.tasks)
            .flat_map(|task| &task.materials)
            .filter_map(|material| {
                let SelectedMaterialSource::MaterialLot {
                    component,
                    material_lot,
                } = &material.source
                else {
                    return None;
                };
                Some(ExecutionMaterialBinding {
                    id: material.input.to_string(),
                    component: component.clone(),
                    material_lot: material_lot.clone(),
                })
            }),
    );
    options
        .materials
        .sort_by(|left, right| left.id.cmp(&right.id));

    let mut requirements = Vec::new();
    let mut nodes = Vec::<ExecutionPlanNode>::new();
    let mut node_tasks = Vec::<BTreeSet<lab_compiler::method::LocalId>>::new();
    let mut task_nodes = BTreeMap::<lab_compiler::method::LocalId, Vec<String>>::new();
    let mut document_nodes = BTreeMap::<(String, String, String), usize>::new();
    for method in &invocations.allocated.methods {
        for task in &method.tasks {
            let binding_scope =
                task.program
                    .as_ref()
                    .map_or(BindingScope::Independent, |program| {
                        program
                            .validate()
                            .expect("adapter invocation validation checked the Procedure program")
                            .capability_formula()
                            .binding_scope
                    });
            let mut task_requirements = Vec::new();
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
                    procedure_implementation: binding
                        .procedure_implementation
                        .as_ref()
                        .map(ToString::to_string),
                    parameters: binding
                        .parameters
                        .iter()
                        .map(|parameter| ExecutionParameterBinding {
                            argument: parameter.property_kind.to_string(),
                            property_kind: parameter.property_kind.to_string(),
                            relation: parameter.relation,
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
                task_requirements.push((requirement, binding, is_manual, document));
            }

            if binding_scope == BindingScope::AtomicAssetAssembly {
                let has_manual = task_requirements
                    .iter()
                    .any(|(_, _, is_manual, _)| *is_manual);
                let all_manual = !task_requirements.is_empty()
                    && task_requirements
                        .iter()
                        .all(|(_, _, is_manual, _)| *is_manual);
                if has_manual && !all_manual {
                    return Err(ExecutionPlanBuildError::UnsupportedAtomicExecution {
                        task: task.id.to_string(),
                        message: "one atomic task mixes manual and adapter-controlled requirements"
                            .to_owned(),
                    });
                }
                if all_manual {
                    let id = format!("manual-{:04}", nodes.len() + 1);
                    nodes.push(ExecutionPlanNode {
                        id: id.clone(),
                        after: Vec::new(),
                        action: ExecutionPlanAction::Manual {
                            requirements: task_requirements
                                .iter()
                                .map(|(requirement, _, _, _)| requirement.clone())
                                .collect(),
                            title: format!("Perform {}", task.operation),
                            instructions: manual_instructions(
                                task,
                                task_requirements.iter().map(|(_, binding, _, _)| *binding),
                            ),
                        },
                    });
                    node_tasks.push(BTreeSet::from([task.id.clone()]));
                    task_nodes.entry(task.id.clone()).or_default().push(id);
                    continue;
                }
                let document = task_requirements
                    .first()
                    .and_then(|(_, _, _, document)| document.clone());
                if task_requirements
                    .iter()
                    .any(|(_, _, _, candidate)| candidate != &document)
                {
                    return Err(ExecutionPlanBuildError::UnsupportedAtomicExecution {
                        task: task.id.to_string(),
                        message: "every requirement must reference the same reviewed run document"
                            .to_owned(),
                    });
                }
                if let Some(document) = &document {
                    let key = (
                        document.path.clone(),
                        document.format.clone(),
                        document.sha256.clone(),
                    );
                    if let Some(index) = document_nodes.get(&key).copied() {
                        let ExecutionPlanAction::Execute { requirements, .. } =
                            &mut nodes[index].action
                        else {
                            unreachable!("reviewed documents index only execute nodes")
                        };
                        requirements.extend(
                            task_requirements
                                .iter()
                                .map(|(requirement, _, _, _)| requirement.clone()),
                        );
                        node_tasks[index].insert(task.id.clone());
                        task_nodes
                            .entry(task.id.clone())
                            .or_default()
                            .push(nodes[index].id.clone());
                        continue;
                    }
                }
                let id = format!("execute-{:04}", nodes.len() + 1);
                nodes.push(ExecutionPlanNode {
                    id: id.clone(),
                    after: Vec::new(),
                    action: ExecutionPlanAction::Execute {
                        requirements: task_requirements
                            .iter()
                            .map(|(requirement, _, _, _)| requirement.clone())
                            .collect(),
                        document,
                    },
                });
                node_tasks.push(BTreeSet::from([task.id.clone()]));
                if let ExecutionPlanAction::Execute {
                    document: Some(document),
                    ..
                } = &nodes
                    .last()
                    .expect("the execute node was just pushed")
                    .action
                {
                    document_nodes.insert(
                        (
                            document.path.clone(),
                            document.format.clone(),
                            document.sha256.clone(),
                        ),
                        nodes.len() - 1,
                    );
                }
                task_nodes.entry(task.id.clone()).or_default().push(id);
                continue;
            }

            for (requirement, binding, is_manual, document) in task_requirements {
                if !is_manual && let Some(document) = &document {
                    let key = (
                        document.path.clone(),
                        document.format.clone(),
                        document.sha256.clone(),
                    );
                    if let Some(index) = document_nodes.get(&key).copied() {
                        let ExecutionPlanAction::Execute { requirements, .. } =
                            &mut nodes[index].action
                        else {
                            unreachable!("reviewed documents index only execute nodes")
                        };
                        requirements.push(requirement);
                        node_tasks[index].insert(task.id.clone());
                        task_nodes
                            .entry(task.id.clone())
                            .or_default()
                            .push(nodes[index].id.clone());
                        continue;
                    }
                }
                let (id, action) = if is_manual {
                    (
                        format!("manual-{:04}", nodes.len() + 1),
                        ExecutionPlanAction::Manual {
                            requirements: vec![requirement],
                            title: format!("Perform {}", task.operation),
                            instructions: manual_instructions(task, std::iter::once(binding)),
                        },
                    )
                } else {
                    (
                        format!("execute-{:04}", nodes.len() + 1),
                        ExecutionPlanAction::Execute {
                            requirements: vec![requirement],
                            document,
                        },
                    )
                };
                nodes.push(ExecutionPlanNode {
                    id: id.clone(),
                    after: Vec::new(),
                    action,
                });
                node_tasks.push(BTreeSet::from([task.id.clone()]));
                if let ExecutionPlanAction::Execute {
                    document: Some(document),
                    ..
                } = &nodes
                    .last()
                    .expect("the execution node was just pushed")
                    .action
                {
                    document_nodes.insert(
                        (
                            document.path.clone(),
                            document.format.clone(),
                            document.sha256.clone(),
                        ),
                        nodes.len() - 1,
                    );
                }
                task_nodes.entry(task.id.clone()).or_default().push(id);
            }
        }
    }
    if let Some(requirement) = options.reviewed_documents.keys().next() {
        return Err(ExecutionPlanBuildError::UnknownDocumentRequirement {
            requirement: requirement.clone(),
        });
    }
    let dependencies = execution_task_dependencies(&invocations.allocated.methods, &task_nodes)?;
    for (node, tasks) in nodes.iter_mut().zip(node_tasks) {
        node.after = tasks
            .iter()
            .flat_map(|task| dependencies.get(task).into_iter().flatten().cloned())
            .filter(|dependency| dependency != &node.id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
    }
    let plan = ExecutionPlanDocument {
        format: EXECUTION_PLAN_FORMAT.to_owned(),
        inventory: ExecutionInventoryReference {
            document: options.inventory_document,
            source_sha256: invocations.allocated.inventory_sha256.clone(),
            facility: invocations.allocated.facility.clone(),
        },
        planning: Some(planning),
        requirements,
        materials: options.materials,
        outputs: options.outputs,
        nodes,
    };
    plan.validate()
        .map_err(ExecutionPlanBuildError::InvalidPlan)?;
    Ok(plan)
}

fn execution_task_dependencies(
    methods: &[AllocatedMethod],
    task_nodes: &BTreeMap<lab_compiler::method::LocalId, Vec<String>>,
) -> Result<BTreeMap<lab_compiler::method::LocalId, Vec<String>>, ExecutionPlanBuildError> {
    let selected = methods
        .iter()
        .map(|method| (method.choice.clone(), method))
        .collect::<BTreeMap<_, _>>();

    let mut dependencies = BTreeMap::new();
    for method in methods {
        // A material edge is stronger than a whole-Method completion edge: it should release only
        // the consuming task. Suppress the blanket `after` edge for any Method whose material is
        // consumed somewhere in this Method, then attach that producer to the consuming task below.
        let task_material_producers = method
            .tasks
            .iter()
            .flat_map(|task| &task.materials)
            .filter_map(|material| match &material.source {
                SelectedMaterialSource::ChoiceOutput { choice } => Some(choice.clone()),
                SelectedMaterialSource::MaterialLot { .. } => None,
            })
            .collect::<BTreeSet<_>>();
        for task in &method.tasks {
            let mut nodes = BTreeSet::new();
            for producer in method
                .after
                .iter()
                .filter(|producer| !task_material_producers.contains(*producer))
            {
                let producer_method = selected.get(producer).copied().ok_or_else(|| {
                    ExecutionPlanBuildError::InvalidExecutionDataflow {
                        message: format!(
                            "method choice '{}' depends on unselected choice '{}'",
                            method.choice, producer
                        ),
                    }
                })?;
                for producer_task in &producer_method.tasks {
                    nodes.extend(task_nodes.get(&producer_task.id).cloned().ok_or_else(|| {
                        ExecutionPlanBuildError::InvalidExecutionDataflow {
                            message: format!(
                                "method choice '{}' depends on unknown Procedure task '{}'",
                                method.choice, producer_task.id
                            ),
                        }
                    })?);
                }
            }
            for input in &task.inputs {
                nodes.extend(source_execution_nodes(
                    &method.choice,
                    &input.source,
                    &selected,
                    task_nodes,
                    &mut BTreeSet::new(),
                )?);
            }
            for material in &task.materials {
                let SelectedMaterialSource::ChoiceOutput { choice: producer } = &material.source
                else {
                    continue;
                };
                let producer_method = selected.get(producer).copied().ok_or_else(|| {
                    ExecutionPlanBuildError::InvalidExecutionDataflow {
                        message: format!(
                            "Procedure material input '{}' depends on unselected choice '{}'",
                            material.input, producer
                        ),
                    }
                })?;
                for producer_task in &producer_method.tasks {
                    nodes.extend(task_nodes.get(&producer_task.id).cloned().ok_or_else(|| {
                        ExecutionPlanBuildError::InvalidExecutionDataflow {
                            message: format!(
                                "Procedure material input '{}' depends on unknown task '{}'",
                                material.input, producer_task.id
                            ),
                        }
                    })?);
                }
            }
            dependencies.insert(task.id.clone(), nodes.into_iter().collect());
        }
    }
    Ok(dependencies)
}

fn source_execution_nodes(
    owner: &lab_compiler::method::LocalId,
    source: &PlanningValueSource,
    selected: &BTreeMap<lab_compiler::method::LocalId, &AllocatedMethod>,
    task_nodes: &BTreeMap<lab_compiler::method::LocalId, Vec<String>>,
    visiting_inputs: &mut BTreeSet<(lab_compiler::method::LocalId, lab_compiler::method::LocalId)>,
) -> Result<Vec<String>, ExecutionPlanBuildError> {
    match source {
        PlanningValueSource::TaskOutput { task, .. } => {
            task_nodes.get(task).cloned().ok_or_else(|| {
                ExecutionPlanBuildError::InvalidExecutionDataflow {
                    message: format!("Procedure task '{owner}' depends on unknown task '{task}'"),
                }
            })
        }
        PlanningValueSource::ChoiceInput { input } => {
            let key = (owner.clone(), input.clone());
            if !visiting_inputs.insert(key.clone()) {
                return Err(ExecutionPlanBuildError::InvalidExecutionDataflow {
                    message: format!(
                        "method choice '{owner}' recursively resolves input '{input}'"
                    ),
                });
            }
            let method = selected.get(owner).copied().ok_or_else(|| {
                ExecutionPlanBuildError::InvalidExecutionDataflow {
                    message: format!("unknown selected method choice '{owner}'"),
                }
            })?;
            let port = method
                .inputs
                .iter()
                .find(|port| port.name == *input)
                .ok_or_else(|| ExecutionPlanBuildError::InvalidExecutionDataflow {
                    message: format!("method choice '{owner}' has no input '{input}'"),
                })?;
            let nodes = port.source.as_ref().map_or_else(
                || Ok(Vec::new()),
                |source| {
                    source_execution_nodes(owner, source, selected, task_nodes, visiting_inputs)
                },
            )?;
            visiting_inputs.remove(&key);
            Ok(nodes)
        }
        PlanningValueSource::ChoiceOutput { choice, output } => {
            let producer = selected.get(choice).copied().ok_or_else(|| {
                ExecutionPlanBuildError::InvalidExecutionDataflow {
                    message: format!("unknown producer method choice '{choice}'"),
                }
            })?;
            let yielded = producer
                .yields
                .iter()
                .find(|yielded| yielded.output == *output)
                .ok_or_else(|| ExecutionPlanBuildError::InvalidExecutionDataflow {
                    message: format!("method choice '{choice}' has no selected output '{output}'"),
                })?;
            source_execution_nodes(
                choice,
                &yielded.source,
                selected,
                task_nodes,
                visiting_inputs,
            )
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExecutionPlanBuildError {
    #[error("adapter invocation plan is invalid: {0}")]
    InvalidInvocations(String),
    #[error("cannot derive the execution DAG: {message}")]
    InvalidExecutionDataflow { message: String },
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
    #[error("atomic Procedure task `{task}` cannot be projected into an execution node: {message}")]
    UnsupportedAtomicExecution { task: String, message: String },
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

/// The manual steps of a lowered program, in the order the plan performs them.
///
/// A step an instrument runs arrives with its own operator document, rendered
/// by the adapter that lowered it. These are the ones a person performs,
/// projected as display-ready text for the run sheet the CLI typesets.
pub fn manual_run_steps(
    invocations: &AdapterInvocationPlan,
) -> Vec<lab_adapters::run_sheet::RunStep> {
    let mut steps = Vec::new();
    for method in &invocations.allocated.methods {
        for task in &method.tasks {
            for binding in &task.requirements {
                if binding.control_mode != ControlMode::Manual.iri() {
                    continue;
                }
                steps.push(lab_adapters::run_sheet::RunStep {
                    title: spaced_words(local_fragment(task.operation.as_str())),
                    operation: task.operation.to_string(),
                    asset: local_fragment(&binding.asset).to_owned(),
                    parameters: task
                        .parameters
                        .iter()
                        .map(|parameter| {
                            (
                                local_fragment(parameter.id.as_str()).to_owned(),
                                display_procedure_value(&parameter.value),
                            )
                        })
                        .collect(),
                });
            }
        }
    }
    steps
}

/// The local name at the end of an IRI or a `::`-qualified identifier.
fn local_fragment(value: &str) -> &str {
    let value = value.rsplit("::").next().unwrap_or(value);
    value
        .rsplit(['#', '/'])
        .next()
        .filter(|fragment| !fragment.is_empty())
        .unwrap_or(value)
}

/// `RealizeArtifact` read aloud: "Realize artifact".
fn spaced_words(value: &str) -> String {
    let mut words = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_uppercase() && index > 0 {
            words.push(' ');
            words.extend(character.to_lowercase());
        } else {
            words.push(character);
        }
    }
    words
}

/// A parameter value the way an operator reads it: text unquoted, a unit by
/// its name, and an empty list said in words.
fn display_procedure_value(value: &ProcedureValue) -> String {
    match value {
        ProcedureValue::Scalar { value } => display_property_value(value),
        ProcedureValue::List { values, .. } if values.is_empty() => "none".to_owned(),
        ProcedureValue::List { values, .. } => values
            .iter()
            .map(display_property_value)
            .collect::<Vec<_>>()
            .join(", "),
    }
}

fn display_property_value(value: &lab_capability::PropertyValue) -> String {
    let scalar = match &value.value {
        ScalarValue::Text(value) => value.clone(),
        ScalarValue::Integer(value) => value.to_string(),
        ScalarValue::Real(value) => value.to_string(),
        ScalarValue::Boolean(value) => value.to_string(),
        ScalarValue::Iri(value) => local_fragment(value.as_str()).to_owned(),
    };
    match &value.unit {
        Some(unit) => format!("{scalar} {}", local_fragment(unit.as_str())),
        None => scalar,
    }
}

fn manual_instructions<'a>(
    task: &AllocatedProcedureTask,
    bindings: impl IntoIterator<Item = &'a AllocatedRequirementBinding>,
) -> String {
    let resources = bindings
        .into_iter()
        .map(|binding| {
            format!(
                "CapabilityOffering '{}' on Asset '{}'",
                binding.offering, binding.asset
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let mut instructions = format!(
        "Use {resources} to perform Procedure operation '{}'. Follow the facility's reviewed local SOP for this operation and confirm completion.",
        task.operation
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

#[cfg(test)]
mod tests {
    use lab_capability::{MethodId, OperationId};
    use lab_compiler::method::{IntentOperationId, LocalId, PortType};
    use lab_compiler::planning::{
        PlanningMethodYield, PlanningPort, PlanningTaskInput, PlanningTaskOutput,
        SelectedMaterialBinding,
    };

    use super::*;

    fn id(value: &str) -> LocalId {
        LocalId::new(value).unwrap()
    }

    fn task(name: &str, inputs: Vec<PlanningTaskInput>) -> AllocatedProcedureTask {
        AllocatedProcedureTask {
            id: id(name),
            operation: OperationId::new("https://example.org/operation").unwrap(),
            program: None,
            inputs,
            outputs: vec![PlanningTaskOutput {
                name: id("value"),
                port_type: PortType::Design,
            }],
            parameters: Vec::new(),
            materials: Vec::new(),
            requirements: Vec::new(),
        }
    }

    fn method(name: &str, tasks: Vec<AllocatedProcedureTask>) -> AllocatedMethod {
        AllocatedMethod {
            choice: id(name),
            source_operation: IntentOperationId::new("example.intent").unwrap(),
            method: MethodId::new(format!("https://example.org/method/{name}")).unwrap(),
            after: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            yields: Vec::new(),
            tasks,
        }
    }

    fn input(source: PlanningValueSource) -> PlanningTaskInput {
        PlanningTaskInput {
            source,
            port_type: PortType::Design,
        }
    }

    fn task_nodes(methods: &[AllocatedMethod]) -> BTreeMap<LocalId, Vec<String>> {
        methods
            .iter()
            .flat_map(|method| &method.tasks)
            .map(|task| (task.id.clone(), vec![format!("node-{}", task.id)]))
            .collect()
    }

    fn dependencies(
        methods: &[AllocatedMethod],
    ) -> Result<BTreeMap<LocalId, Vec<String>>, ExecutionPlanBuildError> {
        execution_task_dependencies(methods, &task_nodes(methods))
    }

    #[test]
    fn task_output_depends_on_the_local_producer() {
        let methods = vec![method(
            "choice",
            vec![
                task("produce", Vec::new()),
                task(
                    "consume",
                    vec![input(PlanningValueSource::TaskOutput {
                        task: id("produce"),
                        output: id("value"),
                    })],
                ),
            ],
        )];

        let dependencies = dependencies(&methods).unwrap();
        assert!(dependencies[&id("produce")].is_empty());
        assert_eq!(dependencies[&id("consume")], ["node-produce"]);
    }

    #[test]
    fn choice_input_resolves_through_the_producer_yield() {
        let mut producer = method("producer", vec![task("produce", Vec::new())]);
        producer.outputs.push(PlanningPort {
            name: id("result"),
            port_type: PortType::Design,
            source: None,
        });
        producer.yields.push(PlanningMethodYield {
            output: id("result"),
            source: PlanningValueSource::TaskOutput {
                task: id("produce"),
                output: id("value"),
            },
        });
        let mut consumer = method(
            "consumer",
            vec![task(
                "consume",
                vec![input(PlanningValueSource::ChoiceInput {
                    input: id("incoming"),
                })],
            )],
        );
        consumer.inputs.push(PlanningPort {
            name: id("incoming"),
            port_type: PortType::Design,
            source: Some(PlanningValueSource::ChoiceOutput {
                choice: id("producer"),
                output: id("result"),
            }),
        });
        let methods = vec![producer, consumer];

        let dependencies = dependencies(&methods).unwrap();
        assert_eq!(dependencies[&id("consume")], ["node-produce"]);
    }

    #[test]
    fn after_depends_on_every_task_in_the_preceding_method() {
        let producer = method(
            "producer",
            vec![task("produce-a", Vec::new()), task("produce-b", Vec::new())],
        );
        let mut consumer = method("consumer", vec![task("consume", Vec::new())]);
        consumer.after.push(id("producer"));
        let methods = vec![producer, consumer];

        let dependencies = dependencies(&methods).unwrap();
        assert_eq!(
            dependencies[&id("consume")],
            ["node-produce-a", "node-produce-b"]
        );
    }

    #[test]
    fn material_edge_releases_only_the_consuming_task() {
        let producer = method(
            "producer",
            vec![task("produce-a", Vec::new()), task("produce-b", Vec::new())],
        );
        let mut material_consumer = task("consume-material", Vec::new());
        material_consumer.materials.push(SelectedMaterialBinding {
            input: id("sample"),
            symbol: "sample".to_owned(),
            source: SelectedMaterialSource::ChoiceOutput {
                choice: id("producer"),
            },
            interchangeable_alternatives: Vec::new(),
        });
        let mut consumer = method(
            "consumer",
            vec![material_consumer, task("consume-other", Vec::new())],
        );
        consumer.after.push(id("producer"));
        let methods = vec![producer, consumer];

        let dependencies = dependencies(&methods).unwrap();
        assert_eq!(
            dependencies[&id("consume-material")],
            ["node-produce-a", "node-produce-b"]
        );
        assert!(dependencies[&id("consume-other")].is_empty());
    }

    #[test]
    fn unbound_choice_input_is_an_execution_root() {
        let mut root = method(
            "root",
            vec![task(
                "consume",
                vec![input(PlanningValueSource::ChoiceInput {
                    input: id("external"),
                })],
            )],
        );
        root.inputs.push(PlanningPort {
            name: id("external"),
            port_type: PortType::Design,
            source: None,
        });

        let dependencies = dependencies(&[root]).unwrap();
        assert!(dependencies[&id("consume")].is_empty());
    }

    #[test]
    fn missing_task_and_choice_output_references_are_errors() {
        let missing_task = vec![method(
            "choice",
            vec![task(
                "consume",
                vec![input(PlanningValueSource::TaskOutput {
                    task: id("missing"),
                    output: id("value"),
                })],
            )],
        )];
        assert!(matches!(
            dependencies(&missing_task),
            Err(ExecutionPlanBuildError::InvalidExecutionDataflow { message })
                if message.contains("unknown task 'missing'")
        ));

        let producer = method("producer", vec![task("produce", Vec::new())]);
        let mut consumer = method(
            "consumer",
            vec![task(
                "consume",
                vec![input(PlanningValueSource::ChoiceInput {
                    input: id("incoming"),
                })],
            )],
        );
        consumer.inputs.push(PlanningPort {
            name: id("incoming"),
            port_type: PortType::Design,
            source: Some(PlanningValueSource::ChoiceOutput {
                choice: id("producer"),
                output: id("missing"),
            }),
        });
        assert!(matches!(
            dependencies(&[producer, consumer]),
            Err(ExecutionPlanBuildError::InvalidExecutionDataflow { message })
                if message.contains("no selected output 'missing'")
        ));
    }
}
