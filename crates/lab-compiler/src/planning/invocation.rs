//! Immutable adapter invocations projected from one exact allocated Procedure program.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use lab_capability::{
    AbsoluteIri, CapabilityKind, ControlMode, MethodId, ProcedureImplementationId,
    QualificationLevel,
};
use lab_method::{IntentOperationId, LocalId};
use lab_procedure::{BindingScope, ProcedureProgram, ValidatedProcedureProgram};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::model::MaterialLotCandidates;
use super::{
    FacilityPlanningSolution, FacilityPlanningSolutionValidationError, MaterialLotBuildInventory,
    PlanningProblem, PlanningProcedureParameter, PlanningTaskInput, PlanningTaskOutput,
    SelectedCapabilityParameter, SelectedMaterialBinding, SelectedMaterialSource,
};

pub const ADAPTER_INVOCATIONS_SCHEMA_VERSION: &str = "lab.adapter-invocations.v7";

/// The complete, immutable backend-facing projection of an allocated Procedure program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AdapterInvocationPlan {
    pub schema_version: String,
    pub problem_sha256: String,
    pub allocated_lair_sha256: String,
    pub inventory_sha256: String,
    pub facility: String,
    pub material_inventory: MaterialLotBuildInventory,
    pub methods: Vec<AllocatedMethod>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invocations: Vec<AdapterInvocation>,
}

/// One selected Method and its facility-bound Procedure graph.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AllocatedMethod {
    pub choice: LocalId,
    pub source_operation: IntentOperationId,
    pub method: MethodId,
    pub tasks: Vec<AllocatedProcedureTask>,
}

/// One semantic Procedure node. The task remains present even when it is manual and therefore has
/// no adapter invocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AllocatedProcedureTask {
    pub id: LocalId,
    pub operation: lab_capability::OperationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<ProcedureProgram>,
    pub inputs: Vec<PlanningTaskInput>,
    pub outputs: Vec<PlanningTaskOutput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<PlanningProcedureParameter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<SelectedMaterialBinding>,
    pub requirements: Vec<AllocatedRequirementBinding>,
}

/// The exact catalog and optional implementation binding for one semantic requirement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AllocatedRequirementBinding {
    pub id: LocalId,
    pub capability_kind: CapabilityKind,
    pub minimum_qualification: QualificationLevel,
    pub accepted_control_modes: BTreeSet<ControlMode>,
    pub offering: String,
    pub asset: String,
    pub observed_qualification: String,
    pub control_mode: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<SelectedCapabilityParameter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedure_implementation: Option<ProcedureImplementationId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<InvocationAdapter>,
}

/// One exact driver and profile used to group work for a physical Asset.
///
/// Versioned Procedure implementation identities remain on individual requirement bindings so a
/// single adapter invocation can realize several explicit contracts without splitting the Asset.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct InvocationAdapter {
    pub driver: String,
    pub profile_path: PathBuf,
    pub profile_sha256: String,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub features: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub accepted_run_formats: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub emitted_run_formats: BTreeSet<String>,
}

/// One exact Asset/adapter invocation. Tasks and requirements refer to the semantic graph above;
/// an adapter never receives unresolved method alternatives.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AdapterInvocation {
    pub id: String,
    pub asset: String,
    pub adapter: InvocationAdapter,
    pub tasks: Vec<LocalId>,
    pub requirements: Vec<LocalId>,
}

impl AdapterInvocationPlan {
    pub(crate) fn project(
        problem: &PlanningProblem,
        solution: &FacilityPlanningSolution,
        allocated_lair_sha256: String,
        material_inventory: MaterialLotBuildInventory,
    ) -> Result<Self, AdapterInvocationError> {
        solution.validate_against(problem)?;
        let selections = solution
            .selections
            .iter()
            .map(|selection| (selection.choice.clone(), selection))
            .collect::<BTreeMap<_, _>>();
        let mut groups = BTreeMap::<(String, InvocationAdapter), InvocationMembers>::new();
        let mut methods = Vec::new();

        for choice in &problem.choices {
            let selection = selections
                .get(&choice.id)
                .expect("solution validation proves every choice has a selection");
            let candidate = choice
                .candidates
                .iter()
                .find(|candidate| candidate.method == selection.method)
                .expect("solution validation proves the selected Method exists");
            let selected_tasks = selection
                .tasks
                .iter()
                .map(|task| (task.task.clone(), task))
                .collect::<BTreeMap<_, _>>();
            let mut tasks = Vec::new();
            for task in &candidate.tasks {
                let selected = selected_tasks
                    .get(&task.id)
                    .expect("solution validation proves every selected task exists");
                let selected_requirements = selected
                    .requirements
                    .iter()
                    .map(|requirement| (requirement.requirement.clone(), requirement))
                    .collect::<BTreeMap<_, _>>();
                let requirements = task
                    .requirements
                    .iter()
                    .map(|requirement| {
                        let selected = selected_requirements
                            .get(&requirement.id)
                            .expect("solution validation proves every selected Requirement exists");
                        let adapter = selected.adapter.as_ref().map(|adapter| InvocationAdapter {
                            driver: adapter.driver.clone(),
                            profile_path: adapter.profile_path.clone(),
                            profile_sha256: adapter.profile_sha256.clone(),
                            features: adapter.features.clone(),
                            accepted_run_formats: adapter.accepted_run_formats.clone(),
                            emitted_run_formats: adapter.emitted_run_formats.clone(),
                        });
                        if let Some(adapter) = &adapter {
                            let members = groups
                                .entry((selected.asset.clone(), adapter.clone()))
                                .or_default();
                            members.tasks.insert(task.id.clone());
                            members.requirements.insert(requirement.id.clone());
                        }
                        AllocatedRequirementBinding {
                            id: requirement.id.clone(),
                            capability_kind: selected.capability_kind.clone(),
                            minimum_qualification: selected.minimum_qualification,
                            accepted_control_modes: selected.accepted_control_modes.clone(),
                            offering: selected.offering.clone(),
                            asset: selected.asset.clone(),
                            observed_qualification: selected.observed_qualification.clone(),
                            control_mode: selected.control_mode.clone(),
                            parameters: selected.parameters.clone(),
                            procedure_implementation: selected
                                .adapter
                                .as_ref()
                                .and_then(|adapter| adapter.procedure_implementation.clone()),
                            adapter,
                        }
                    })
                    .collect();
                tasks.push(AllocatedProcedureTask {
                    id: task.id.clone(),
                    operation: task.operation.clone(),
                    program: task.program.clone(),
                    inputs: task.inputs.clone(),
                    outputs: task.outputs.clone(),
                    parameters: task.parameters.clone(),
                    materials: selected.materials.clone(),
                    requirements,
                });
            }
            methods.push(AllocatedMethod {
                choice: choice.id.clone(),
                source_operation: choice.source_operation.clone(),
                method: selection.method.clone(),
                tasks,
            });
        }

        let invocations = groups
            .into_iter()
            .map(|((asset, adapter), members)| AdapterInvocation {
                id: adapter_invocation_id(&asset, &adapter),
                asset,
                adapter,
                tasks: members.tasks.into_iter().collect(),
                requirements: members.requirements.into_iter().collect(),
            })
            .collect();
        let plan = Self {
            schema_version: ADAPTER_INVOCATIONS_SCHEMA_VERSION.to_owned(),
            problem_sha256: solution.problem_sha256.clone(),
            allocated_lair_sha256,
            inventory_sha256: solution.inventory_sha256.clone(),
            facility: solution.facility.clone(),
            material_inventory,
            methods,
            invocations,
        };
        plan.validate()?;
        Ok(plan)
    }

    /// Revalidate a deserialized invocation document before a backend consumes it.
    pub fn validate(&self) -> Result<(), AdapterInvocationValidationError> {
        if self.schema_version != ADAPTER_INVOCATIONS_SCHEMA_VERSION {
            return Err(AdapterInvocationValidationError::WrongSchema {
                found: self.schema_version.clone(),
            });
        }
        for (label, digest) in [
            ("planning problem", &self.problem_sha256),
            ("allocated LAIR", &self.allocated_lair_sha256),
            ("inventory", &self.inventory_sha256),
        ] {
            if !is_sha256(digest) {
                return Err(AdapterInvocationValidationError::InvalidDigest { label });
            }
        }
        if AbsoluteIri::new(&self.facility).is_err() {
            return Err(AdapterInvocationValidationError::InvalidFacility);
        }
        validate_material_inventory(self)?;
        if self.methods.is_empty() {
            return Err(AdapterInvocationValidationError::EmptyMethods);
        }
        let known_choices = self
            .methods
            .iter()
            .map(|method| method.choice.clone())
            .collect::<BTreeSet<_>>();
        let mut choices = BTreeSet::new();
        let mut tasks = BTreeSet::new();
        let mut materials = BTreeSet::new();
        let mut requirements = BTreeMap::new();
        for method in &self.methods {
            if !choices.insert(method.choice.clone()) {
                return Err(AdapterInvocationValidationError::DuplicateChoice {
                    choice: method.choice.clone(),
                });
            }
            if method.tasks.is_empty() {
                return Err(AdapterInvocationValidationError::EmptyMethod {
                    choice: method.choice.clone(),
                });
            }
            for task in &method.tasks {
                if !tasks.insert(task.id.clone()) {
                    return Err(AdapterInvocationValidationError::DuplicateTask {
                        task: task.id.clone(),
                    });
                }
                if task.requirements.is_empty() {
                    return Err(AdapterInvocationValidationError::EmptyTask {
                        task: task.id.clone(),
                    });
                }
                if let Some(program) = &task.program {
                    let validated = program.validate().map_err(|error| {
                        AdapterInvocationValidationError::InvalidProcedureProgram {
                            task: task.id.clone(),
                            message: error.to_string(),
                        }
                    })?;
                    validate_program_bindings(task, &validated)?;
                }
                for material in &task.materials {
                    if !materials.insert(material.input.clone()) {
                        return Err(AdapterInvocationValidationError::DuplicateMaterialInput {
                            input: material.input.clone(),
                        });
                    }
                    if material.symbol.is_empty()
                        || !valid_material_source(
                            &material.symbol,
                            &material.source,
                            &known_choices,
                            &self.material_inventory,
                        )
                    {
                        return Err(AdapterInvocationValidationError::InvalidMaterialBinding {
                            input: material.input.clone(),
                        });
                    }
                }
                for requirement in &task.requirements {
                    if AbsoluteIri::new(&requirement.offering).is_err()
                        || AbsoluteIri::new(&requirement.asset).is_err()
                    {
                        return Err(AdapterInvocationValidationError::InvalidBinding {
                            requirement: requirement.id.clone(),
                        });
                    }
                    let observed_qualification =
                        QualificationLevel::try_from(requirement.observed_qualification.as_str())
                            .map_err(|_| AdapterInvocationValidationError::InvalidBinding {
                            requirement: requirement.id.clone(),
                        })?;
                    let observed_control = ControlMode::try_from(requirement.control_mode.as_str())
                        .map_err(|_| AdapterInvocationValidationError::InvalidBinding {
                            requirement: requirement.id.clone(),
                        })?;
                    if !requirement
                        .minimum_qualification
                        .is_satisfied_by(observed_qualification)
                        || !requirement
                            .accepted_control_modes
                            .contains(&observed_control)
                    {
                        return Err(AdapterInvocationValidationError::InvalidBinding {
                            requirement: requirement.id.clone(),
                        });
                    }
                    let needs_implementation =
                        task.program.is_some() && requirement.adapter.is_some();
                    if requirement.procedure_implementation.is_some() != needs_implementation {
                        return Err(
                            AdapterInvocationValidationError::ProcedureImplementationBinding {
                                task: task.id.clone(),
                                requirement: requirement.id.clone(),
                            },
                        );
                    }
                    if requirements
                        .insert(requirement.id.clone(), (task.id.clone(), requirement))
                        .is_some()
                    {
                        return Err(AdapterInvocationValidationError::DuplicateRequirement {
                            requirement: requirement.id.clone(),
                        });
                    }
                }
            }
        }

        let mut invocation_ids = BTreeSet::new();
        let mut invoked_requirements = BTreeSet::new();
        for invocation in &self.invocations {
            if invocation.id != adapter_invocation_id(&invocation.asset, &invocation.adapter) {
                return Err(AdapterInvocationValidationError::InvalidInvocation {
                    invocation: invocation.id.clone(),
                });
            }
            if !invocation_ids.insert(invocation.id.as_str()) {
                return Err(AdapterInvocationValidationError::DuplicateInvocation {
                    invocation: invocation.id.clone(),
                });
            }
            if AbsoluteIri::new(&invocation.asset).is_err()
                || invocation.adapter.driver.is_empty()
                || !is_relative_path(&invocation.adapter.profile_path)
                || !is_sha256(&invocation.adapter.profile_sha256)
                || invocation
                    .adapter
                    .accepted_run_formats
                    .iter()
                    .chain(&invocation.adapter.emitted_run_formats)
                    .any(|format| format.is_empty())
            {
                return Err(AdapterInvocationValidationError::InvalidInvocation {
                    invocation: invocation.id.clone(),
                });
            }
            if invocation.requirements.is_empty() || invocation.tasks.is_empty() {
                return Err(AdapterInvocationValidationError::EmptyInvocation {
                    invocation: invocation.id.clone(),
                });
            }
            let mut invocation_tasks = BTreeSet::new();
            for task in &invocation.tasks {
                if !invocation_tasks.insert(task) || !tasks.contains(task) {
                    return Err(AdapterInvocationValidationError::UnknownTask {
                        invocation: invocation.id.clone(),
                        task: task.clone(),
                    });
                }
            }
            let mut invocation_requirements = BTreeSet::new();
            for requirement_id in &invocation.requirements {
                let Some((task, requirement)) = requirements.get(requirement_id) else {
                    return Err(AdapterInvocationValidationError::UnknownRequirement {
                        invocation: invocation.id.clone(),
                        requirement: requirement_id.clone(),
                    });
                };
                if !invocation_requirements.insert(requirement_id)
                    || !invocation.tasks.contains(task)
                    || requirement.asset != invocation.asset
                    || requirement.adapter.as_ref() != Some(&invocation.adapter)
                    || !invoked_requirements.insert(requirement_id.clone())
                {
                    return Err(AdapterInvocationValidationError::InvocationMismatch {
                        invocation: invocation.id.clone(),
                        requirement: requirement_id.clone(),
                    });
                }
            }
        }
        let expected = requirements
            .into_iter()
            .filter_map(|(id, (_, requirement))| requirement.adapter.is_some().then_some(id))
            .collect::<BTreeSet<_>>();
        if invoked_requirements != expected {
            return Err(AdapterInvocationValidationError::InvocationCoverage);
        }
        Ok(())
    }
}

fn validate_program_bindings(
    task: &AllocatedProcedureTask,
    program: &ValidatedProcedureProgram,
) -> Result<(), AdapterInvocationValidationError> {
    match program {
        ValidatedProcedureProgram::PipettingV1(program) => {
            if program.as_program().vessels.iter().any(|vessel| {
                matches!(
                    &vessel.role,
                    lab_procedure::VesselRole::ProcedureInput { input }
                        if usize::try_from(*input).map_or(true, |input| input >= task.inputs.len())
                )
            }) {
                return Err(AdapterInvocationValidationError::ProcedureInputBindings {
                    task: task.id.clone(),
                });
            }
            let task_materials = task
                .materials
                .iter()
                .map(|material| material.input.as_str())
                .collect::<BTreeSet<_>>();
            let program_materials = program
                .as_program()
                .materials
                .iter()
                .map(|material| material.id.as_str())
                .collect::<BTreeSet<_>>();
            if !program_materials.is_subset(&task_materials) {
                return Err(
                    AdapterInvocationValidationError::ProcedureMaterialBindings {
                        task: task.id.clone(),
                    },
                );
            }
            let task_outputs = task
                .outputs
                .iter()
                .map(|output| output.name.as_str())
                .collect::<BTreeSet<_>>();
            let program_outputs = program
                .as_program()
                .outputs
                .iter()
                .map(|output| output.id.as_str())
                .collect::<BTreeSet<_>>();
            if task_outputs != program_outputs {
                return Err(AdapterInvocationValidationError::ProcedureOutputBindings {
                    task: task.id.clone(),
                });
            }
        }
        ValidatedProcedureProgram::ThermalV1(program) => {
            let program = program.as_program();
            if usize::try_from(program.load.input).map_or(true, |input| input >= task.inputs.len())
            {
                return Err(AdapterInvocationValidationError::ProcedureInputBindings {
                    task: task.id.clone(),
                });
            }
            if !task.materials.is_empty() {
                return Err(
                    AdapterInvocationValidationError::ProcedureMaterialBindings {
                        task: task.id.clone(),
                    },
                );
            }
            let task_outputs = task
                .outputs
                .iter()
                .map(|output| output.name.as_str())
                .collect::<BTreeSet<_>>();
            let program_outputs = program
                .load
                .outputs
                .iter()
                .map(|output| output.as_str())
                .collect::<BTreeSet<_>>();
            if task_outputs != program_outputs {
                return Err(AdapterInvocationValidationError::ProcedureOutputBindings {
                    task: task.id.clone(),
                });
            }
        }
    }
    let formula = program.capability_formula();
    if task.requirements.len() != formula.all_of.len() {
        return Err(
            AdapterInvocationValidationError::ProcedureCapabilityBindings {
                task: task.id.clone(),
            },
        );
    }
    for clause in &formula.all_of {
        let expected_id = format!("{}::requirement::{}", task.id, clause.role);
        let Some(requirement) = task
            .requirements
            .iter()
            .find(|requirement| requirement.id.as_str() == expected_id)
        else {
            return Err(
                AdapterInvocationValidationError::ProcedureCapabilityBindings {
                    task: task.id.clone(),
                },
            );
        };
        if requirement.capability_kind != clause.capability_kind
            || requirement.parameters.len() != clause.constraints.len()
            || !clause.constraints.iter().all(|constraint| {
                requirement.parameters.iter().any(|parameter| {
                    parameter.property_kind == constraint.property_kind
                        && parameter.relation == constraint.relation
                        && parameter.required == constraint.required
                })
            })
        {
            return Err(
                AdapterInvocationValidationError::ProcedureCapabilityBindings {
                    task: task.id.clone(),
                },
            );
        }
    }
    if formula.binding_scope == BindingScope::AtomicAssetAssembly {
        let Some(first) = task.requirements.first() else {
            unreachable!("the formula and requirement cardinalities were checked")
        };
        if task.requirements.iter().any(|requirement| {
            requirement.asset != first.asset
                || requirement.adapter != first.adapter
                || requirement.procedure_implementation != first.procedure_implementation
        }) {
            return Err(
                AdapterInvocationValidationError::ProcedureCapabilityBindings {
                    task: task.id.clone(),
                },
            );
        }
    }
    Ok(())
}

fn validate_material_inventory(
    plan: &AdapterInvocationPlan,
) -> Result<(), AdapterInvocationValidationError> {
    if plan.material_inventory.source_sha256 != plan.inventory_sha256
        || plan.material_inventory.facility != plan.facility
    {
        return Err(AdapterInvocationValidationError::MaterialInventoryMismatch);
    }
    for (kind, entries) in [
        ("material", &plan.material_inventory.materials),
        ("artifact", &plan.material_inventory.artifacts),
    ] {
        for (symbol, candidates) in entries {
            if symbol.is_empty() {
                return Err(AdapterInvocationValidationError::InvalidMaterialInventory {
                    kind,
                    symbol: symbol.clone(),
                });
            }
            let MaterialLotCandidates::Identified {
                component,
                material_lots,
            } = candidates
            else {
                continue;
            };
            let strictly_sorted = material_lots.windows(2).all(|lots| lots[0] < lots[1]);
            if AbsoluteIri::new(component).is_err()
                || material_lots
                    .iter()
                    .any(|material_lot| AbsoluteIri::new(material_lot).is_err())
                || !strictly_sorted
            {
                return Err(AdapterInvocationValidationError::InvalidMaterialInventory {
                    kind,
                    symbol: symbol.clone(),
                });
            }
        }
    }
    Ok(())
}

fn valid_material_source(
    symbol: &str,
    source: &SelectedMaterialSource,
    choices: &BTreeSet<LocalId>,
    inventory: &MaterialLotBuildInventory,
) -> bool {
    match source {
        SelectedMaterialSource::MaterialLot {
            component,
            material_lot,
        } => {
            let Some(MaterialLotCandidates::Identified {
                component: expected_component,
                material_lots,
            }) = inventory
                .materials
                .get(symbol)
                .or_else(|| inventory.artifacts.get(symbol))
            else {
                return false;
            };
            AbsoluteIri::new(component).is_ok()
                && AbsoluteIri::new(material_lot).is_ok()
                && component == expected_component
                && material_lots.contains(material_lot)
        }
        SelectedMaterialSource::ChoiceOutput { choice } => choices.contains(choice),
    }
}

#[derive(Default)]
struct InvocationMembers {
    tasks: BTreeSet<LocalId>,
    requirements: BTreeSet<LocalId>,
}

/// Derive the stable logical ID for an exact Asset and adapter binding.
pub fn adapter_invocation_id(asset: &str, adapter: &InvocationAdapter) -> String {
    let identity = format!(
        "{}\0{}\0{}\0{}",
        asset,
        adapter.driver,
        adapter.profile_path.display(),
        adapter.profile_sha256
    );
    let digest = hex_sha256(identity.as_bytes());
    format!("{}-{}", adapter.driver.replace('.', "-"), &digest[..12])
}

pub(crate) fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && !path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

#[derive(Debug, Error)]
pub enum AdapterInvocationError {
    #[error(transparent)]
    InvalidSolution(#[from] FacilityPlanningSolutionValidationError),
    #[error(transparent)]
    InvalidProjection(#[from] AdapterInvocationValidationError),
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum AdapterInvocationValidationError {
    #[error(
        "adapter invocations declare schema `{found}`, expected `{ADAPTER_INVOCATIONS_SCHEMA_VERSION}`"
    )]
    WrongSchema { found: String },
    #[error("adapter invocations contain an invalid {label} SHA-256 digest")]
    InvalidDigest { label: &'static str },
    #[error("adapter invocations name a facility that is not an absolute IRI")]
    InvalidFacility,
    #[error("adapter invocation material inventory does not match its inventory hash and facility")]
    MaterialInventoryMismatch,
    #[error("adapter invocation material inventory contains invalid {kind} `{symbol}`")]
    InvalidMaterialInventory { kind: &'static str, symbol: String },
    #[error("adapter invocations contain no selected Methods")]
    EmptyMethods,
    #[error("adapter invocations repeat Method choice `{choice}`")]
    DuplicateChoice { choice: LocalId },
    #[error("selected Method choice `{choice}` contains no Procedure tasks")]
    EmptyMethod { choice: LocalId },
    #[error("adapter invocations repeat Procedure task `{task}`")]
    DuplicateTask { task: LocalId },
    #[error("Procedure task `{task}` contains no capability requirements")]
    EmptyTask { task: LocalId },
    #[error("Procedure task `{task}` has an invalid normalized program: {message}")]
    InvalidProcedureProgram { task: LocalId, message: String },
    #[error("Procedure task `{task}` normalized program references an undeclared material input")]
    ProcedureMaterialBindings { task: LocalId },
    #[error("Procedure task `{task}` normalized program references an unavailable task input")]
    ProcedureInputBindings { task: LocalId },
    #[error("Procedure task `{task}` normalized program does not bind exactly its outputs")]
    ProcedureOutputBindings { task: LocalId },
    #[error(
        "Procedure task `{task}` does not preserve its normalized capability formula and atomic bindings"
    )]
    ProcedureCapabilityBindings { task: LocalId },
    #[error(
        "Procedure task `{task}` requirement `{requirement}` has an inconsistent Procedure implementation binding"
    )]
    ProcedureImplementationBinding { task: LocalId, requirement: LocalId },
    #[error("adapter invocations repeat Procedure material input `{input}`")]
    DuplicateMaterialInput { input: LocalId },
    #[error("Procedure material input `{input}` has an invalid physical source")]
    InvalidMaterialBinding { input: LocalId },
    #[error("adapter invocations repeat capability requirement `{requirement}`")]
    DuplicateRequirement { requirement: LocalId },
    #[error("capability requirement `{requirement}` has an invalid offering or Asset IRI")]
    InvalidBinding { requirement: LocalId },
    #[error("adapter invocation ID `{invocation}` is empty or repeated")]
    DuplicateInvocation { invocation: String },
    #[error("adapter invocation `{invocation}` has invalid Asset, driver, profile, or digest data")]
    InvalidInvocation { invocation: String },
    #[error("adapter invocation `{invocation}` contains no tasks or requirements")]
    EmptyInvocation { invocation: String },
    #[error("adapter invocation `{invocation}` references unknown task `{task}`")]
    UnknownTask { invocation: String, task: LocalId },
    #[error("adapter invocation `{invocation}` references unknown requirement `{requirement}`")]
    UnknownRequirement {
        invocation: String,
        requirement: LocalId,
    },
    #[error("adapter invocation `{invocation}` does not match requirement `{requirement}`")]
    InvocationMismatch {
        invocation: String,
        requirement: LocalId,
    },
    #[error("adapter invocations do not cover every and only adapter-bound requirement")]
    InvocationCoverage,
}
