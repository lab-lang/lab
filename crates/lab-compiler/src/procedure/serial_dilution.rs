use lab_procedure::{
    FluidPathPolicy, Location, MaterialInput, MaterialOutput, PipettingConstraints,
    PipettingProgramV1, PipettingStep, ProcedureProgram, Vessel, VesselRole, Volume,
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
    let serial_dilutions = view.integer_parameter("serial_dilutions", None)?;
    let medium_volume = view.integer_parameter("medium_volume_ul", Some(MICROLITRE))?;
    let culture_volume = view.integer_parameter("culture_volume_ul", Some(MICROLITRE))?;
    let mix_cycles = view.integer_parameter("mix_cycles", None)?;
    let mix_volume = view.integer_parameter("mix_volume_ul", Some(MICROLITRE))?;
    for (name, value) in [
        ("serial_dilutions", serial_dilutions),
        ("medium_volume_ul", medium_volume),
        ("culture_volume_ul", culture_volume),
        ("mix_cycles", mix_cycles),
        ("mix_volume_ul", mix_volume),
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
    let destinations = (0..serial_dilutions)
        .map(|position| Location {
            vessel: dilution_vessel.clone(),
            position,
        })
        .collect::<Vec<_>>();
    let mut steps = Vec::with_capacity(
        usize::try_from(serial_dilutions)
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
    });
    for position in 0..serial_dilutions {
        let source = if position == 0 {
            Location {
                vessel: culture_vessel.clone(),
                position: 0,
            }
        } else {
            Location {
                vessel: dilution_vessel.clone(),
                position: position - 1,
            }
        };
        let destination = destinations[usize::try_from(position)
            .map_err(|_| "serial-dilution position does not fit this platform".to_owned())?]
        .clone();
        steps.push(PipettingStep::Transfer {
            id: procedure_id(&format!("dilute-{position:04}"))?,
            source,
            destination: destination.clone(),
            volume: volume(culture_volume)?,
            fluid_path: FluidPathPolicy::IsolatedDestinations,
        });
        steps.push(PipettingStep::Mix {
            id: procedure_id(&format!("mix-{position:04}"))?,
            targets: vec![destination],
            cycles: mix_cycles,
            volume: volume(mix_volume)?,
            fluid_path: FluidPathPolicy::IsolatedDestinations,
        });
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
                positions: 1,
            },
            Vessel {
                id: medium_vessel,
                role: VesselRole::MaterialSource {
                    material: medium_id,
                },
                positions: 1,
            },
            Vessel {
                id: dilution_vessel,
                role: VesselRole::Product { output },
                positions: serial_dilutions,
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
