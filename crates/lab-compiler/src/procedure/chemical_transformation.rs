use lab_procedure::{
    AspirationStrategy, DispenseStrategy, Duration, FluidPathPolicy, Length, Location,
    MaterialInput, MaterialOutput, MixTechnique, PipettingConstraints, PipettingProgramV1,
    PipettingStep, ProcedureProgram, Temperature, TemperatureRange, ThermalLoad, ThermalProgramV1,
    ThermalStage, ThermalStep, TransferTechnique, Vessel, VesselRole, Volume,
};

use super::ProcedureTaskInstance;
use super::view::{TaskView, material_symbols, procedure_id};

const MICROLITRE: &str = "http://qudt.org/vocab/unit/MicroL";
const MILLIMETRE: &str = "http://qudt.org/vocab/unit/MilliM";
const DEGREE_CELSIUS: &str = "http://qudt.org/vocab/unit/DEG_C";
const MINUTE: &str = "http://qudt.org/vocab/unit/MIN";

pub(super) fn normalize_prepare(
    task: &ProcedureTaskInstance<'_>,
) -> Result<ProcedureProgram, String> {
    if task.input_count != 2 {
        return Err(format!(
            "the transformation-preparation contract requires a design and competent-cell input, found {} inputs",
            task.input_count
        ));
    }
    if task.outputs.len() != 1 {
        return Err(format!(
            "the transformation-preparation contract requires exactly one mixture output, found {}",
            task.outputs.len()
        ));
    }
    let view = TaskView::new(task);
    view.require_material_roles(&["dependencies"])?;
    let dependencies = view.text_list_parameter("dependencies")?;
    let plasmids = view.text_list_parameter("plasmids")?;
    let materials = view.materials("dependencies");
    if dependencies.is_empty() || dependencies.len() != plasmids.len() {
        return Err("transformation must provide one ordered DNA material dependency for every plasmid in the strain design".to_owned());
    }
    if material_symbols(&materials) != dependencies {
        return Err("transformation dependencies do not match their material inputs".to_owned());
    }
    let dna_count = positive(&view, "dna_count", None)?;
    if usize::try_from(dna_count).ok() != Some(materials.len()) {
        return Err(format!(
            "parameter `dna_count` is {dna_count}, but the transformation has {} DNA materials",
            materials.len()
        ));
    }
    let replicates = positive(&view, "replicates", None)?;
    let cell_volume = positive(&view, "cell_volume_ul", Some(MICROLITRE))?;
    let dna_volume = positive(&view, "dna_volume_ul", Some(MICROLITRE))?;
    let cell_mix_cycles = positive(&view, "cell_mix_cycles", None)?;
    let cell_mix_volume = positive(&view, "cell_mix_volume_ul", Some(MICROLITRE))?;
    let dna_mix_cycles = positive(&view, "dna_mix_cycles", None)?;
    let bubble_clear_cycles = positive(&view, "bubble_clear_cycles", None)?;
    let bubble_clear_volume = positive(&view, "bubble_clear_volume_ul", Some(MICROLITRE))?;
    let bubble_offset =
        view.integer_parameter("bubble_clear_dispense_offset_mm", Some(MILLIMETRE))?;
    // Chemically competent cells lose transformation efficiency quickly at bench temperature, so
    // the Method states the staging requirement and the facility must supply an offering that
    // holds it. Without this the aliquot would be planned onto an ambient rack.
    let cell_staging_temperature =
        view.integer_parameter("cell_staging_temperature_c", Some(DEGREE_CELSIUS))?;

    let cells = procedure_id("competent-cells")?;
    let mixture = procedure_id(task.outputs[0].as_str())?;
    let reactions = procedure_id("transformation-reactions")?;
    let destinations = (0..replicates)
        .map(|position| Location {
            vessel: reactions.clone(),
            position,
        })
        .collect::<Vec<_>>();
    let mut normalized_materials = Vec::with_capacity(materials.len());
    let mut vessels = Vec::with_capacity(materials.len() + 2);
    let mut steps = Vec::new();
    vessels.push(Vessel {
        id: cells.clone(),
        role: VesselRole::ProcedureInput { input: 1 },
        positions: 1,
        initial_volume_each: None,
        temperature: Some(TemperatureRange::exact(
            Temperature::parse_degrees_celsius(cell_staging_temperature.to_string())
                .map_err(|error| error.to_string())?,
        )),
    });
    vessels.push(Vessel {
        id: reactions.clone(),
        role: VesselRole::Product {
            output: mixture.clone(),
        },
        positions: replicates,
        initial_volume_each: None,
        temperature: None,
    });
    steps.push(PipettingStep::Mix {
        id: procedure_id("mix-competent-cells")?,
        targets: vec![Location {
            vessel: cells.clone(),
            position: 0,
        }],
        cycles: cell_mix_cycles,
        volume: volume(cell_mix_volume)?,
        fluid_path: FluidPathPolicy::IsolatedDestinations,
        fluid_path_group: Some(procedure_id("competent-cell-path")?),
        technique: MixTechnique::default(),
    });
    steps.push(PipettingStep::Distribute {
        id: procedure_id("add-competent-cells")?,
        source: Location {
            vessel: cells,
            position: 0,
        },
        destinations: destinations.clone(),
        volume_each: volume(cell_volume)?,
        fluid_path: FluidPathPolicy::SharedSourceNoReentry,
        fluid_path_group: Some(procedure_id("competent-cell-path")?),
        technique: TransferTechnique::default(),
    });

    for (material_index, material) in materials.into_iter().enumerate() {
        let material_id = procedure_id(material.id.as_str())?;
        let source = procedure_id(&format!("dna-source-{material_index:04}"))?;
        normalized_materials.push(MaterialInput {
            id: material_id.clone(),
        });
        vessels.push(Vessel {
            id: source.clone(),
            role: VesselRole::MaterialSource {
                material: material_id,
            },
            positions: 1,
            initial_volume_each: None,
            temperature: None,
        });
        for (replicate, destination) in destinations.iter().enumerate() {
            let group = procedure_id(&format!("dna-{material_index:04}-{replicate:04}"))?;
            steps.push(PipettingStep::Mix {
                id: procedure_id(&format!("mix-dna-{material_index:04}-{replicate:04}"))?,
                targets: vec![Location {
                    vessel: source.clone(),
                    position: 0,
                }],
                cycles: dna_mix_cycles,
                volume: volume(dna_volume)?,
                fluid_path: FluidPathPolicy::IsolatedDestinations,
                fluid_path_group: Some(group.clone()),
                technique: MixTechnique::default(),
            });
            steps.push(PipettingStep::Transfer {
                id: procedure_id(&format!("add-dna-{material_index:04}-{replicate:04}"))?,
                source: Location {
                    vessel: source.clone(),
                    position: 0,
                },
                destination: destination.clone(),
                volume: volume(dna_volume)?,
                fluid_path: FluidPathPolicy::IsolatedDestinations,
                fluid_path_group: Some(group.clone()),
                technique: TransferTechnique {
                    blow_out: true,
                    ..TransferTechnique::default()
                },
            });
            steps.push(PipettingStep::Mix {
                id: procedure_id(&format!("clear-bubbles-{material_index:04}-{replicate:04}"))?,
                targets: vec![destination.clone()],
                cycles: bubble_clear_cycles,
                volume: volume(bubble_clear_volume)?,
                fluid_path: FluidPathPolicy::IsolatedDestinations,
                fluid_path_group: Some(group),
                technique: MixTechnique {
                    aspiration: AspirationStrategy::VesselBottom {
                        offset: Length::parse_millimetres("0")
                            .map_err(|error| error.to_string())?,
                    },
                    dispense: DispenseStrategy::VesselBottom {
                        offset: Length::parse_millimetres(bubble_offset.to_string())
                            .map_err(|error| error.to_string())?,
                    },
                    blow_out: false,
                    touch_tip: true,
                },
            });
        }
    }

    let program = PipettingProgramV1::new(
        normalized_materials,
        vec![MaterialOutput { id: mixture }],
        vessels,
        steps,
        PipettingConstraints::default(),
    )
    .validate()
    .map_err(|error| error.to_string())?;
    Ok(ProcedureProgram::from_pipetting(&program))
}

pub(super) fn normalize_heat_shock(
    task: &ProcedureTaskInstance<'_>,
) -> Result<ProcedureProgram, String> {
    if task.input_count != 1 || task.outputs.len() != 2 {
        return Err(format!(
            "the heat-shock contract requires one mixture input and two strain/culture outputs, found {} inputs and {} outputs",
            task.input_count,
            task.outputs.len()
        ));
    }
    let view = TaskView::new(task);
    view.require_material_roles(&[])?;
    let replicates = positive(&view, "replicates", None)?;
    let dna_count = positive(&view, "dna_count", None)?;
    let cell_volume = positive(&view, "cell_volume_ul", Some(MICROLITRE))?;
    let dna_volume = positive(&view, "dna_volume_ul", Some(MICROLITRE))?;
    let cold_temperature = view.integer_parameter("cold_temperature_c", Some(DEGREE_CELSIUS))?;
    let cold_minutes = positive(&view, "cold_minutes", Some(MINUTE))?;
    let shock_temperature =
        view.integer_parameter("heat_shock_temperature_c", Some(DEGREE_CELSIUS))?;
    let shock_minutes = positive(&view, "heat_shock_minutes", Some(MINUTE))?;
    let post_shock_minutes = positive(&view, "post_shock_minutes", Some(MINUTE))?;
    let hold_temperature = view.integer_parameter("hold_temperature_c", Some(DEGREE_CELSIUS))?;
    let volume_each = dna_volume
        .checked_mul(dna_count)
        .and_then(|dna| dna.checked_add(cell_volume))
        .ok_or_else(|| "heat-shock sample volume arithmetic overflows".to_owned())?;

    let program = ThermalProgramV1 {
        load: ThermalLoad {
            input: 0,
            outputs: task
                .outputs
                .iter()
                .map(|output| procedure_id(output.as_str()))
                .collect::<Result<Vec<_>, _>>()?,
            sample_count: replicates,
            volume_each: volume(volume_each)?,
        },
        lid_temperature: None,
        stages: vec![ThermalStage {
            id: procedure_id("heat-shock")?,
            repeats: 1,
            steps: vec![
                thermal_step("cold-incubation", cold_temperature, cold_minutes)?,
                thermal_step("heat-shock", shock_temperature, shock_minutes)?,
                thermal_step("post-shock-cold", cold_temperature, post_shock_minutes)?,
            ],
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

fn thermal_step(id: &str, celsius: u32, minutes: u32) -> Result<ThermalStep, String> {
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
