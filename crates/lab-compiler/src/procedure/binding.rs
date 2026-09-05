//! One authoritative interface between a Procedure task and its canonical program body.

use std::collections::{BTreeMap, BTreeSet};

use lab_capability::{CapabilityKind, ControlMode, PropertyConstraint, QualificationLevel};
use thiserror::Error;

use crate::procedure::{
    BindingScope, CapabilityFormula, ProcedureLocalId, ValidatedProcedureProgram,
};

/// Whether a canonical program may reference some or must reference all task materials.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialReferencePolicy {
    Subset,
    Exact,
}

/// The task-facing references contained in a validated canonical program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcedureProgramInterface {
    pub inputs: BTreeSet<u32>,
    pub materials: BTreeSet<ProcedureLocalId>,
    pub outputs: BTreeSet<ProcedureLocalId>,
    pub material_policy: MaterialReferencePolicy,
}

/// The enclosing task surface against which a canonical program is checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcedureTaskInterface {
    pub input_count: usize,
    pub materials: Vec<ProcedureLocalId>,
    pub outputs: Vec<ProcedureLocalId>,
}

impl ProcedureTaskInterface {
    pub fn new(
        input_count: usize,
        materials: impl IntoIterator<Item = ProcedureLocalId>,
        outputs: impl IntoIterator<Item = ProcedureLocalId>,
    ) -> Self {
        Self {
            input_count,
            materials: materials.into_iter().collect(),
            outputs: outputs.into_iter().collect(),
        }
    }
}

/// One requirement represented beside a Procedure task in LAIR or a derived document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcedureCapabilityRequirement {
    pub id: ProcedureLocalId,
    pub capability_kind: CapabilityKind,
    pub minimum_qualification: QualificationLevel,
    pub accepted_control_modes: BTreeSet<ControlMode>,
    pub constraints: Vec<PropertyConstraint>,
}

/// A canonical program proven to agree with its complete enclosing task aggregate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedProcedureTaskContract<'program> {
    program: &'program ValidatedProcedureProgram,
    formula: CapabilityFormula,
}

impl<'program> ValidatedProcedureTaskContract<'program> {
    pub fn program(&self) -> &'program ValidatedProcedureProgram {
        self.program
    }

    pub fn capability_formula(&self) -> &CapabilityFormula {
        &self.formula
    }
}

impl ValidatedProcedureProgram {
    /// Return every enclosing-task reference contained in this canonical body.
    pub fn interface(&self) -> ProcedureProgramInterface {
        self.analysis().interface.clone()
    }

    /// Verify the references that are owned directly by `procedure.task`.
    ///
    /// Material declarations are sibling operations, so aggregate verification adds those through
    /// [`Self::validate_task_contract`].
    pub fn validate_task_ports(
        &self,
        input_count: usize,
        outputs: &[ProcedureLocalId],
    ) -> Result<(), ProcedureBindingError> {
        let interface = self.interface();
        if let Some(input) = interface
            .inputs
            .iter()
            .find(|input| usize::try_from(**input).map_or(true, |input| input >= input_count))
        {
            return Err(ProcedureBindingError::UnavailableInput {
                input: *input,
                input_count,
            });
        }

        let output_set = outputs.iter().cloned().collect::<BTreeSet<_>>();
        if output_set.len() != outputs.len() {
            return Err(ProcedureBindingError::DuplicateTaskOutput);
        }
        if interface.outputs != output_set {
            return Err(ProcedureBindingError::OutputMismatch {
                expected: output_set,
                program: interface.outputs,
            });
        }
        Ok(())
    }

    /// Verify that every program reference resolves against the enclosing task.
    pub fn validate_task_interface(
        &self,
        task: &ProcedureTaskInterface,
    ) -> Result<(), ProcedureBindingError> {
        self.validate_task_ports(task.input_count, &task.outputs)?;

        let task_materials = task.materials.iter().cloned().collect::<BTreeSet<_>>();
        if task_materials.len() != task.materials.len() {
            return Err(ProcedureBindingError::DuplicateTaskMaterial);
        }
        let program_materials = self.interface().materials;
        match self.interface().material_policy {
            MaterialReferencePolicy::Subset => {
                if let Some(material) = program_materials.difference(&task_materials).next() {
                    return Err(ProcedureBindingError::UndeclaredMaterial {
                        material: material.clone(),
                    });
                }
            }
            MaterialReferencePolicy::Exact if program_materials != task_materials => {
                return Err(ProcedureBindingError::MaterialMismatch {
                    task: task_materials,
                    program: program_materials,
                });
            }
            MaterialReferencePolicy::Exact => {}
        }
        Ok(())
    }

    /// Verify the exact requirement formula derived from this program.
    ///
    /// Requirements and constraints are matched by semantic identity rather than operation order,
    /// because LAIR represents them as independently addressable sibling operations.
    pub fn validate_capability_requirements(
        &self,
        task: &ProcedureLocalId,
        declared_scope: Option<BindingScope>,
        requirements: &[ProcedureCapabilityRequirement],
    ) -> Result<CapabilityFormula, ProcedureBindingError> {
        let formula = self.capability_formula();
        if let Some(declared) = declared_scope
            && declared != formula.binding_scope
        {
            return Err(ProcedureBindingError::BindingScopeMismatch {
                expected: formula.binding_scope,
                declared,
            });
        }

        let mut by_id = BTreeMap::new();
        for requirement in requirements {
            if by_id.insert(requirement.id.clone(), requirement).is_some() {
                return Err(ProcedureBindingError::DuplicateRequirement {
                    requirement: requirement.id.clone(),
                });
            }
        }
        if by_id.len() != formula.all_of.len() {
            return Err(ProcedureBindingError::RequirementCount {
                expected: formula.all_of.len(),
                actual: by_id.len(),
            });
        }

        let policy = requirements.first().map(|requirement| {
            (
                requirement.minimum_qualification,
                &requirement.accepted_control_modes,
            )
        });
        for requirement in requirements {
            if policy.is_some_and(|(qualification, modes)| {
                requirement.minimum_qualification != qualification
                    || &requirement.accepted_control_modes != modes
            }) {
                return Err(ProcedureBindingError::RequirementPolicyMismatch {
                    requirement: requirement.id.clone(),
                });
            }
        }

        for clause in &formula.all_of {
            let expected_id =
                ProcedureLocalId::new(format!("{task}::requirement::{}", clause.role))
                    .expect("stable task and role IDs compose into a stable requirement ID");
            let Some(requirement) = by_id.remove(&expected_id) else {
                return Err(ProcedureBindingError::MissingRequirement {
                    requirement: expected_id,
                });
            };
            if requirement.capability_kind != clause.capability_kind {
                return Err(ProcedureBindingError::RequirementKindMismatch {
                    requirement: requirement.id.clone(),
                    expected: clause.capability_kind.clone(),
                    actual: requirement.capability_kind.clone(),
                });
            }
            if !equal_multiset(&requirement.constraints, &clause.constraints) {
                return Err(ProcedureBindingError::RequirementConstraintsMismatch {
                    requirement: requirement.id.clone(),
                });
            }
        }
        debug_assert!(by_id.is_empty(), "formula cardinality was checked first");
        Ok(formula)
    }

    /// Verify the complete task/program aggregate and retain the once-derived formula.
    pub fn validate_task_contract<'program>(
        &'program self,
        task_id: &ProcedureLocalId,
        task: &ProcedureTaskInterface,
        declared_scope: Option<BindingScope>,
        requirements: &[ProcedureCapabilityRequirement],
    ) -> Result<ValidatedProcedureTaskContract<'program>, ProcedureBindingError> {
        self.validate_task_interface(task)?;
        let formula =
            self.validate_capability_requirements(task_id, declared_scope, requirements)?;
        Ok(ValidatedProcedureTaskContract {
            program: self,
            formula,
        })
    }
}

fn equal_multiset<T: PartialEq>(left: &[T], right: &[T]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut matched = vec![false; right.len()];
    left.iter().all(|candidate| {
        let Some(index) = right.iter().enumerate().find_map(|(index, expected)| {
            (!matched[index] && candidate == expected).then_some(index)
        }) else {
            return false;
        };
        matched[index] = true;
        true
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ProcedureBindingError {
    #[error("program input {input} is outside the task's {input_count} inputs")]
    UnavailableInput { input: u32, input_count: usize },
    #[error("the task repeats an output identity")]
    DuplicateTaskOutput,
    #[error("the task repeats a material identity")]
    DuplicateTaskMaterial,
    #[error("program material `{material}` is not declared by the task")]
    UndeclaredMaterial { material: ProcedureLocalId },
    #[error("program materials {program:?} do not match task materials {task:?}")]
    MaterialMismatch {
        task: BTreeSet<ProcedureLocalId>,
        program: BTreeSet<ProcedureLocalId>,
    },
    #[error("program outputs {program:?} do not exactly match task outputs {expected:?}")]
    OutputMismatch {
        expected: BTreeSet<ProcedureLocalId>,
        program: BTreeSet<ProcedureLocalId>,
    },
    #[error("program requires binding scope {expected:?}, not {declared:?}")]
    BindingScopeMismatch {
        expected: BindingScope,
        declared: BindingScope,
    },
    #[error("requirement `{requirement}` occurs more than once")]
    DuplicateRequirement { requirement: ProcedureLocalId },
    #[error("program derives {expected} requirements, but the task carries {actual}")]
    RequirementCount { expected: usize, actual: usize },
    #[error("program-derived requirement `{requirement}` is missing")]
    MissingRequirement { requirement: ProcedureLocalId },
    #[error("requirement `{requirement}` has capability kind `{actual}`, expected `{expected}`")]
    RequirementKindMismatch {
        requirement: ProcedureLocalId,
        expected: CapabilityKind,
        actual: CapabilityKind,
    },
    #[error("requirement `{requirement}` does not carry the program-derived constraints")]
    RequirementConstraintsMismatch { requirement: ProcedureLocalId },
    #[error("requirement `{requirement}` does not share its task's qualification/control policy")]
    RequirementPolicyMismatch { requirement: ProcedureLocalId },
}

#[cfg(test)]
mod tests {
    use lab_capability::{ControlMode, QualificationLevel};

    use crate::procedure::{
        Duration, MaterialInput, MaterialOutput, PipettingConstraints, PipettingProgramV1,
        ProcedureLocalId, ProcedureProgram, Temperature, ThermalLoad, ThermalProgramV1,
        ThermalStage, ThermalStep, ValidatedProcedureProgram, Vessel, VesselRole, Volume,
    };

    use super::{ProcedureBindingError, ProcedureCapabilityRequirement, ProcedureTaskInterface};

    fn id(value: &str) -> ProcedureLocalId {
        ProcedureLocalId::new(value).unwrap()
    }

    fn pipetting(role: VesselRole) -> ValidatedProcedureProgram {
        let program = PipettingProgramV1::new(
            vec![MaterialInput { id: id("water") }],
            vec![MaterialOutput { id: id("product") }],
            vec![
                Vessel {
                    id: id("input"),
                    role,
                    positions: 1,
                    initial_volume_each: Some(Volume::parse_microlitres("10").unwrap()),
                    working_capacity_each: None,
                    dead_volume_each: None,
                    temperature: None,
                },
                Vessel {
                    id: id("water"),
                    role: VesselRole::MaterialSource {
                        material: id("water"),
                    },
                    positions: 1,
                    initial_volume_each: Some(Volume::parse_microlitres("10").unwrap()),
                    working_capacity_each: None,
                    dead_volume_each: None,
                    temperature: None,
                },
                Vessel {
                    id: id("product"),
                    role: VesselRole::Product {
                        output: id("product"),
                    },
                    positions: 1,
                    initial_volume_each: None,
                    working_capacity_each: None,
                    dead_volume_each: None,
                    temperature: None,
                },
            ],
            vec![crate::procedure::PipettingStep::Transfer {
                id: id("transfer"),
                source: crate::procedure::Location {
                    vessel: id("water"),
                    position: 0,
                },
                destination: crate::procedure::Location {
                    vessel: id("product"),
                    position: 0,
                },
                volume: Volume::parse_microlitres("1").unwrap(),
                fluid_path: crate::procedure::FluidPathPolicy::IsolatedDestinations,
                fluid_path_group: None,
                technique: Default::default(),
            }],
            PipettingConstraints::default(),
        )
        .validate()
        .unwrap();
        ProcedureProgram::from_pipetting(&program)
            .validate()
            .unwrap()
    }

    fn thermal() -> ValidatedProcedureProgram {
        let program = ThermalProgramV1 {
            load: ThermalLoad {
                input: 0,
                outputs: vec![id("product")],
                sample_count: 1,
                volume_each: Volume::parse_microlitres("20").unwrap(),
            },
            lid_temperature: Some(Temperature::parse_degrees_celsius("105").unwrap()),
            stages: vec![ThermalStage {
                id: id("stage"),
                repeats: 1,
                steps: vec![ThermalStep {
                    id: id("hold"),
                    temperature: Temperature::parse_degrees_celsius("37").unwrap(),
                    hold: Duration::parse_seconds("60").unwrap(),
                    ramp_rate: None,
                }],
            }],
            final_hold: None,
        }
        .validate()
        .unwrap();
        ProcedureProgram::from_thermal(&program).validate().unwrap()
    }

    fn requirements(
        task: &ProcedureLocalId,
        program: &ValidatedProcedureProgram,
    ) -> Vec<ProcedureCapabilityRequirement> {
        program
            .capability_formula()
            .all_of
            .into_iter()
            .map(|clause| ProcedureCapabilityRequirement {
                id: id(&format!("{task}::requirement::{}", clause.role)),
                capability_kind: clause.capability_kind,
                minimum_qualification: QualificationLevel::Executable,
                accepted_control_modes: [ControlMode::ReviewedFile].into_iter().collect(),
                constraints: clause.constraints,
            })
            .collect()
    }

    #[test]
    fn every_input_consuming_vessel_role_is_bounds_checked() {
        for role in [
            VesselRole::ProcedureInput { input: 1 },
            VesselRole::InputOutput {
                input: 1,
                output: id("product"),
            },
        ] {
            let error = pipetting(role)
                .validate_task_ports(1, &[id("product")])
                .unwrap_err();
            assert!(matches!(
                error,
                ProcedureBindingError::UnavailableInput { .. }
            ));
        }
    }

    #[test]
    fn program_materials_may_be_a_subset_but_must_be_declared() {
        let program = pipetting(VesselRole::ProcedureInput { input: 0 });
        program
            .validate_task_interface(&ProcedureTaskInterface::new(
                1,
                [id("water"), id("unused")],
                [id("product")],
            ))
            .unwrap();
        let error = program
            .validate_task_interface(&ProcedureTaskInterface::new(
                1,
                [id("buffer")],
                [id("product")],
            ))
            .unwrap_err();
        assert!(matches!(
            error,
            ProcedureBindingError::UndeclaredMaterial { .. }
        ));
    }

    #[test]
    fn thermal_programs_reject_materials_and_task_output_duplicates() {
        let program = thermal();
        let error = program
            .validate_task_interface(&ProcedureTaskInterface::new(
                1,
                [id("unexpected")],
                [id("product")],
            ))
            .unwrap_err();
        assert!(matches!(
            error,
            ProcedureBindingError::MaterialMismatch { .. }
        ));
        let error = program
            .validate_task_ports(1, &[id("product"), id("product")])
            .unwrap_err();
        assert_eq!(error, ProcedureBindingError::DuplicateTaskOutput);
    }

    #[test]
    fn requirement_and_constraint_sibling_order_is_semantically_irrelevant() {
        let task = id("cycle");
        let program = thermal();
        let mut requirements = requirements(&task, &program);
        requirements.reverse();
        for requirement in &mut requirements {
            requirement.constraints.reverse();
        }
        program
            .validate_task_contract(
                &task,
                &ProcedureTaskInterface::new(1, [], [id("product")]),
                Some(program.capability_formula().binding_scope),
                &requirements,
            )
            .unwrap();
    }

    #[test]
    fn formula_identity_constraints_scope_and_policy_are_enforced() {
        let task = id("cycle");
        let program = thermal();
        let interface = ProcedureTaskInterface::new(1, [], [id("product")]);
        let valid = requirements(&task, &program);

        let mut wrong_id = valid.clone();
        wrong_id[0].id = id("cycle::requirement::wrong");
        assert!(matches!(
            program.validate_task_contract(&task, &interface, None, &wrong_id),
            Err(ProcedureBindingError::MissingRequirement { .. })
        ));

        let mut wrong_constraints = valid.clone();
        wrong_constraints[0].constraints.pop();
        assert!(matches!(
            program.validate_task_contract(&task, &interface, None, &wrong_constraints),
            Err(ProcedureBindingError::RequirementConstraintsMismatch { .. })
        ));

        let mut wrong_policy = valid.clone();
        wrong_policy[1].minimum_qualification = QualificationLevel::Qualified;
        assert!(matches!(
            program.validate_task_contract(&task, &interface, None, &wrong_policy),
            Err(ProcedureBindingError::RequirementPolicyMismatch { .. })
        ));

        assert!(matches!(
            program.validate_task_contract(
                &task,
                &interface,
                Some(crate::procedure::BindingScope::Independent),
                &valid,
            ),
            Err(ProcedureBindingError::BindingScopeMismatch { .. })
        ));
    }
}
