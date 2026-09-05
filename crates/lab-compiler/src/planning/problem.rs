//! Stable, facility-independent constraint input projected from refined LAIR.

use std::collections::{BTreeMap, BTreeSet};

use crate::method::{IntentOperationId, LocalId, PortType, ProcedureValue};
use crate::procedure::binding::{
    ProcedureBindingError, ProcedureCapabilityRequirement, ProcedureTaskInterface,
};
use crate::procedure::{
    BindingScope, ProcedureContractRegistry, ProcedureProgram, ProcedureTaskProgramValidationError,
    ValidatedProcedureProgram, validate_task_program,
};
use crate::workflow::IntentAction;
use lab_capability::{
    CapabilityKind, ControlMode, MethodId, OperationId, PropertyConstraint, PropertyKind,
    QualificationLevel,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const PLANNING_PROBLEM_SCHEMA_VERSION: &str = "lab.planning-problem.v2";

/// Every unresolved method choice and its complete Procedure requirement graph.
///
/// This record is immutable, serializable, and contains no Pliron handles or facility facts. A
/// facility planner combines it with one validated inventory snapshot and explicit adapter
/// bindings to construct the global allocation problem.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanningProblem {
    pub schema_version: String,
    pub choices: Vec<PlanningMethodChoice>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanningMethodChoice {
    pub id: LocalId,
    pub source_operation: IntentOperationId,
    /// The complete checked action that this Method choice refines.
    ///
    /// Schemars represents this as arbitrary JSON because the checked-language
    /// graph intentionally has no schema-generation dependency. Serde still
    /// encodes and decodes the exact typed `IntentAction`.
    #[schemars(with = "serde_json::Value")]
    pub source_intent: IntentAction,
    /// Explicit completion dependencies that are not represented by an SSA operand, such as a
    /// strain build's requirement for a separately realized plasmid.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<LocalId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<PlanningPort>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<PlanningPort>,
    pub candidates: Vec<PlanningMethodCandidate>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanningPort {
    pub name: LocalId,
    pub port_type: PortType,
    /// The output of an earlier method choice that supplies this input, when it is not a
    /// module-level Design or other external value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PlanningValueSource>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanningMethodCandidate {
    pub method: MethodId,
    pub tasks: Vec<PlanningProcedureTask>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub yields: Vec<PlanningMethodYield>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanningProcedureTask {
    pub id: LocalId,
    pub operation: OperationId,
    /// Canonical, device-neutral operational semantics when this task's open operation has a
    /// registered normalization contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<ProcedureProgram>,
    /// How the capability clauses derived for this task may be allocated.
    #[serde(default)]
    pub binding_scope: BindingScope,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<PlanningTaskInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<PlanningTaskOutput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<PlanningProcedureParameter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<PlanningMaterialInput>,
    pub requirements: Vec<PlanningCapabilityRequirement>,
}

/// One stable physical material input before facility allocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanningMaterialInput {
    pub id: LocalId,
    pub symbol: String,
    pub source: PlanningMaterialSource,
}

/// The physical origin of a Procedure material input before facility allocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanningMaterialSource {
    /// Resolve the symbol through its checked SBOL Component identity and an active MaterialLot.
    Inventory,
    /// Consume the physical result of another Method choice in this same plan.
    ChoiceOutput { choice: LocalId },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanningTaskInput {
    pub source: PlanningValueSource,
    pub port_type: PortType,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanningTaskOutput {
    pub name: LocalId,
    pub port_type: PortType,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanningProcedureParameter {
    pub id: LocalId,
    pub property_kind: PropertyKind,
    pub value: ProcedureValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanningCapabilityRequirement {
    pub id: LocalId,
    pub capability_kind: CapabilityKind,
    pub minimum_qualification: QualificationLevel,
    pub accepted_control_modes: BTreeSet<ControlMode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<PropertyConstraint>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanningMethodYield {
    pub output: LocalId,
    pub source: PlanningValueSource,
}

/// A stable edge in one candidate Procedure graph.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanningValueSource {
    ChoiceInput { input: LocalId },
    ChoiceOutput { choice: LocalId, output: LocalId },
    TaskOutput { task: LocalId, output: LocalId },
}

impl PlanningProblem {
    /// Digest the canonical serde representation used to bind a solution to this exact problem.
    pub fn sha256(&self) -> String {
        let bytes = serde_json::to_vec(self)
            .expect("PlanningProblem contains only infallibly serializable semantic values");
        hex_sha256(&bytes)
    }

    /// Validate a deserialized problem before a solver or facility query consumes it.
    pub fn validate(
        &self,
        contracts: &ProcedureContractRegistry,
    ) -> Result<(), PlanningProblemValidationError> {
        if self.schema_version != PLANNING_PROBLEM_SCHEMA_VERSION {
            return Err(PlanningProblemValidationError::WrongSchema {
                found: self.schema_version.clone(),
            });
        }
        if self.choices.is_empty() {
            return Err(PlanningProblemValidationError::EmptyProblem);
        }
        let mut choices = BTreeSet::new();
        let mut task_ids = BTreeSet::new();
        let mut requirement_ids = BTreeSet::new();
        let mut parameter_ids = BTreeSet::new();
        let mut material_input_ids = BTreeSet::new();
        let mut output_ports = BTreeMap::new();
        for choice in &self.choices {
            if !choices.insert(choice.id.clone()) {
                return Err(PlanningProblemValidationError::DuplicateChoice {
                    choice: choice.id.clone(),
                });
            }
            choice.source_intent.validate().map_err(|message| {
                PlanningProblemValidationError::InvalidSourceIntent {
                    choice: choice.id.clone(),
                    message,
                }
            })?;
            if choice.source_intent.operation() != Some(choice.source_operation.as_str()) {
                return Err(PlanningProblemValidationError::SourceOperationMismatch {
                    choice: choice.id.clone(),
                });
            }
            validate_ports(&choice.id, "input", &choice.inputs)?;
            validate_ports(&choice.id, "output", &choice.outputs)?;
            choice
                .source_intent
                .validate_ports(
                    choice
                        .inputs
                        .iter()
                        .map(|port| (port.name.as_str(), &port.port_type)),
                    choice
                        .outputs
                        .iter()
                        .map(|port| (port.name.as_str(), &port.port_type)),
                )
                .map_err(
                    |message| PlanningProblemValidationError::SourcePortMismatch {
                        choice: choice.id.clone(),
                        message,
                    },
                )?;
            if let Some(output) = choice.outputs.iter().find(|output| output.source.is_some()) {
                return Err(PlanningProblemValidationError::InvalidPortSource {
                    choice: choice.id.clone(),
                    port: output.name.clone(),
                });
            }
            for input in &choice.inputs {
                if input.source.as_ref().is_some_and(|source| {
                    !matches!(source, PlanningValueSource::ChoiceOutput { .. })
                }) {
                    return Err(PlanningProblemValidationError::InvalidPortSource {
                        choice: choice.id.clone(),
                        port: input.name.clone(),
                    });
                }
            }
            for output in &choice.outputs {
                output_ports.insert(
                    PlanningValueSource::ChoiceOutput {
                        choice: choice.id.clone(),
                        output: output.name.clone(),
                    },
                    output.port_type.clone(),
                );
            }
            let mut explicit_dependencies = BTreeSet::new();
            if let Some(dependency) = choice
                .after
                .iter()
                .find(|dependency| !explicit_dependencies.insert((*dependency).clone()))
            {
                return Err(PlanningProblemValidationError::DuplicateChoiceDependency {
                    choice: choice.id.clone(),
                    dependency: dependency.clone(),
                });
            }
            if choice.candidates.is_empty() {
                return Err(PlanningProblemValidationError::EmptyChoice {
                    choice: choice.id.clone(),
                });
            }
            let mut methods = BTreeSet::new();
            for candidate in &choice.candidates {
                if !methods.insert(candidate.method.clone()) {
                    return Err(PlanningProblemValidationError::DuplicateCandidate {
                        choice: choice.id.clone(),
                        method: candidate.method.clone(),
                    });
                }
                validate_candidate(
                    choice,
                    candidate,
                    contracts,
                    &mut task_ids,
                    &mut requirement_ids,
                    &mut parameter_ids,
                    &mut material_input_ids,
                )?;
            }
        }
        let mut dependencies = self
            .choices
            .iter()
            .map(|choice| (choice.id.clone(), BTreeSet::new()))
            .collect::<BTreeMap<_, _>>();
        for choice in &self.choices {
            for producer in &choice.after {
                if !dependencies.contains_key(producer) {
                    return Err(PlanningProblemValidationError::UnknownChoiceDependency {
                        choice: choice.id.clone(),
                        dependency: producer.clone(),
                    });
                }
                dependencies
                    .get_mut(&choice.id)
                    .expect("every validated choice has a dependency set")
                    .insert(producer.clone());
            }
            for input in &choice.inputs {
                let Some(
                    source @ PlanningValueSource::ChoiceOutput {
                        choice: producer, ..
                    },
                ) = &input.source
                else {
                    continue;
                };
                let Some(source_type) = output_ports.get(source) else {
                    return Err(PlanningProblemValidationError::UnknownChoiceOutput {
                        choice: choice.id.clone(),
                        port: input.name.clone(),
                        value_source: source.clone(),
                    });
                };
                if source_type != &input.port_type {
                    return Err(PlanningProblemValidationError::ValueTypeMismatch {
                        owner: choice.id.clone(),
                        value_source: source.clone(),
                    });
                }
                dependencies
                    .get_mut(&choice.id)
                    .expect("every validated choice has a dependency set")
                    .insert(producer.clone());
            }
            for material in choice
                .candidates
                .iter()
                .flat_map(|candidate| &candidate.tasks)
                .flat_map(|task| &task.materials)
            {
                let PlanningMaterialSource::ChoiceOutput { choice: producer } = &material.source
                else {
                    continue;
                };
                if !dependencies.contains_key(producer) {
                    return Err(PlanningProblemValidationError::UnknownMaterialProducer {
                        material: material.id.clone(),
                        producer: producer.clone(),
                    });
                }
                dependencies
                    .get_mut(&choice.id)
                    .expect("every validated choice has a dependency set")
                    .insert(producer.clone());
            }
        }
        if !choice_dependencies_are_acyclic(&dependencies) {
            return Err(PlanningProblemValidationError::ChoiceDependencyCycle);
        }
        Ok(())
    }
}

fn choice_dependencies_are_acyclic(dependencies: &BTreeMap<LocalId, BTreeSet<LocalId>>) -> bool {
    let mut indegree = dependencies
        .iter()
        .map(|(choice, producers)| (choice.clone(), producers.len()))
        .collect::<BTreeMap<_, _>>();
    let mut dependents = BTreeMap::<LocalId, BTreeSet<LocalId>>::new();
    for (choice, producers) in dependencies {
        for producer in producers {
            dependents
                .entry(producer.clone())
                .or_default()
                .insert(choice.clone());
        }
    }
    let mut ready = indegree
        .iter()
        .filter_map(|(choice, degree)| (*degree == 0).then_some(choice.clone()))
        .collect::<BTreeSet<_>>();
    let mut visited = 0;
    while let Some(choice) = ready.pop_first() {
        visited += 1;
        for dependent in dependents.get(&choice).into_iter().flatten() {
            let degree = indegree
                .get_mut(dependent)
                .expect("every dependent is a declared choice");
            *degree -= 1;
            if *degree == 0 {
                ready.insert(dependent.clone());
            }
        }
    }
    visited == dependencies.len()
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn validate_ports(
    choice: &LocalId,
    kind: &'static str,
    ports: &[PlanningPort],
) -> Result<(), PlanningProblemValidationError> {
    let mut names = BTreeSet::new();
    if let Some(port) = ports.iter().find(|port| !names.insert(port.name.clone())) {
        return Err(PlanningProblemValidationError::DuplicatePort {
            choice: choice.clone(),
            kind,
            port: port.name.clone(),
        });
    }
    Ok(())
}

fn validate_candidate(
    choice: &PlanningMethodChoice,
    candidate: &PlanningMethodCandidate,
    contracts: &ProcedureContractRegistry,
    global_tasks: &mut BTreeSet<LocalId>,
    global_requirements: &mut BTreeSet<LocalId>,
    global_parameters: &mut BTreeSet<LocalId>,
    global_material_inputs: &mut BTreeSet<LocalId>,
) -> Result<(), PlanningProblemValidationError> {
    if candidate.tasks.is_empty() {
        return Err(PlanningProblemValidationError::EmptyProcedure {
            choice: choice.id.clone(),
            method: candidate.method.clone(),
        });
    }
    let mut available = choice
        .inputs
        .iter()
        .map(|input| {
            (
                PlanningValueSource::ChoiceInput {
                    input: input.name.clone(),
                },
                input.port_type.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for task in &candidate.tasks {
        if !global_tasks.insert(task.id.clone()) {
            return Err(PlanningProblemValidationError::DuplicateTask {
                task: task.id.clone(),
            });
        }
        for input in &task.inputs {
            let Some(available_type) = available.get(&input.source) else {
                return Err(PlanningProblemValidationError::UnavailableValue {
                    task: task.id.clone(),
                    value_source: input.source.clone(),
                });
            };
            if available_type != &input.port_type {
                return Err(PlanningProblemValidationError::ValueTypeMismatch {
                    owner: task.id.clone(),
                    value_source: input.source.clone(),
                });
            }
        }
        let mut output_names = BTreeSet::new();
        for output in &task.outputs {
            if !output_names.insert(output.name.clone()) {
                return Err(PlanningProblemValidationError::DuplicateTaskOutput {
                    task: task.id.clone(),
                    output: output.name.clone(),
                });
            }
            available.insert(
                PlanningValueSource::TaskOutput {
                    task: task.id.clone(),
                    output: output.name.clone(),
                },
                output.port_type.clone(),
            );
        }
        for parameter in &task.parameters {
            if !global_parameters.insert(parameter.id.clone()) {
                return Err(PlanningProblemValidationError::DuplicateParameter {
                    parameter: parameter.id.clone(),
                });
            }
        }
        for material in &task.materials {
            if !global_material_inputs.insert(material.id.clone()) {
                return Err(PlanningProblemValidationError::DuplicateMaterialInput {
                    material: material.id.clone(),
                });
            }
            if material.symbol.is_empty() {
                return Err(PlanningProblemValidationError::EmptyMaterialSymbol {
                    material: material.id.clone(),
                });
            }
        }
        let validated = validate_task_program(&task.id, task.program.as_ref(), contracts)?;
        if let Some(validated) = validated {
            validate_program_contract(task, &validated)?;
        } else if task.binding_scope != BindingScope::Independent {
            return Err(PlanningProblemValidationError::ProcedureCapabilityFormula {
                task: task.id.clone(),
            });
        }
        if task.requirements.is_empty() {
            return Err(PlanningProblemValidationError::MissingRequirement {
                task: task.id.clone(),
            });
        }
        for requirement in &task.requirements {
            if !global_requirements.insert(requirement.id.clone()) {
                return Err(PlanningProblemValidationError::DuplicateRequirement {
                    requirement: requirement.id.clone(),
                });
            }
            if requirement.accepted_control_modes.is_empty()
                || requirement
                    .accepted_control_modes
                    .contains(&ControlMode::Unspecified)
            {
                return Err(PlanningProblemValidationError::InvalidControlPolicy {
                    requirement: requirement.id.clone(),
                });
            }
        }
    }
    if candidate.yields.len() != choice.outputs.len() {
        return Err(PlanningProblemValidationError::YieldArity {
            choice: choice.id.clone(),
            method: candidate.method.clone(),
        });
    }
    for (expected, yielded) in choice.outputs.iter().zip(&candidate.yields) {
        if yielded.output != expected.name {
            return Err(PlanningProblemValidationError::YieldName {
                choice: choice.id.clone(),
                method: candidate.method.clone(),
                expected: expected.name.clone(),
                found: yielded.output.clone(),
            });
        }
        let Some(available_type) = available.get(&yielded.source) else {
            return Err(PlanningProblemValidationError::UnavailableYield {
                choice: choice.id.clone(),
                method: candidate.method.clone(),
                value_source: yielded.source.clone(),
            });
        };
        if available_type != &expected.port_type {
            return Err(PlanningProblemValidationError::ValueTypeMismatch {
                owner: choice.id.clone(),
                value_source: yielded.source.clone(),
            });
        }
    }
    Ok(())
}

fn validate_program_contract(
    task: &PlanningProcedureTask,
    program: &ValidatedProcedureProgram,
) -> Result<(), PlanningProblemValidationError> {
    let interface = ProcedureTaskInterface::new(
        task.inputs.len(),
        task.materials.iter().map(|material| material.id.clone()),
        task.outputs.iter().map(|output| output.name.clone()),
    );
    let requirements = task
        .requirements
        .iter()
        .map(|requirement| ProcedureCapabilityRequirement {
            id: requirement.id.clone(),
            capability_kind: requirement.capability_kind.clone(),
            minimum_qualification: requirement.minimum_qualification,
            accepted_control_modes: requirement.accepted_control_modes.clone(),
            constraints: requirement.constraints.clone(),
        })
        .collect::<Vec<_>>();
    program
        .validate_task_contract(
            &task.id,
            &interface,
            Some(task.binding_scope),
            &requirements,
        )
        .map(|_| ())
        .map_err(|error| map_procedure_binding_error(&task.id, error))
}

fn map_procedure_binding_error(
    task: &LocalId,
    error: ProcedureBindingError,
) -> PlanningProblemValidationError {
    match error {
        ProcedureBindingError::UnavailableInput { .. } => {
            PlanningProblemValidationError::ProcedureInputBindings { task: task.clone() }
        }
        ProcedureBindingError::DuplicateTaskOutput
        | ProcedureBindingError::OutputMismatch { .. } => {
            PlanningProblemValidationError::ProcedureOutputBindings { task: task.clone() }
        }
        ProcedureBindingError::DuplicateTaskMaterial
        | ProcedureBindingError::UndeclaredMaterial { .. }
        | ProcedureBindingError::MaterialMismatch { .. } => {
            PlanningProblemValidationError::ProcedureMaterialBindings { task: task.clone() }
        }
        ProcedureBindingError::BindingScopeMismatch { .. }
        | ProcedureBindingError::DuplicateRequirement { .. }
        | ProcedureBindingError::RequirementCount { .. }
        | ProcedureBindingError::MissingRequirement { .. }
        | ProcedureBindingError::RequirementKindMismatch { .. }
        | ProcedureBindingError::RequirementConstraintsMismatch { .. }
        | ProcedureBindingError::RequirementPolicyMismatch { .. } => {
            PlanningProblemValidationError::ProcedureCapabilityFormula { task: task.clone() }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PlanningProblemValidationError {
    #[error(
        "planning problem declares schema `{found}`, expected `{PLANNING_PROBLEM_SCHEMA_VERSION}`"
    )]
    WrongSchema { found: String },
    #[error("planning problem contains no method choices")]
    EmptyProblem,
    #[error("method choice `{choice}` occurs more than once")]
    DuplicateChoice { choice: LocalId },
    #[error("method choice `{choice}` has invalid source Intent: {message}")]
    InvalidSourceIntent { choice: LocalId, message: String },
    #[error("method choice `{choice}` source operation does not match its source Intent")]
    SourceOperationMismatch { choice: LocalId },
    #[error("method choice `{choice}` ports do not match its source Intent: {message}")]
    SourcePortMismatch { choice: LocalId, message: String },
    #[error("method choice `{choice}` has no candidates")]
    EmptyChoice { choice: LocalId },
    #[error("method choice `{choice}` {kind} port `{port}` occurs more than once")]
    DuplicatePort {
        choice: LocalId,
        kind: &'static str,
        port: LocalId,
    },
    #[error("method choice `{choice}` port `{port}` has an invalid value source")]
    InvalidPortSource { choice: LocalId, port: LocalId },
    #[error("method choice `{choice}` input `{port}` references unknown output `{value_source:?}`")]
    UnknownChoiceOutput {
        choice: LocalId,
        port: LocalId,
        value_source: PlanningValueSource,
    },
    #[error("method-choice dataflow contains a dependency cycle")]
    ChoiceDependencyCycle,
    #[error("method choice `{choice}` repeats completion dependency `{dependency}`")]
    DuplicateChoiceDependency {
        choice: LocalId,
        dependency: LocalId,
    },
    #[error("method choice `{choice}` references unknown completion dependency `{dependency}`")]
    UnknownChoiceDependency {
        choice: LocalId,
        dependency: LocalId,
    },
    #[error("method choice `{choice}` repeats candidate `{method}`")]
    DuplicateCandidate { choice: LocalId, method: MethodId },
    #[error("method choice `{choice}` candidate `{method}` contains no Procedure tasks")]
    EmptyProcedure { choice: LocalId, method: MethodId },
    #[error("Procedure task `{task}` occurs more than once")]
    DuplicateTask { task: LocalId },
    #[error("Procedure task `{task}` output `{output}` occurs more than once")]
    DuplicateTaskOutput { task: LocalId, output: LocalId },
    #[error("Procedure task `{task}` references unavailable value `{value_source:?}`")]
    UnavailableValue {
        task: LocalId,
        value_source: PlanningValueSource,
    },
    #[error("value `{value_source:?}` has the wrong type for `{owner}`")]
    ValueTypeMismatch {
        owner: LocalId,
        value_source: PlanningValueSource,
    },
    #[error("Procedure parameter `{parameter}` occurs more than once")]
    DuplicateParameter { parameter: LocalId },
    #[error("Procedure material input `{material}` occurs more than once")]
    DuplicateMaterialInput { material: LocalId },
    #[error("Procedure material input `{material}` has an empty inventory symbol")]
    EmptyMaterialSymbol { material: LocalId },
    #[error("Procedure material input `{material}` references unknown Method choice `{producer}`")]
    UnknownMaterialProducer {
        material: LocalId,
        producer: LocalId,
    },
    #[error(transparent)]
    InvalidProcedureTaskProgram(#[from] ProcedureTaskProgramValidationError),
    #[error("Procedure task `{task}` normalized program references an undeclared material input")]
    ProcedureMaterialBindings { task: LocalId },
    #[error("Procedure task `{task}` normalized program references an unavailable task input")]
    ProcedureInputBindings { task: LocalId },
    #[error("Procedure task `{task}` normalized program does not bind exactly its outputs")]
    ProcedureOutputBindings { task: LocalId },
    #[error(
        "Procedure task `{task}` does not carry the exact capability formula derived from its normalized program"
    )]
    ProcedureCapabilityFormula { task: LocalId },
    #[error("Procedure task `{task}` has no Capability requirement")]
    MissingRequirement { task: LocalId },
    #[error("Capability requirement `{requirement}` occurs more than once")]
    DuplicateRequirement { requirement: LocalId },
    #[error("Capability requirement `{requirement}` has no concrete control policy")]
    InvalidControlPolicy { requirement: LocalId },
    #[error("method choice `{choice}` candidate `{method}` yields the wrong number of values")]
    YieldArity { choice: LocalId, method: MethodId },
    #[error(
        "method choice `{choice}` candidate `{method}` yields `{found}` where `{expected}` is required"
    )]
    YieldName {
        choice: LocalId,
        method: MethodId,
        expected: LocalId,
        found: LocalId,
    },
    #[error(
        "method choice `{choice}` candidate `{method}` yields unavailable value `{value_source:?}`"
    )]
    UnavailableYield {
        choice: LocalId,
        method: MethodId,
        value_source: PlanningValueSource,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contracts() -> &'static ProcedureContractRegistry {
        crate::procedure::builtin_procedure_contracts()
    }

    fn external_problem() -> PlanningProblem {
        PlanningProblem {
            schema_version: PLANNING_PROBLEM_SCHEMA_VERSION.to_owned(),
            choices: vec![PlanningMethodChoice {
                id: LocalId::new("choice").unwrap(),
                source_operation: IntentOperationId::new("example.operation").unwrap(),
                source_intent: crate::workflow::ir::synthetic_intent("example.operation"),
                after: Vec::new(),
                inputs: Vec::new(),
                outputs: Vec::new(),
                candidates: vec![PlanningMethodCandidate {
                    method: MethodId::new("https://example.org/method").unwrap(),
                    tasks: vec![PlanningProcedureTask {
                        id: LocalId::new("choice::task").unwrap(),
                        operation: OperationId::new("https://example.org/procedure/external")
                            .unwrap(),
                        program: None,
                        binding_scope: BindingScope::Independent,
                        inputs: Vec::new(),
                        outputs: Vec::new(),
                        parameters: Vec::new(),
                        materials: Vec::new(),
                        requirements: vec![PlanningCapabilityRequirement {
                            id: LocalId::new("choice::task::requirement").unwrap(),
                            capability_kind: CapabilityKind::new("https://example.org/capability")
                                .unwrap(),
                            minimum_qualification: QualificationLevel::Plannable,
                            accepted_control_modes: BTreeSet::from([ControlMode::Manual]),
                            constraints: Vec::new(),
                        }],
                    }],
                    yields: Vec::new(),
                }],
            }],
        }
    }

    #[test]
    fn a_deserialized_problem_is_revalidated_before_use() {
        let problem = PlanningProblem {
            schema_version: PLANNING_PROBLEM_SCHEMA_VERSION.to_owned(),
            choices: Vec::new(),
        };
        let json = serde_json::to_string(&problem).unwrap();
        let decoded: PlanningProblem = serde_json::from_str(&json).unwrap();

        assert_eq!(
            decoded.validate(contracts()).unwrap_err(),
            PlanningProblemValidationError::EmptyProblem
        );
        assert_eq!(decoded.sha256().len(), 64);
    }

    #[test]
    fn persisted_planning_records_reject_unknown_fields() {
        let mut document = serde_json::to_value(external_problem()).unwrap();
        document
            .as_object_mut()
            .unwrap()
            .insert("legacy_target".to_owned(), serde_json::json!("robot"));

        let error = serde_json::from_value::<PlanningProblem>(document).unwrap_err();
        assert!(error.to_string().contains("legacy_target"), "{error}");
    }

    #[test]
    fn planning_rejects_ports_and_results_that_disagree_with_source_intent() {
        let evidence = PortType::Data {
            data_kind: lab_capability::AbsoluteIri::new(format!(
                "{}Evidence",
                crate::workflow::ir::DATA_NS
            ))
            .unwrap(),
        };
        let mut port_tamper = external_problem();
        port_tamper.choices[0].source_intent = crate::workflow::ir::synthetic_intent_with_ports(
            "example.operation",
            &[],
            &[("evidence".to_owned(), evidence.clone())],
        );
        port_tamper.choices[0].outputs = vec![PlanningPort {
            name: LocalId::new("other").unwrap(),
            port_type: evidence,
            source: None,
        }];
        assert!(matches!(
            port_tamper.validate(contracts()),
            Err(PlanningProblemValidationError::SourcePortMismatch { .. })
        ));

        let mut result_tamper = external_problem();
        result_tamper.choices[0].source_intent = crate::workflow::ir::synthetic_intent_with_ports(
            "example.operation",
            &[],
            &[(
                "evidence".to_owned(),
                PortType::Data {
                    data_kind: lab_capability::AbsoluteIri::new(format!(
                        "{}Evidence",
                        crate::workflow::ir::DATA_NS
                    ))
                    .unwrap(),
                },
            )],
        );
        result_tamper.choices[0].source_intent.result_bindings[0].r#type =
            lab_language::CheckedType::String;
        assert!(matches!(
            result_tamper.validate(contracts()),
            Err(PlanningProblemValidationError::InvalidSourceIntent { .. })
        ));
    }

    #[test]
    fn task_operations_are_descriptive_and_do_not_select_program_builders() {
        let mut problem = external_problem();
        problem.validate(contracts()).unwrap();

        problem.choices[0].candidates[0].tasks[0].operation =
            OperationId::new(crate::procedure::vocabulary::PLATE_DILUTED_CULTURE).unwrap();
        problem.validate(contracts()).unwrap();
    }
}
