//! Typed semantic projections for built-in Procedure operations.
//!
//! A Method defines the Procedure graph and carries every scientific value into its tasks. These
//! projections validate the open operation identity and turn that generic, immutable task record
//! into the typed values shared by concrete adapters. They contain no deck addresses, labware
//! allocation, device commands, or facility selection.

use std::collections::{BTreeMap, BTreeSet};

use lab_procedure::{PipettingStep, ValidatedProcedureProgram, VesselRole, Volume};

use crate::backend::invocation::{MICROLITRE, ProcedureTaskView, material_role};
use crate::planning::{
    AllocatedProcedureTask, AllocatedRequirementBinding, PlanningValueSource,
    SelectedMaterialBinding,
};

pub(crate) const SETUP_GOLDEN_GATE: &str =
    "https://www.lab-compiler.org/ns/procedure#SetupGoldenGateReaction";
pub(crate) const CYCLE_GOLDEN_GATE: &str =
    "https://www.lab-compiler.org/ns/procedure#ThermalCycleGoldenGateReaction";
pub(crate) const SERIAL_DILUTION: &str =
    "https://www.lab-compiler.org/ns/procedure#SeriallyDiluteCulture";

pub(crate) const LIQUID_HANDLING: &str = "https://sbol.io/ns/capability#LiquidHandling";
pub(crate) const THERMAL_CYCLING: &str = "https://sbol.io/ns/capability#ThermalCycling";

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

pub(crate) struct ThermalCycleGoldenGate {
    pub(crate) artifact: String,
    pub(crate) replicates: usize,
    pub(crate) reaction_volume_ul: u32,
    pub(crate) cycles: u32,
    pub(crate) digest_temperature_c: u32,
    pub(crate) digest_minutes: u32,
    pub(crate) ligate_temperature_c: u32,
    pub(crate) ligate_minutes: u32,
    pub(crate) lid_temperature_c: u32,
    pub(crate) final_digest_temperature_c: u32,
    pub(crate) final_digest_minutes: u32,
    pub(crate) heat_inactivation_temperature_c: u32,
    pub(crate) heat_inactivation_minutes: u32,
    pub(crate) hold_temperature_c: u32,
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
    let ValidatedProcedureProgram::PipettingV1(program) = validated;
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

pub(crate) fn thermal_cycle_golden_gate(
    adapter: &str,
    task: &AllocatedProcedureTask,
    requirement: &AllocatedRequirementBinding,
) -> Result<ThermalCycleGoldenGate, String> {
    use crate::backend::invocation::{DEGREE_CELSIUS, MINUTE};

    let view = ProcedureTaskView::new(adapter, task);
    view.require_capability(requirement, THERMAL_CYCLING)?;
    view.require_material_roles(&[])?;
    let artifact = view.text_parameter("artifact")?;
    let replicates = view.usize_parameter("assembly_replicates", None)?;
    view.require_nonzero("assembly_replicates", replicates as u32)?;
    let reaction_volume_ul = view.integer_parameter("reaction_volume_ul", Some(MICROLITRE))?;
    let cycles = view.integer_parameter("cycles", None)?;
    let digest_temperature_c =
        view.integer_parameter("digest_temperature_c", Some(DEGREE_CELSIUS))?;
    let digest_minutes = view.integer_parameter("digest_minutes", Some(MINUTE))?;
    let ligate_temperature_c =
        view.integer_parameter("ligate_temperature_c", Some(DEGREE_CELSIUS))?;
    let ligate_minutes = view.integer_parameter("ligate_minutes", Some(MINUTE))?;
    let lid_temperature_c = view.integer_parameter("lid_temperature_c", Some(DEGREE_CELSIUS))?;
    let final_digest_temperature_c =
        view.integer_parameter("final_digest_temperature_c", Some(DEGREE_CELSIUS))?;
    let final_digest_minutes = view.integer_parameter("final_digest_minutes", Some(MINUTE))?;
    let heat_inactivation_temperature_c =
        view.integer_parameter("heat_inactivation_temperature_c", Some(DEGREE_CELSIUS))?;
    let heat_inactivation_minutes =
        view.integer_parameter("heat_inactivation_minutes", Some(MINUTE))?;
    let hold_temperature_c = view.integer_parameter("hold_temperature_c", Some(DEGREE_CELSIUS))?;
    for (name, value) in [
        ("reaction_volume_ul", reaction_volume_ul),
        ("cycles", cycles),
        ("digest_minutes", digest_minutes),
        ("ligate_minutes", ligate_minutes),
        ("lid_temperature_c", lid_temperature_c),
        ("final_digest_minutes", final_digest_minutes),
        ("heat_inactivation_minutes", heat_inactivation_minutes),
    ] {
        view.require_nonzero(name, value)?;
    }

    Ok(ThermalCycleGoldenGate {
        artifact,
        replicates,
        reaction_volume_ul,
        cycles,
        digest_temperature_c,
        digest_minutes,
        ligate_temperature_c,
        ligate_minutes,
        lid_temperature_c,
        final_digest_temperature_c,
        final_digest_minutes,
        heat_inactivation_temperature_c,
        heat_inactivation_minutes,
        hold_temperature_c,
    })
}

pub(crate) fn serial_dilution<'a>(
    adapter: &str,
    task: &'a AllocatedProcedureTask,
    requirement: &AllocatedRequirementBinding,
) -> Result<SerialDilution<'a>, String> {
    let view = ProcedureTaskView::new(adapter, task);
    view.require_capability(requirement, LIQUID_HANDLING)?;
    view.require_material_roles(&["medium"])?;
    if task.inputs.len() != 1 {
        return Err(format!(
            "{adapter} serial-dilution task '{}' must have exactly one culture input",
            task.id
        ));
    }
    let serial_dilutions = view.usize_parameter("serial_dilutions", None)?;
    view.require_nonzero("serial_dilutions", serial_dilutions as u32)?;
    let medium_volume_ul = view.integer_parameter("medium_volume_ul", Some(MICROLITRE))?;
    let culture_volume_ul = view.integer_parameter("culture_volume_ul", Some(MICROLITRE))?;
    let mix_cycles = view.integer_parameter("mix_cycles", None)?;
    let mix_volume_ul = view.integer_parameter("mix_volume_ul", Some(MICROLITRE))?;
    for (name, value) in [
        ("medium_volume_ul", medium_volume_ul),
        ("culture_volume_ul", culture_volume_ul),
        ("mix_cycles", mix_cycles),
        ("mix_volume_ul", mix_volume_ul),
    ] {
        view.require_nonzero(name, value)?;
    }
    let diluted_volume = medium_volume_ul
        .checked_add(culture_volume_ul)
        .ok_or_else(|| {
            format!(
                "{adapter} Procedure task '{}' dilution volume overflows",
                task.id
            )
        })?;
    if mix_volume_ul > diluted_volume {
        return Err(format!(
            "{adapter} Procedure task '{}' mix volume {} uL exceeds its {} uL dilution",
            task.id, mix_volume_ul, diluted_volume
        ));
    }

    Ok(SerialDilution {
        culture_source: &task.inputs[0].source,
        medium: view.one_material("medium")?,
        serial_dilutions,
        medium_volume_ul,
        culture_volume_ul,
        mix_cycles,
        mix_volume_ul,
    })
}
