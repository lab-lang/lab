use lab_procedure::{
    AspirationStrategy, DispenseStrategy, Duration, FluidPathPolicy, Location, MaterialInput,
    MaterialOutput, PipettingConstraints, PipettingProgramV1, PipettingStep, ProcedureProgram,
    Temperature, ThermalLoad, ThermalProgramV1, ThermalStage, ThermalStep, TransferTechnique,
    Vessel, VesselRole, Volume,
};

use super::ProcedureTaskInstance;
use super::view::{TaskView, procedure_id};

const MICROLITRE: &str = "http://qudt.org/vocab/unit/MicroL";
const DEGREE_CELSIUS: &str = "http://qudt.org/vocab/unit/DEG_C";
const HOUR: &str = "http://qudt.org/vocab/unit/HR";
const MINUTE: &str = "http://qudt.org/vocab/unit/MIN";

pub(super) fn normalize_add_medium(
    task: &ProcedureTaskInstance<'_>,
) -> Result<ProcedureProgram, String> {
    if task.input_count != 1 || task.outputs.len() != 1 {
        return Err(format!(
            "the recovery-medium contract requires one culture input and one mixture output, found {} inputs and {} outputs",
            task.input_count,
            task.outputs.len()
        ));
    }
    let view = TaskView::new(task);
    view.require_material_roles(&["medium"])?;
    let medium = view.one_material("medium")?;
    let replicates = positive(&view, "replicates", None)?;
    let initial_volume = positive(&view, "initial_volume_ul", Some(MICROLITRE))?;
    let recovery_volume = positive(&view, "recovery_volume_ul", Some(MICROLITRE))?;
    let air_gap = positive(&view, "air_gap_ul", Some(MICROLITRE))?;
    let medium_id = procedure_id(medium.id.as_str())?;
    let medium_vessel = procedure_id("recovery-medium")?;
    let output = procedure_id(task.outputs[0].as_str())?;
    let culture_vessel = procedure_id("recovery-cultures")?;
    let destinations = (0..replicates)
        .map(|position| Location {
            vessel: culture_vessel.clone(),
            position,
        })
        .collect::<Vec<_>>();
    let program = PipettingProgramV1::new(
        vec![MaterialInput {
            id: medium_id.clone(),
        }],
        vec![MaterialOutput { id: output.clone() }],
        vec![
            Vessel {
                id: medium_vessel.clone(),
                role: VesselRole::MaterialSource {
                    material: medium_id,
                },
                positions: 1,
                working_capacity_each: None,
                dead_volume_each: None,
                initial_volume_each: None,
                temperature: None,
            },
            Vessel {
                id: culture_vessel,
                role: VesselRole::InputOutput { input: 0, output },
                positions: replicates,
                working_capacity_each: None,
                dead_volume_each: None,
                initial_volume_each: Some(volume(initial_volume)?),
                temperature: None,
            },
        ],
        vec![PipettingStep::Distribute {
            id: procedure_id("add-recovery-medium")?,
            source: Location {
                vessel: medium_vessel,
                position: 0,
            },
            destinations,
            volume_each: volume(recovery_volume)?,
            fluid_path: FluidPathPolicy::SharedSourceNoReentry,
            fluid_path_group: Some(procedure_id("recovery-medium-path")?),
            technique: TransferTechnique {
                aspiration: AspirationStrategy::Liquid,
                dispense: DispenseStrategy::AboveLiquid,
                air_gap: Some(volume(air_gap)?),
                blow_out: false,
                touch_tip: false,
            },
        }],
        PipettingConstraints::default(),
    )
    .validate()
    .map_err(|error| error.to_string())?;
    Ok(ProcedureProgram::from_pipetting(&program))
}

pub(super) fn normalize_incubation(
    task: &ProcedureTaskInstance<'_>,
) -> Result<ProcedureProgram, String> {
    if task.input_count != 1 || task.outputs.len() != 1 {
        return Err(format!(
            "the recovery-incubation contract requires one mixture input and one culture output, found {} inputs and {} outputs",
            task.input_count,
            task.outputs.len()
        ));
    }
    let view = TaskView::new(task);
    view.require_material_roles(&[])?;
    let replicates = positive(&view, "replicates", None)?;
    let initial_volume = positive(&view, "initial_volume_ul", Some(MICROLITRE))?;
    let recovery_volume = positive(&view, "recovery_volume_ul", Some(MICROLITRE))?;
    let temperature_c = view.integer_parameter("recovery_temperature_c", Some(DEGREE_CELSIUS))?;
    let hold_temperature = view.integer_parameter("hold_temperature_c", Some(DEGREE_CELSIUS))?;
    let (duration, unit) = view.decimal_parameter("duration")?;
    if duration.is_zero() || duration.is_negative() {
        return Err("parameter `duration` must be greater than zero".to_owned());
    }
    let seconds = match unit {
        Some(HOUR) => duration.multiplied_by_u32(3_600),
        Some(MINUTE) => duration.multiplied_by_u32(60),
        found => {
            return Err(format!(
                "parameter `duration` must use hours or minutes, found {found:?}"
            ));
        }
    };
    let total_volume = initial_volume
        .checked_add(recovery_volume)
        .ok_or_else(|| "recovery sample volume arithmetic overflows".to_owned())?;
    let program = ThermalProgramV1 {
        load: ThermalLoad {
            input: 0,
            outputs: vec![procedure_id(task.outputs[0].as_str())?],
            sample_count: replicates,
            volume_each: volume(total_volume)?,
        },
        lid_temperature: None,
        stages: vec![ThermalStage {
            id: procedure_id("recovery-incubation")?,
            repeats: 1,
            steps: vec![ThermalStep {
                id: procedure_id("recover")?,
                temperature: temperature(temperature_c)?,
                hold: Duration::seconds(seconds).map_err(|error| error.to_string())?,
                ramp_rate: None,
            }],
        }],
        final_hold: Some(temperature(hold_temperature)?),
    }
    .validate()
    .map_err(|error| error.to_string())?;
    Ok(ProcedureProgram::from_thermal(&program))
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

fn temperature(value: u32) -> Result<Temperature, String> {
    Temperature::parse_degrees_celsius(value.to_string()).map_err(|error| error.to_string())
}
