//! Global selection of method alternatives and exact facility capability bindings.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lab_capability::{
    AbsoluteIri, ConstraintRelation, ControlMode, MethodId, PropertyConstraint, PropertyKind,
    PropertyValue, QualificationLevel, ScalarValue, UnitIri,
};
use lab_inventory::{
    FacilityAsset, FacilityAssetError, FacilityCapabilityOffering, FacilityCapabilityParameter,
    FacilityScalarValue, InventorySnapshot,
};
use lab_method::{IntentOperationId, LocalId};
use sbol_inventory::vocabulary::Qualification;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{
    ADAPTER_BINDINGS_SCHEMA_VERSION, AdapterBindingSnapshot, PlanningCapabilityRequirement,
    PlanningMethodCandidate, PlanningProblem, PlanningProblemValidationError,
    ResolvedAdapterBinding,
};

pub const FACILITY_PLANNING_SOLUTION_SCHEMA_VERSION: &str = "lab.facility-planning-solution.v1";

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
    pub requirements: Vec<SelectedRequirementBinding>,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SelectedAdapter {
    pub driver: String,
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
    MissingPlanningAdapter,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RejectedMethodCandidate {
    pub method: MethodId,
    pub rejected_requirements: Vec<RejectedPlanningRequirement>,
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
    pub bindings: Vec<AlternativeRequirementBinding>,
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
        adapters: Option<&AdapterBindingSnapshot>,
        policy: FacilityPlanningPolicy,
    ) -> Result<Self, FacilityPlanningError> {
        problem.validate()?;
        validate_adapter_snapshot(inventory, adapters)?;
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
                match allocate_method(candidate, &assets, adapters, policy.adapter_requirement) {
                    MethodAllocation::Feasible(mut selections) => {
                        candidate_selections.append(&mut selections)
                    }
                    MethodAllocation::Rejected(rejected) => {
                        rejected_methods.push(RejectedMethodCandidate {
                            method: candidate.method.clone(),
                            rejected_requirements: rejected,
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
                    || selected_task.requirements.len() != task.requirements.len()
                {
                    return Err(FacilityPlanningSolutionValidationError::TaskSet {
                        choice: selection.choice.clone(),
                    });
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
}

#[derive(Clone)]
struct SelectedMethodCandidate {
    method: MethodId,
    tasks: Vec<SelectedProcedureTask>,
}

enum MethodAllocation {
    Feasible(Vec<SelectedMethodCandidate>),
    Rejected(Vec<RejectedPlanningRequirement>),
}

#[derive(Clone)]
struct RequirementCandidate {
    binding: SelectedRequirementBinding,
}

fn allocate_method(
    candidate: &PlanningMethodCandidate,
    assets: &[FacilityAsset],
    adapters: Option<&AdapterBindingSnapshot>,
    adapter_requirement: AdapterRequirement,
) -> MethodAllocation {
    let mut tasks = Vec::new();
    let mut rejected = Vec::new();
    for task in &candidate.tasks {
        let mut task_requirements = Vec::new();
        for requirement in &task.requirements {
            let (bindings, rejections) =
                requirement_candidates(requirement, assets, adapters, adapter_requirement);
            if bindings.is_empty() {
                rejected.push(RejectedPlanningRequirement {
                    requirement: requirement.id.clone(),
                    capability_kind: requirement.capability_kind.clone(),
                    candidates: rejections,
                });
            } else {
                task_requirements.push((bindings, rejections));
            }
        }
        tasks.push((task.id.clone(), task_requirements));
    }
    if !rejected.is_empty() {
        return MethodAllocation::Rejected(rejected);
    }

    let mut alternatives = vec![Vec::<SelectedProcedureTask>::new()];
    for (task, requirements) in tasks {
        let mut task_alternatives = vec![Vec::<SelectedRequirementBinding>::new()];
        for (bindings, rejections) in requirements {
            let mut combined = Vec::new();
            'outer: for prefix in &task_alternatives {
                for candidate in &bindings {
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
            task_alternatives = combined;
        }
        let mut combined = Vec::new();
        'outer: for prefix in &alternatives {
            for requirements in &task_alternatives {
                let mut selection = prefix.clone();
                selection.push(SelectedProcedureTask {
                    task: task.clone(),
                    requirements: requirements.clone(),
                });
                combined.push(selection);
                if combined.len() == 2 {
                    break 'outer;
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

fn requirement_candidates(
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
            let adapter_candidates = matching_adapters(adapters, asset, offering);
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
        .map(selected_adapter)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        (&left.driver, &left.profile_path).cmp(&(&right.driver, &right.profile_path))
    });
    candidates
}

fn adapter_supports(binding: &ResolvedAdapterBinding, offering: &str) -> bool {
    binding
        .offerings
        .iter()
        .any(|candidate| candidate.offering == offering && candidate.planning_eligible)
}

fn selected_adapter(binding: &ResolvedAdapterBinding) -> SelectedAdapter {
    SelectedAdapter {
        driver: binding.driver.clone(),
        profile_path: binding.profile_path.clone(),
        profile_sha256: binding.profile_sha256.clone(),
        features: binding.features.clone(),
        accepted_run_formats: binding.accepted_run_formats.clone(),
        emitted_run_formats: binding.emitted_run_formats.clone(),
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
) -> (&str, &str, Option<&str>, Option<&PathBuf>) {
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
    use tempfile::TempDir;

    use crate::planning::{
        PLANNING_PROBLEM_SCHEMA_VERSION, PlanningMethodChoice, PlanningMethodYield, PlanningPort,
        PlanningProcedureTask, PlanningTaskOutput, PlanningValueSource,
    };

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
            inputs: Vec::new(),
            outputs: vec![PlanningTaskOutput {
                name: id("result"),
                port_type: PortType::Data {
                    data_kind: AbsoluteIri::new("https://example.org/data/result").unwrap(),
                },
            }],
            parameters: Vec::new(),
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

ex:facility a sbol:TopLevel, fac:Facility ; sbol:displayId "facility" ;
    sbol:hasNamespace <https://example.org/facility> .
ex:room a sbol:TopLevel, fac:Zone ; sbol:displayId "room" ;
    sbol:hasNamespace <https://example.org/facility> ; fac:facility ex:facility ;
    fac:zoneKind fac:Room ; fac:isActive true .
ex:robot a sbol:TopLevel, fac:Asset ; sbol:displayId "robot" ;
    sbol:hasNamespace <https://example.org/facility> ; fac:facility ex:facility ;
    fac:assetKind fac:Instrument ; fac:locatedIn ex:room ; fac:isActive true ;
    fac:capability ex:liquid, ex:thermal .
ex:liquid a sbol:Identified, fac:CapabilityOffering ; sbol:displayId "liquid" ;
    fac:capabilityKind cap:LiquidHandling ; fac:qualification fac:Plannable ;
    fac:controlMode fac:ReviewedFileControl ; fac:isActive true .
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

    #[test]
    fn facility_feasibility_selects_one_complete_method_graph() {
        let (_directory, inventory) = inventory(false);
        let problem = problem();

        let solution = FacilityPlanningSolution::solve(
            &problem,
            &inventory,
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
        let selected = FacilityPlanningSolution::solve(&problem, &inventory, None, policy).unwrap();
        assert_eq!(selected.selections[0].method, method("automated"));
    }

    #[test]
    fn non_manual_policy_requires_an_implementation_binding() {
        let (_directory, inventory) = inventory(false);
        let error = FacilityPlanningSolution::solve(
            &problem(),
            &inventory,
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
}
