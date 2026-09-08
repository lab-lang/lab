//! Shared decoding and allocation checks for canonical Procedure programs.
//!
//! Adapters consume open, typed program contracts. Scientific operation names remain descriptive
//! metadata and never select a device lowering path.

use lab_compiler::allocation::{AllocatedProcedureTask, AllocatedRequirementBinding};
use lab_compiler::procedure::{
    ProcedureContractRegistry, ValidatedPipettingProgramV1, ValidatedProcedureProgram, Volume,
};
use lab_instruments::{ThermalProfile, ThermalStage, ThermalStep};

use crate::backend::invocation::ProcedureTaskView;

pub(crate) struct NormalizedThermalProgram {
    pub(crate) artifact: String,
    pub(crate) title: String,
    pub(crate) sample_count: usize,
    pub(crate) volume_each_ul: f64,
    pub(crate) lid_temperature_c: Option<f64>,
    pub(crate) profile: ThermalProfile,
    pub(crate) final_hold_celsius: Option<f64>,
}

/// Validate one task's canonical pipetting program and its exact allocated requirements.
pub(crate) fn canonical_pipetting_program(
    adapter: &str,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
    contracts: &ProcedureContractRegistry,
) -> Result<ValidatedPipettingProgramV1, String> {
    let document = task.program.as_ref().ok_or_else(|| {
        format!(
            "{adapter} Procedure task '{}' is missing its normalized pipetting program",
            task.id
        )
    })?;
    let validated = document.validate(contracts).map_err(|error| {
        format!(
            "{adapter} Procedure task '{}' has an invalid normalized program: {error}",
            task.id
        )
    })?;
    validate_normalized_requirements(adapter, task, requirements, &validated)?;
    let program = validated.pipetting().map_err(|error| {
        format!(
            "{adapter} Procedure task '{}' normalized to a non-pipetting contract: {error}",
            task.id
        )
    })?;
    Ok(program)
}

/// Represent one exact canonical volume in a device planner's finite units.
pub(crate) fn volume_microlitres(
    adapter: &str,
    task: &AllocatedProcedureTask,
    operation: &str,
    volume: &Volume,
) -> Result<f64, String> {
    finite_f64(adapter, task, operation, volume.value())
}

/// Ensure the allocated capability clauses are exactly those derived from the canonical program.
fn validate_normalized_requirements(
    adapter: &str,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
    program: &ValidatedProcedureProgram,
) -> Result<(), String> {
    let formula = program.capability_formula();
    if requirements.len() != formula.all_of.len() {
        return Err(format!(
            "{adapter} Procedure task '{}' requires {} derived capability bindings, found {}",
            task.id,
            formula.all_of.len(),
            requirements.len()
        ));
    }
    let mut implementation = None;
    for clause in formula.all_of {
        let expected_id = format!("{}::requirement::{}", task.id, clause.role);
        let requirement = requirements
            .iter()
            .find(|requirement| requirement.id.as_str() == expected_id)
            .ok_or_else(|| {
                format!(
                    "{adapter} Procedure task '{}' is missing derived capability role '{}'",
                    task.id, clause.role
                )
            })?;
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
            return Err(format!(
                "{adapter} Procedure task '{}' capability role '{}' does not preserve its derived constraints",
                task.id, clause.role
            ));
        }
        let selected = requirement.procedure_implementation.as_ref().ok_or_else(|| {
            format!(
                "{adapter} Procedure task '{}' capability role '{}' has no Procedure implementation",
                task.id, clause.role
            )
        })?;
        if implementation
            .replace(selected)
            .is_some_and(|first| first != selected)
        {
            return Err(format!(
                "{adapter} Procedure task '{}' capability clauses use different Procedure implementations",
                task.id
            ));
        }
    }
    Ok(())
}

pub(crate) fn normalized_thermal_program(
    adapter: &str,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
    contracts: &ProcedureContractRegistry,
) -> Result<NormalizedThermalProgram, String> {
    let operation_title = task
        .operation
        .as_str()
        .rsplit(['#', '/', '.'])
        .find(|part| !part.is_empty())
        .unwrap_or("thermal program")
        .replace(['_', '-'], " ");
    let document = task.program.as_ref().ok_or_else(|| {
        format!(
            "{adapter} Procedure task '{}' is missing its normalized thermal program",
            task.id
        )
    })?;
    let validated = document.validate(contracts).map_err(|error| {
        format!(
            "{adapter} Procedure task '{}' has an invalid normalized program: {error}",
            task.id
        )
    })?;
    validate_normalized_requirements(adapter, task, requirements, &validated)?;
    let program = validated.thermal().map_err(|error| {
        format!(
            "{adapter} thermal task '{}' normalized to a non-thermal contract: {error}",
            task.id
        )
    })?;
    let program = program.as_program();
    let view = ProcedureTaskView::new(adapter, task);
    if !task.materials.is_empty() {
        return Err(format!(
            "{adapter} thermal Procedure task '{}' cannot bind pipetting materials",
            task.id
        ));
    }
    let subject = view
        .optional_text_parameter("artifact")?
        .or(view.optional_text_parameter("subject")?);
    let title = match &subject {
        Some(subject) => format!("{operation_title} for {subject}"),
        None => operation_title.clone(),
    };
    let lid_temperature_c = program
        .lid_temperature
        .as_ref()
        .map(|temperature| finite_f64(adapter, task, "lid temperature", temperature.value()))
        .transpose()?;
    let profile = ThermalProfile {
        stages: program
            .stages
            .iter()
            .map(|stage| {
                Ok(ThermalStage {
                    repeats: stage.repeats,
                    steps: stage
                        .steps
                        .iter()
                        .map(|step| {
                            Ok(ThermalStep {
                                celsius: finite_f64(
                                    adapter,
                                    task,
                                    "block temperature",
                                    step.temperature.value(),
                                )?,
                                hold_seconds: finite_f64(
                                    adapter,
                                    task,
                                    "hold duration",
                                    step.hold.value(),
                                )?,
                                ramp_c_per_s: step
                                    .ramp_rate
                                    .as_ref()
                                    .map(|rate| {
                                        finite_f64(adapter, task, "ramp rate", rate.value())
                                    })
                                    .transpose()?,
                                lid_celsius: lid_temperature_c,
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    };
    Ok(NormalizedThermalProgram {
        artifact: subject.unwrap_or(operation_title),
        title,
        sample_count: usize::try_from(program.load.sample_count).map_err(|_| {
            format!(
                "{adapter} Procedure task '{}' sample count does not fit this platform",
                task.id
            )
        })?,
        volume_each_ul: finite_f64(
            adapter,
            task,
            "sample volume",
            program.load.volume_each.value(),
        )?,
        lid_temperature_c,
        profile,
        final_hold_celsius: program
            .final_hold
            .as_ref()
            .map(|temperature| {
                finite_f64(adapter, task, "final hold temperature", temperature.value())
            })
            .transpose()?,
    })
}

fn finite_f64(
    adapter: &str,
    task: &AllocatedProcedureTask,
    quantity: &str,
    value: &lab_capability::ExactDecimal,
) -> Result<f64, String> {
    let parsed = value.to_string().parse::<f64>().map_err(|_| {
        format!(
            "{adapter} Procedure task '{}' {quantity} `{value}` cannot be represented by this adapter",
            task.id
        )
    })?;
    if !parsed.is_finite() {
        return Err(format!(
            "{adapter} Procedure task '{}' {quantity} `{value}` is outside this adapter's finite range",
            task.id
        ));
    }
    Ok(parsed)
}

#[cfg(test)]
mod canonical_interpreter_tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use lab_capability::{
        ControlMode, MethodId, OperationId, ProcedureImplementationId, QualificationLevel,
    };
    use lab_compiler::allocation::{
        AllocatedMethod, AllocatedProcedureTask, AllocatedProgram, AllocatedRequirementBinding,
        InvocationAdapter,
    };
    use lab_compiler::method::{IntentOperationId, LocalId, PortType};
    use lab_compiler::planning::{
        PlanningTaskOutput, SelectedCapabilityParameter, SelectedMaterialBinding,
        SelectedMaterialSource,
    };
    use lab_compiler::procedure::vocabulary::PIPETTING_PROGRAM_V1;
    use lab_compiler::procedure::{
        FluidPathPolicy, Location, MaterialInput, MaterialOutput, PipettingConstraints,
        PipettingProgramV1, PipettingStep, ProcedureLocalId, ProcedureProgram, Vessel, VesselRole,
        Volume, builtin_procedure_contracts,
    };

    use crate::backend::hamilton::star::{self, StarAdapterProfile};
    use crate::backend::opentrons::flex::{self, FlexAdapterProfile};
    use crate::backend::opentrons::ot2::{self, Ot2AdapterProfile};
    use crate::{
        ADAPTER_INVOCATIONS_SCHEMA_VERSION, AdapterInvocation, AdapterInvocationPlan,
        adapter_invocation_id, builtin_adapter_registry,
    };

    fn local(value: &str) -> LocalId {
        LocalId::new(value).unwrap()
    }

    fn procedure_local(value: &str) -> ProcedureLocalId {
        ProcedureLocalId::new(value).unwrap()
    }

    fn location(vessel: &str, position: u32) -> Location {
        Location {
            vessel: procedure_local(vessel),
            position,
        }
    }

    fn novel_task() -> AllocatedProcedureTask {
        let task = local("novel-buffer-exchange");
        let water = procedure_local("novel-buffer-exchange::material::water");
        let buffer = procedure_local("novel-buffer-exchange::material::buffer");
        let output = procedure_local("exchanged-samples");
        let canonical = PipettingProgramV1::new(
            vec![
                MaterialInput { id: water.clone() },
                MaterialInput { id: buffer.clone() },
            ],
            vec![MaterialOutput { id: output.clone() }],
            vec![
                Vessel {
                    id: procedure_local("water-source"),
                    role: VesselRole::MaterialSource {
                        material: water.clone(),
                    },
                    positions: 1,
                    initial_volume_each: None,
                    working_capacity_each: None,
                    dead_volume_each: None,
                    temperature: None,
                },
                Vessel {
                    id: procedure_local("buffer-source"),
                    role: VesselRole::MaterialSource {
                        material: buffer.clone(),
                    },
                    positions: 1,
                    initial_volume_each: None,
                    working_capacity_each: None,
                    dead_volume_each: None,
                    temperature: None,
                },
                Vessel {
                    id: procedure_local("exchange-products"),
                    role: VesselRole::Product {
                        output: output.clone(),
                    },
                    positions: 2,
                    initial_volume_each: None,
                    working_capacity_each: None,
                    dead_volume_each: None,
                    temperature: None,
                },
            ],
            vec![
                PipettingStep::Transfer {
                    id: procedure_local("seed-first-product"),
                    source: location("water-source", 0),
                    destination: location("exchange-products", 0),
                    volume: Volume::parse_microlitres("3").unwrap(),
                    fluid_path: FluidPathPolicy::IsolatedDestinations,
                    fluid_path_group: None,
                    technique: Default::default(),
                },
                PipettingStep::Distribute {
                    id: procedure_local("add-buffer-to-both"),
                    source: location("buffer-source", 0),
                    destinations: vec![
                        location("exchange-products", 0),
                        location("exchange-products", 1),
                    ],
                    volume_each: Volume::parse_microlitres("2").unwrap(),
                    fluid_path: FluidPathPolicy::SharedSourceNoReentry,
                    fluid_path_group: None,
                    technique: Default::default(),
                },
                PipettingStep::Mix {
                    id: procedure_local("homogenize-products"),
                    targets: vec![
                        location("exchange-products", 0),
                        location("exchange-products", 1),
                    ],
                    cycles: 2,
                    volume: Volume::parse_microlitres("2").unwrap(),
                    fluid_path: FluidPathPolicy::IsolatedDestinations,
                    fluid_path_group: None,
                    technique: Default::default(),
                },
            ],
            PipettingConstraints::default(),
        )
        .validate()
        .unwrap();
        let formula = canonical.capability_formula();
        let requirements = formula
            .all_of
            .into_iter()
            .map(|clause| {
                let id = local(&format!("{task}::requirement::{}", clause.role));
                AllocatedRequirementBinding {
                    id,
                    capability_kind: clause.capability_kind,
                    minimum_qualification: QualificationLevel::Executable,
                    accepted_control_modes: BTreeSet::from([ControlMode::ReviewedFile]),
                    offering: "https://example.org/offering/canonical-pipetting".to_owned(),
                    asset: "https://example.org/asset/liquid-handler".to_owned(),
                    observed_qualification: QualificationLevel::Executable.to_string(),
                    control_mode: ControlMode::ReviewedFile.to_string(),
                    parameters: clause
                        .constraints
                        .into_iter()
                        .enumerate()
                        .map(|(index, constraint)| SelectedCapabilityParameter {
                            property_kind: constraint.property_kind,
                            relation: constraint.relation,
                            required: constraint.required.clone(),
                            offering_parameter: format!(
                                "https://example.org/offering/parameter/{index}"
                            ),
                            observed: constraint.required,
                        })
                        .collect(),
                    procedure_implementation: Some(
                        ProcedureImplementationId::new(
                            "https://example.org/implementation/canonical-pipetting",
                        )
                        .unwrap(),
                    ),
                    adapter: None,
                }
            })
            .collect();
        AllocatedProcedureTask {
            id: task,
            operation: OperationId::new("https://example.org/science/novel-buffer-exchange")
                .unwrap(),
            program: Some(ProcedureProgram::from_pipetting(&canonical)),
            inputs: Vec::new(),
            outputs: vec![PlanningTaskOutput {
                name: local(output.as_str()),
                port_type: PortType::Material {
                    state: lab_capability::AbsoluteIri::new(
                        "https://example.org/material/exchanged-sample",
                    )
                    .unwrap(),
                },
            }],
            parameters: Vec::new(),
            materials: vec![material(water, "water"), material(buffer, "buffer")],
            requirements,
        }
    }

    fn material(id: ProcedureLocalId, symbol: &str) -> SelectedMaterialBinding {
        SelectedMaterialBinding {
            input: local(id.as_str()),
            symbol: symbol.to_owned(),
            source: SelectedMaterialSource::MaterialLot {
                component: format!("https://example.org/component/{symbol}"),
                material_lot: format!("https://example.org/material-lot/{symbol}"),
            },
            interchangeable_alternatives: Vec::new(),
        }
    }

    fn assert_public_lowering(driver: &str, artifact_path: &str) {
        let registry = builtin_adapter_registry().unwrap();
        let profile = registry
            .validate_profile(driver, "canonical-conformance", "")
            .unwrap();
        let descriptor = registry.descriptors().descriptor(driver).unwrap();
        let implementation = descriptor
            .procedure_implementations
            .iter()
            .find(|implementation| {
                implementation.contract.as_str() == PIPETTING_PROGRAM_V1
                    && implementation.services.lowering
            })
            .unwrap();
        let adapter = InvocationAdapter {
            driver: descriptor.id.clone(),
            profile_path: PathBuf::from(format!("adapters/{driver}.toml")),
            profile_sha256: profile.sha256.clone(),
            features: descriptor.features.clone(),
            accepted_run_formats: implementation.accepted_run_formats.clone(),
            emitted_run_formats: implementation.emitted_run_formats.clone(),
        };
        let asset = format!("https://example.org/asset/{driver}");
        let mut task = novel_task();
        for requirement in &mut task.requirements {
            requirement.offering = format!("https://example.org/offering/{driver}");
            requirement.asset = asset.clone();
            requirement.procedure_implementation = Some(implementation.id.clone());
            requirement.adapter = Some(adapter.clone());
        }
        let task_id = task.id.clone();
        let requirement_ids = task
            .requirements
            .iter()
            .map(|requirement| requirement.id.clone())
            .collect();
        let operation = task.operation.to_string();
        let allocated = AllocatedProgram {
            problem_sha256: "a".repeat(64),
            inventory_sha256: "b".repeat(64),
            facility: "https://example.org/facility".to_owned(),
            methods: vec![AllocatedMethod {
                choice: local("novel-buffer-exchange-choice"),
                source_operation: IntentOperationId::new(&operation).unwrap(),
                source_intent: crate::test_source_intent(&operation),
                method: MethodId::new("https://example.org/method/novel-buffer-exchange").unwrap(),
                after: Vec::new(),
                inputs: Vec::new(),
                outputs: Vec::new(),
                yields: Vec::new(),
                tasks: vec![task],
            }],
        };
        let invocation = AdapterInvocation {
            id: adapter_invocation_id(&asset, &adapter),
            asset,
            adapter,
            tasks: vec![task_id],
            requirements: requirement_ids,
        };
        let plan = AdapterInvocationPlan {
            schema_version: ADAPTER_INVOCATIONS_SCHEMA_VERSION.to_owned(),
            allocated,
            allocated_lair_sha256: "c".repeat(64),
            invocations: vec![invocation.clone()],
        };
        let lowered = registry
            .lower_invocation(&profile, &plan, &invocation, builtin_procedure_contracts())
            .unwrap_or_else(|error| panic!("{driver} failed canonical lowering: {error}"));
        let vendor_artifact = lowered
            .artifacts
            .get(artifact_path)
            .unwrap_or_else(|| panic!("{driver} did not emit {artifact_path}"))
            .text_contents()
            .unwrap();
        assert!(vendor_artifact.contains("aspirat"));
        assert!(vendor_artifact.contains("dispens"));
        assert!(
            lowered
                .documents
                .iter()
                .any(|document| document.path == artifact_path)
        );
        let manifest = lowered
            .artifacts
            .get("tasks/001-pipetting-program/invocation_manifest.json")
            .unwrap()
            .text_contents()
            .unwrap();
        assert!(manifest.contains("novel-buffer-exchange"));
        for step in [
            "seed-first-product",
            "add-buffer-to-both",
            "homogenize-products",
        ] {
            assert!(manifest.contains(step));
        }
    }

    #[test]
    fn novel_canonical_transfer_distribute_mix_lowers_without_a_recipe_projection() {
        let task = novel_task();
        let contracts = builtin_procedure_contracts();
        flex::check_task_feasibility(&FlexAdapterProfile::default(), &task, contracts)
            .expect("Flex interprets the canonical program");
        ot2::check_task_feasibility(&Ot2AdapterProfile::default(), &task, contracts)
            .expect("OT-2 interprets the canonical program");
        star::check_task_feasibility(&StarAdapterProfile::default(), &task, contracts)
            .expect("STAR interprets the canonical program");

        for (driver, artifact) in [
            (
                "opentrons.ot2",
                "tasks/001-pipetting-program/automation_protocol.py",
            ),
            (
                "opentrons.flex",
                "tasks/001-pipetting-program/automation_protocol.json",
            ),
            (
                "hamilton.star",
                "tasks/001-pipetting-program/automation_run.json",
            ),
        ] {
            assert_public_lowering(driver, artifact);
        }
    }
}
