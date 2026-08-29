//! Typed semantic projections for built-in Procedure operations.
//!
//! A Method defines the Procedure graph and carries every scientific value into its tasks. These
//! projections validate the open operation identity and turn that generic, immutable task record
//! into the typed values shared by concrete adapters. They contain no deck addresses, labware
//! allocation, device commands, or facility selection.

use std::collections::BTreeMap;

use crate::backend::invocation::{MICROLITRE, ProcedureTaskView, material_symbols};
use crate::planning::{
    AllocatedProcedureTask, AllocatedRequirementBinding, PlanningValueSource,
    SelectedMaterialBinding, SelectedMaterialSource,
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
    pub(crate) role: &'static str,
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

pub(crate) fn setup_golden_gate<'a>(
    adapter: &str,
    task: &'a AllocatedProcedureTask,
    requirement: &AllocatedRequirementBinding,
) -> Result<SetupGoldenGate<'a>, String> {
    let view = ProcedureTaskView::new(adapter, task);
    view.require_capability(requirement, LIQUID_HANDLING)?;
    view.require_material_roles(&[
        "backbone",
        "components",
        "dependencies",
        "restriction-enzyme",
        "ligase",
        "buffer",
        "water",
    ])?;
    let artifact = view.text_parameter("artifact")?;
    let backbone = view.text_parameter("backbone")?;
    let components = view.text_list_parameter("components")?;
    let dependencies = view.text_list_parameter("dependencies")?;
    let restriction_enzyme = view.text_parameter("restriction_enzyme")?;
    let replicates = view.usize_parameter("assembly_replicates", None)?;
    view.require_nonzero("assembly_replicates", replicates as u32)?;
    let reaction_volume_ul = view.integer_parameter("reaction_volume_ul", Some(MICROLITRE))?;
    let part_volume_ul = view.integer_parameter("part_volume_ul", Some(MICROLITRE))?;
    let enzyme_volume_ul = view.integer_parameter("enzyme_volume_ul", Some(MICROLITRE))?;
    let ligase_volume_ul = view.integer_parameter("ligase_volume_ul", Some(MICROLITRE))?;
    let buffer_volume_ul = view.integer_parameter("buffer_volume_ul", Some(MICROLITRE))?;
    let mix_cycles = view.integer_parameter("mix_cycles", None)?;
    let mix_volume_ul = view.integer_parameter("mix_volume_ul", Some(MICROLITRE))?;
    for (name, value) in [
        ("reaction_volume_ul", reaction_volume_ul),
        ("part_volume_ul", part_volume_ul),
        ("enzyme_volume_ul", enzyme_volume_ul),
        ("ligase_volume_ul", ligase_volume_ul),
        ("buffer_volume_ul", buffer_volume_ul),
        ("mix_cycles", mix_cycles),
        ("mix_volume_ul", mix_volume_ul),
    ] {
        view.require_nonzero(name, value)?;
    }
    if mix_volume_ul > reaction_volume_ul {
        return Err(format!(
            "{adapter} Procedure task '{}' mix volume {} uL exceeds its {} uL reaction",
            task.id, mix_volume_ul, reaction_volume_ul
        ));
    }

    let backbone_material = view.one_material("backbone")?;
    if backbone_material.symbol != backbone {
        return Err(view.material_parameter_mismatch("backbone"));
    }
    let component_materials = view.materials("components");
    if material_symbols(&component_materials) != components {
        return Err(view.material_parameter_mismatch("components"));
    }
    let dependency_materials = view.materials("dependencies");
    if material_symbols(&dependency_materials) != dependencies {
        return Err(view.material_parameter_mismatch("dependencies"));
    }
    let enzyme_material = view.one_material("restriction-enzyme")?;
    if enzyme_material.symbol != restriction_enzyme {
        return Err(view.material_parameter_mismatch("restriction_enzyme"));
    }
    let ligase_material = view.one_material("ligase")?;
    let buffer_material = view.one_material("buffer")?;
    let water_material = view.one_material("water")?;

    let dna_piece_count = u32::try_from(component_materials.len() + dependency_materials.len() + 1)
        .map_err(|_| {
            format!(
                "{adapter} Procedure task '{}' has too many DNA pieces",
                task.id
            )
        })?;
    let consumed = buffer_volume_ul
        .checked_add(ligase_volume_ul)
        .and_then(|value| value.checked_add(enzyme_volume_ul))
        .and_then(|value| value.checked_add(part_volume_ul.checked_mul(dna_piece_count)?))
        .ok_or_else(|| {
            format!(
                "{adapter} Procedure task '{}' reaction volume overflows",
                task.id
            )
        })?;
    let water_volume_ul = reaction_volume_ul.checked_sub(consumed).ok_or_else(|| {
        format!(
            "{adapter} Procedure task '{}' requires {consumed} uL before water in a {reaction_volume_ul} uL reaction",
            task.id
        )
    })?;

    let mut additions = vec![
        MaterialVolume {
            role: "water",
            material: water_material,
            volume_ul: water_volume_ul,
        },
        MaterialVolume {
            role: "buffer",
            material: buffer_material,
            volume_ul: buffer_volume_ul,
        },
        MaterialVolume {
            role: "ligase",
            material: ligase_material,
            volume_ul: ligase_volume_ul,
        },
        MaterialVolume {
            role: "restriction-enzyme",
            material: enzyme_material,
            volume_ul: enzyme_volume_ul,
        },
        MaterialVolume {
            role: "backbone",
            material: backbone_material,
            volume_ul: part_volume_ul,
        },
    ];
    additions.extend(
        component_materials
            .into_iter()
            .map(|material| MaterialVolume {
                role: "component",
                material,
                volume_ul: part_volume_ul,
            }),
    );
    additions.extend(
        dependency_materials
            .into_iter()
            .map(|material| MaterialVolume {
                role: "dependency",
                material,
                volume_ul: part_volume_ul,
            }),
    );

    let mut sources = BTreeMap::<String, &SelectedMaterialSource>::new();
    for addition in &additions {
        if let Some(previous) =
            sources.insert(addition.material.symbol.clone(), &addition.material.source)
            && previous != &addition.material.source
        {
            return Err(format!(
                "{adapter} Procedure task '{}' assigns material '{}' to several physical sources",
                task.id, addition.material.symbol
            ));
        }
    }

    Ok(SetupGoldenGate {
        artifact,
        replicates,
        additions,
        reaction_volume_ul,
        mix_cycles,
        mix_volume_ul,
    })
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
