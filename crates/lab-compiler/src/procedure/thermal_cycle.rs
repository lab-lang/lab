use lab_procedure::{
    Duration, ProcedureProgram, Temperature, ThermalLoad, ThermalProgramV1, ThermalStage,
    ThermalStep, Volume,
};

use super::ProcedureTaskInstance;
use super::view::{TaskView, procedure_id};

const MICROLITRE: &str = "http://qudt.org/vocab/unit/MicroL";
const DEGREE_CELSIUS: &str = "http://qudt.org/vocab/unit/DEG_C";
const MINUTE: &str = "http://qudt.org/vocab/unit/MIN";

pub(super) fn normalize(task: &ProcedureTaskInstance<'_>) -> Result<ProcedureProgram, String> {
    if task.input_count != 1 {
        return Err(format!(
            "the Golden Gate thermal contract requires exactly one reaction input, found {}",
            task.input_count
        ));
    }
    if task.outputs.len() != 1 {
        return Err(format!(
            "the Golden Gate thermal contract requires exactly one product output, found {}",
            task.outputs.len()
        ));
    }
    let view = TaskView::new(task);
    view.require_material_roles(&[])?;
    let sample_count = positive(&view, "assembly_replicates", None)?;
    let volume_each = positive(&view, "reaction_volume_ul", Some(MICROLITRE))?;
    let cycles = positive(&view, "cycles", None)?;
    let digest_temperature =
        view.integer_parameter("digest_temperature_c", Some(DEGREE_CELSIUS))?;
    let digest_minutes = positive(&view, "digest_minutes", Some(MINUTE))?;
    let ligate_temperature =
        view.integer_parameter("ligate_temperature_c", Some(DEGREE_CELSIUS))?;
    let ligate_minutes = positive(&view, "ligate_minutes", Some(MINUTE))?;
    let lid_temperature = positive(&view, "lid_temperature_c", Some(DEGREE_CELSIUS))?;
    let final_digest_temperature =
        view.integer_parameter("final_digest_temperature_c", Some(DEGREE_CELSIUS))?;
    let final_digest_minutes = positive(&view, "final_digest_minutes", Some(MINUTE))?;
    let heat_inactivation_temperature =
        view.integer_parameter("heat_inactivation_temperature_c", Some(DEGREE_CELSIUS))?;
    let heat_inactivation_minutes = positive(&view, "heat_inactivation_minutes", Some(MINUTE))?;
    let hold_temperature = view.integer_parameter("hold_temperature_c", Some(DEGREE_CELSIUS))?;

    let program = ThermalProgramV1 {
        load: ThermalLoad {
            input: 0,
            output: procedure_id(task.outputs[0].as_str())?,
            sample_count,
            volume_each: Volume::parse_microlitres(volume_each.to_string())
                .map_err(|error| error.to_string())?,
        },
        lid_temperature: Some(temperature(lid_temperature)?),
        stages: vec![
            ThermalStage {
                id: procedure_id("digest-ligate-cycle")?,
                repeats: cycles,
                steps: vec![
                    step("digest", digest_temperature, digest_minutes)?,
                    step("ligate", ligate_temperature, ligate_minutes)?,
                ],
            },
            ThermalStage {
                id: procedure_id("completion")?,
                repeats: 1,
                steps: vec![
                    step(
                        "final-digest",
                        final_digest_temperature,
                        final_digest_minutes,
                    )?,
                    step(
                        "heat-inactivation",
                        heat_inactivation_temperature,
                        heat_inactivation_minutes,
                    )?,
                ],
            },
        ],
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

fn temperature(celsius: u32) -> Result<Temperature, String> {
    Temperature::parse_degrees_celsius(celsius.to_string()).map_err(|error| error.to_string())
}

fn step(id: &str, celsius: u32, minutes: u32) -> Result<ThermalStep, String> {
    let seconds = minutes
        .checked_mul(60)
        .ok_or_else(|| format!("thermal step `{id}` duration overflows seconds"))?;
    Ok(ThermalStep {
        id: procedure_id(id)?,
        temperature: temperature(celsius)?,
        hold: Duration::parse_seconds(seconds.to_string()).map_err(|error| error.to_string())?,
        ramp_rate: None,
    })
}
