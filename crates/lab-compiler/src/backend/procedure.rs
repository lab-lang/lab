//! Typed semantic projections for built-in Procedure operations.
//!
//! A Method defines the Procedure graph and carries every scientific value into its tasks. These
//! projections validate the open operation identity and turn that generic, immutable task record
//! into the typed values shared by concrete adapters. They contain no deck addresses, labware
//! allocation, device commands, or facility selection.

use std::collections::{BTreeMap, BTreeSet};

use lab_instruments::{ThermalProfile, ThermalStage, ThermalStep};
use lab_procedure::{
    FluidPathPolicy, Location, MixTechnique, PipettingProgramV1, PipettingStep, TransferTechnique,
    ValidatedPipettingProgramV1, ValidatedProcedureProgram, VesselRole, Volume,
};

use crate::backend::invocation::{ProcedureTaskView, material_role};
use crate::planning::{
    AllocatedProcedureTask, AllocatedRequirementBinding, PlanningValueSource,
    SelectedMaterialBinding,
};

const MICROLITRE: &str = "http://qudt.org/vocab/unit/MicroL";

pub(crate) use crate::procedure::{
    ADD_RECOVERY_MEDIUM, CYCLE_GOLDEN_GATE, HEAT_SHOCK_TRANSFORMATION, INCUBATE_RECOVERY_CULTURE,
    PLATE_DILUTED_CULTURE, PREPARE_CHEMICAL_TRANSFORMATION, SERIAL_DILUTION, SETUP_GOLDEN_GATE,
};

pub(crate) struct MaterialVolume<'a> {
    pub(crate) role: String,
    pub(crate) material: &'a SelectedMaterialBinding,
    pub(crate) volume_ul: u32,
    pub(crate) source_mix: Option<GoldenGateMix>,
    pub(crate) transfer_technique: TransferTechnique,
    pub(crate) reuse_tip_for_final_mix: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct GoldenGateMix {
    pub(crate) cycles: u32,
    pub(crate) volume_ul: u32,
    pub(crate) technique: MixTechnique,
}

pub(crate) struct SetupGoldenGate<'a> {
    pub(crate) artifact: String,
    pub(crate) replicates: usize,
    pub(crate) additions: Vec<MaterialVolume<'a>>,
    pub(crate) reaction_volume_ul: u32,
    pub(crate) final_mix: GoldenGateMix,
    pub(crate) source_temperature_c: Option<f64>,
}

pub(crate) fn require_basic_golden_gate_techniques(
    adapter: &str,
    task: &AllocatedProcedureTask,
    setup: &SetupGoldenGate<'_>,
) -> Result<(), String> {
    if setup.source_temperature_c.is_some()
        || setup.final_mix.technique != MixTechnique::default()
        || setup.additions.iter().any(|addition| {
            addition.source_mix.is_some()
                || addition.transfer_technique != TransferTechnique::default()
                || addition.reuse_tip_for_final_mix
        })
    {
        return Err(format!(
            "{adapter} Procedure task '{}' does not implement its normalized Golden Gate setup techniques",
            task.id
        ));
    }
    Ok(())
}

pub(crate) struct NormalizedThermalProgram {
    pub(crate) artifact: String,
    pub(crate) title: String,
    pub(crate) sample_count: usize,
    pub(crate) volume_each_ul: f64,
    pub(crate) lid_temperature_c: Option<f64>,
    pub(crate) profile: ThermalProfile,
    pub(crate) final_hold_celsius: Option<f64>,
}

pub(crate) struct SerialDilution<'a> {
    pub(crate) subject: String,
    pub(crate) culture_source: &'a PlanningValueSource,
    pub(crate) medium: &'a SelectedMaterialBinding,
    pub(crate) culture_replicates: usize,
    pub(crate) serial_dilutions: usize,
    pub(crate) initial_volume_ul: u32,
    pub(crate) medium_volume_ul: u32,
    pub(crate) culture_volume_ul: u32,
    pub(crate) mix_cycles: u32,
    pub(crate) mix_volume_ul: u32,
    pub(crate) medium_technique: TransferTechnique,
    pub(crate) transfer_technique: TransferTechnique,
    pub(crate) mix_technique: MixTechnique,
}

pub(crate) struct ChemicalTransformation<'a> {
    pub(crate) artifact: String,
    pub(crate) cell_source: &'a PlanningValueSource,
    pub(crate) dna: Vec<&'a SelectedMaterialBinding>,
    pub(crate) replicates: usize,
    pub(crate) cell_mix_cycles: u32,
    pub(crate) cell_mix_volume_ul: u32,
    pub(crate) cell_mix_technique: MixTechnique,
    pub(crate) cell_volume_ul: u32,
    pub(crate) cell_transfer_technique: TransferTechnique,
    pub(crate) dna_mix_cycles: u32,
    pub(crate) dna_mix_volume_ul: u32,
    pub(crate) dna_mix_technique: MixTechnique,
    pub(crate) dna_volume_ul: u32,
    pub(crate) dna_transfer_technique: TransferTechnique,
    pub(crate) bubble_clear_cycles: u32,
    pub(crate) bubble_clear_volume_ul: u32,
    pub(crate) bubble_clear_technique: MixTechnique,
    /// Exact temperature the competent-cell aliquot must be staged at.
    pub(crate) cell_staging_temperature_c: Option<f64>,
    /// Total volume this task draws from its competent-cell aliquot, taken from the program's own
    /// liquid ledger rather than recomputed from parameters that could drift from the steps.
    pub(crate) cell_withdrawal_ul: u32,
}

pub(crate) struct RecoveryMediumAddition<'a> {
    pub(crate) subject: String,
    pub(crate) culture_source: &'a PlanningValueSource,
    pub(crate) medium: &'a SelectedMaterialBinding,
    pub(crate) replicates: usize,
    pub(crate) initial_volume_ul: u32,
    pub(crate) recovery_volume_ul: u32,
    pub(crate) technique: TransferTechnique,
}

pub(crate) struct SelectivePlating<'a> {
    pub(crate) subject: String,
    pub(crate) culture_source: &'a PlanningValueSource,
    pub(crate) selection: &'a SelectedMaterialBinding,
    pub(crate) culture_replicates: usize,
    pub(crate) serial_dilutions: usize,
    pub(crate) plating_replicates: usize,
    pub(crate) initial_volume_by_dilution_ul: Vec<u32>,
    pub(crate) medium_volume_ul: u32,
    pub(crate) culture_volume_ul: u32,
    pub(crate) colony_volume_ul: u32,
    pub(crate) technique: TransferTechnique,
}

pub(crate) fn normalized_chemical_transformation<'a>(
    adapter: &str,
    task: &'a AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
) -> Result<ChemicalTransformation<'a>, String> {
    let program = normalized_pipetting_program(
        adapter,
        task,
        requirements,
        PREPARE_CHEMICAL_TRANSFORMATION,
        "chemical-transformation preparation",
    )?;
    if task.inputs.len() != 2 {
        return Err(format!(
            "{adapter} transformation task '{}' must have design and competent-cell inputs",
            task.id
        ));
    }
    let view = ProcedureTaskView::new(adapter, task);
    view.require_material_roles(&["dependencies"])?;
    let artifact = view.text_parameter("artifact")?;
    let ledger = program.liquid_ledger().clone();
    let program = program.as_program();
    let [output] = program.outputs.as_slice() else {
        return Err(format!(
            "{adapter} transformation task '{}' must declare exactly one mixture output",
            task.id
        ));
    };
    let cells = program
        .vessels
        .iter()
        .filter(|vessel| matches!(vessel.role, VesselRole::ProcedureInput { input: 1 }))
        .collect::<Vec<_>>();
    let reactions = program
        .vessels
        .iter()
        .filter(|vessel| {
            matches!(&vessel.role, VesselRole::Product { output: candidate } if candidate == &output.id)
        })
        .collect::<Vec<_>>();
    let ([cells], [reactions]) = (cells.as_slice(), reactions.as_slice()) else {
        return Err(format!(
            "{adapter} transformation task '{}' must map competent cells and its mixture to one vessel each",
            task.id
        ));
    };
    let destinations = (0..reactions.positions)
        .map(|position| Location {
            vessel: reactions.id.clone(),
            position,
        })
        .collect::<Vec<_>>();
    let replicates = destinations.len();
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
    let dna = program
        .materials
        .iter()
        .map(|material| {
            materials.get(material.id.as_str()).copied().ok_or_else(|| {
                format!(
                    "{adapter} transformation task '{}' normalized DNA '{}' has no exact material allocation",
                    task.id, material.id
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if dna.is_empty() || dna.len() != materials.len() {
        return Err(format!(
            "{adapter} transformation task '{}' must consume every allocated DNA exactly once",
            task.id
        ));
    }
    let mut steps = program.steps.iter();
    let PipettingStep::Mix {
        targets,
        cycles: cell_mix_cycles,
        volume: cell_mix_volume,
        technique: cell_mix_technique,
        fluid_path_group: cell_group,
        ..
    } = steps.next().ok_or_else(|| {
        format!(
            "{adapter} transformation task '{}' has no competent-cell mix",
            task.id
        )
    })?
    else {
        return Err(format!(
            "{adapter} transformation task '{}' must begin by mixing competent cells",
            task.id
        ));
    };
    let cell_location = Location {
        vessel: cells.id.clone(),
        position: 0,
    };
    if targets.as_slice() != std::slice::from_ref(&cell_location) {
        return Err(format!(
            "{adapter} transformation task '{}' mixes the wrong competent-cell source",
            task.id
        ));
    }
    let PipettingStep::Distribute {
        source,
        destinations: cell_destinations,
        volume_each: cell_volume,
        technique: cell_transfer_technique,
        fluid_path: FluidPathPolicy::SharedSourceNoReentry,
        fluid_path_group,
        ..
    } = steps.next().ok_or_else(|| {
        format!(
            "{adapter} transformation task '{}' has no competent-cell distribution",
            task.id
        )
    })?
    else {
        return Err(format!(
            "{adapter} transformation task '{}' must distribute competent cells after mixing",
            task.id
        ));
    };
    if source != &cell_location
        || cell_destinations != &destinations
        || cell_group.is_none()
        || cell_group != fluid_path_group
    {
        return Err(format!(
            "{adapter} transformation task '{}' does not preserve one contamination-safe competent-cell path",
            task.id
        ));
    }

    let mut dna_mix = None;
    let mut dna_transfer = None;
    let mut bubble_clear = None;
    for (material_index, material) in program.materials.iter().enumerate() {
        let source_vessel = vessels
            .values()
            .filter(|vessel| {
                matches!(&vessel.role, VesselRole::MaterialSource { material: candidate } if candidate == &material.id)
            })
            .copied()
            .collect::<Vec<_>>();
        let [source_vessel] = source_vessel.as_slice() else {
            return Err(format!(
                "{adapter} transformation task '{}' DNA material '{}' must map to one source vessel",
                task.id, material.id
            ));
        };
        let dna_location = Location {
            vessel: source_vessel.id.clone(),
            position: 0,
        };
        for (replicate, destination) in destinations.iter().enumerate() {
            let trio = [
                steps.next().ok_or_else(|| {
                    format!(
                        "{adapter} transformation task '{}' is missing DNA mix {material_index}/{replicate}",
                        task.id
                    )
                })?,
                steps.next().ok_or_else(|| {
                    format!(
                        "{adapter} transformation task '{}' is missing DNA transfer {material_index}/{replicate}",
                        task.id
                    )
                })?,
                steps.next().ok_or_else(|| {
                    format!(
                        "{adapter} transformation task '{}' is missing bubble clearing {material_index}/{replicate}",
                        task.id
                    )
                })?,
            ];
            let PipettingStep::Mix {
                targets,
                cycles,
                volume,
                technique,
                fluid_path_group: mix_group,
                ..
            } = trio[0]
            else {
                return Err(format!(
                    "{adapter} transformation task '{}' must mix DNA before transfer",
                    task.id
                ));
            };
            if targets.as_slice() != std::slice::from_ref(&dna_location) {
                return Err(format!(
                    "{adapter} transformation task '{}' mixes the wrong DNA source",
                    task.id
                ));
            }
            let mix = (
                *cycles,
                whole_microlitres(adapter, task, "DNA mix", volume)?,
                technique.clone(),
            );
            if dna_mix
                .replace(mix.clone())
                .is_some_and(|first| first != mix)
            {
                return Err(format!(
                    "{adapter} transformation task '{}' uses inconsistent DNA mixing",
                    task.id
                ));
            }
            let PipettingStep::Transfer {
                source,
                destination: transfer_destination,
                volume,
                technique,
                fluid_path_group: transfer_group,
                ..
            } = trio[1]
            else {
                return Err(format!(
                    "{adapter} transformation task '{}' must transfer DNA after mixing",
                    task.id
                ));
            };
            if source != &dna_location
                || transfer_destination != destination
                || mix_group.is_none()
                || mix_group != transfer_group
            {
                return Err(format!(
                    "{adapter} transformation task '{}' DNA transfer does not preserve its isolated path",
                    task.id
                ));
            }
            let transfer = (
                whole_microlitres(adapter, task, "DNA transfer", volume)?,
                technique.clone(),
            );
            if dna_transfer
                .replace(transfer.clone())
                .is_some_and(|first| first != transfer)
            {
                return Err(format!(
                    "{adapter} transformation task '{}' uses inconsistent DNA transfer technique",
                    task.id
                ));
            }
            let PipettingStep::Mix {
                targets,
                cycles,
                volume,
                technique,
                fluid_path_group: bubble_group,
                ..
            } = trio[2]
            else {
                return Err(format!(
                    "{adapter} transformation task '{}' must clear bubbles after DNA transfer",
                    task.id
                ));
            };
            if targets.as_slice() != std::slice::from_ref(destination)
                || bubble_group != transfer_group
            {
                return Err(format!(
                    "{adapter} transformation task '{}' bubble clearing does not preserve the DNA path",
                    task.id
                ));
            }
            let bubble = (
                *cycles,
                whole_microlitres(adapter, task, "bubble-clear mix", volume)?,
                technique.clone(),
            );
            if bubble_clear
                .replace(bubble.clone())
                .is_some_and(|first| first != bubble)
            {
                return Err(format!(
                    "{adapter} transformation task '{}' uses inconsistent bubble clearing",
                    task.id
                ));
            }
        }
    }
    if steps.next().is_some() {
        return Err(format!(
            "{adapter} transformation task '{}' contains operations outside its canonical shape",
            task.id
        ));
    }
    let (dna_mix_cycles, dna_mix_volume_ul, dna_mix_technique) =
        dna_mix.expect("validated transformation has DNA mixing");
    let (dna_volume_ul, dna_transfer_technique) =
        dna_transfer.expect("validated transformation has DNA transfer");
    let (bubble_clear_cycles, bubble_clear_volume_ul, bubble_clear_technique) =
        bubble_clear.expect("validated transformation has bubble clearing");
    Ok(ChemicalTransformation {
        artifact,
        cell_source: &task.inputs[1].source,
        dna,
        replicates,
        cell_mix_cycles: *cell_mix_cycles,
        cell_mix_volume_ul: whole_microlitres(
            adapter,
            task,
            "competent-cell mix",
            cell_mix_volume,
        )?,
        cell_mix_technique: cell_mix_technique.clone(),
        cell_volume_ul: whole_microlitres(adapter, task, "competent-cell transfer", cell_volume)?,
        cell_transfer_technique: cell_transfer_technique.clone(),
        dna_mix_cycles,
        dna_mix_volume_ul,
        dna_mix_technique,
        dna_volume_ul,
        dna_transfer_technique,
        bubble_clear_cycles,
        bubble_clear_volume_ul,
        bubble_clear_technique,
        cell_staging_temperature_c: exact_source_temperature(adapter, task, program)?,
        cell_withdrawal_ul: whole_microlitres(
            adapter,
            task,
            "competent-cell withdrawal",
            &Volume::microlitres(
                ledger
                    .withdrawn(&Location {
                        vessel: cells.id.clone(),
                        position: 0,
                    })
                    .cloned()
                    .ok_or_else(|| {
                        format!(
                            "{adapter} transformation task '{}' draws nothing from its competent-cell aliquot",
                            task.id
                        )
                    })?,
            )
            .map_err(|error| error.to_string())?,
        )?,
    })
}

pub(crate) fn normalized_recovery_medium<'a>(
    adapter: &str,
    task: &'a AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
) -> Result<RecoveryMediumAddition<'a>, String> {
    let program = normalized_pipetting_program(
        adapter,
        task,
        requirements,
        ADD_RECOVERY_MEDIUM,
        "recovery-medium addition",
    )?;
    if task.inputs.len() != 1 {
        return Err(format!(
            "{adapter} recovery task '{}' must have one transformed-culture input",
            task.id
        ));
    }
    let view = ProcedureTaskView::new(adapter, task);
    let subject = view.text_parameter("subject")?;
    view.require_material_roles(&["medium"])?;
    let medium = view.one_material("medium")?;
    let program = program.as_program();
    let [material] = program.materials.as_slice() else {
        return Err(format!(
            "{adapter} recovery task '{}' must declare one medium",
            task.id
        ));
    };
    if medium.input.as_str() != material.id.as_str() {
        return Err(format!(
            "{adapter} recovery task '{}' normalized medium does not match its allocation",
            task.id
        ));
    }
    let [source_vessel, culture_vessel] = program.vessels.as_slice() else {
        return Err(format!(
            "{adapter} recovery task '{}' must map medium and culture to two vessels",
            task.id
        ));
    };
    if !matches!(&source_vessel.role, VesselRole::MaterialSource { material: candidate } if candidate == &material.id)
    {
        return Err(format!(
            "{adapter} recovery task '{}' has a non-canonical medium vessel",
            task.id
        ));
    }
    let VesselRole::InputOutput {
        input: 0,
        output: culture_output,
    } = &culture_vessel.role
    else {
        return Err(format!(
            "{adapter} recovery task '{}' must update its input culture in place",
            task.id
        ));
    };
    if !program
        .outputs
        .iter()
        .any(|output| &output.id == culture_output)
    {
        return Err(format!(
            "{adapter} recovery task '{}' culture vessel names an unknown output",
            task.id
        ));
    }
    let initial_volume_ul = culture_vessel
        .initial_volume_each
        .as_ref()
        .ok_or_else(|| {
            format!(
                "{adapter} recovery task '{}' has no exact initial culture volume",
                task.id
            )
        })
        .and_then(|volume| whole_microlitres(adapter, task, "initial culture", volume))?;
    let [
        PipettingStep::Distribute {
            source,
            destinations,
            volume_each,
            fluid_path: FluidPathPolicy::SharedSourceNoReentry,
            technique,
            ..
        },
    ] = program.steps.as_slice()
    else {
        return Err(format!(
            "{adapter} recovery task '{}' must contain one contamination-safe medium distribution",
            task.id
        ));
    };
    let expected_destinations = (0..culture_vessel.positions)
        .map(|position| Location {
            vessel: culture_vessel.id.clone(),
            position,
        })
        .collect::<Vec<_>>();
    if source
        != &(Location {
            vessel: source_vessel.id.clone(),
            position: 0,
        })
        || destinations != &expected_destinations
    {
        return Err(format!(
            "{adapter} recovery task '{}' medium distribution addresses the wrong cultures",
            task.id
        ));
    }
    Ok(RecoveryMediumAddition {
        subject,
        culture_source: &task.inputs[0].source,
        medium,
        replicates: usize::try_from(culture_vessel.positions).map_err(|_| {
            format!(
                "{adapter} recovery task '{}' replicate count does not fit this platform",
                task.id
            )
        })?,
        initial_volume_ul,
        recovery_volume_ul: whole_microlitres(adapter, task, "recovery medium", volume_each)?,
        technique: technique.clone(),
    })
}

pub(crate) fn normalized_selective_plating<'a>(
    adapter: &str,
    task: &'a AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
) -> Result<SelectivePlating<'a>, String> {
    let program = normalized_pipetting_program(
        adapter,
        task,
        requirements,
        PLATE_DILUTED_CULTURE,
        "selective plating",
    )?;
    if task.inputs.len() != 1 {
        return Err(format!(
            "{adapter} plating task '{}' must have one diluted-culture input",
            task.id
        ));
    }
    let view = ProcedureTaskView::new(adapter, task);
    let subject = view.text_parameter("subject")?;
    view.require_material_roles(&["selection"])?;
    let selection = view.one_material("selection")?;
    let program = program.as_program();
    let [material] = program.materials.as_slice() else {
        return Err(format!(
            "{adapter} plating task '{}' must declare one selective medium",
            task.id
        ));
    };
    if selection.input.as_str() != material.id.as_str() {
        return Err(format!(
            "{adapter} plating task '{}' normalized selection does not match its allocation",
            task.id
        ));
    }
    let [output] = program.outputs.as_slice() else {
        return Err(format!(
            "{adapter} plating task '{}' must declare one plate output",
            task.id
        ));
    };
    let inputs = program
        .vessels
        .iter()
        .filter(|vessel| matches!(vessel.role, VesselRole::ProcedureInput { input: 0 }))
        .collect::<Vec<_>>();
    let products = program
        .vessels
        .iter()
        .filter(|vessel| {
            matches!(&vessel.role, VesselRole::MaterialProduct { material: candidate, output: candidate_output }
                if candidate == &material.id && candidate_output == &output.id)
        })
        .collect::<Vec<_>>();
    let [product] = products.as_slice() else {
        return Err(format!(
            "{adapter} plating task '{}' must map selective medium and output to one plate vessel",
            task.id
        ));
    };
    if inputs.is_empty() {
        return Err(format!(
            "{adapter} plating task '{}' has no dilution vessels",
            task.id
        ));
    }
    let culture_replicates = usize::try_from(inputs[0].positions).map_err(|_| {
        format!(
            "{adapter} plating task '{}' culture replicate count does not fit this platform",
            task.id
        )
    })?;
    if inputs.iter().any(|vessel| {
        usize::try_from(vessel.positions).ok() != Some(culture_replicates)
            || vessel.initial_volume_each.is_none()
    }) {
        return Err(format!(
            "{adapter} plating task '{}' dilution vessels do not preserve one uniform replicate layout",
            task.id
        ));
    }
    let serial_dilutions = inputs.len();
    let source_count = culture_replicates
        .checked_mul(serial_dilutions)
        .ok_or_else(|| {
            format!(
                "{adapter} plating task '{}' source count overflows",
                task.id
            )
        })?;
    if program.steps.len() != source_count {
        return Err(format!(
            "{adapter} plating task '{}' must contain one distribution per dilution and culture replicate",
            task.id
        ));
    }
    let spot_count = usize::try_from(product.positions).map_err(|_| {
        format!(
            "{adapter} plating task '{}' spot count does not fit this platform",
            task.id
        )
    })?;
    if spot_count % source_count != 0 {
        return Err(format!(
            "{adapter} plating task '{}' spot count is not divisible by its culture sources",
            task.id
        ));
    }
    let plating_replicates = spot_count / source_count;
    let product_locations = (0..product.positions)
        .map(|position| Location {
            vessel: product.id.clone(),
            position,
        })
        .collect::<Vec<_>>();
    let mut colony_volume_ul = None;
    let mut technique = None;
    let mut spot_cursor = 0;
    for (source_index, step) in program.steps.iter().enumerate() {
        let dilution = source_index / culture_replicates;
        let replicate = source_index % culture_replicates;
        let PipettingStep::Distribute {
            source,
            destinations,
            volume_each,
            fluid_path: FluidPathPolicy::SharedSourceNoReentry,
            technique: step_technique,
            ..
        } = step
        else {
            return Err(format!(
                "{adapter} plating task '{}' contains a non-distribution operation",
                task.id
            ));
        };
        let expected_source = Location {
            vessel: inputs[dilution].id.clone(),
            position: u32::try_from(replicate).map_err(|_| {
                format!(
                    "{adapter} plating task '{}' replicate does not fit this platform",
                    task.id
                )
            })?,
        };
        let next_cursor = spot_cursor + plating_replicates;
        if source != &expected_source
            || destinations != &product_locations[spot_cursor..next_cursor]
        {
            return Err(format!(
                "{adapter} plating task '{}' does not preserve dilution-major spot ordering",
                task.id
            ));
        }
        spot_cursor = next_cursor;
        let volume = whole_microlitres(adapter, task, "colony spot", volume_each)?;
        if colony_volume_ul
            .replace(volume)
            .is_some_and(|first| first != volume)
        {
            return Err(format!(
                "{adapter} plating task '{}' uses inconsistent colony volumes",
                task.id
            ));
        }
        if technique
            .replace(step_technique.clone())
            .is_some_and(|first| first != *step_technique)
        {
            return Err(format!(
                "{adapter} plating task '{}' uses inconsistent plating techniques",
                task.id
            ));
        }
    }
    let initial_volume_by_dilution_ul = inputs
        .iter()
        .map(|vessel| {
            whole_microlitres(
                adapter,
                task,
                "dilution input",
                vessel
                    .initial_volume_each
                    .as_ref()
                    .expect("dilution input volume checked above"),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let medium_volume_ul = view.integer_parameter("medium_volume_ul", Some(MICROLITRE))?;
    let culture_volume_ul = view.integer_parameter("culture_volume_ul", Some(MICROLITRE))?;
    let full_dilution_volume =
        medium_volume_ul
            .checked_add(culture_volume_ul)
            .ok_or_else(|| {
                format!(
                    "{adapter} plating task '{}' dilution volume overflows",
                    task.id
                )
            })?;
    for (dilution, observed) in initial_volume_by_dilution_ul.iter().enumerate() {
        let expected = if dilution + 1 == serial_dilutions {
            full_dilution_volume
        } else {
            medium_volume_ul
        };
        if *observed != expected {
            return Err(format!(
                "{adapter} plating task '{}' dilution {} has {observed} uL remaining, expected {expected} uL from its canonical serial-dilution handoff",
                task.id,
                dilution + 1
            ));
        }
    }
    Ok(SelectivePlating {
        subject,
        culture_source: &task.inputs[0].source,
        selection,
        culture_replicates,
        serial_dilutions,
        plating_replicates,
        initial_volume_by_dilution_ul,
        medium_volume_ul,
        culture_volume_ul,
        colony_volume_ul: colony_volume_ul.expect("validated plating has colony transfers"),
        technique: technique.expect("validated plating has a technique"),
    })
}

fn normalized_pipetting_program(
    adapter: &str,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
    expected_operation: &str,
    label: &str,
) -> Result<ValidatedPipettingProgramV1, String> {
    if task.operation.as_str() != expected_operation {
        return Err(format!(
            "{adapter} expected normalized {label}, found operation '{}' in task '{}'",
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
            "{adapter} {label} task '{}' normalized to a non-pipetting contract",
            task.id
        ));
    };
    Ok(program)
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
    let (additions, reaction_volume_ul, final_mix) =
        match view.text_parameter("setup_strategy")?.as_str() {
            "basic_v1" => project_basic_golden_gate(adapter, task, program, &destinations)?,
            "temperature_staged_v1" => {
                project_temperature_staged_golden_gate(adapter, task, program, &destinations)?
            }
            value => {
                return Err(format!(
                    "{adapter} Golden Gate task '{}' uses unsupported setup strategy '{value}'",
                    task.id
                ));
            }
        };
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
        final_mix,
        source_temperature_c: exact_source_temperature(adapter, task, program)?,
    })
}

fn project_basic_golden_gate<'a>(
    adapter: &str,
    task: &'a AllocatedProcedureTask,
    program: &PipettingProgramV1,
    destinations: &[Location],
) -> Result<(Vec<MaterialVolume<'a>>, u32, GoldenGateMix), String> {
    let mut additions = Vec::new();
    let mut reaction_volume_ul = 0_u32;
    let mut final_mix = None;
    for step in &program.steps {
        match step {
            PipettingStep::Distribute {
                source,
                destinations: step_destinations,
                volume_each,
                fluid_path: FluidPathPolicy::SharedSourceNoReentry,
                fluid_path_group: None,
                technique,
                ..
            } => {
                if step_destinations != destinations {
                    return Err(format!(
                        "{adapter} Golden Gate task '{}' basic additions must address every product position in order",
                        task.id
                    ));
                }
                let (role, material) = golden_gate_source(adapter, task, program, source)?;
                let volume_ul = whole_microlitres(adapter, task, "transfer", volume_each)?;
                reaction_volume_ul =
                    reaction_volume_ul.checked_add(volume_ul).ok_or_else(|| {
                        format!(
                            "{adapter} Golden Gate task '{}' reaction volume overflows",
                            task.id
                        )
                    })?;
                additions.push(MaterialVolume {
                    role,
                    material,
                    volume_ul,
                    source_mix: None,
                    transfer_technique: technique.clone(),
                    reuse_tip_for_final_mix: false,
                });
            }
            PipettingStep::Mix {
                targets,
                cycles,
                volume,
                fluid_path: FluidPathPolicy::IsolatedDestinations,
                fluid_path_group: None,
                technique,
                ..
            } if targets == destinations && final_mix.is_none() => {
                final_mix = Some(GoldenGateMix {
                    cycles: *cycles,
                    volume_ul: whole_microlitres(adapter, task, "mix", volume)?,
                    technique: technique.clone(),
                });
            }
            _ => {
                return Err(format!(
                    "{adapter} Golden Gate task '{}' is not a canonical basic setup program",
                    task.id
                ));
            }
        }
    }
    let final_mix = final_mix.ok_or_else(|| {
        format!(
            "{adapter} Golden Gate task '{}' has no normalized final mix",
            task.id
        )
    })?;
    Ok((additions, reaction_volume_ul, final_mix))
}

fn project_temperature_staged_golden_gate<'a>(
    adapter: &str,
    task: &'a AllocatedProcedureTask,
    program: &PipettingProgramV1,
    destinations: &[Location],
) -> Result<(Vec<MaterialVolume<'a>>, u32, GoldenGateMix), String> {
    let sources = program
        .materials
        .iter()
        .map(|material| {
            let vessels = program
                .vessels
                .iter()
                .filter(|vessel| {
                    matches!(&vessel.role, VesselRole::MaterialSource { material: candidate } if candidate == &material.id)
                })
                .collect::<Vec<_>>();
            let [vessel] = vessels.as_slice() else {
                return Err(format!(
                    "{adapter} Golden Gate task '{}' material '{}' must map to one source vessel",
                    task.id, material.id
                ));
            };
            let location = Location {
                vessel: vessel.id.clone(),
                position: 0,
            };
            let (role, binding) = golden_gate_source(adapter, task, program, &location)?;
            Ok((location, role, binding))
        })
        .collect::<Result<Vec<_>, String>>()?;
    if sources.is_empty() {
        return Err(format!(
            "{adapter} Golden Gate task '{}' has no normalized reagent additions",
            task.id
        ));
    }

    let mut steps = program.steps.iter();
    let mut additions = Vec::<MaterialVolume<'a>>::new();
    let mut reaction_volume_ul = 0_u32;
    let mut final_mix = None;
    for (replicate, destination) in destinations.iter().enumerate() {
        for (addition_index, (source, role, material)) in sources.iter().enumerate() {
            let source_mix = if role == "water" {
                None
            } else {
                let Some(PipettingStep::Mix {
                    targets,
                    cycles,
                    volume,
                    fluid_path: FluidPathPolicy::IsolatedDestinations,
                    fluid_path_group,
                    technique,
                    ..
                }) = steps.next()
                else {
                    return Err(format!(
                        "{adapter} Golden Gate task '{}' must mix reagent {} before replicate {} transfer",
                        task.id,
                        addition_index + 1,
                        replicate + 1
                    ));
                };
                if targets.as_slice() != std::slice::from_ref(source) || fluid_path_group.is_none()
                {
                    return Err(format!(
                        "{adapter} Golden Gate task '{}' does not preserve the isolated source-mix path for reagent {} replicate {}",
                        task.id,
                        addition_index + 1,
                        replicate + 1
                    ));
                }
                Some((
                    GoldenGateMix {
                        cycles: *cycles,
                        volume_ul: whole_microlitres(adapter, task, "source mix", volume)?,
                        technique: technique.clone(),
                    },
                    fluid_path_group.clone(),
                ))
            };

            let Some(PipettingStep::Transfer {
                source: transfer_source,
                destination: transfer_destination,
                volume,
                fluid_path: FluidPathPolicy::IsolatedDestinations,
                fluid_path_group,
                technique,
                ..
            }) = steps.next()
            else {
                return Err(format!(
                    "{adapter} Golden Gate task '{}' is missing reagent {} transfer for replicate {}",
                    task.id,
                    addition_index + 1,
                    replicate + 1
                ));
            };
            if transfer_source != source
                || transfer_destination != destination
                || source_mix
                    .as_ref()
                    .map(|(_, group)| group)
                    .is_some_and(|group| group != fluid_path_group)
                || (source_mix.is_none() && fluid_path_group.is_some())
            {
                return Err(format!(
                    "{adapter} Golden Gate task '{}' does not preserve reagent {} fluid-path continuity for replicate {}",
                    task.id,
                    addition_index + 1,
                    replicate + 1
                ));
            }
            let volume_ul = whole_microlitres(adapter, task, "transfer", volume)?;
            let reuse_tip_for_final_mix = addition_index + 1 == sources.len();
            let candidate = MaterialVolume {
                role: role.clone(),
                material,
                volume_ul,
                source_mix: source_mix.map(|(mix, _)| mix),
                transfer_technique: technique.clone(),
                reuse_tip_for_final_mix,
            };
            if replicate == 0 {
                reaction_volume_ul =
                    reaction_volume_ul.checked_add(volume_ul).ok_or_else(|| {
                        format!(
                            "{adapter} Golden Gate task '{}' reaction volume overflows",
                            task.id
                        )
                    })?;
                additions.push(candidate);
            } else {
                let expected = &additions[addition_index];
                if expected.role != candidate.role
                    || expected.material.input != candidate.material.input
                    || expected.volume_ul != candidate.volume_ul
                    || expected.source_mix != candidate.source_mix
                    || expected.transfer_technique != candidate.transfer_technique
                    || expected.reuse_tip_for_final_mix != candidate.reuse_tip_for_final_mix
                {
                    return Err(format!(
                        "{adapter} Golden Gate task '{}' uses inconsistent reagent {} technique across replicates",
                        task.id,
                        addition_index + 1
                    ));
                }
            }

            if reuse_tip_for_final_mix {
                let Some(PipettingStep::Mix {
                    targets,
                    cycles,
                    volume,
                    fluid_path: FluidPathPolicy::IsolatedDestinations,
                    fluid_path_group: mix_group,
                    technique,
                    ..
                }) = steps.next()
                else {
                    return Err(format!(
                        "{adapter} Golden Gate task '{}' is missing final bubble clearing for replicate {}",
                        task.id,
                        replicate + 1
                    ));
                };
                if targets.as_slice() != std::slice::from_ref(destination)
                    || mix_group.is_none()
                    || mix_group != fluid_path_group
                {
                    return Err(format!(
                        "{adapter} Golden Gate task '{}' final bubble clearing does not reuse the final reagent path for replicate {}",
                        task.id,
                        replicate + 1
                    ));
                }
                let candidate = GoldenGateMix {
                    cycles: *cycles,
                    volume_ul: whole_microlitres(adapter, task, "bubble clearing", volume)?,
                    technique: technique.clone(),
                };
                if final_mix
                    .replace(candidate.clone())
                    .is_some_and(|expected| expected != candidate)
                {
                    return Err(format!(
                        "{adapter} Golden Gate task '{}' uses inconsistent bubble clearing across replicates",
                        task.id
                    ));
                }
            }
        }
    }
    if steps.next().is_some() {
        return Err(format!(
            "{adapter} Golden Gate task '{}' contains operations outside its canonical temperature-staged setup shape",
            task.id
        ));
    }
    let final_mix = final_mix.ok_or_else(|| {
        format!(
            "{adapter} Golden Gate task '{}' has no normalized final bubble clearing",
            task.id
        )
    })?;
    Ok((additions, reaction_volume_ul, final_mix))
}

fn golden_gate_source<'a>(
    adapter: &str,
    task: &'a AllocatedProcedureTask,
    program: &PipettingProgramV1,
    source: &Location,
) -> Result<(String, &'a SelectedMaterialBinding), String> {
    let source_vessel = program
        .vessels
        .iter()
        .find(|vessel| vessel.id == source.vessel)
        .expect("validated pipetting source vessel exists");
    let VesselRole::MaterialSource { material } = &source_vessel.role else {
        return Err(format!(
            "{adapter} Golden Gate task '{}' source '{}' is not a material source",
            task.id, source.vessel
        ));
    };
    let material_binding = task
        .materials
        .iter()
        .find(|binding| binding.input.as_str() == material.as_str())
        .ok_or_else(|| {
            format!(
                "{adapter} Golden Gate task '{}' normalized material '{}' has no exact allocation",
                task.id, material
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
    Ok((role, material_binding))
}

fn exact_source_temperature(
    adapter: &str,
    task: &AllocatedProcedureTask,
    program: &PipettingProgramV1,
) -> Result<Option<f64>, String> {
    let Some(temperature) = lab_procedure::staged_temperature_envelope(&program.vessels) else {
        return Ok(None);
    };
    if temperature.minimum != temperature.maximum {
        return Err(format!(
            "{adapter} Golden Gate task '{}' requires a source-temperature range, but this operation needs one exact staging setpoint",
            task.id
        ));
    }
    finite_f64(
        adapter,
        task,
        "source temperature",
        temperature.minimum.value(),
    )
    .map(Some)
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
    let operation_title = match task.operation.as_str() {
        CYCLE_GOLDEN_GATE => "Thermal cycle Golden Gate reaction",
        HEAT_SHOCK_TRANSFORMATION => "Heat-shock transformation",
        INCUBATE_RECOVERY_CULTURE => "Incubate recovery culture",
        operation => {
            return Err(format!(
                "{adapter} expected a normalized thermal operation, found operation '{operation}' in task '{}'",
                task.id
            ));
        }
    };
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
    let subject = view
        .optional_text_parameter("artifact")?
        .or(view.optional_text_parameter("subject")?);
    let title = match &subject {
        Some(subject) => format!("{operation_title} for {subject}"),
        None => operation_title.to_owned(),
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
        artifact: subject.unwrap_or_else(|| operation_title.to_owned()),
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
    let subject = view.text_parameter("subject")?;
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
    if program.vessels.len() != 3 || medium_vessel.positions != 1 {
        return Err(format!(
            "{adapter} serial-dilution task '{}' has a non-canonical logical vessel layout",
            task.id
        ));
    }
    let culture_replicates = usize::try_from(culture_vessel.positions).map_err(|_| {
        format!(
            "{adapter} serial-dilution task '{}' culture replicate count does not fit this platform",
            task.id
        )
    })?;
    let position_count = usize::try_from(product_vessel.positions).map_err(|_| {
        format!(
            "{adapter} serial-dilution task '{}' destination count does not fit this platform",
            task.id
        )
    })?;
    if position_count % culture_replicates != 0 {
        return Err(format!(
            "{adapter} serial-dilution task '{}' destination count is not divisible by its culture replicates",
            task.id
        ));
    }
    let serial_dilutions = position_count / culture_replicates;
    let initial_volume_ul = culture_vessel
        .initial_volume_each
        .as_ref()
        .ok_or_else(|| {
            format!(
                "{adapter} serial-dilution task '{}' has no exact initial culture volume",
                task.id
            )
        })
        .and_then(|volume| whole_microlitres(adapter, task, "initial culture", volume))?;
    let destinations = (0..product_vessel.positions)
        .map(|position| Location {
            vessel: product_vessel.id.clone(),
            position,
        })
        .collect::<Vec<_>>();
    let expected_steps = serial_dilutions
        .checked_mul(culture_replicates)
        .and_then(|steps| steps.checked_mul(2))
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
        technique: medium_technique,
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
    let mut transfer_technique = None;
    let mut mix_technique = None;
    for (series_position, pair) in program.steps[1..].chunks_exact(2).enumerate() {
        let replicate = series_position / serial_dilutions;
        let dilution = series_position % serial_dilutions;
        let position = dilution
            .checked_mul(culture_replicates)
            .and_then(|base| base.checked_add(replicate))
            .ok_or_else(|| {
                format!(
                    "{adapter} serial-dilution task '{}' position arithmetic overflows",
                    task.id
                )
            })?;
        let destination = &destinations[position];
        let expected_source = if dilution == 0 {
            Location {
                vessel: culture_vessel.id.clone(),
                position: u32::try_from(replicate).map_err(|_| {
                    format!(
                        "{adapter} serial-dilution task '{}' replicate does not fit this platform",
                        task.id
                    )
                })?,
            }
        } else {
            destinations[position - culture_replicates].clone()
        };
        let PipettingStep::Transfer {
            source,
            destination: step_destination,
            volume,
            fluid_path: FluidPathPolicy::IsolatedDestinations,
            technique,
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
        if transfer_technique
            .replace(technique.clone())
            .is_some_and(|first| first != *technique)
        {
            return Err(format!(
                "{adapter} serial-dilution task '{}' uses inconsistent transfer techniques",
                task.id
            ));
        }
        let PipettingStep::Mix {
            targets,
            cycles,
            volume,
            fluid_path: FluidPathPolicy::IsolatedDestinations,
            technique,
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
        if mix_technique
            .replace(technique.clone())
            .is_some_and(|first| first != *technique)
        {
            return Err(format!(
                "{adapter} serial-dilution task '{}' uses inconsistent mix techniques",
                task.id
            ));
        }
    }
    let culture_volume_ul = culture_volume_ul.expect("a validated product vessel is non-empty");
    let (mix_cycles, mix_volume_ul) = mixing.expect("a validated product vessel is non-empty");

    Ok(SerialDilution {
        subject,
        culture_source: &task.inputs[0].source,
        medium,
        culture_replicates,
        serial_dilutions,
        initial_volume_ul,
        medium_volume_ul,
        culture_volume_ul,
        mix_cycles,
        mix_volume_ul,
        medium_technique: medium_technique.clone(),
        transfer_technique: transfer_technique
            .expect("a validated serial dilution has a transfer technique"),
        mix_technique: mix_technique.expect("a validated serial dilution has a mix technique"),
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
