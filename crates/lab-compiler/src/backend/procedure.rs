//! Typed semantic projections for built-in Procedure operations.
//!
//! A Method defines the Procedure graph and carries every scientific value into its tasks. These
//! projections validate the open operation identity and turn that generic, immutable task record
//! into the typed values shared by concrete adapters. They contain no deck addresses, labware
//! allocation, device commands, or facility selection.

use std::collections::{BTreeMap, BTreeSet};

use lab_instruments::{ThermalProfile, ThermalStage, ThermalStep};
use lab_procedure::{
    FluidPathPolicy, Location, PipettingStep, ValidatedProcedureProgram, VesselRole, Volume,
};

use crate::backend::invocation::{ProcedureTaskView, material_role};
use crate::planning::{
    AllocatedProcedureTask, AllocatedRequirementBinding, PlanningValueSource,
    SelectedMaterialBinding,
};

pub(crate) use crate::procedure::{CYCLE_GOLDEN_GATE, SERIAL_DILUTION, SETUP_GOLDEN_GATE};

pub(crate) struct MaterialVolume<'a> {
    pub(crate) role: String,
    pub(crate) material: &'a SelectedMaterialBinding,
    pub(crate) volume_ul: u32,
}

pub(crate) struct SetupGoldenGate<'a> {
    pub(crate) artifact: String,
    pub(crate) replicates: usize,
    pub(crate) additions: Vec<MaterialVolume<'a>>,
    pub(crate) reaction_volume_ul: u32,
    pub(crate) mix_cycles: u32,
    pub(crate) mix_volume_ul: u32,
}

pub(crate) struct NormalizedThermalProgram {
    pub(crate) artifact: String,
    pub(crate) sample_count: usize,
    pub(crate) volume_each_ul: f64,
    pub(crate) lid_temperature_c: Option<f64>,
    pub(crate) profile: ThermalProfile,
    pub(crate) final_hold_celsius: Option<f64>,
}

pub(crate) struct SerialDilution<'a> {
    pub(crate) culture_source: &'a PlanningValueSource,
    pub(crate) medium: &'a SelectedMaterialBinding,
    pub(crate) serial_dilutions: usize,
    pub(crate) medium_volume_ul: u32,
    pub(crate) culture_volume_ul: u32,
    pub(crate) mix_cycles: u32,
    pub(crate) mix_volume_ul: u32,
}

/// Project the normalized Golden Gate setup through the shared pipetting contract.
///
/// Device adapters consume the canonical material operations here. The original Method
/// parameters remain available only for human-facing labels such as the artifact name; they no
/// longer define transfer volumes, destination multiplicity, mixing, or contamination behavior.
pub(crate) fn normalized_golden_gate_setup<'a>(
    adapter: &str,
    task: &'a AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
) -> Result<SetupGoldenGate<'a>, String> {
    if task.operation.as_str() != SETUP_GOLDEN_GATE {
        return Err(format!(
            "{adapter} expected normalized Golden Gate setup, found operation '{}' in task '{}'",
            task.operation, task.id
        ));
    }
    let document = task.program.as_ref().ok_or_else(|| {
        format!(
            "{adapter} Procedure task '{}' is missing its normalized pipetting program",
            task.id
        )
    })?;
    let validated = document.validate().map_err(|error| {
        format!(
            "{adapter} Procedure task '{}' has an invalid normalized program: {error}",
            task.id
        )
    })?;
    validate_normalized_requirements(adapter, task, requirements, &validated)?;
    let ValidatedProcedureProgram::PipettingV1(program) = validated else {
        return Err(format!(
            "{adapter} Golden Gate task '{}' normalized to a non-pipetting contract",
            task.id
        ));
    };
    let program = program.as_program();
    let view = ProcedureTaskView::new(adapter, task);
    let artifact = view.text_parameter("artifact")?;
    let [output] = program.outputs.as_slice() else {
        return Err(format!(
            "{adapter} Golden Gate task '{}' must produce exactly one normalized material output",
            task.id
        ));
    };
    let destination_vessels = program
        .vessels
        .iter()
        .filter(|vessel| {
            matches!(&vessel.role, VesselRole::Product { output: candidate } if candidate == &output.id)
        })
        .collect::<Vec<_>>();
    let [destination_vessel] = destination_vessels.as_slice() else {
        return Err(format!(
            "{adapter} Golden Gate task '{}' must map its product to exactly one logical vessel",
            task.id
        ));
    };
    let destinations = (0..destination_vessel.positions)
        .map(|position| lab_procedure::Location {
            vessel: destination_vessel.id.clone(),
            position,
        })
        .collect::<Vec<_>>();
    let materials = task
        .materials
        .iter()
        .map(|material| (material.input.as_str(), material))
        .collect::<BTreeMap<_, _>>();
    let vessels = program
        .vessels
        .iter()
        .map(|vessel| (&vessel.id, vessel))
        .collect::<BTreeMap<_, _>>();
    let mut additions = Vec::new();
    let mut reaction_volume_ul = 0_u32;
    let mut mixing = None;
    for step in &program.steps {
        match step {
            PipettingStep::Distribute {
                source,
                destinations: step_destinations,
                volume_each,
                ..
            } => {
                if step_destinations != &destinations {
                    return Err(format!(
                        "{adapter} Golden Gate task '{}' distribute steps must address every product position in order",
                        task.id
                    ));
                }
                let source_vessel = vessels
                    .get(&source.vessel)
                    .expect("validated pipetting source vessel exists");
                let VesselRole::MaterialSource { material } = &source_vessel.role else {
                    return Err(format!(
                        "{adapter} Golden Gate task '{}' distribute source '{}' is not a material source",
                        task.id, source.vessel
                    ));
                };
                let material_binding = materials.get(material.as_str()).ok_or_else(|| {
                    format!(
                        "{adapter} Golden Gate task '{}' normalized material '{}' has no exact allocation",
                        task.id, material
                    )
                })?;
                let volume_ul = whole_microlitres(adapter, task, "transfer", volume_each)?;
                reaction_volume_ul =
                    reaction_volume_ul.checked_add(volume_ul).ok_or_else(|| {
                        format!(
                            "{adapter} Golden Gate task '{}' reaction volume overflows",
                            task.id
                        )
                    })?;
                let role = material_role(material_binding)
                    .map(normalized_material_role)
                    .ok_or_else(|| {
                        format!(
                            "{adapter} Golden Gate task '{}' material '{}' has no stable role",
                            task.id, material_binding.input
                        )
                    })?;
                additions.push(MaterialVolume {
                    role,
                    material: material_binding,
                    volume_ul,
                });
            }
            PipettingStep::Mix {
                targets,
                cycles,
                volume,
                ..
            } => {
                if targets != &destinations || mixing.is_some() {
                    return Err(format!(
                        "{adapter} Golden Gate task '{}' must contain one final mix over every product position",
                        task.id
                    ));
                }
                mixing = Some((*cycles, whole_microlitres(adapter, task, "mix", volume)?));
            }
            PipettingStep::Transfer { .. } | PipettingStep::Barrier { .. } => {
                return Err(format!(
                    "{adapter} Golden Gate task '{}' contains a pipetting step outside its normalized setup shape",
                    task.id
                ));
            }
        }
    }
    let (mix_cycles, mix_volume_ul) = mixing.ok_or_else(|| {
        format!(
            "{adapter} Golden Gate task '{}' has no normalized final mix",
            task.id
        )
    })?;
    if additions.is_empty() {
        return Err(format!(
            "{adapter} Golden Gate task '{}' has no normalized reagent additions",
            task.id
        ));
    }
    let used_materials = additions
        .iter()
        .map(|addition| addition.material.input.as_str())
        .collect::<BTreeSet<_>>();
    let declared_materials = program
        .materials
        .iter()
        .map(|material| material.id.as_str())
        .collect::<BTreeSet<_>>();
    if additions.len() != declared_materials.len() || used_materials != declared_materials {
        return Err(format!(
            "{adapter} Golden Gate task '{}' does not consume each normalized material exactly once",
            task.id
        ));
    }
    Ok(SetupGoldenGate {
        artifact,
        replicates: usize::try_from(destination_vessel.positions).map_err(|_| {
            format!(
                "{adapter} Golden Gate task '{}' destination count does not fit this platform",
                task.id
            )
        })?,
        additions,
        reaction_volume_ul,
        mix_cycles,
        mix_volume_ul,
    })
}

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

fn whole_microlitres(
    adapter: &str,
    task: &AllocatedProcedureTask,
    operation: &str,
    volume: &Volume,
) -> Result<u32, String> {
    volume.value().to_string().parse::<u32>().map_err(|_| {
        format!(
            "{adapter} Procedure task '{}' {operation} volume {} uL is not supported by this integer-volume device planner",
            task.id,
            volume.value()
        )
    })
}

fn normalized_material_role(role: &str) -> String {
    match role {
        "components" => "component".to_owned(),
        "dependencies" => "dependency".to_owned(),
        role => role.to_owned(),
    }
}

pub(crate) fn normalized_thermal_program(
    adapter: &str,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
) -> Result<NormalizedThermalProgram, String> {
    if task.operation.as_str() != CYCLE_GOLDEN_GATE {
        return Err(format!(
            "{adapter} expected normalized Golden Gate thermal cycling, found operation '{}' in task '{}'",
            task.operation, task.id
        ));
    }
    let document = task.program.as_ref().ok_or_else(|| {
        format!(
            "{adapter} Procedure task '{}' is missing its normalized thermal program",
            task.id
        )
    })?;
    let validated = document.validate().map_err(|error| {
        format!(
            "{adapter} Procedure task '{}' has an invalid normalized program: {error}",
            task.id
        )
    })?;
    validate_normalized_requirements(adapter, task, requirements, &validated)?;
    let ValidatedProcedureProgram::ThermalV1(program) = validated else {
        return Err(format!(
            "{adapter} thermal task '{}' normalized to a non-thermal contract",
            task.id
        ));
    };
    let program = program.as_program();
    let view = ProcedureTaskView::new(adapter, task);
    view.require_material_roles(&[])?;
    let artifact = view.text_parameter("artifact")?;
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
        artifact,
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

/// Project a canonical serial dilution through the shared pipetting contract.
///
/// The projection recognizes the exact logical liquid graph produced by Method refinement. It
/// recovers the compact values expected by the current device-specific planners without reading
/// the original Method parameters as an independent source of liquid-handling semantics.
pub(crate) fn normalized_serial_dilution<'a>(
    adapter: &str,
    task: &'a AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
) -> Result<SerialDilution<'a>, String> {
    if task.operation.as_str() != SERIAL_DILUTION {
        return Err(format!(
            "{adapter} expected normalized serial dilution, found operation '{}' in task '{}'",
            task.operation, task.id
        ));
    }
    let document = task.program.as_ref().ok_or_else(|| {
        format!(
            "{adapter} Procedure task '{}' is missing its normalized pipetting program",
            task.id
        )
    })?;
    let validated = document.validate().map_err(|error| {
        format!(
            "{adapter} Procedure task '{}' has an invalid normalized program: {error}",
            task.id
        )
    })?;
    validate_normalized_requirements(adapter, task, requirements, &validated)?;
    let ValidatedProcedureProgram::PipettingV1(program) = validated else {
        return Err(format!(
            "{adapter} serial-dilution task '{}' normalized to a non-pipetting contract",
            task.id
        ));
    };
    let program = program.as_program();
    let view = ProcedureTaskView::new(adapter, task);
    view.require_material_roles(&["medium"])?;
    if task.inputs.len() != 1 {
        return Err(format!(
            "{adapter} serial-dilution task '{}' must have exactly one culture input",
            task.id
        ));
    }
    let [material] = program.materials.as_slice() else {
        return Err(format!(
            "{adapter} serial-dilution task '{}' must declare exactly one normalized medium",
            task.id
        ));
    };
    let medium = view.one_material("medium")?;
    if medium.input.as_str() != material.id.as_str() {
        return Err(format!(
            "{adapter} serial-dilution task '{}' normalized medium '{}' does not match its exact material allocation '{}'",
            task.id, material.id, medium.input
        ));
    }
    let [output] = program.outputs.as_slice() else {
        return Err(format!(
            "{adapter} serial-dilution task '{}' must declare exactly one normalized output",
            task.id
        ));
    };
    let culture_vessels = program
        .vessels
        .iter()
        .filter(|vessel| matches!(vessel.role, VesselRole::ProcedureInput { input: 0 }))
        .collect::<Vec<_>>();
    let medium_vessels = program
        .vessels
        .iter()
        .filter(|vessel| {
            matches!(&vessel.role, VesselRole::MaterialSource { material: candidate } if candidate == &material.id)
        })
        .collect::<Vec<_>>();
    let product_vessels = program
        .vessels
        .iter()
        .filter(|vessel| {
            matches!(&vessel.role, VesselRole::Product { output: candidate } if candidate == &output.id)
        })
        .collect::<Vec<_>>();
    let ([culture_vessel], [medium_vessel], [product_vessel]) = (
        culture_vessels.as_slice(),
        medium_vessels.as_slice(),
        product_vessels.as_slice(),
    ) else {
        return Err(format!(
            "{adapter} serial-dilution task '{}' must map its culture, medium, and product to exactly one logical vessel each",
            task.id
        ));
    };
    if program.vessels.len() != 3 || culture_vessel.positions != 1 || medium_vessel.positions != 1 {
        return Err(format!(
            "{adapter} serial-dilution task '{}' has a non-canonical logical vessel layout",
            task.id
        ));
    }
    let serial_dilutions = usize::try_from(product_vessel.positions).map_err(|_| {
        format!(
            "{adapter} serial-dilution task '{}' destination count does not fit this platform",
            task.id
        )
    })?;
    let destinations = (0..product_vessel.positions)
        .map(|position| Location {
            vessel: product_vessel.id.clone(),
            position,
        })
        .collect::<Vec<_>>();
    let expected_steps = serial_dilutions
        .checked_mul(2)
        .and_then(|steps| steps.checked_add(1))
        .ok_or_else(|| {
            format!(
                "{adapter} serial-dilution task '{}' step count overflows",
                task.id
            )
        })?;
    if program.steps.len() != expected_steps {
        return Err(format!(
            "{adapter} serial-dilution task '{}' must contain one medium distribution followed by one transfer and mix per dilution",
            task.id
        ));
    }
    let PipettingStep::Distribute {
        source,
        destinations: medium_destinations,
        volume_each,
        fluid_path: FluidPathPolicy::SharedSourceNoReentry,
        ..
    } = &program.steps[0]
    else {
        return Err(format!(
            "{adapter} serial-dilution task '{}' must begin with a contamination-safe medium distribution",
            task.id
        ));
    };
    if source
        != &(Location {
            vessel: medium_vessel.id.clone(),
            position: 0,
        })
        || medium_destinations != &destinations
    {
        return Err(format!(
            "{adapter} serial-dilution task '{}' medium distribution does not address every dilution position in order",
            task.id
        ));
    }
    let medium_volume_ul = whole_microlitres(adapter, task, "medium transfer", volume_each)?;
    let mut culture_volume_ul = None;
    let mut mixing = None;
    for (position, pair) in program.steps[1..].chunks_exact(2).enumerate() {
        let destination = &destinations[position];
        let expected_source = if position == 0 {
            Location {
                vessel: culture_vessel.id.clone(),
                position: 0,
            }
        } else {
            destinations[position - 1].clone()
        };
        let PipettingStep::Transfer {
            source,
            destination: step_destination,
            volume,
            fluid_path: FluidPathPolicy::IsolatedDestinations,
            ..
        } = &pair[0]
        else {
            return Err(format!(
                "{adapter} serial-dilution task '{}' position {position} has no isolated culture transfer",
                task.id
            ));
        };
        if source != &expected_source || step_destination != destination {
            return Err(format!(
                "{adapter} serial-dilution task '{}' position {position} does not continue the dilution chain",
                task.id
            ));
        }
        let transfer_volume = whole_microlitres(adapter, task, "culture transfer", volume)?;
        if culture_volume_ul
            .replace(transfer_volume)
            .is_some_and(|first| first != transfer_volume)
        {
            return Err(format!(
                "{adapter} serial-dilution task '{}' uses inconsistent culture-transfer volumes",
                task.id
            ));
        }
        let PipettingStep::Mix {
            targets,
            cycles,
            volume,
            fluid_path: FluidPathPolicy::IsolatedDestinations,
            ..
        } = &pair[1]
        else {
            return Err(format!(
                "{adapter} serial-dilution task '{}' position {position} has no isolated mix",
                task.id
            ));
        };
        if targets.as_slice() != std::slice::from_ref(destination) {
            return Err(format!(
                "{adapter} serial-dilution task '{}' position {position} mixes the wrong logical vessel",
                task.id
            ));
        }
        let mix = (*cycles, whole_microlitres(adapter, task, "mix", volume)?);
        if mixing.replace(mix).is_some_and(|first| first != mix) {
            return Err(format!(
                "{adapter} serial-dilution task '{}' uses inconsistent mixing parameters",
                task.id
            ));
        }
    }
    let culture_volume_ul = culture_volume_ul.expect("a validated product vessel is non-empty");
    let (mix_cycles, mix_volume_ul) = mixing.expect("a validated product vessel is non-empty");

    Ok(SerialDilution {
        culture_source: &task.inputs[0].source,
        medium,
        serial_dilutions,
        medium_volume_ul,
        culture_volume_ul,
        mix_cycles,
        mix_volume_ul,
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
