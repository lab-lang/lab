use crate::procedure::{
    DispenseStrategy, FluidPathPolicy, Location, MaterialInput, MaterialOutput,
    PipettingConstraints, PipettingProgramV1, PipettingStep, ProcedureProgram, TransferTechnique,
    Vessel, VesselRole, Volume,
};

use super::ProcedureTaskInstance;
use super::view::{TaskView, procedure_id};

const MICROLITRE: &str = "http://qudt.org/vocab/unit/MicroL";

pub(super) fn normalize(task: &ProcedureTaskInstance<'_>) -> Result<ProcedureProgram, String> {
    if task.input_count != 1 || task.outputs.len() != 1 {
        return Err(format!(
            "the selective-plating contract requires one diluted-culture input and one plate output, found {} inputs and {} outputs",
            task.input_count,
            task.outputs.len()
        ));
    }
    let view = TaskView::new(task);
    view.require_material_roles(&["selection"])?;
    let selection = view.text_parameter("selection")?;
    let selection_material = view.one_material("selection")?;
    if selection_material.symbol != selection {
        return Err("parameter `selection` does not match its material input".to_owned());
    }
    let plating_replicates = positive(&view, "replicates", None)?;
    let culture_replicates = positive(&view, "culture_replicates", None)?;
    let serial_dilutions = positive(&view, "serial_dilutions", None)?;
    let medium_volume = positive(&view, "medium_volume_ul", Some(MICROLITRE))?;
    let culture_volume = positive(&view, "culture_volume_ul", Some(MICROLITRE))?;
    let colony_volume = positive(&view, "colony_volume_ul", Some(MICROLITRE))?;
    let dilution_volume = medium_volume
        .checked_add(culture_volume)
        .ok_or_else(|| "dilution volume arithmetic overflows".to_owned())?;
    let consumed_from_non_final = colony_volume
        .checked_mul(plating_replicates)
        .and_then(|volume| volume.checked_add(culture_volume))
        .ok_or_else(|| "plating volume arithmetic overflows".to_owned())?;
    if consumed_from_non_final > dilution_volume {
        return Err(format!(
            "a non-final dilution contains {dilution_volume} uL but plating and the next dilution require {consumed_from_non_final} uL"
        ));
    }
    let consumed_from_final = colony_volume
        .checked_mul(plating_replicates)
        .ok_or_else(|| "plating volume arithmetic overflows".to_owned())?;
    if consumed_from_final > dilution_volume {
        return Err(format!(
            "a final dilution contains {dilution_volume} uL but plating requires {consumed_from_final} uL"
        ));
    }

    let selection_id = procedure_id(selection_material.id.as_str())?;
    let output = procedure_id(task.outputs[0].as_str())?;
    let agar = procedure_id("selective-agar")?;
    let spot_count = culture_replicates
        .checked_mul(serial_dilutions)
        .and_then(|value| value.checked_mul(plating_replicates))
        .ok_or_else(|| "plating position count overflows".to_owned())?;
    let mut vessels = Vec::with_capacity(
        usize::try_from(serial_dilutions)
            .unwrap_or(usize::MAX)
            .saturating_add(1),
    );
    let mut steps = Vec::new();
    let mut next_spot = 0_u32;
    for dilution in 0..serial_dilutions {
        let source_vessel = procedure_id(&format!("dilution-{dilution:04}"))?;
        let remaining = if dilution + 1 == serial_dilutions {
            dilution_volume
        } else {
            medium_volume
        };
        vessels.push(Vessel {
            id: source_vessel.clone(),
            role: VesselRole::ProcedureInput { input: 0 },
            positions: culture_replicates,
            working_capacity_each: None,
            dead_volume_each: None,
            initial_volume_each: Some(volume(remaining)?),
            temperature: None,
        });
        for replicate in 0..culture_replicates {
            let destinations = (0..plating_replicates)
                .map(|_| {
                    let location = Location {
                        vessel: agar.clone(),
                        position: next_spot,
                    };
                    next_spot += 1;
                    location
                })
                .collect::<Vec<_>>();
            steps.push(PipettingStep::Distribute {
                id: procedure_id(&format!("plate-{dilution:04}-{replicate:04}"))?,
                source: Location {
                    vessel: source_vessel.clone(),
                    position: replicate,
                },
                destinations,
                volume_each: volume(colony_volume)?,
                fluid_path: FluidPathPolicy::SharedSourceNoReentry,
                fluid_path_group: Some(procedure_id(&format!(
                    "plate-path-{dilution:04}-{replicate:04}"
                ))?),
                technique: TransferTechnique {
                    dispense: DispenseStrategy::MaterialSurface,
                    blow_out: true,
                    ..TransferTechnique::default()
                },
            });
        }
    }
    vessels.push(Vessel {
        id: agar,
        role: VesselRole::MaterialProduct {
            material: selection_id.clone(),
            output: output.clone(),
        },
        positions: spot_count,
        working_capacity_each: None,
        dead_volume_each: None,
        initial_volume_each: None,
        temperature: None,
    });
    let program = PipettingProgramV1::new(
        vec![MaterialInput { id: selection_id }],
        vec![MaterialOutput { id: output }],
        vessels,
        steps,
        PipettingConstraints::default(),
    )
    .validate()
    .map_err(|error| error.to_string())?;
    Ok(ProcedureProgram::from_pipetting(&program))
}

fn positive(view: &TaskView<'_, '_>, name: &str, unit: Option<&str>) -> Result<u32, String> {
    let value = view.integer_parameter(name, unit)?;
    if value == 0 {
        return Err(format!("parameter `{name}` must be greater than zero"));
    }
    Ok(value)
}

fn volume(value: u32) -> Result<Volume, String> {
    Volume::parse_microlitres(value.to_string()).map_err(|error| error.to_string())
}
