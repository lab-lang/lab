use lab_procedure::{
    AspirationStrategy, DispenseStrategy, FluidPathPolicy, Length, Location, MaterialInput,
    MaterialOutput, MixTechnique, PipettingConstraints, PipettingProgramV1, PipettingStep,
    ProcedureProgram, Temperature, TemperatureRange, TransferTechnique, Vessel, VesselRole, Volume,
};

use super::ProcedureTaskInstance;
use super::view::{TaskView, material_symbols, procedure_id};

const MICROLITRE: &str = "http://qudt.org/vocab/unit/MicroL";
const MILLIMETRE: &str = "http://qudt.org/vocab/unit/MilliM";
const DEGREE_CELSIUS: &str = "http://qudt.org/vocab/unit/DEG_C";

enum SetupStrategy {
    Basic {
        mix_cycles: u32,
        mix_volume: u32,
    },
    TemperatureStaged {
        source_mix_cycles: u32,
        source_temperature: u32,
        bubble_clear_cycles: u32,
        bubble_clear_volume: u32,
        bubble_clear_offset: u32,
    },
}

pub(super) fn normalize(task: &ProcedureTaskInstance<'_>) -> Result<ProcedureProgram, String> {
    let view = TaskView::new(task);
    view.require_material_roles(&[
        "backbone",
        "components",
        "dependencies",
        "restriction-enzyme",
        "ligase",
        "buffer",
        "water",
    ])?;

    let backbone = view.text_parameter("backbone")?;
    let components = view.text_list_parameter("components")?;
    let dependencies = view.text_list_parameter("dependencies")?;
    let restriction_enzyme = view.text_parameter("restriction_enzyme")?;
    let replicates = view.integer_parameter("assembly_replicates", None)?;
    let reaction_volume = view.integer_parameter("reaction_volume_ul", Some(MICROLITRE))?;
    let part_volume = view.integer_parameter("part_volume_ul", Some(MICROLITRE))?;
    let enzyme_volume = view.integer_parameter("enzyme_volume_ul", Some(MICROLITRE))?;
    let ligase_volume = view.integer_parameter("ligase_volume_ul", Some(MICROLITRE))?;
    let buffer_volume = view.integer_parameter("buffer_volume_ul", Some(MICROLITRE))?;
    let setup_strategy = match view.text_parameter("setup_strategy")?.as_str() {
        "basic_v1" => SetupStrategy::Basic {
            mix_cycles: view.integer_parameter("mix_cycles", None)?,
            mix_volume: view.integer_parameter("mix_volume_ul", Some(MICROLITRE))?,
        },
        "temperature_staged_v1" => {
            let bubble_clear_divisor =
                view.integer_parameter("bubble_clear_divisor_ul", Some(MICROLITRE))?;
            let bubble_clear_max_volume =
                view.integer_parameter("bubble_clear_max_volume_ul", Some(MICROLITRE))?;
            if bubble_clear_divisor == 0 {
                return Err(
                    "parameter `bubble_clear_divisor_ul` must be greater than zero".to_owned(),
                );
            }
            if bubble_clear_max_volume == 0 {
                return Err(
                    "parameter `bubble_clear_max_volume_ul` must be greater than zero".to_owned(),
                );
            }
            let bubble_clear_cycles = reaction_volume / bubble_clear_divisor;
            if bubble_clear_cycles == 0 {
                return Err(format!(
                    "reaction volume {reaction_volume} uL is too small for a bubble-clearing movement every {bubble_clear_divisor} uL"
                ));
            }
            SetupStrategy::TemperatureStaged {
                source_mix_cycles: view.integer_parameter("source_mix_cycles", None)?,
                source_temperature: view
                    .integer_parameter("source_temperature_c", Some(DEGREE_CELSIUS))?,
                bubble_clear_cycles,
                bubble_clear_volume: reaction_volume.min(bubble_clear_max_volume),
                bubble_clear_offset: view
                    .integer_parameter("bubble_clear_dispense_offset_mm", Some(MILLIMETRE))?,
            }
        }
        value => {
            return Err(format!("unsupported Golden Gate setup strategy `{value}`"));
        }
    };

    for (name, value) in [
        ("assembly_replicates", replicates),
        ("reaction_volume_ul", reaction_volume),
        ("part_volume_ul", part_volume),
        ("enzyme_volume_ul", enzyme_volume),
        ("ligase_volume_ul", ligase_volume),
        ("buffer_volume_ul", buffer_volume),
    ] {
        if value == 0 {
            return Err(format!("parameter `{name}` must be greater than zero"));
        }
    }
    match &setup_strategy {
        SetupStrategy::Basic {
            mix_cycles,
            mix_volume,
        } => {
            if *mix_cycles == 0 || *mix_volume == 0 {
                return Err("basic Golden Gate mixing values must be greater than zero".to_owned());
            }
            if *mix_volume > reaction_volume {
                return Err(format!(
                    "mix volume {mix_volume} uL exceeds the {reaction_volume} uL reaction volume"
                ));
            }
        }
        SetupStrategy::TemperatureStaged {
            source_mix_cycles, ..
        } if *source_mix_cycles == 0 => {
            return Err("parameter `source_mix_cycles` must be greater than zero".to_owned());
        }
        SetupStrategy::TemperatureStaged { .. } => {}
    }

    let backbone_material = view.one_material("backbone")?;
    if backbone_material.symbol != backbone {
        return Err("parameter `backbone` does not match its material input".to_owned());
    }
    let component_materials = view.materials("components");
    if material_symbols(&component_materials) != components {
        return Err("parameter `components` does not match its material inputs".to_owned());
    }
    let dependency_materials = view.materials("dependencies");
    if material_symbols(&dependency_materials) != dependencies {
        return Err("parameter `dependencies` does not match its material inputs".to_owned());
    }
    let enzyme_material = view.one_material("restriction-enzyme")?;
    if enzyme_material.symbol != restriction_enzyme {
        return Err("parameter `restriction_enzyme` does not match its material input".to_owned());
    }
    let ligase_material = view.one_material("ligase")?;
    let buffer_material = view.one_material("buffer")?;
    let water_material = view.one_material("water")?;

    let dna_piece_count = u32::try_from(component_materials.len() + dependency_materials.len() + 1)
        .map_err(|_| "the reaction contains too many DNA pieces".to_owned())?;
    let consumed = buffer_volume
        .checked_add(ligase_volume)
        .and_then(|value| value.checked_add(enzyme_volume))
        .and_then(|value| value.checked_add(part_volume.checked_mul(dna_piece_count)?))
        .ok_or_else(|| "reaction volume arithmetic overflows".to_owned())?;
    let water_volume = reaction_volume.checked_sub(consumed).ok_or_else(|| {
        format!("reagents require {consumed} uL before water in a {reaction_volume} uL reaction")
    })?;
    let mut additions = Vec::new();
    if water_volume > 0 {
        additions.push((water_material, water_volume, false));
    }
    additions.extend([
        (buffer_material, buffer_volume, true),
        (ligase_material, ligase_volume, true),
        (enzyme_material, enzyme_volume, true),
        (backbone_material, part_volume, true),
    ]);
    additions.extend(
        component_materials
            .into_iter()
            .map(|material| (material, part_volume, true)),
    );
    additions.extend(
        dependency_materials
            .into_iter()
            .map(|material| (material, part_volume, true)),
    );

    let product = task
        .outputs
        .first()
        .ok_or_else(|| "the task has no material output".to_owned())?;
    if task.outputs.len() != 1 {
        return Err(format!(
            "the pipetting contract requires exactly one product output, found {}",
            task.outputs.len()
        ));
    }
    let product = procedure_id(product.as_str())?;
    let destination_vessel = procedure_id("reaction-plate")?;
    let destinations = (0..replicates)
        .map(|position| Location {
            vessel: destination_vessel.clone(),
            position,
        })
        .collect::<Vec<_>>();

    let mut materials = Vec::with_capacity(additions.len());
    // Staged reagents are held at temperature for the whole program, so the requirement belongs to
    // the source vessels rather than to the reaction they are pipetted into.
    let staging_temperature = match &setup_strategy {
        SetupStrategy::Basic { .. } => None,
        SetupStrategy::TemperatureStaged {
            source_temperature, ..
        } => Some(TemperatureRange::exact(
            Temperature::parse_degrees_celsius(source_temperature.to_string())
                .map_err(|error| error.to_string())?,
        )),
    };
    let mut vessels = Vec::with_capacity(additions.len() + 1);
    let mut normalized_additions = Vec::with_capacity(additions.len());
    for (index, (material, volume, mix_source)) in additions.into_iter().enumerate() {
        let material_id = procedure_id(material.id.as_str())?;
        let source_vessel = procedure_id(&format!("source-{index:04}"))?;
        materials.push(MaterialInput {
            id: material_id.clone(),
        });
        vessels.push(Vessel {
            id: source_vessel.clone(),
            role: VesselRole::MaterialSource {
                material: material_id,
            },
            positions: 1,
            initial_volume_each: None,
            temperature: staging_temperature.clone(),
        });
        normalized_additions.push((
            Location {
                vessel: source_vessel,
                position: 0,
            },
            volume,
            mix_source,
        ));
    }
    vessels.push(Vessel {
        id: destination_vessel,
        role: VesselRole::Product {
            output: product.clone(),
        },
        positions: replicates,
        initial_volume_each: None,
        temperature: None,
    });
    let (steps, constraints) = match setup_strategy {
        SetupStrategy::Basic {
            mix_cycles,
            mix_volume,
        } => {
            let mut steps = Vec::with_capacity(normalized_additions.len() + 1);
            for (index, (source, volume, _)) in normalized_additions.iter().enumerate() {
                steps.push(PipettingStep::Distribute {
                    id: procedure_id(&format!("add-{index:04}"))?,
                    source: source.clone(),
                    destinations: destinations.clone(),
                    volume_each: Volume::parse_microlitres(volume.to_string())
                        .map_err(|error| error.to_string())?,
                    fluid_path: FluidPathPolicy::SharedSourceNoReentry,
                    fluid_path_group: None,
                    technique: TransferTechnique::default(),
                });
            }
            steps.push(PipettingStep::Mix {
                id: procedure_id("mix-reactions")?,
                targets: destinations,
                cycles: mix_cycles,
                volume: Volume::parse_microlitres(mix_volume.to_string())
                    .map_err(|error| error.to_string())?,
                fluid_path: FluidPathPolicy::IsolatedDestinations,
                fluid_path_group: None,
                technique: MixTechnique::default(),
            });
            (steps, PipettingConstraints::default())
        }
        SetupStrategy::TemperatureStaged {
            source_mix_cycles,
            source_temperature: _,
            bubble_clear_cycles,
            bubble_clear_volume,
            bubble_clear_offset,
        } => {
            let mut steps = Vec::new();
            for (replicate, destination) in destinations.iter().enumerate() {
                for (addition, (source, volume, mix_source)) in
                    normalized_additions.iter().enumerate()
                {
                    let fluid_path_group = procedure_id(&format!(
                        "addition-{addition:04}-replicate-{replicate:04}-path"
                    ))?;
                    if *mix_source {
                        steps.push(PipettingStep::Mix {
                            id: procedure_id(&format!(
                                "mix-source-{addition:04}-replicate-{replicate:04}"
                            ))?,
                            targets: vec![source.clone()],
                            cycles: source_mix_cycles,
                            volume: Volume::parse_microlitres(volume.to_string())
                                .map_err(|error| error.to_string())?,
                            fluid_path: FluidPathPolicy::IsolatedDestinations,
                            fluid_path_group: Some(fluid_path_group.clone()),
                            technique: MixTechnique::default(),
                        });
                    }
                    steps.push(PipettingStep::Transfer {
                        id: procedure_id(&format!("add-{addition:04}-replicate-{replicate:04}"))?,
                        source: source.clone(),
                        destination: destination.clone(),
                        volume: Volume::parse_microlitres(volume.to_string())
                            .map_err(|error| error.to_string())?,
                        fluid_path: FluidPathPolicy::IsolatedDestinations,
                        fluid_path_group: mix_source.then_some(fluid_path_group.clone()),
                        technique: TransferTechnique {
                            blow_out: true,
                            touch_tip: true,
                            ..TransferTechnique::default()
                        },
                    });
                    if addition + 1 == normalized_additions.len() {
                        steps.push(PipettingStep::Mix {
                            id: procedure_id(&format!("clear-bubbles-replicate-{replicate:04}"))?,
                            targets: vec![destination.clone()],
                            cycles: bubble_clear_cycles,
                            volume: Volume::parse_microlitres(bubble_clear_volume.to_string())
                                .map_err(|error| error.to_string())?,
                            fluid_path: FluidPathPolicy::IsolatedDestinations,
                            fluid_path_group: Some(fluid_path_group),
                            technique: MixTechnique {
                                aspiration: AspirationStrategy::VesselBottom {
                                    offset: Length::parse_millimetres("0")
                                        .map_err(|error| error.to_string())?,
                                },
                                dispense: DispenseStrategy::VesselBottom {
                                    offset: Length::parse_millimetres(
                                        bubble_clear_offset.to_string(),
                                    )
                                    .map_err(|error| error.to_string())?,
                                },
                                blow_out: true,
                                touch_tip: true,
                            },
                        });
                    }
                }
            }
            (steps, PipettingConstraints::default())
        }
    };

    let program = PipettingProgramV1::new(
        materials,
        vec![MaterialOutput { id: product }],
        vessels,
        steps,
        constraints,
    )
    .validate()
    .map_err(|error| error.to_string())?;
    Ok(ProcedureProgram::from_pipetting(&program))
}
