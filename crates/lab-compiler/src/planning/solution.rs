//! Durable facility-planning policy, solution, and validation contracts.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::method::{IntentOperationId, LocalId};
use crate::procedure::BindingScope;
use lab_capability::{
    AbsoluteIri, CapabilityKind, ConstraintRelation, ControlMode, MethodId,
    ProcedureImplementationId, PropertyKind, PropertyValue, QualificationLevel,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{PlanningMaterialSource, PlanningProblem};

pub const FACILITY_PLANNING_SOLUTION_SCHEMA_VERSION: &str = "lab.facility-planning-solution.v1";

/// Explicit choices that are allowed to turn an otherwise ambiguous solution space into a plan.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FacilityPlanningPolicy {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub method_pins: Vec<MethodPin>,
    /// Restricts which Asset may satisfy a requirement.
    ///
    /// Two instruments of the same model are not interchangeable: they carry their own calibration
    /// and sit in their own place. When a facility offers more than one, choosing is the
    /// laboratory's call, and this is how it states it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asset_pins: Vec<AssetPin>,
    #[serde(default)]
    pub adapter_requirement: AdapterRequirement,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct AssetPin {
    #[serde(flatten)]
    pub selector: AssetPinSelector,
    pub asset: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum AssetPinSelector {
    /// Every requirement the named Asset can satisfy binds it.
    AnyRequirement,
    /// Every requirement demanding this capability kind binds the named Asset.
    CapabilityKind { capability_kind: CapabilityKind },
    /// One exact requirement from the emitted planning problem binds the named Asset.
    Requirement { requirement: LocalId },
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
    /// Equally usable MaterialLots the solver did not take, in review order.
    ///
    /// Choosing between two active lots of the same component is inventory management rather than
    /// a scientific decision, so the solver picks one and records the rest here instead of
    /// refusing to plan. A reviewer can see exactly what else was on the shelf.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interchangeable_alternatives: Vec<String>,
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
    /// No Asset in the facility offers this capability kind at all.
    NoOfferingOfKind {
        assets_considered: usize,
    },
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
    ExcludedByExactAssetPin {
        pinned_asset: String,
    },
    MissingPlanningAdapter,
}

impl FacilityPlanningSolution {
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
                    if binding
                        .adapter
                        .as_ref()
                        .is_some_and(|adapter| adapter.profile_path.to_str().is_none())
                    {
                        return Err(
                            FacilityPlanningSolutionValidationError::NonUtf8AdapterProfile {
                                requirement: requirement.id.clone(),
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
    #[error(
        "facility solution contains a non-UTF-8 adapter profile path for requirement `{requirement}`"
    )]
    NonUtf8AdapterProfile { requirement: LocalId },
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
