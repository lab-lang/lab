use lab_procedure::{
    AspirationStrategy, FluidPathPolicy, Location, MaterialInput, MaterialOutput,
    PipettingConstraints, PipettingProgramV1, PipettingStep, ProcedureProgram, TransferTechnique,
    Vessel, VesselRole, Volume,
};

use super::ProcedureTaskInstance;
use super::view::{TaskView, procedure_id};

const MICROLITRE: &str = "http://qudt.org/vocab/unit/MicroL";

pub(super) fn normalize(task: &ProcedureTaskInstance<'_>) -> Result<ProcedureProgram, String> {
    if task.input_count != 1 {
        return Err(format!(
            "the serial-dilution contract requires exactly one culture input, found {}",
            task.input_count
        ));
    }
    if task.outputs.len() != 1 {
        return Err(format!(
            "the serial-dilution contract requires exactly one product output, found {}",
            task.outputs.len()
        ));
    }

    let view = TaskView::new(task);
    view.require_material_roles(&["medium"])?;
    let medium = view.one_material("medium")?;
    let replicates = view.integer_parameter("replicates", None)?;
    let initial_volume = view.integer_parameter("initial_volume_ul", Some(MICROLITRE))?;
    let serial_dilutions = view.integer_parameter("serial_dilutions", None)?;
    let medium_volume = view.integer_parameter("medium_volume_ul", Some(MICROLITRE))?;
    let culture_volume = view.integer_parameter("culture_volume_ul", Some(MICROLITRE))?;
    let mix_cycles = view.integer_parameter("mix_cycles", None)?;
    let mix_volume = view.integer_parameter("mix_volume_ul", Some(MICROLITRE))?;
    // The medium the operator loads before the run. Aspiration follows this source's falling
    // surface, so the compiler must be able to follow it too.
    let medium_source_volume =
        view.integer_parameter("medium_source_volume_ul", Some(MICROLITRE))?;
    for (name, value) in [
        ("serial_dilutions", serial_dilutions),
        ("replicates", replicates),
        ("initial_volume_ul", initial_volume),
        ("medium_volume_ul", medium_volume),
        ("culture_volume_ul", culture_volume),
        ("mix_cycles", mix_cycles),
        ("mix_volume_ul", mix_volume),
        ("medium_source_volume_ul", medium_source_volume),
    ] {
        if value == 0 {
            return Err(format!("parameter `{name}` must be greater than zero"));
        }
    }
    let dilution_volume = medium_volume
        .checked_add(culture_volume)
        .ok_or_else(|| "dilution volume arithmetic overflows".to_owned())?;
    if mix_volume > dilution_volume {
        return Err(format!(
            "mix volume {mix_volume} uL exceeds the {dilution_volume} uL dilution volume"
        ));
    }

    let culture_vessel = procedure_id("culture-input")?;
    let medium_id = procedure_id(medium.id.as_str())?;
    let medium_vessel = procedure_id("medium-source")?;
    let output = procedure_id(task.outputs[0].as_str())?;
    let dilution_vessel = procedure_id("dilution-plate")?;
    let position_count = replicates
        .checked_mul(serial_dilutions)
        .ok_or_else(|| "serial-dilution position count overflows".to_owned())?;
    let destinations = (0..position_count)
        .map(|position| Location {
            vessel: dilution_vessel.clone(),
            position,
        })
        .collect::<Vec<_>>();
    let mut steps = Vec::with_capacity(
        usize::try_from(position_count)
            .unwrap_or(usize::MAX)
            .saturating_mul(2)
            .saturating_add(1),
    );
    steps.push(PipettingStep::Distribute {
        id: procedure_id("add-medium")?,
        source: Location {
            vessel: medium_vessel.clone(),
            position: 0,
        },
        destinations: destinations.clone(),
        volume_each: volume(medium_volume)?,
        fluid_path: FluidPathPolicy::SharedSourceNoReentry,
        fluid_path_group: None,
        technique: TransferTechnique {
            aspiration: AspirationStrategy::TrackedLiquidSurface,
            ..TransferTechnique::default()
        },
    });
    for replicate in 0..replicates {
        let group = procedure_id(&format!("series-{replicate:04}"))?;
        for dilution in 0..serial_dilutions {
            let position = dilution
                .checked_mul(replicates)
                .and_then(|base| base.checked_add(replicate))
                .ok_or_else(|| "serial-dilution position arithmetic overflows".to_owned())?;
            let source = if dilution == 0 {
                Location {
                    vessel: culture_vessel.clone(),
                    position: replicate,
                }
            } else {
                Location {
                    vessel: dilution_vessel.clone(),
                    position: (dilution - 1)
                        .checked_mul(replicates)
                        .and_then(|base| base.checked_add(replicate))
                        .ok_or_else(|| {
                            "serial-dilution source position arithmetic overflows".to_owned()
                        })?,
                }
            };
            let destination = destinations[usize::try_from(position)
                .map_err(|_| "serial-dilution position does not fit this platform".to_owned())?]
            .clone();
            steps.push(PipettingStep::Transfer {
                id: procedure_id(&format!("dilute-{replicate:04}-{dilution:04}"))?,
                source,
                destination: destination.clone(),
                volume: volume(culture_volume)?,
                fluid_path: FluidPathPolicy::IsolatedDestinations,
                fluid_path_group: Some(group.clone()),
                technique: Default::default(),
            });
            steps.push(PipettingStep::Mix {
                id: procedure_id(&format!("mix-{replicate:04}-{dilution:04}"))?,
                targets: vec![destination],
                cycles: mix_cycles,
                volume: volume(mix_volume)?,
                fluid_path: FluidPathPolicy::IsolatedDestinations,
                fluid_path_group: Some(group.clone()),
                technique: Default::default(),
            });
        }
    }

    let program = PipettingProgramV1::new(
        vec![MaterialInput {
            id: medium_id.clone(),
        }],
        vec![MaterialOutput { id: output.clone() }],
        vec![
            Vessel {
                id: culture_vessel,
                role: VesselRole::ProcedureInput { input: 0 },
                positions: replicates,
                working_capacity_each: None,
                dead_volume_each: None,
                initial_volume_each: Some(volume(initial_volume)?),
                temperature: None,
            },
            Vessel {
                id: medium_vessel,
                role: VesselRole::MaterialSource {
                    material: medium_id,
                },
                positions: 1,
                working_capacity_each: None,
                dead_volume_each: None,
                initial_volume_each: Some(volume(medium_source_volume)?),
                temperature: None,
            },
            Vessel {
                id: dilution_vessel,
                role: VesselRole::Product { output },
                positions: position_count,
                working_capacity_each: None,
                dead_volume_each: None,
                initial_volume_each: None,
                temperature: None,
            },
        ],
        steps,
        PipettingConstraints::default(),
    )
    .validate()
    .map_err(|error| error.to_string())?;
    Ok(ProcedureProgram::from_pipetting(&program))
}

fn volume(microlitres: u32) -> Result<Volume, String> {
    Volume::parse_microlitres(microlitres.to_string()).map_err(|error| error.to_string())
}
