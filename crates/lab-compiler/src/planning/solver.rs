//! Global selection of method alternatives and exact facility capability bindings.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lab_capability::{
    AbsoluteIri, ConstraintRelation, ControlMode, MethodId, ProcedureImplementationId,
    PropertyConstraint, PropertyKind, PropertyValue, QualificationLevel, ScalarValue, UnitIri,
};
use lab_inventory::{
    FacilityAsset, FacilityAssetError, FacilityCapabilityOffering, FacilityCapabilityParameter,
    FacilityScalarValue, InventorySnapshot,
};
use lab_method::{IntentOperationId, LocalId};
use lab_procedure::BindingScope;
use sbol_inventory::vocabulary::Qualification;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::model::MaterialLotCandidates;
use super::{
    ADAPTER_BINDINGS_SCHEMA_VERSION, AdapterBindingSnapshot, MaterialLotBuildInventory,
    PlanningCapabilityRequirement, PlanningMaterialInput, PlanningMaterialSource,
    PlanningMethodCandidate, PlanningProblem, PlanningProblemValidationError,
    PlanningProcedureTask, ResolvedAdapterBinding,
};

pub const FACILITY_PLANNING_SOLUTION_SCHEMA_VERSION: &str = "lab.facility-planning-solution.v3";

/// Explicit choices that are allowed to turn an otherwise ambiguous solution space into a plan.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FacilityPlanningPolicy {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub method_pins: Vec<MethodPin>,
    #[serde(default)]
    pub adapter_requirement: AdapterRequirement,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct MethodPin {
    #[serde(flatten)]
    pub selector: MethodPinSelector,
    pub method: MethodId,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum MethodPinSelector {
    Choice { choice: LocalId },
    SourceOperation { source_operation: IntentOperationId },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdapterRequirement {
    /// Freeze a compatible configured adapter when one exists, while retaining manual and
    /// planning-only facility bindings.
    #[default]
    Optional,
    /// Require a configured planning adapter for every non-manual offering.
    NonManual,
}

/// One complete, reviewable solution to the facility-wide constraint problem.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FacilityPlanningSolution {
    pub schema_version: String,
    pub problem_sha256: String,
    pub inventory_sha256: String,
    pub facility: String,
    pub policy: FacilityPlanningPolicy,
    pub selections: Vec<SelectedMethod>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SelectedMethod {
    pub choice: LocalId,
    pub source_operation: IntentOperationId,
    pub method: MethodId,
    pub tasks: Vec<SelectedProcedureTask>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SelectedProcedureTask {
    pub task: LocalId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<SelectedMaterialBinding>,
    pub requirements: Vec<SelectedRequirementBinding>,
}

/// One exact physical input selected for a Procedure task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SelectedMaterialBinding {
    pub input: LocalId,
    pub symbol: String,
    pub source: SelectedMaterialSource,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SelectedMaterialSource {
    MaterialLot {
        component: String,
        material_lot: String,
    },
    ChoiceOutput {
        choice: LocalId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SelectedRequirementBinding {
    pub requirement: LocalId,
    pub capability_kind: lab_capability::CapabilityKind,
    pub minimum_qualification: QualificationLevel,
    pub accepted_control_modes: BTreeSet<ControlMode>,
    pub offering: String,
    pub asset: String,
    pub observed_qualification: String,
    pub control_mode: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<SelectedCapabilityParameter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<SelectedAdapter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected_candidates: Vec<PlanningRejectedOffering>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SelectedCapabilityParameter {
    pub property_kind: PropertyKind,
    pub relation: ConstraintRelation,
    pub required: PropertyValue,
    pub offering_parameter: String,
    pub observed: PropertyValue,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct SelectedAdapter {
    pub driver: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedure_implementation: Option<ProcedureImplementationId>,
    pub profile_path: PathBuf,
    pub profile_sha256: String,
    pub features: BTreeSet<String>,
    pub accepted_run_formats: BTreeSet<String>,
    pub emitted_run_formats: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningRejectedOffering {
    pub offering: String,
    pub asset: String,
    pub observed_qualification: String,
    pub control_mode: String,
    pub reasons: Vec<PlanningCandidateRejectionReason>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum PlanningCandidateRejectionReason {
    Inactive,
    InsufficientQualification {
        required: String,
        observed: String,
    },
    UnsupportedControlMode {
        accepted: BTreeSet<String>,
        observed: String,
    },
    MissingParameter {
        property_kind: String,
    },
    UnitMismatch {
        property_kind: String,
        required: Option<String>,
        observed: Option<String>,
    },
    ValueMismatch {
        property_kind: String,
        required: String,
        observed: String,
    },
    IncomparableValue {
        property_kind: String,
    },
    AtomicBindingConflict {
        binding_scope: BindingScope,
    },
    MissingPlanningAdapter,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RejectedMethodCandidate {
    pub method: MethodId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected_materials: Vec<RejectedPlanningMaterial>,
    pub rejected_requirements: Vec<RejectedPlanningRequirement>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RejectedPlanningMaterial {
    pub input: LocalId,
    pub symbol: String,
    pub reason: PlanningMaterialRejectionReason,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum PlanningMaterialRejectionReason {
    UnknownSymbol,
    MissingDesignIdentity,
    NoActiveMaterialLot { component: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RejectedPlanningRequirement {
    pub requirement: LocalId,
    pub capability_kind: lab_capability::CapabilityKind,
    pub candidates: Vec<PlanningRejectedOffering>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningAlternative {
    pub methods: Vec<AlternativeMethod>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AlternativeMethod {
    pub choice: LocalId,
    pub method: MethodId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<AlternativeMaterialBinding>,
    pub bindings: Vec<AlternativeRequirementBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AlternativeMaterialBinding {
    pub input: LocalId,
    pub symbol: String,
    pub source: SelectedMaterialSource,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AlternativeRequirementBinding {
    pub requirement: LocalId,
    pub offering: String,
    pub asset: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
}

#[derive(Debug, Error)]
pub enum FacilityPlanningError {
    #[error(transparent)]
    InvalidProblem(#[from] PlanningProblemValidationError),
    #[error(
        "adapter bindings declare schema `{found}`, expected `{ADAPTER_BINDINGS_SCHEMA_VERSION}`"
    )]
    WrongAdapterSchema { found: String },
    #[error(
        "adapter bindings freeze inventory `{binding_hash}` facility `{binding_facility}`, but planning uses inventory `{inventory_hash}` facility `{inventory_facility}`"
    )]
    AdapterInventoryMismatch {
        binding_hash: String,
        binding_facility: String,
        inventory_hash: String,
        inventory_facility: String,
    },
    #[error(
        "material inventory freezes inventory `{material_hash}` facility `{material_facility}`, but planning uses inventory `{inventory_hash}` facility `{inventory_facility}`"
    )]
    MaterialInventoryMismatch {
        material_hash: String,
        material_facility: String,
        inventory_hash: String,
        inventory_facility: String,
    },
    #[error(transparent)]
    Asset(#[from] FacilityAssetError),
    #[error("method policy repeats selector `{selector}`")]
    DuplicateMethodPin { selector: String },
    #[error("method policy selector `{selector}` does not match a planning choice")]
    UnmatchedMethodPin { selector: String },
    #[error("method policy pins choice `{choice}` to conflicting methods `{first}` and `{second}`")]
    ConflictingMethodPin {
        choice: LocalId,
        first: MethodId,
        second: MethodId,
    },
    #[error(
        "method policy pins choice `{choice}` to `{method}`, which is not one of its candidates"
    )]
    UnknownPinnedMethod { choice: LocalId, method: MethodId },
    #[error("method choice `{choice}` has no complete facility-feasible method")]
    NoFeasibleMethod {
        choice: LocalId,
        candidates: Vec<RejectedMethodCandidate>,
    },
    #[error("the facility admits more than one complete plan; add explicit allocation policy")]
    AmbiguousPlan {
        alternatives: Vec<PlanningAlternative>,
    },
}

impl FacilityPlanningSolution {
    pub fn solve(
        problem: &PlanningProblem,
        inventory: &InventorySnapshot,
        material_inventory: &MaterialLotBuildInventory,
        adapters: Option<&AdapterBindingSnapshot>,
        policy: FacilityPlanningPolicy,
    ) -> Result<Self, FacilityPlanningError> {
        problem.validate()?;
        validate_adapter_snapshot(inventory, adapters)?;
        validate_material_inventory(inventory, material_inventory)?;
        let pins = resolve_method_pins(problem, &policy)?;
        let assets = inventory.facility_assets()?;
        let mut alternatives = vec![Vec::<SelectedMethod>::new()];
        for choice in &problem.choices {
            let mut candidate_selections = Vec::new();
            let mut rejected_methods = Vec::new();
            for candidate in &choice.candidates {
                if pins
                    .get(&choice.id)
                    .is_some_and(|method| *method != candidate.method)
                {
                    continue;
                }
                match allocate_method(
                    candidate,
                    material_inventory,
                    &assets,
                    adapters,
                    policy.adapter_requirement,
                ) {
                    MethodAllocation::Feasible(mut selections) => {
                        candidate_selections.append(&mut selections)
                    }
                    MethodAllocation::Rejected(rejected) => {
                        rejected_methods.push(RejectedMethodCandidate {
                            method: candidate.method.clone(),
                            rejected_materials: rejected.materials,
                            rejected_requirements: rejected.requirements,
                        })
                    }
                }
            }
            if candidate_selections.is_empty() {
                return Err(FacilityPlanningError::NoFeasibleMethod {
                    choice: choice.id.clone(),
                    candidates: rejected_methods,
                });
            }
            let mut combined = Vec::new();
            'outer: for prefix in &alternatives {
                for candidate in &candidate_selections {
                    let mut selection = prefix.clone();
                    selection.push(SelectedMethod {
                        choice: choice.id.clone(),
                        source_operation: choice.source_operation.clone(),
                        method: candidate.method.clone(),
                        tasks: candidate.tasks.clone(),
                    });
                    combined.push(selection);
                    if combined.len() == 2 {
                        break 'outer;
                    }
                }
            }
            alternatives = combined;
        }
        if alternatives.len() > 1 {
            return Err(FacilityPlanningError::AmbiguousPlan {
                alternatives: alternatives
                    .iter()
                    .map(|selection| summarize_alternative(selection))
                    .collect(),
            });
        }
        let selections = alternatives
            .pop()
            .expect("a validated non-empty problem leaves one solution");
        Ok(Self {
            schema_version: FACILITY_PLANNING_SOLUTION_SCHEMA_VERSION.to_owned(),
            problem_sha256: problem.sha256(),
            inventory_sha256: inventory.source_sha256().to_owned(),
            facility: inventory.facility().as_str().to_owned(),
            policy,
            selections,
        })
    }

    /// Prove that stable decisions still refer to the exact problem from which they were solved.
    pub fn validate_against(
        &self,
        problem: &PlanningProblem,
    ) -> Result<(), FacilityPlanningSolutionValidationError> {
        if self.schema_version != FACILITY_PLANNING_SOLUTION_SCHEMA_VERSION {
            return Err(FacilityPlanningSolutionValidationError::WrongSchema {
                found: self.schema_version.clone(),
            });
        }
        let expected = problem.sha256();
        if self.problem_sha256 != expected {
            return Err(FacilityPlanningSolutionValidationError::ProblemDigest {
                expected,
                found: self.problem_sha256.clone(),
            });
        }
        if self.selections.len() != problem.choices.len() {
            return Err(FacilityPlanningSolutionValidationError::ChoiceSet);
        }
        for (selection, choice) in self.selections.iter().zip(&problem.choices) {
            if selection.choice != choice.id
                || selection.source_operation != choice.source_operation
            {
                return Err(FacilityPlanningSolutionValidationError::ChoiceSet);
            }
            let candidate = choice
                .candidates
                .iter()
                .find(|candidate| candidate.method == selection.method)
                .ok_or_else(|| FacilityPlanningSolutionValidationError::UnknownMethod {
                    choice: selection.choice.clone(),
                    method: selection.method.clone(),
                })?;
            if selection.tasks.len() != candidate.tasks.len() {
                return Err(FacilityPlanningSolutionValidationError::TaskSet {
                    choice: selection.choice.clone(),
                });
            }
            for (selected_task, task) in selection.tasks.iter().zip(&candidate.tasks) {
                if selected_task.task != task.id
                    || selected_task.materials.len() != task.materials.len()
                    || selected_task.requirements.len() != task.requirements.len()
                {
                    return Err(FacilityPlanningSolutionValidationError::TaskSet {
                        choice: selection.choice.clone(),
                    });
                }
                for (binding, material) in selected_task.materials.iter().zip(&task.materials) {
                    if binding.input != material.id
                        || binding.symbol != material.symbol
                        || !material_source_matches(&binding.source, &material.source)
                    {
                        return Err(FacilityPlanningSolutionValidationError::MaterialSet {
                            task: task.id.clone(),
                        });
                    }
                    if let SelectedMaterialSource::MaterialLot {
                        component,
                        material_lot,
                    } = &binding.source
                        && (AbsoluteIri::new(component).is_err()
                            || AbsoluteIri::new(material_lot).is_err())
                    {
                        return Err(
                            FacilityPlanningSolutionValidationError::InvalidMaterialBinding {
                                input: binding.input.clone(),
                            },
                        );
                    }
                }
                for (binding, requirement) in
                    selected_task.requirements.iter().zip(&task.requirements)
                {
                    if binding.requirement != requirement.id
                        || binding.capability_kind != requirement.capability_kind
                        || binding.minimum_qualification != requirement.minimum_qualification
                        || binding.accepted_control_modes != requirement.accepted_control_modes
                    {
                        return Err(FacilityPlanningSolutionValidationError::RequirementSet {
                            task: task.id.clone(),
                        });
                    }
                    if binding.adapter.as_ref().is_some_and(|adapter| {
                        adapter.procedure_implementation.is_some() != task.program.is_some()
                    }) {
                        return Err(
                            FacilityPlanningSolutionValidationError::ProcedureImplementation {
                                task: task.id.clone(),
                            },
                        );
                    }
                }
                if task.binding_scope == BindingScope::AtomicAssetAssembly {
                    let first = selected_task
                        .requirements
                        .first()
                        .expect("validated tasks have capability requirements");
                    if selected_task.requirements.iter().any(|requirement| {
                        requirement.asset != first.asset || requirement.adapter != first.adapter
                    }) {
                        return Err(FacilityPlanningSolutionValidationError::AtomicBinding {
                            task: task.id.clone(),
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum FacilityPlanningSolutionValidationError {
    #[error(
        "facility solution declares schema `{found}`, expected `{FACILITY_PLANNING_SOLUTION_SCHEMA_VERSION}`"
    )]
    WrongSchema { found: String },
    #[error("facility solution freezes planning problem `{found}`, expected `{expected}`")]
    ProblemDigest { expected: String, found: String },
    #[error("facility solution does not select every planning choice exactly once and in order")]
    ChoiceSet,
    #[error("facility solution selects unknown method `{method}` for choice `{choice}`")]
    UnknownMethod { choice: LocalId, method: MethodId },
    #[error(
        "facility solution does not preserve the exact Procedure task set for choice `{choice}`"
    )]
    TaskSet { choice: LocalId },
    #[error("facility solution does not preserve the exact requirement set for task `{task}`")]
    RequirementSet { task: LocalId },
    #[error(
        "facility solution does not bind a Procedure implementation exactly when normalized task `{task}` requires one"
    )]
    ProcedureImplementation { task: LocalId },
    #[error("facility solution splits atomic Procedure task `{task}` across several bindings")]
    AtomicBinding { task: LocalId },
    #[error("facility solution does not preserve the exact material-input set for task `{task}`")]
    MaterialSet { task: LocalId },
    #[error("facility solution contains an invalid MaterialLot binding for input `{input}`")]
    InvalidMaterialBinding { input: LocalId },
}

#[derive(Clone)]
struct SelectedMethodCandidate {
    method: MethodId,
    tasks: Vec<SelectedProcedureTask>,
}

enum MethodAllocation {
    Feasible(Vec<SelectedMethodCandidate>),
    Rejected(MethodRejections),
}

#[derive(Default)]
struct MethodRejections {
    materials: Vec<RejectedPlanningMaterial>,
    requirements: Vec<RejectedPlanningRequirement>,
}

#[derive(Clone)]
struct RequirementCandidate {
    binding: SelectedRequirementBinding,
}

fn allocate_method(
    candidate: &PlanningMethodCandidate,
    material_inventory: &MaterialLotBuildInventory,
    assets: &[FacilityAsset],
    adapters: Option<&AdapterBindingSnapshot>,
    adapter_requirement: AdapterRequirement,
) -> MethodAllocation {
    let mut tasks = Vec::new();
    let mut rejected = MethodRejections::default();
    for task in &candidate.tasks {
        let mut task_materials = Vec::new();
        for material in &task.materials {
            match material_candidates(material, material_inventory) {
                Ok(candidates) => task_materials.push(candidates),
                Err(rejection) => rejected.materials.push(rejection),
            }
        }
        let mut task_requirements = Vec::new();
        for requirement in &task.requirements {
            let (bindings, rejections) =
                requirement_candidates(task, requirement, assets, adapters, adapter_requirement);
            if bindings.is_empty() {
                rejected.requirements.push(RejectedPlanningRequirement {
                    requirement: requirement.id.clone(),
                    capability_kind: requirement.capability_kind.clone(),
                    candidates: rejections,
                });
            } else {
                task_requirements.push((requirement.clone(), bindings, rejections));
            }
        }
        tasks.push((
            task.id.clone(),
            task.binding_scope,
            task_materials,
            task_requirements,
        ));
    }
    if !rejected.materials.is_empty() || !rejected.requirements.is_empty() {
        return MethodAllocation::Rejected(rejected);
    }

    let mut alternatives = vec![Vec::<SelectedProcedureTask>::new()];
    for (task, binding_scope, materials, requirements) in tasks {
        let mut material_alternatives = vec![Vec::<SelectedMaterialBinding>::new()];
        for candidates in materials {
            let mut combined = Vec::new();
            'outer: for prefix in &material_alternatives {
                for candidate in &candidates {
                    let mut selection = prefix.clone();
                    selection.push(candidate.clone());
                    combined.push(selection);
                    if combined.len() == 2 {
                        break 'outer;
                    }
                }
            }
            material_alternatives = combined;
        }
        let task_alternatives = requirement_alternatives(binding_scope, &requirements);
        if task_alternatives.is_empty() {
            rejected
                .requirements
                .extend(atomic_binding_rejections(binding_scope, &requirements));
            return MethodAllocation::Rejected(rejected);
        }
        let mut combined = Vec::new();
        'outer: for prefix in &alternatives {
            for materials in &material_alternatives {
                for requirements in &task_alternatives {
                    let mut selection = prefix.clone();
                    selection.push(SelectedProcedureTask {
                        task: task.clone(),
                        materials: materials.clone(),
                        requirements: requirements.clone(),
                    });
                    combined.push(selection);
                    if combined.len() == 2 {
                        break 'outer;
                    }
                }
            }
        }
        alternatives = combined;
    }
    MethodAllocation::Feasible(
        alternatives
            .into_iter()
            .map(|tasks| SelectedMethodCandidate {
                method: candidate.method.clone(),
                tasks,
            })
            .collect(),
    )
}

type RequirementCandidates = (
    PlanningCapabilityRequirement,
    Vec<RequirementCandidate>,
    Vec<PlanningRejectedOffering>,
);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct AtomicBindingKey {
    asset: String,
    adapter: Option<SelectedAdapter>,
}

fn requirement_alternatives(
    binding_scope: BindingScope,
    requirements: &[RequirementCandidates],
) -> Vec<Vec<SelectedRequirementBinding>> {
    match binding_scope {
        BindingScope::Independent => combine_requirement_candidates(requirements, None),
        BindingScope::AtomicAssetAssembly => {
            let Some((_, first, _)) = requirements.first() else {
                return Vec::new();
            };
            let keys = first
                .iter()
                .map(|candidate| atomic_binding_key(&candidate.binding))
                .collect::<BTreeSet<_>>();
            let mut alternatives = Vec::new();
            for key in keys {
                alternatives.extend(combine_requirement_candidates(requirements, Some(&key)));
                if alternatives.len() >= 2 {
                    alternatives.truncate(2);
                    break;
                }
            }
            alternatives
        }
    }
}

fn combine_requirement_candidates(
    requirements: &[RequirementCandidates],
    atomic_key: Option<&AtomicBindingKey>,
) -> Vec<Vec<SelectedRequirementBinding>> {
    let mut alternatives = vec![Vec::new()];
    for (_, candidates, rejections) in requirements {
        let mut combined = Vec::new();
        'outer: for prefix in &alternatives {
            for candidate in candidates.iter().filter(|candidate| {
                atomic_key.is_none_or(|key| atomic_binding_key(&candidate.binding) == *key)
            }) {
                let mut binding = candidate.binding.clone();
                binding.rejected_candidates = rejections.clone();
                let mut selection = prefix.clone();
                selection.push(binding);
                combined.push(selection);
                if combined.len() == 2 {
                    break 'outer;
                }
            }
        }
        alternatives = combined;
        if alternatives.is_empty() {
            break;
        }
    }
    alternatives
}

fn atomic_binding_key(binding: &SelectedRequirementBinding) -> AtomicBindingKey {
    AtomicBindingKey {
        asset: binding.asset.clone(),
        adapter: binding.adapter.clone(),
    }
}

fn atomic_binding_rejections(
    binding_scope: BindingScope,
    requirements: &[RequirementCandidates],
) -> Vec<RejectedPlanningRequirement> {
    requirements
        .iter()
        .map(|(requirement, eligible, rejected)| {
            let mut candidates = rejected.clone();
            candidates.extend(eligible.iter().map(|candidate| PlanningRejectedOffering {
                offering: candidate.binding.offering.clone(),
                asset: candidate.binding.asset.clone(),
                observed_qualification: candidate.binding.observed_qualification.clone(),
                control_mode: candidate.binding.control_mode.clone(),
                reasons: vec![PlanningCandidateRejectionReason::AtomicBindingConflict {
                    binding_scope,
                }],
            }));
            candidates.sort_by(|left, right| {
                (&left.asset, &left.offering).cmp(&(&right.asset, &right.offering))
            });
            RejectedPlanningRequirement {
                requirement: requirement.id.clone(),
                capability_kind: requirement.capability_kind.clone(),
                candidates,
            }
        })
        .collect()
}

fn material_candidates(
    material: &PlanningMaterialInput,
    inventory: &MaterialLotBuildInventory,
) -> Result<Vec<SelectedMaterialBinding>, RejectedPlanningMaterial> {
    if let PlanningMaterialSource::ChoiceOutput { choice } = &material.source {
        return Ok(vec![SelectedMaterialBinding {
            input: material.id.clone(),
            symbol: material.symbol.clone(),
            source: SelectedMaterialSource::ChoiceOutput {
                choice: choice.clone(),
            },
        }]);
    }
    let candidates = inventory
        .materials
        .get(&material.symbol)
        .or_else(|| inventory.artifacts.get(&material.symbol))
        .ok_or_else(|| {
            rejected_material(material, PlanningMaterialRejectionReason::UnknownSymbol)
        })?;
    let MaterialLotCandidates::Identified {
        component,
        material_lots,
    } = candidates
    else {
        return Err(rejected_material(
            material,
            PlanningMaterialRejectionReason::MissingDesignIdentity,
        ));
    };
    if material_lots.is_empty() {
        return Err(rejected_material(
            material,
            PlanningMaterialRejectionReason::NoActiveMaterialLot {
                component: component.clone(),
            },
        ));
    }
    Ok(material_lots
        .iter()
        .map(|material_lot| SelectedMaterialBinding {
            input: material.id.clone(),
            symbol: material.symbol.clone(),
            source: SelectedMaterialSource::MaterialLot {
                component: component.clone(),
                material_lot: material_lot.clone(),
            },
        })
        .collect())
}

fn rejected_material(
    material: &PlanningMaterialInput,
    reason: PlanningMaterialRejectionReason,
) -> RejectedPlanningMaterial {
    RejectedPlanningMaterial {
        input: material.id.clone(),
        symbol: material.symbol.clone(),
        reason,
    }
}

fn material_source_matches(
    selected: &SelectedMaterialSource,
    planned: &PlanningMaterialSource,
) -> bool {
    matches!(
        (selected, planned),
        (
            SelectedMaterialSource::MaterialLot { .. },
            PlanningMaterialSource::Inventory
        )
    ) || matches!(
        (selected, planned),
        (
            SelectedMaterialSource::ChoiceOutput { choice: selected },
            PlanningMaterialSource::ChoiceOutput { choice: planned }
        ) if selected == planned
    )
}

fn requirement_candidates(
    task: &PlanningProcedureTask,
    requirement: &PlanningCapabilityRequirement,
    assets: &[FacilityAsset],
    adapters: Option<&AdapterBindingSnapshot>,
    adapter_requirement: AdapterRequirement,
) -> (Vec<RequirementCandidate>, Vec<PlanningRejectedOffering>) {
    let minimum = inventory_qualification(requirement.minimum_qualification);
    let accepted = requirement
        .accepted_control_modes
        .iter()
        .map(|mode| mode.iri().to_owned())
        .collect::<BTreeSet<_>>();
    let mut eligible = Vec::new();
    let mut rejected = Vec::new();
    for asset in assets {
        for offering in &asset.offerings {
            if offering.capability_kind.as_str() != requirement.capability_kind.as_str() {
                continue;
            }
            let mut reasons = Vec::new();
            if !offering.effectively_active {
                reasons.push(PlanningCandidateRejectionReason::Inactive);
            }
            if offering.qualification < minimum {
                reasons.push(
                    PlanningCandidateRejectionReason::InsufficientQualification {
                        required: requirement.minimum_qualification.iri().to_owned(),
                        observed: offering.qualification.iri().to_owned(),
                    },
                );
            }
            if !accepted.contains(offering.control_mode.iri()) {
                reasons.push(PlanningCandidateRejectionReason::UnsupportedControlMode {
                    accepted: accepted.clone(),
                    observed: offering.control_mode.iri().to_owned(),
                });
            }
            let mut parameters = Vec::new();
            for constraint in &requirement.constraints {
                match match_parameter(constraint, offering) {
                    Ok(parameter) => parameters.push(parameter),
                    Err(reason) => reasons.push(reason),
                }
            }
            let adapter_candidates = matching_adapters(
                adapters,
                task,
                &requirement.capability_kind,
                asset,
                offering,
            );
            if adapter_requirement == AdapterRequirement::NonManual
                && offering.control_mode.iri() != ControlMode::Manual.iri()
                && adapter_candidates.is_empty()
            {
                reasons.push(PlanningCandidateRejectionReason::MissingPlanningAdapter);
            }
            if reasons.is_empty() {
                let adapter_candidates = if adapter_candidates.is_empty() {
                    vec![None]
                } else {
                    adapter_candidates.into_iter().map(Some).collect()
                };
                eligible.extend(adapter_candidates.into_iter().map(|adapter| {
                    RequirementCandidate {
                        binding: SelectedRequirementBinding {
                            requirement: requirement.id.clone(),
                            capability_kind: requirement.capability_kind.clone(),
                            minimum_qualification: requirement.minimum_qualification,
                            accepted_control_modes: requirement.accepted_control_modes.clone(),
                            offering: offering.identity.as_str().to_owned(),
                            asset: asset.identity.as_str().to_owned(),
                            observed_qualification: offering.qualification.iri().to_owned(),
                            control_mode: offering.control_mode.iri().to_owned(),
                            parameters: parameters.clone(),
                            adapter,
                            rejected_candidates: Vec::new(),
                        },
                    }
                }));
            } else {
                rejected.push(rejected_offering(asset, offering, reasons));
            }
        }
    }
    eligible.sort_by(|left, right| binding_key(&left.binding).cmp(&binding_key(&right.binding)));
    rejected
        .sort_by(|left, right| (&left.asset, &left.offering).cmp(&(&right.asset, &right.offering)));
    (eligible, rejected)
}

fn match_parameter(
    constraint: &PropertyConstraint,
    offering: &FacilityCapabilityOffering,
) -> Result<SelectedCapabilityParameter, PlanningCandidateRejectionReason> {
    let Some(parameter) = offering
        .parameters
        .iter()
        .find(|parameter| parameter.property_kind.as_str() == constraint.property_kind.as_str())
    else {
        return Err(PlanningCandidateRejectionReason::MissingParameter {
            property_kind: constraint.property_kind.to_string(),
        });
    };
    let observed = property_value(parameter).ok_or_else(|| {
        PlanningCandidateRejectionReason::IncomparableValue {
            property_kind: constraint.property_kind.to_string(),
        }
    })?;
    if constraint.required.unit != observed.unit {
        return Err(PlanningCandidateRejectionReason::UnitMismatch {
            property_kind: constraint.property_kind.to_string(),
            required: constraint.required.unit.as_ref().map(ToString::to_string),
            observed: observed.unit.as_ref().map(ToString::to_string),
        });
    }
    match constraint.is_satisfied_by(&observed) {
        Ok(true) => Ok(SelectedCapabilityParameter {
            property_kind: constraint.property_kind.clone(),
            relation: constraint.relation,
            required: constraint.required.clone(),
            offering_parameter: parameter.identity.as_str().to_owned(),
            observed,
        }),
        Ok(false) => Err(PlanningCandidateRejectionReason::ValueMismatch {
            property_kind: constraint.property_kind.to_string(),
            required: render_property_value(&constraint.required),
            observed: render_property_value(&observed),
        }),
        Err(_) => Err(PlanningCandidateRejectionReason::IncomparableValue {
            property_kind: constraint.property_kind.to_string(),
        }),
    }
}

fn property_value(parameter: &FacilityCapabilityParameter) -> Option<PropertyValue> {
    let value = match &parameter.value {
        FacilityScalarValue::Text(value) => ScalarValue::Text(value.clone()),
        FacilityScalarValue::Integer(value) => {
            ScalarValue::Integer(lab_capability::ExactInteger::parse(value).ok()?)
        }
        FacilityScalarValue::Real(value) => {
            ScalarValue::Real(lab_capability::ExactDecimal::parse(value).ok()?)
        }
        FacilityScalarValue::Boolean(value) => ScalarValue::Boolean(*value),
        FacilityScalarValue::Iri(value) => ScalarValue::Iri(AbsoluteIri::new(value.as_str()).ok()?),
    };
    let unit = parameter
        .unit
        .as_ref()
        .map(|unit| UnitIri::new(unit.as_str()))
        .transpose()
        .ok()?;
    PropertyValue::new(value, unit).ok()
}

fn matching_adapters(
    adapters: Option<&AdapterBindingSnapshot>,
    task: &PlanningProcedureTask,
    capability_kind: &lab_capability::CapabilityKind,
    asset: &FacilityAsset,
    offering: &FacilityCapabilityOffering,
) -> Vec<SelectedAdapter> {
    let Some(adapters) = adapters else {
        return Vec::new();
    };
    let mut candidates = adapters
        .bindings
        .iter()
        .filter(|binding| binding.asset == asset.identity.as_str())
        .filter(|binding| adapter_supports(binding, offering.identity.as_str()))
        .flat_map(|binding| {
            selected_adapters(binding, task, capability_kind, offering.control_mode.iri())
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        (
            &left.driver,
            &left.procedure_implementation,
            &left.profile_path,
        )
            .cmp(&(
                &right.driver,
                &right.procedure_implementation,
                &right.profile_path,
            ))
    });
    candidates
}

fn adapter_supports(binding: &ResolvedAdapterBinding, offering: &str) -> bool {
    binding
        .offerings
        .iter()
        .any(|candidate| candidate.offering == offering && candidate.planning_eligible)
}

fn selected_adapters(
    binding: &ResolvedAdapterBinding,
    task: &PlanningProcedureTask,
    capability_kind: &lab_capability::CapabilityKind,
    control_mode: &str,
) -> Vec<SelectedAdapter> {
    let Some(program) = &task.program else {
        return vec![selected_adapter(binding, None)];
    };
    binding
        .procedure_implementations
        .iter()
        .filter(|implementation| implementation.contract == program.contract)
        .filter(|implementation| implementation.operations.contains(&task.operation))
        .filter(|implementation| implementation.capability_kinds.contains(capability_kind))
        .filter(|implementation| implementation.services.planning)
        .filter(|implementation| {
            implementation
                .control_modes
                .iter()
                .any(|mode| mode.iri() == control_mode)
        })
        .map(|implementation| selected_adapter(binding, Some(implementation)))
        .collect()
}

fn selected_adapter(
    binding: &ResolvedAdapterBinding,
    implementation: Option<&super::BoundProcedureImplementation>,
) -> SelectedAdapter {
    SelectedAdapter {
        driver: binding.driver.clone(),
        procedure_implementation: implementation.map(|value| value.id.clone()),
        profile_path: binding.profile_path.clone(),
        profile_sha256: binding.profile_sha256.clone(),
        features: binding.features.clone(),
        accepted_run_formats: implementation.map_or_else(
            || binding.accepted_run_formats.clone(),
            |value| value.accepted_run_formats.clone(),
        ),
        emitted_run_formats: implementation.map_or_else(
            || binding.emitted_run_formats.clone(),
            |value| value.emitted_run_formats.clone(),
        ),
    }
}

fn validate_adapter_snapshot(
    inventory: &InventorySnapshot,
    adapters: Option<&AdapterBindingSnapshot>,
) -> Result<(), FacilityPlanningError> {
    let Some(adapters) = adapters else {
        return Ok(());
    };
    if adapters.schema_version != ADAPTER_BINDINGS_SCHEMA_VERSION {
        return Err(FacilityPlanningError::WrongAdapterSchema {
            found: adapters.schema_version.clone(),
        });
    }
    if adapters.inventory_sha256 != inventory.source_sha256()
        || adapters.facility != inventory.facility().as_str()
    {
        return Err(FacilityPlanningError::AdapterInventoryMismatch {
            binding_hash: adapters.inventory_sha256.clone(),
            binding_facility: adapters.facility.clone(),
            inventory_hash: inventory.source_sha256().to_owned(),
            inventory_facility: inventory.facility().as_str().to_owned(),
        });
    }
    Ok(())
}

fn validate_material_inventory(
    inventory: &InventorySnapshot,
    material_inventory: &MaterialLotBuildInventory,
) -> Result<(), FacilityPlanningError> {
    if material_inventory.source_sha256 != inventory.source_sha256()
        || material_inventory.facility != inventory.facility().as_str()
    {
        return Err(FacilityPlanningError::MaterialInventoryMismatch {
            material_hash: material_inventory.source_sha256.clone(),
            material_facility: material_inventory.facility.clone(),
            inventory_hash: inventory.source_sha256().to_owned(),
            inventory_facility: inventory.facility().as_str().to_owned(),
        });
    }
    Ok(())
}

fn resolve_method_pins(
    problem: &PlanningProblem,
    policy: &FacilityPlanningPolicy,
) -> Result<BTreeMap<LocalId, MethodId>, FacilityPlanningError> {
    let mut seen = BTreeSet::new();
    for pin in &policy.method_pins {
        if !seen.insert(pin.selector.clone()) {
            return Err(FacilityPlanningError::DuplicateMethodPin {
                selector: render_selector(&pin.selector),
            });
        }
        if !problem
            .choices
            .iter()
            .any(|choice| selector_matches(&pin.selector, choice))
        {
            return Err(FacilityPlanningError::UnmatchedMethodPin {
                selector: render_selector(&pin.selector),
            });
        }
    }
    let mut pins = BTreeMap::new();
    for choice in &problem.choices {
        for pin in policy
            .method_pins
            .iter()
            .filter(|pin| selector_matches(&pin.selector, choice))
        {
            if let Some(first) = pins.insert(choice.id.clone(), pin.method.clone())
                && first != pin.method
            {
                return Err(FacilityPlanningError::ConflictingMethodPin {
                    choice: choice.id.clone(),
                    first,
                    second: pin.method.clone(),
                });
            }
        }
    }
    for (choice_id, method) in &pins {
        let choice = problem
            .choices
            .iter()
            .find(|choice| &choice.id == choice_id)
            .expect("resolved method pins name an existing choice");
        if !choice
            .candidates
            .iter()
            .any(|candidate| &candidate.method == method)
        {
            return Err(FacilityPlanningError::UnknownPinnedMethod {
                choice: choice_id.clone(),
                method: method.clone(),
            });
        }
    }
    Ok(pins)
}

fn selector_matches(selector: &MethodPinSelector, choice: &super::PlanningMethodChoice) -> bool {
    match selector {
        MethodPinSelector::Choice { choice: selected } => selected == &choice.id,
        MethodPinSelector::SourceOperation { source_operation } => {
            source_operation == &choice.source_operation
        }
    }
}

fn render_selector(selector: &MethodPinSelector) -> String {
    match selector {
        MethodPinSelector::Choice { choice } => format!("choice:{choice}"),
        MethodPinSelector::SourceOperation { source_operation } => {
            format!("source-operation:{source_operation}")
        }
    }
}

fn inventory_qualification(qualification: QualificationLevel) -> Qualification {
    match qualification {
        QualificationLevel::Discovered => Qualification::Discovered,
        QualificationLevel::Described => Qualification::Described,
        QualificationLevel::Plannable => Qualification::Plannable,
        QualificationLevel::Simulatable => Qualification::Simulatable,
        QualificationLevel::Executable => Qualification::Executable,
        QualificationLevel::Qualified => Qualification::Qualified,
    }
}

fn rejected_offering(
    asset: &FacilityAsset,
    offering: &FacilityCapabilityOffering,
    reasons: Vec<PlanningCandidateRejectionReason>,
) -> PlanningRejectedOffering {
    PlanningRejectedOffering {
        offering: offering.identity.as_str().to_owned(),
        asset: asset.identity.as_str().to_owned(),
        observed_qualification: offering.qualification.iri().to_owned(),
        control_mode: offering.control_mode.iri().to_owned(),
        reasons,
    }
}

fn binding_key(
    binding: &SelectedRequirementBinding,
) -> (
    &str,
    &str,
    Option<&str>,
    Option<&ProcedureImplementationId>,
    Option<&PathBuf>,
) {
    (
        &binding.asset,
        &binding.offering,
        binding
            .adapter
            .as_ref()
            .map(|adapter| adapter.driver.as_str()),
        binding
            .adapter
            .as_ref()
            .and_then(|adapter| adapter.procedure_implementation.as_ref()),
        binding
            .adapter
            .as_ref()
            .map(|adapter| &adapter.profile_path),
    )
}

fn render_property_value(value: &PropertyValue) -> String {
    let scalar = match &value.value {
        ScalarValue::Text(value) => value.clone(),
        ScalarValue::Integer(value) => value.to_string(),
        ScalarValue::Real(value) => value.to_string(),
        ScalarValue::Boolean(value) => value.to_string(),
        ScalarValue::Iri(value) => value.to_string(),
    };
    value
        .unit
        .as_ref()
        .map_or(scalar.clone(), |unit| format!("{scalar} {unit}"))
}

fn summarize_alternative(selection: &[SelectedMethod]) -> PlanningAlternative {
    PlanningAlternative {
        methods: selection
            .iter()
            .map(|method| AlternativeMethod {
                choice: method.choice.clone(),
                method: method.method.clone(),
                materials: method
                    .tasks
                    .iter()
                    .flat_map(|task| &task.materials)
                    .map(|binding| AlternativeMaterialBinding {
                        input: binding.input.clone(),
                        symbol: binding.symbol.clone(),
                        source: binding.source.clone(),
                    })
                    .collect(),
                bindings: method
                    .tasks
                    .iter()
                    .flat_map(|task| &task.requirements)
                    .map(|binding| AlternativeRequirementBinding {
                        requirement: binding.requirement.clone(),
                        offering: binding.offering.clone(),
                        asset: binding.asset.clone(),
                        adapter: binding
                            .adapter
                            .as_ref()
                            .map(|adapter| adapter.driver.clone()),
                    })
                    .collect(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use lab_capability::{CapabilityKind, ExactInteger, OperationId, ScalarValue};
    use lab_method::PortType;
    use lab_procedure::{
        FluidPathPolicy, Location, MaterialOutput, PipettingConstraints, PipettingProgramV1,
        PipettingStep, ProcedureLocalId, ProcedureProgram, Vessel, VesselRole, Volume,
    };
    use tempfile::TempDir;

    use crate::planning::{
        AdapterBindingRequest, PLANNING_PROBLEM_SCHEMA_VERSION, PlanningMethodChoice,
        PlanningMethodYield, PlanningPort, PlanningProcedureTask, PlanningTaskOutput,
        PlanningValueSource,
    };
    use crate::{backend::validate_adapter_profile, procedure::SETUP_GOLDEN_GATE};

    use super::*;

    fn id(value: &str) -> LocalId {
        LocalId::new(value).unwrap()
    }

    fn method(value: &str) -> MethodId {
        MethodId::new(format!("https://example.org/method/{value}")).unwrap()
    }

    fn requirement(
        id_value: &str,
        capability: &str,
        mode: ControlMode,
        constraints: Vec<PropertyConstraint>,
    ) -> PlanningCapabilityRequirement {
        PlanningCapabilityRequirement {
            id: id(id_value),
            capability_kind: CapabilityKind::new(format!(
                "https://sbol.io/ns/capability#{capability}"
            ))
            .unwrap(),
            minimum_qualification: QualificationLevel::Plannable,
            accepted_control_modes: BTreeSet::from([mode]),
            constraints,
        }
    }

    fn task(
        id_value: &str,
        operation: &str,
        requirement: PlanningCapabilityRequirement,
    ) -> PlanningProcedureTask {
        PlanningProcedureTask {
            id: id(id_value),
            operation: OperationId::new(format!("https://example.org/procedure/{operation}"))
                .unwrap(),
            program: None,
            binding_scope: lab_procedure::BindingScope::Independent,
            inputs: Vec::new(),
            outputs: vec![PlanningTaskOutput {
                name: id("result"),
                port_type: PortType::Data {
                    data_kind: AbsoluteIri::new("https://example.org/data/result").unwrap(),
                },
            }],
            parameters: Vec::new(),
            materials: Vec::new(),
            requirements: vec![requirement],
        }
    }

    fn problem() -> PlanningProblem {
        let cycle_constraint = PropertyConstraint {
            property_kind: PropertyKind::new("https://sbol.io/ns/capability#CycleCount").unwrap(),
            relation: ConstraintRelation::AtLeast,
            required: PropertyValue::unitless(ScalarValue::Integer(
                ExactInteger::parse("30").unwrap(),
            )),
        };
        PlanningProblem {
            schema_version: PLANNING_PROBLEM_SCHEMA_VERSION.to_owned(),
            choices: vec![PlanningMethodChoice {
                id: id("build-0"),
                source_operation: IntentOperationId::new("std.bio.build.realize").unwrap(),
                after: Vec::new(),
                inputs: Vec::new(),
                outputs: vec![PlanningPort {
                    name: id("product"),
                    port_type: PortType::Data {
                        data_kind: AbsoluteIri::new("https://example.org/data/result").unwrap(),
                    },
                    source: None,
                }],
                candidates: vec![
                    PlanningMethodCandidate {
                        method: method("manual"),
                        tasks: vec![task(
                            "build-0::manual::realize",
                            "realize-manually",
                            requirement(
                                "build-0::manual::realize::artifact",
                                "ArtifactRealization",
                                ControlMode::Manual,
                                Vec::new(),
                            ),
                        )],
                        yields: vec![PlanningMethodYield {
                            output: id("product"),
                            source: PlanningValueSource::TaskOutput {
                                task: id("build-0::manual::realize"),
                                output: id("result"),
                            },
                        }],
                    },
                    PlanningMethodCandidate {
                        method: method("automated"),
                        tasks: vec![
                            task(
                                "build-0::automated::handle",
                                "handle-liquid",
                                requirement(
                                    "build-0::automated::handle::liquid",
                                    "LiquidHandling",
                                    ControlMode::ReviewedFile,
                                    Vec::new(),
                                ),
                            ),
                            task(
                                "build-0::automated::cycle",
                                "cycle",
                                requirement(
                                    "build-0::automated::cycle::thermal",
                                    "ThermalCycling",
                                    ControlMode::ReviewedFile,
                                    vec![cycle_constraint],
                                ),
                            ),
                        ],
                        yields: vec![PlanningMethodYield {
                            output: id("product"),
                            source: PlanningValueSource::TaskOutput {
                                task: id("build-0::automated::cycle"),
                                output: id("result"),
                            },
                        }],
                    },
                ],
            }],
        }
    }

    fn inventory(include_manual: bool) -> (TempDir, InventorySnapshot) {
        let manual = if include_manual {
            r#"
ex:operator a sbol:TopLevel, fac:Asset ; sbol:displayId "operator" ;
    sbol:hasNamespace <https://example.org/facility> ; fac:facility ex:facility ;
    fac:assetKind fac:Human ; fac:locatedIn ex:room ; fac:isActive true ;
    fac:capability ex:manual_realization .
ex:manual_realization a sbol:Identified, fac:CapabilityOffering ;
    sbol:displayId "manual_realization" ; fac:capabilityKind cap:ArtifactRealization ;
    fac:qualification fac:Plannable ; fac:controlMode fac:ManualControl ; fac:isActive true .
"#
        } else {
            ""
        };
        let contents = format!(
            r#"@prefix cap: <https://sbol.io/ns/capability#> .
@prefix ex: <https://example.org/facility/> .
@prefix fac: <https://sbol.io/ns/facility#> .
@prefix sbol: <http://sbols.org/v3#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:facility a sbol:TopLevel, fac:Facility ; sbol:displayId "facility" ;
    sbol:hasNamespace <https://example.org/facility> .
ex:room a sbol:TopLevel, fac:Zone ; sbol:displayId "room" ;
    sbol:hasNamespace <https://example.org/facility> ; fac:facility ex:facility ;
    fac:zoneKind fac:Room ; fac:isActive true .
ex:robot a sbol:TopLevel, fac:Asset ; sbol:displayId "robot" ;
    sbol:hasNamespace <https://example.org/facility> ; fac:facility ex:facility ;
    fac:assetKind fac:Instrument ; fac:locatedIn ex:room ; fac:isActive true ;
    fac:capability ex:liquid, ex:metered, ex:mixing, ex:thermal .
ex:liquid a sbol:Identified, fac:CapabilityOffering ; sbol:displayId "liquid" ;
    fac:capabilityKind cap:LiquidHandling ; fac:qualification fac:Plannable ;
    fac:controlMode fac:ReviewedFileControl ; fac:isActive true .
ex:metered a sbol:Identified, fac:CapabilityOffering ; sbol:displayId "metered" ;
    fac:capabilityKind cap:MeteredLiquidTransfer ; fac:qualification fac:Plannable ;
    fac:controlMode fac:ReviewedFileControl ; fac:isActive true ;
    fac:parameter ex:minimum_transfer, ex:maximum_transfer .
ex:minimum_transfer a sbol:Identified, fac:PropertyValue ; sbol:displayId "minimum_transfer" ;
    fac:propertyKind cap:MinimumTransferVolume ; fac:realValue "1"^^xsd:double ;
    fac:unit <http://qudt.org/vocab/unit/MicroL> .
ex:maximum_transfer a sbol:Identified, fac:PropertyValue ; sbol:displayId "maximum_transfer" ;
    fac:propertyKind cap:MaximumTransferVolume ; fac:realValue "300"^^xsd:double ;
    fac:unit <http://qudt.org/vocab/unit/MicroL> .
ex:mixing a sbol:Identified, fac:CapabilityOffering ; sbol:displayId "mixing" ;
    fac:capabilityKind cap:InWellMixing ; fac:qualification fac:Plannable ;
    fac:controlMode fac:ReviewedFileControl ; fac:isActive true ;
    fac:parameter ex:maximum_mix .
ex:maximum_mix a sbol:Identified, fac:PropertyValue ; sbol:displayId "maximum_mix" ;
    fac:propertyKind cap:MaximumMixVolume ; fac:realValue "300"^^xsd:double ;
    fac:unit <http://qudt.org/vocab/unit/MicroL> .
ex:thermal a sbol:Identified, fac:CapabilityOffering ; sbol:displayId "thermal" ;
    fac:capabilityKind cap:ThermalCycling ; fac:qualification fac:Plannable ;
    fac:controlMode fac:ReviewedFileControl ; fac:isActive true ; fac:parameter ex:cycles .
ex:cycles a sbol:Identified, fac:PropertyValue ; sbol:displayId "cycles" ;
    fac:propertyKind cap:CycleCount ; fac:integerValue 40 .
{manual}"#
        );
        let directory = TempDir::new().unwrap();
        fs::write(directory.path().join("inventory.ttl"), contents).unwrap();
        let snapshot = InventorySnapshot::load(directory.path(), "inventory.ttl", None).unwrap();
        (directory, snapshot)
    }

    fn material_inventory(inventory: &InventorySnapshot) -> MaterialLotBuildInventory {
        MaterialLotBuildInventory {
            source_sha256: inventory.source_sha256().to_owned(),
            facility: inventory.facility().as_str().to_owned(),
            materials: BTreeMap::new(),
            artifacts: BTreeMap::new(),
        }
    }

    fn pipetting_program() -> ProcedureProgram {
        let local = |value| ProcedureLocalId::new(value).unwrap();
        let program = PipettingProgramV1::new(
            Vec::new(),
            vec![MaterialOutput {
                id: local("result"),
            }],
            vec![
                Vessel {
                    id: local("source"),
                    role: VesselRole::Intermediate,
                    positions: 1,
                    initial_volume_each: None,
                },
                Vessel {
                    id: local("destination"),
                    role: VesselRole::Product {
                        output: local("result"),
                    },
                    positions: 1,
                    initial_volume_each: None,
                },
            ],
            vec![
                PipettingStep::Transfer {
                    id: local("transfer"),
                    source: Location {
                        vessel: local("source"),
                        position: 0,
                    },
                    destination: Location {
                        vessel: local("destination"),
                        position: 0,
                    },
                    volume: Volume::parse_microlitres("1").unwrap(),
                    fluid_path: FluidPathPolicy::IsolatedDestinations,
                    fluid_path_group: None,
                    technique: Default::default(),
                },
                PipettingStep::Mix {
                    id: local("mix"),
                    targets: vec![Location {
                        vessel: local("destination"),
                        position: 0,
                    }],
                    cycles: 3,
                    volume: Volume::parse_microlitres("1").unwrap(),
                    fluid_path: FluidPathPolicy::IsolatedDestinations,
                    fluid_path_group: None,
                    technique: Default::default(),
                },
            ],
            PipettingConstraints::default(),
        )
        .validate()
        .unwrap();
        ProcedureProgram::from_pipetting(&program)
    }

    fn ot2_bindings(inventory: &InventorySnapshot) -> AdapterBindingSnapshot {
        AdapterBindingSnapshot::resolve(
            inventory,
            vec![AdapterBindingRequest {
                asset: "https://example.org/facility/robot".to_owned(),
                driver: "opentrons.ot2".to_owned(),
                profile_path: PathBuf::from("adapters/ot2.toml"),
                profile: validate_adapter_profile("opentrons.ot2", "ot2", "").unwrap(),
            }],
        )
        .unwrap()
    }

    fn normalize_test_task(task: &mut PlanningProcedureTask, operation: &str) {
        let program = pipetting_program();
        let formula = program.validate().unwrap().capability_formula();
        let policy = task.requirements[0].clone();
        task.operation = OperationId::new(operation).unwrap();
        task.requirements = formula
            .all_of
            .into_iter()
            .map(|clause| PlanningCapabilityRequirement {
                id: id(&format!("{}::requirement::{}", task.id, clause.role)),
                capability_kind: clause.capability_kind,
                minimum_qualification: policy.minimum_qualification,
                accepted_control_modes: policy.accepted_control_modes.clone(),
                constraints: clause.constraints,
            })
            .collect();
        task.binding_scope = formula.binding_scope;
        task.program = Some(program);
    }

    #[test]
    fn facility_feasibility_selects_one_complete_method_graph() {
        let (_directory, inventory) = inventory(false);
        let problem = problem();

        let solution = FacilityPlanningSolution::solve(
            &problem,
            &inventory,
            &material_inventory(&inventory),
            None,
            FacilityPlanningPolicy::default(),
        )
        .unwrap();

        assert_eq!(solution.selections[0].method, method("automated"));
        assert_eq!(solution.selections[0].tasks.len(), 2);
        assert_eq!(
            solution.selections[0].tasks[1].requirements[0].parameters[0].offering_parameter,
            "https://example.org/facility/cycles"
        );
        solution.validate_against(&problem).unwrap();
        let json = serde_json::to_string_pretty(&solution).unwrap();
        let decoded: FacilityPlanningSolution = serde_json::from_str(&json).unwrap();
        decoded.validate_against(&problem).unwrap();
        assert_eq!(decoded, solution);
    }

    #[test]
    fn complete_solutions_remain_ambiguous_until_policy_selects_one() {
        let (_directory, inventory) = inventory(true);
        let problem = problem();

        let error = FacilityPlanningSolution::solve(
            &problem,
            &inventory,
            &material_inventory(&inventory),
            None,
            FacilityPlanningPolicy::default(),
        )
        .unwrap_err();
        let FacilityPlanningError::AmbiguousPlan { alternatives } = error else {
            panic!("expected a global ambiguity")
        };
        assert_eq!(alternatives.len(), 2);
        assert_ne!(
            alternatives[0].methods[0].method,
            alternatives[1].methods[0].method
        );

        let policy = FacilityPlanningPolicy {
            method_pins: vec![MethodPin {
                selector: MethodPinSelector::SourceOperation {
                    source_operation: IntentOperationId::new("std.bio.build.realize").unwrap(),
                },
                method: method("automated"),
            }],
            adapter_requirement: AdapterRequirement::Optional,
        };
        let selected = FacilityPlanningSolution::solve(
            &problem,
            &inventory,
            &material_inventory(&inventory),
            None,
            policy,
        )
        .unwrap();
        assert_eq!(selected.selections[0].method, method("automated"));
    }

    #[test]
    fn non_manual_policy_requires_an_implementation_binding() {
        let (_directory, inventory) = inventory(false);
        let error = FacilityPlanningSolution::solve(
            &problem(),
            &inventory,
            &material_inventory(&inventory),
            None,
            FacilityPlanningPolicy {
                method_pins: Vec::new(),
                adapter_requirement: AdapterRequirement::NonManual,
            },
        )
        .unwrap_err();
        let FacilityPlanningError::NoFeasibleMethod { candidates, .. } = error else {
            panic!("expected method infeasibility")
        };
        assert!(candidates.iter().any(|candidate| {
            candidate.rejected_requirements.iter().any(|requirement| {
                requirement.candidates.iter().any(|offering| {
                    offering
                        .reasons
                        .contains(&PlanningCandidateRejectionReason::MissingPlanningAdapter)
                })
            })
        }));
    }

    #[test]
    fn normalized_tasks_freeze_an_exact_operation_aware_procedure_implementation() {
        let (_directory, inventory) = inventory(false);
        let mut problem = problem();
        problem.choices[0].candidates.retain(|candidate| {
            candidate.method.as_str() == "https://example.org/method/automated"
        });
        normalize_test_task(
            &mut problem.choices[0].candidates[0].tasks[0],
            SETUP_GOLDEN_GATE,
        );
        problem.validate().unwrap();
        let adapters = ot2_bindings(&inventory);

        let solution = FacilityPlanningSolution::solve(
            &problem,
            &inventory,
            &material_inventory(&inventory),
            Some(&adapters),
            FacilityPlanningPolicy {
                method_pins: Vec::new(),
                adapter_requirement: AdapterRequirement::NonManual,
            },
        )
        .unwrap();

        let requirements = &solution.selections[0].tasks[0].requirements;
        assert_eq!(requirements.len(), 2);
        assert_eq!(
            requirements
                .iter()
                .map(|requirement| requirement.capability_kind.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "https://sbol.io/ns/capability#InWellMixing",
                "https://sbol.io/ns/capability#MeteredLiquidTransfer",
            ])
        );
        assert!(requirements.iter().all(|requirement| {
            requirement.asset == "https://example.org/facility/robot"
                && requirement.adapter.as_ref().is_some_and(|selected| {
                    selected
                        .procedure_implementation
                        .as_ref()
                        .is_some_and(|implementation| {
                            implementation.as_str()
                                == "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsOt2PipettingV1"
                        })
                })
        }));
    }

    #[test]
    fn broad_adapter_support_does_not_claim_an_unimplemented_normalized_operation() {
        let (_directory, inventory) = inventory(false);
        let mut problem = problem();
        problem.choices[0].candidates.retain(|candidate| {
            candidate.method.as_str() == "https://example.org/method/automated"
        });
        normalize_test_task(
            &mut problem.choices[0].candidates[0].tasks[0],
            "https://example.org/procedure/handle-liquid",
        );
        problem.validate().unwrap();
        let adapters = ot2_bindings(&inventory);

        let error = FacilityPlanningSolution::solve(
            &problem,
            &inventory,
            &material_inventory(&inventory),
            Some(&adapters),
            FacilityPlanningPolicy {
                method_pins: Vec::new(),
                adapter_requirement: AdapterRequirement::NonManual,
            },
        )
        .unwrap_err();

        let FacilityPlanningError::NoFeasibleMethod { candidates, .. } = error else {
            panic!("expected method infeasibility")
        };
        assert!(candidates.iter().any(|candidate| {
            candidate.rejected_requirements.iter().any(|requirement| {
                requirement.candidates.iter().any(|offering| {
                    offering
                        .reasons
                        .contains(&PlanningCandidateRejectionReason::MissingPlanningAdapter)
                })
            })
        }));
    }

    #[test]
    fn material_lot_ambiguity_is_a_global_plan_ambiguity() {
        let (_directory, inventory) = inventory(false);
        let mut problem = problem();
        problem.choices[0].candidates.retain(|candidate| {
            candidate.method.as_str() == "https://example.org/method/automated"
        });
        problem.choices[0].candidates[0].tasks[0]
            .materials
            .push(PlanningMaterialInput {
                id: id("build-0::automated::handle::material::sample"),
                symbol: "sample".to_owned(),
                source: PlanningMaterialSource::Inventory,
            });
        let mut materials = material_inventory(&inventory);
        materials.materials.insert(
            "sample".to_owned(),
            MaterialLotCandidates::Identified {
                component: "https://example.org/material/sample".to_owned(),
                material_lots: vec![
                    "https://example.org/material/sample-lot-a".to_owned(),
                    "https://example.org/material/sample-lot-b".to_owned(),
                ],
            },
        );

        let error = FacilityPlanningSolution::solve(
            &problem,
            &inventory,
            &materials,
            None,
            FacilityPlanningPolicy::default(),
        )
        .unwrap_err();
        let FacilityPlanningError::AmbiguousPlan { alternatives } = error else {
            panic!("expected exact MaterialLot ambiguity")
        };
        assert_eq!(alternatives.len(), 2);
        let lots = alternatives
            .iter()
            .map(
                |alternative| match &alternative.methods[0].materials[0].source {
                    SelectedMaterialSource::MaterialLot { material_lot, .. } => material_lot,
                    SelectedMaterialSource::ChoiceOutput { .. } => panic!("expected external lot"),
                },
            )
            .collect::<BTreeSet<_>>();
        assert_eq!(lots.len(), 2);
    }
}
