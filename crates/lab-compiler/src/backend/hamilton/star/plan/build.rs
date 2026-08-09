//! Construction of the validated, resource-allocated STAR execution plan.
//!
//! Planning runs the choreography twice: the first pass records what every
//! source position must supply, which sets the fills the operator loads
//! (consumption plus dead volume); the second pass runs over a bench seeded
//! with those fills, so every height reflects the liquid actually present.
//! Both passes are the same deterministic function, so the operations of
//! the second are the plan.

use std::collections::{BTreeMap, BTreeSet};

use pliron::context::Context;

use crate::ProtocolLairProgram;
use crate::backend::hamilton::star::BACKEND;
use crate::backend::hamilton::star::plan::choreograph::{
    ASSEMBLY_MIX, DILUTION_MIX, DNA_MIX, RunBuilder, TipFeeder, Transfer,
};
use crate::backend::hamilton::star::plan::constraints::{
    plate_capacity_error, validate_assembly_constraints, validate_strain_constraints,
    validate_uniform_batch_settings,
};
use crate::backend::hamilton::star::plan::error::StarPlanningError;
use crate::backend::hamilton::star::plan::execution::{
    ManualStep, SourceFill, StarAssemblyChemistry, StarAssemblyPlan, StarExecutionPlan,
    StarPlatingPlan, StarRunPlan, StarStrainChemistry, StarStrainPlan, StarTransformationPlan,
    StarWell, ThermalRequirement, TipClass,
};
use crate::backend::hamilton::star::plan::liquids::{
    AGAR_SPOT_HEIGHT_MM, DeckIndex, LiquidState, PLATE_DEAD_VOLUME_UL, TROUGH_DEAD_VOLUME_UL,
    TUBE_DEAD_VOLUME_UL,
};
use crate::backend::hamilton::star::profile::StarTargetProfile;
use crate::backend::resources::{
    PlateAllocator, assembly_source_keys, assign_source_wells, plate_wells,
    transformation_source_keys,
};
use crate::backend::trace::{AssemblyTrace, ProtocolTraces, StrainTrace, analyze_protocol};

/// Validate and allocate a STAR build against one bench without rendering
/// any output files. The returned execution plan is the single input shared
/// by the run and Markdown emitters.
pub fn plan_build(
    protocol: &ProtocolLairProgram,
    profile: &StarTargetProfile,
) -> Result<StarExecutionPlan, StarPlanningError> {
    plan_selected_build(protocol, profile, None)
}

pub(in crate::backend) fn plan_selected_build(
    protocol: &ProtocolLairProgram,
    profile: &StarTargetProfile,
    selected_artifacts: Option<&BTreeSet<String>>,
) -> Result<StarExecutionPlan, StarPlanningError> {
    profile.validate()?;
    let context = protocol.context();
    let traces = analyze_protocol(protocol, selected_artifacts)?;
    let deck = DeckIndex::build(profile)?;

    let (_, reaction_well_ul, _) = deck.vessel("reaction_plate");
    for trace in &traces.assemblies {
        validate_assembly_constraints(trace, context, reaction_well_ul)?;
    }
    for trace in &traces.strains {
        validate_strain_constraints(trace, context, reaction_well_ul)?;
    }
    validate_uniform_batch_settings(&traces.strains, context)?;

    let reaction_wells = plate_wells(profile.deck.reaction_plate.capacity);
    let assemblies = build_assembly_plans(&traces.assemblies, context, &reaction_wells)?;
    let dna_source_wells = allocate_dna_wells(&traces.strains, context, profile)?;
    let strains = build_strain_plans(
        &traces.strains,
        context,
        &reaction_wells,
        &dna_source_wells,
        profile,
    )?;
    let (assembly_source_wells, transformation_source_wells) =
        assign_stage_source_wells(&traces, context, profile.deck.source_rack.capacity)?;

    let science = Science {
        assemblies: &assemblies,
        strains: &strains,
        assembly_source_wells: &assembly_source_wells,
        transformation_source_wells: &transformation_source_wells,
        dna_source_wells: &dna_source_wells,
        medium_well: &profile.stages.plating.media_rack.medium_well,
    };

    // Pass one over an empty bench records consumption; the fills seed pass
    // two, whose heights are the ones the machine will see.
    let mut discovery = LiquidState::new();
    choreograph_program(profile, &deck, &science, &mut discovery)?;
    let source_fills = compute_fills(&deck, &science, &discovery)?;

    let mut liquids = LiquidState::new();
    for fill in &source_fills {
        liquids.seed(&fill.location, fill.load_ul);
    }
    let (runs, tip_usage) = choreograph_program(profile, &deck, &science, &mut liquids)?;

    Ok(StarExecutionPlan {
        schema_version: "lab.automation.v0".into(),
        target: BACKEND.into(),
        deck: profile.clone(),
        assembly_source_wells,
        transformation_source_wells,
        dna_source_wells,
        assemblies,
        strains,
        source_fills,
        tip_usage,
        runs,
    })
}

/// The allocated science, borrowed together for the choreography passes.
struct Science<'a> {
    assemblies: &'a [StarAssemblyPlan],
    strains: &'a [StarStrainPlan],
    assembly_source_wells: &'a BTreeMap<String, String>,
    transformation_source_wells: &'a BTreeMap<String, String>,
    dna_source_wells: &'a BTreeMap<String, StarWell>,
    /// The media vessel's well (the trough's single `A1`).
    medium_well: &'a str,
}

impl Science<'_> {
    fn assembly_source(&self, key: &str) -> StarWell {
        StarWell::new(
            "assembly_sources",
            self.assembly_source_wells
                .get(key)
                .expect("planning assigned every assembly reagent, DNA, and enzyme a source well")
                .clone(),
        )
    }

    fn transformation_source(&self, key: &str) -> StarWell {
        StarWell::new(
            "transformation_sources",
            self.transformation_source_wells
                .get(key)
                .expect("planning assigned every cells and media key a source well")
                .clone(),
        )
    }
}

/// Allocates each assembly its reaction wells and computes the water top-up
/// volume for every plasmid the batch builds.
fn build_assembly_plans(
    traces: &[AssemblyTrace],
    context: &Context,
    reaction_plate: &[String],
) -> Result<Vec<StarAssemblyPlan>, StarPlanningError> {
    let mut cursor = 0;
    let mut assemblies = Vec::new();
    for trace in traces {
        let assembly_replicates = trace.assembly_replicates(context);
        let end = cursor + usize::from(assembly_replicates);
        if end > reaction_plate.len() {
            return Err(plate_capacity_error(
                "assembly",
                "reaction_plate",
                end,
                reaction_plate.len(),
            ));
        }
        let assembly_wells = reaction_plate[cursor..end].to_vec();
        cursor = end;
        let components = trace.components(context);
        let chemistry = StarAssemblyChemistry {
            reaction_volume_ul: trace.chemistry(context, "reaction_volume_ul"),
            part_volume_ul: trace.chemistry(context, "part_volume_ul"),
            enzyme_volume_ul: trace.chemistry(context, "enzyme_volume_ul"),
            ligase_volume_ul: trace.chemistry(context, "ligase_volume_ul"),
            buffer_volume_ul: trace.chemistry(context, "buffer_volume_ul"),
            cycles: trace.chemistry(context, "cycles"),
            digest_temperature_c: trace.chemistry(context, "digest_temperature_c"),
            digest_minutes: trace.chemistry(context, "digest_minutes"),
            ligate_temperature_c: trace.chemistry(context, "ligate_temperature_c"),
            ligate_minutes: trace.chemistry(context, "ligate_minutes"),
        };
        // The backbone enters the reaction alongside every component.
        let dna_pieces = (1 + components.len()) as u16;
        let consumed = chemistry.buffer_volume_ul
            + chemistry.ligase_volume_ul
            + chemistry.enzyme_volume_ul
            + chemistry.part_volume_ul * dna_pieces;
        let water_volume_ul = chemistry
            .reaction_volume_ul
            .checked_sub(consumed)
            .expect("constraint validation rejected an over-subscribed reaction");
        assemblies.push(StarAssemblyPlan {
            artifact: trace.artifact(context),
            sequence: trace.sequence(context),
            backbone: trace.backbone(context),
            components,
            dependencies: trace.dependencies(context),
            restriction_enzyme: trace.restriction_enzyme(context),
            assembly_replicates,
            water_volume_ul,
            assembly_wells,
            chemistry,
        });
    }
    Ok(assemblies)
}

/// Gives every distinct plasmid carried by any strain one DNA-plate well.
fn allocate_dna_wells(
    traces: &[StrainTrace],
    context: &Context,
    profile: &StarTargetProfile,
) -> Result<BTreeMap<String, StarWell>, StarPlanningError> {
    let carried = traces
        .iter()
        .flat_map(|trace| trace.plasmids(context))
        .collect::<BTreeSet<_>>();
    let mut allocator = PlateAllocator::new(
        BACKEND,
        "transformation",
        "dna_plate",
        &profile.stages.transformation.dna_plate,
    );
    let wells = allocator.take(carried.len())?;
    Ok(carried
        .into_iter()
        .zip(wells)
        .map(|(plasmid, well)| {
            (
                plasmid,
                StarWell::new(format!("dna_plate/{}", well.plate + 1), well.well),
            )
        })
        .collect())
}

/// Builds each strain's transformation and serial-dilution/plating layout,
/// drawing culture wells from the shared reaction plate.
fn build_strain_plans(
    traces: &[StrainTrace],
    context: &Context,
    reaction_plate: &[String],
    dna_source_wells: &BTreeMap<String, StarWell>,
    profile: &StarTargetProfile,
) -> Result<Vec<StarStrainPlan>, StarPlanningError> {
    let mut culture_cursor = 0;
    let mut dilutions = PlateAllocator::new(
        BACKEND,
        "plating",
        "dilution_plate",
        &profile.stages.plating.dilution_plate,
    );
    let mut agar = PlateAllocator::new(
        BACKEND,
        "plating",
        "agar_plate",
        &profile.stages.plating.agar_plate,
    );
    let mut strains = Vec::new();
    for trace in traces {
        let plasmids = trace.plasmids(context);
        let source_wells = plasmids
            .iter()
            .map(|plasmid| {
                dna_source_wells
                    .get(plasmid)
                    .cloned()
                    .expect("every carried plasmid was allocated a DNA-plate well")
            })
            .collect::<Vec<_>>();
        let transformation_replicates = trace.transformation_replicates(context);
        let plating_replicates = trace.plating_replicates(context);
        let serial_dilutions = trace.serial_dilutions(context);

        let mut transformations = Vec::new();
        let mut plating = Vec::new();
        for _ in 0..transformation_replicates {
            if culture_cursor >= reaction_plate.len() {
                return Err(plate_capacity_error(
                    "transformation",
                    "reaction_plate",
                    culture_cursor + 1,
                    reaction_plate.len(),
                ));
            }
            let culture_well = reaction_plate[culture_cursor].clone();
            culture_cursor += 1;
            transformations.push(StarTransformationPlan {
                culture_well: culture_well.clone(),
                source_wells: source_wells.clone(),
            });

            let dilution_wells = dilutions
                .take(usize::from(serial_dilutions))?
                .into_iter()
                .map(|well| StarWell::new(format!("dilution_plate/{}", well.plate + 1), well.well))
                .collect::<Vec<_>>();
            let mut agar_wells = Vec::new();
            for _ in 0..serial_dilutions {
                agar_wells.push(
                    agar.take(usize::from(plating_replicates))?
                        .into_iter()
                        .map(|well| {
                            StarWell::new(format!("agar_plate/{}", well.plate + 1), well.well)
                        })
                        .collect::<Vec<_>>(),
                );
            }
            plating.push(StarPlatingPlan {
                culture_well,
                dilution_wells,
                agar_wells,
            });
        }

        strains.push(StarStrainPlan {
            artifact: trace.artifact(context),
            host: trace.host(context),
            plasmids,
            dependencies: trace.dependencies(context),
            selection: trace.selection(context),
            transformation_replicates,
            plating_replicates,
            serial_dilutions,
            transformations,
            plating,
            chemistry: StarStrainChemistry {
                cell_volume_ul: trace.chemistry(context, "cell_volume_ul"),
                dna_volume_ul: trace.chemistry(context, "dna_volume_ul"),
                recovery_volume_ul: trace.chemistry(context, "recovery_volume_ul"),
                cold_minutes: trace.chemistry(context, "cold_minutes"),
                heat_shock_temperature_c: trace.chemistry(context, "heat_shock_temperature_c"),
                heat_shock_minutes: trace.chemistry(context, "heat_shock_minutes"),
                recovery_temperature_c: trace.chemistry(context, "recovery_temperature_c"),
                recovery_minutes: trace.chemistry(context, "recovery_minutes"),
                medium_volume_ul: trace.chemistry(context, "medium_volume_ul"),
                culture_volume_ul: trace.chemistry(context, "culture_volume_ul"),
                colony_volume_ul: trace.chemistry(context, "colony_volume_ul"),
            },
        });
    }
    Ok(strains)
}

type SourceWells = BTreeMap<String, String>;

fn assign_stage_source_wells(
    traces: &ProtocolTraces,
    context: &Context,
    rack_capacity: usize,
) -> Result<(SourceWells, SourceWells), StarPlanningError> {
    let assembly = assign_source_wells(
        BACKEND,
        "assembly",
        assembly_source_keys(&traces.assemblies, context),
        rack_capacity,
    )?;
    let transformation = assign_source_wells(
        BACKEND,
        "transformation",
        transformation_source_keys(&traces.strains, context),
        rack_capacity,
    )?;
    Ok((assembly, transformation))
}

/// Lowers the whole program into runs. Deterministic: called twice, once to
/// discover consumption and once over the seeded bench.
fn choreograph_program(
    profile: &StarTargetProfile,
    deck: &DeckIndex,
    science: &Science<'_>,
    liquids: &mut LiquidState,
) -> Result<(Vec<StarRunPlan>, BTreeMap<String, usize>), StarPlanningError> {
    let mut runs = Vec::new();
    let mut tip_usage = BTreeMap::new();
    let record = |feeders: Vec<TipFeeder>, usage: &mut BTreeMap<String, usize>| {
        for feeder in feeders {
            for (resource, used) in feeder.usage() {
                usage.insert(resource, used);
            }
        }
    };

    if !science.assemblies.is_empty() {
        let mut builder = RunBuilder::new(
            profile,
            deck,
            liquids,
            Some(stage_feeder(
                "assembly_small_tips",
                deck,
                &profile.stages.assembly.small_tips,
            )),
            None,
        );
        choreograph_assembly(&mut builder, science)?;
        let (operations, feeders) = builder.finish();
        record(feeders, &mut tip_usage);
        runs.push(StarRunPlan {
            id: "assembly_run".into(),
            title: "Golden Gate assembly".into(),
            operations,
            manual_after: assembly_manual_steps(science),
            thermal_after: assembly_thermal_after(science),
        });
        // The operator moves the reactions off and stages a fresh plate, so
        // the culture wells start empty even where names repeat.
        liquids.clear_resource("reaction_plate");
    }

    if !science.strains.is_empty() {
        let mut builder = RunBuilder::new(
            profile,
            deck,
            liquids,
            Some(stage_feeder(
                "transformation_small_tips",
                deck,
                &profile.stages.transformation.small_tips,
            )),
            None,
        );
        choreograph_transformation_mix(&mut builder, science)?;
        let (operations, feeders) = builder.finish();
        record(feeders, &mut tip_usage);
        runs.push(StarRunPlan {
            id: "transformation_mix_run".into(),
            title: "Heat-shock transformation: cells and DNA".into(),
            operations,
            manual_after: transformation_mix_manual_steps(science),
            thermal_after: transformation_mix_thermal_after(science),
        });

        let mut builder = RunBuilder::new(
            profile,
            deck,
            liquids,
            None,
            Some(stage_feeder(
                "transformation_large_tips",
                deck,
                &profile.stages.transformation.large_tips,
            )),
        );
        choreograph_transformation_recovery(&mut builder, science)?;
        let (operations, feeders) = builder.finish();
        record(feeders, &mut tip_usage);
        runs.push(StarRunPlan {
            id: "transformation_recovery_run".into(),
            title: "Heat-shock transformation: recovery medium".into(),
            operations,
            manual_after: transformation_recovery_manual_steps(science),
            thermal_after: transformation_recovery_thermal_after(science),
        });

        let mut builder = RunBuilder::new(
            profile,
            deck,
            liquids,
            Some(stage_feeder(
                "plating_small_tips",
                deck,
                &profile.stages.plating.small_tips,
            )),
            Some(stage_feeder(
                "plating_large_tips",
                deck,
                &profile.stages.plating.large_tips,
            )),
        );
        choreograph_plating(&mut builder, science)?;
        let (operations, feeders) = builder.finish();
        record(feeders, &mut tip_usage);
        runs.push(StarRunPlan {
            id: "plating_run".into(),
            title: "Serial dilution and plating".into(),
            operations,
            manual_after: plating_manual_steps(science),
            thermal_after: Vec::new(),
        });
    }

    Ok((runs, tip_usage))
}

fn stage_feeder(
    prefix: &str,
    deck: &DeckIndex,
    racks: &crate::backend::profile::TipRacks,
) -> TipFeeder {
    TipFeeder::new(prefix, deck, racks.slots.len(), racks.capacity)
}

/// Assembly: reagent-major distribution — every well's share of one source
/// in one pass — then the DNA parts, then a fresh-tip mix of every
/// reaction.
fn choreograph_assembly(
    builder: &mut RunBuilder<'_>,
    science: &Science<'_>,
) -> Result<(), StarPlanningError> {
    let mut additions: BTreeMap<String, Vec<Transfer>> = BTreeMap::new();
    for construct in science.assemblies {
        let chemistry = &construct.chemistry;
        let part = f64::from(chemistry.part_volume_ul);
        let mut per_well: Vec<(String, f64)> = vec![
            (
                "reagent:nuclease_free_water".into(),
                f64::from(construct.water_volume_ul),
            ),
            (
                "reagent:T4_DNA_ligase_buffer".into(),
                f64::from(chemistry.buffer_volume_ul),
            ),
            (
                "reagent:T4_DNA_ligase".into(),
                f64::from(chemistry.ligase_volume_ul),
            ),
            (
                format!("enzyme:{}", construct.restriction_enzyme),
                f64::from(chemistry.enzyme_volume_ul),
            ),
            (format!("dna:{}", construct.backbone), part),
        ];
        per_well.extend(
            construct
                .components
                .iter()
                .map(|component| (format!("dna:{component}"), part)),
        );
        for well in &construct.assembly_wells {
            for (key, volume) in &per_well {
                additions
                    .entry(key.clone())
                    .or_default()
                    .push(Transfer::new(
                        science.assembly_source(key),
                        StarWell::new("reaction_plate", well.clone()),
                        *volume,
                    ));
            }
        }
    }
    // Water, buffer, and ligase lead in that fixed order; the remaining
    // sources follow deterministically sorted.
    for key in [
        "reagent:nuclease_free_water",
        "reagent:T4_DNA_ligase_buffer",
        "reagent:T4_DNA_ligase",
    ] {
        if let Some(transfers) = additions.remove(key) {
            builder.distribute(TipClass::Small, &transfers)?;
        }
    }
    for transfers in additions.values() {
        builder.distribute(TipClass::Small, transfers)?;
    }

    let wells: Vec<StarWell> = science
        .assemblies
        .iter()
        .flat_map(|construct| &construct.assembly_wells)
        .map(|well| StarWell::new("reaction_plate", well.clone()))
        .collect();
    builder.mix_wells(TipClass::Small, &wells, ASSEMBLY_MIX)
}

/// Transformation part one: chilled cells into every culture well, then
/// each reaction's DNA with a mix.
fn choreograph_transformation_mix(
    builder: &mut RunBuilder<'_>,
    science: &Science<'_>,
) -> Result<(), StarPlanningError> {
    let mut by_host: BTreeMap<String, Vec<Transfer>> = BTreeMap::new();
    for strain in science.strains {
        let cells = science.transformation_source(&format!("cells:{}", strain.host));
        for reaction in &strain.transformations {
            by_host
                .entry(strain.host.clone())
                .or_default()
                .push(Transfer::new(
                    cells.clone(),
                    StarWell::new("reaction_plate", reaction.culture_well.clone()),
                    f64::from(strain.chemistry.cell_volume_ul),
                ));
        }
    }
    for transfers in by_host.values() {
        builder.distribute(TipClass::Small, transfers)?;
    }

    let mut dna: Vec<Transfer> = Vec::new();
    for strain in science.strains {
        for reaction in &strain.transformations {
            for source in &reaction.source_wells {
                dna.push(
                    Transfer::new(
                        source.clone(),
                        StarWell::new("reaction_plate", reaction.culture_well.clone()),
                        f64::from(strain.chemistry.dna_volume_ul),
                    )
                    .with_mix(DNA_MIX),
                );
            }
        }
    }
    builder.plate_transfers(TipClass::Small, &dna)
}

/// Transformation part two: recovery medium into every culture well.
fn choreograph_transformation_recovery(
    builder: &mut RunBuilder<'_>,
    science: &Science<'_>,
) -> Result<(), StarPlanningError> {
    let recovery = science.transformation_source("reagent:recovery_medium");
    let transfers: Vec<Transfer> = science
        .strains
        .iter()
        .flat_map(|strain| {
            strain.transformations.iter().map(|reaction| {
                Transfer::new(
                    recovery.clone(),
                    StarWell::new("reaction_plate", reaction.culture_well.clone()),
                    f64::from(strain.chemistry.recovery_volume_ul),
                )
            })
        })
        .collect();
    builder.distribute(TipClass::Large, &transfers)
}

/// Plating: medium into every dilution well, then dilution rounds — every
/// chain advances one step, batched by column where the wells align — with
/// each round's spots multi-dispensed onto its agar wells.
fn choreograph_plating(
    builder: &mut RunBuilder<'_>,
    science: &Science<'_>,
) -> Result<(), StarPlanningError> {
    let medium: Vec<Transfer> = science
        .strains
        .iter()
        .flat_map(|strain| {
            strain.plating.iter().flat_map(move |layout| {
                layout.dilution_wells.iter().map(move |well| {
                    Transfer::new(
                        StarWell::new("media_rack", science.medium_well.to_string()),
                        well.clone(),
                        f64::from(strain.chemistry.medium_volume_ul),
                    )
                })
            })
        })
        .collect();
    builder.distribute(TipClass::Large, &medium)?;

    let rounds = science
        .strains
        .first()
        .map(|strain| usize::from(strain.serial_dilutions))
        .unwrap_or(0);
    for round in 0..rounds {
        let mut transfers = Vec::new();
        for strain in science.strains {
            for layout in &strain.plating {
                let source = if round == 0 {
                    StarWell::new("reaction_plate", layout.culture_well.clone())
                } else {
                    layout.dilution_wells[round - 1].clone()
                };
                transfers.push(
                    Transfer::new(
                        source,
                        layout.dilution_wells[round].clone(),
                        f64::from(strain.chemistry.culture_volume_ul),
                    )
                    .with_mix(DILUTION_MIX),
                );
            }
        }
        builder.plate_transfers(TipClass::Small, &transfers)?;

        for strain in science.strains {
            for layout in &strain.plating {
                let spots: Vec<Transfer> = layout.agar_wells[round]
                    .iter()
                    .map(|agar| {
                        Transfer::new(
                            layout.dilution_wells[round].clone(),
                            agar.clone(),
                            f64::from(strain.chemistry.colony_volume_ul),
                        )
                        .at_fixed_height(AGAR_SPOT_HEIGHT_MM)
                    })
                    .collect();
                builder.distribute(TipClass::Small, &spots)?;
            }
        }
    }
    Ok(())
}

/// One plateau with the device-default ramp and lid.
fn plateau(celsius: f64, hold_seconds: f64) -> lab_instruments::ThermalStep {
    lab_instruments::ThermalStep {
        celsius,
        hold_seconds,
        ramp_c_per_s: None,
        lid_celsius: None,
    }
}

/// The assembly thermal program behind the first manual step: the same
/// digest/ligate cycling and finish the prose states, as data.
fn assembly_thermal_after(science: &Science<'_>) -> Vec<ThermalRequirement> {
    let Some(first) = science.assemblies.first() else {
        return Vec::new();
    };
    let chemistry = &first.chemistry;
    vec![ThermalRequirement {
        id: "assembly_thermocycle".into(),
        title: "Thermocycle the assembly reactions".into(),
        plate: "reaction_plate".into(),
        profile: lab_instruments::ThermalProfile {
            stages: vec![
                lab_instruments::ThermalStage {
                    steps: vec![
                        plateau(
                            f64::from(chemistry.digest_temperature_c),
                            f64::from(chemistry.digest_minutes) * 60.0,
                        ),
                        plateau(
                            f64::from(chemistry.ligate_temperature_c),
                            f64::from(chemistry.ligate_minutes) * 60.0,
                        ),
                    ],
                    repeats: u32::from(chemistry.cycles),
                },
                lab_instruments::ThermalStage {
                    steps: vec![plateau(50.0, 300.0), plateau(80.0, 600.0)],
                    repeats: 1,
                },
            ],
        },
        final_hold_celsius: Some(4.0),
        fill_volume_ul: f64::from(chemistry.reaction_volume_ul),
        fallback_index: 0,
    }]
}

/// The cold-hold and heat-shock program behind the transformation mix's
/// manual step: ice approximated as a 4 °C block hold.
fn transformation_mix_thermal_after(science: &Science<'_>) -> Vec<ThermalRequirement> {
    let Some(first) = science.strains.first() else {
        return Vec::new();
    };
    let chemistry = &first.chemistry;
    vec![ThermalRequirement {
        id: "transformation_heat_shock".into(),
        title: "Cold hold and heat shock".into(),
        plate: "reaction_plate".into(),
        profile: lab_instruments::ThermalProfile {
            stages: vec![lab_instruments::ThermalStage {
                steps: vec![
                    plateau(4.0, f64::from(chemistry.cold_minutes) * 60.0),
                    plateau(
                        f64::from(chemistry.heat_shock_temperature_c),
                        f64::from(chemistry.heat_shock_minutes) * 60.0,
                    ),
                    plateau(4.0, 120.0),
                ],
                repeats: 1,
            }],
        },
        final_hold_celsius: Some(4.0),
        fill_volume_ul: f64::from(chemistry.cell_volume_ul) + f64::from(chemistry.dna_volume_ul),
        fallback_index: 0,
    }]
}

/// The recovery incubation behind the recovery run's manual step: a single
/// warm hold.
fn transformation_recovery_thermal_after(science: &Science<'_>) -> Vec<ThermalRequirement> {
    let Some(first) = science.strains.first() else {
        return Vec::new();
    };
    let chemistry = &first.chemistry;
    vec![ThermalRequirement {
        id: "transformation_recovery".into(),
        title: "Recovery incubation".into(),
        plate: "reaction_plate".into(),
        profile: lab_instruments::ThermalProfile {
            stages: vec![lab_instruments::ThermalStage {
                steps: vec![plateau(
                    f64::from(chemistry.recovery_temperature_c),
                    f64::from(chemistry.recovery_minutes) * 60.0,
                )],
                repeats: 1,
            }],
        },
        final_hold_celsius: None,
        fill_volume_ul: f64::from(chemistry.cell_volume_ul)
            + f64::from(chemistry.dna_volume_ul)
            + f64::from(chemistry.recovery_volume_ul),
        fallback_index: 0,
    }]
}

fn assembly_manual_steps(science: &Science<'_>) -> Vec<ManualStep> {
    let Some(first) = science.assemblies.first() else {
        return Vec::new();
    };
    let chemistry = &first.chemistry;
    let mut steps = vec![ManualStep {
        title: "Thermocycle the assembly reactions off-deck".into(),
        instructions: format!(
            "Seal the reaction plate and thermocycle: {} cycles of {} °C for {} min then {} °C for {} min; finish with 50 °C for 5 min and 80 °C for 10 min; hold at 4 °C. Every assembly in the batch shares this profile.",
            chemistry.cycles,
            chemistry.digest_temperature_c,
            chemistry.digest_minutes,
            chemistry.ligate_temperature_c,
            chemistry.ligate_minutes,
        ),
    }];
    if !science.strains.is_empty() {
        steps.push(ManualStep {
            title: "Stage the DNA plate and a fresh reaction plate".into(),
            instructions: "Transfer each assembly product to its DNA-plate well (see the automation manifest's dna_source_wells), load retrieved plasmids likewise, then place a fresh reaction plate before the transformation run.".into(),
        });
    }
    steps
}

fn transformation_mix_manual_steps(science: &Science<'_>) -> Vec<ManualStep> {
    let Some(first) = science.strains.first() else {
        return Vec::new();
    };
    let chemistry = &first.chemistry;
    vec![ManualStep {
        title: "Cold hold and heat shock off-deck".into(),
        instructions: format!(
            "Hold the reaction plate on ice for {} min, heat-shock at {} °C for {} min in a water bath, then return it to ice for 2 min before the recovery run. Every strain in the batch shares this profile.",
            chemistry.cold_minutes,
            chemistry.heat_shock_temperature_c,
            chemistry.heat_shock_minutes,
        ),
    }]
}

fn transformation_recovery_manual_steps(science: &Science<'_>) -> Vec<ManualStep> {
    let Some(first) = science.strains.first() else {
        return Vec::new();
    };
    let chemistry = &first.chemistry;
    vec![ManualStep {
        title: "Recovery incubation off-deck".into(),
        instructions: format!(
            "Incubate the reaction plate at {} °C for {} min, then return it to the deck for the plating run.",
            chemistry.recovery_temperature_c, chemistry.recovery_minutes,
        ),
    }]
}

fn plating_manual_steps(science: &Science<'_>) -> Vec<ManualStep> {
    if science.strains.is_empty() {
        return Vec::new();
    }
    vec![ManualStep {
        title: "Incubate the selective agar".into(),
        instructions:
            "Incubate the spotted agar plates under host-appropriate conditions and record colony counts per dilution."
                .into(),
    }]
}

/// The fills: everything pass one drew from operator-loaded positions, plus
/// each vessel's dead volume, checked against its working volume.
fn compute_fills(
    deck: &DeckIndex,
    science: &Science<'_>,
    discovery: &LiquidState,
) -> Result<Vec<SourceFill>, StarPlanningError> {
    let mut fills = Vec::new();
    let mut push = |key: String, location: StarWell, dead: f64| -> Result<(), StarPlanningError> {
        let consumed = discovery
            .drawn()
            .get(&(location.resource.clone(), location.well.clone()))
            .copied()
            .unwrap_or(0.0);
        let load = consumed + dead;
        let (_, working_ul, _) = deck.vessel(&location.resource);
        if load > working_ul {
            return Err(crate::backend::TargetConstraintError::CapacityExceeded {
                target: BACKEND.into(),
                operation: "source_loading".into(),
                subject: key.clone(),
                resource: location.resource.clone(),
                required: load.ceil() as u64,
                capacity: working_ul as u64,
                unit: "uL".into(),
            }
            .into());
        }
        fills.push(SourceFill {
            key,
            location,
            consumed_ul: consumed,
            load_ul: load,
        });
        Ok(())
    };

    for (key, well) in science.assembly_source_wells {
        push(
            key.clone(),
            StarWell::new("assembly_sources", well.clone()),
            TUBE_DEAD_VOLUME_UL,
        )?;
    }
    for (key, well) in science.transformation_source_wells {
        push(
            key.clone(),
            StarWell::new("transformation_sources", well.clone()),
            TUBE_DEAD_VOLUME_UL,
        )?;
    }
    for (plasmid, well) in science.dna_source_wells {
        push(
            format!("plasmid:{plasmid}"),
            well.clone(),
            PLATE_DEAD_VOLUME_UL,
        )?;
    }
    if !science.strains.is_empty() {
        push(
            "medium".into(),
            StarWell::new("media_rack", "A1"),
            TROUGH_DEAD_VOLUME_UL,
        )?;
    }
    Ok(fills)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::hamilton::star::plan::execution::StarOperation;
    use crate::test_support::golden_gate_protocol;

    #[test]
    fn allocates_both_stages_against_the_reference_bench() {
        let protocol = golden_gate_protocol();
        let plan = plan_build(&protocol, &StarTargetProfile::default())
            .expect("the example compiles for the reference bench");
        assert_eq!(plan.target, "hamilton.star");
        assert_eq!(plan.schema_version, "lab.automation.v0");
        assert_eq!(plan.assemblies.len(), 2, "two plasmids are assembled");
        assert_eq!(plan.strains.len(), 4, "one plasmid feeds two chassis");
        assert_eq!(plan.assemblies[0].assembly_wells, ["A1"]);
        assert!(
            plan.assembly_source_wells
                .contains_key("reagent:nuclease_free_water"),
            "assembly reagents share the source rack"
        );
        assert_eq!(
            plan.runs.len(),
            4,
            "assembly, transformation mix, recovery, and plating each get a run"
        );
    }

    #[test]
    fn source_fills_cover_consumption_plus_dead_volume() {
        let protocol = golden_gate_protocol();
        let plan =
            plan_build(&protocol, &StarTargetProfile::default()).expect("the example compiles");
        let water = plan
            .source_fills
            .iter()
            .find(|fill| fill.key == "reagent:nuclease_free_water")
            .expect("water is a planned source");
        assert!(
            water.consumed_ul > 0.0,
            "the assembly run draws water: {water:?}"
        );
        assert!(
            (water.load_ul - water.consumed_ul - TUBE_DEAD_VOLUME_UL).abs() < 1e-9,
            "the operator loads consumption plus the tube dead volume"
        );
    }

    #[test]
    fn every_run_returns_its_tips_to_the_waste() {
        let protocol = golden_gate_protocol();
        let plan =
            plan_build(&protocol, &StarTargetProfile::default()).expect("the example compiles");
        for run in &plan.runs {
            let picked: usize = run
                .operations
                .iter()
                .map(|operation| match operation {
                    StarOperation::PickUpTips { positions, .. } => positions.len(),
                    _ => 0,
                })
                .sum();
            let discarded: usize = run
                .operations
                .iter()
                .map(|operation| match operation {
                    StarOperation::DiscardTips { channels } => channels.len(),
                    _ => 0,
                })
                .sum();
            assert_eq!(
                picked, discarded,
                "run {} picks and discards the same tip count",
                run.id
            );
        }
    }

    #[test]
    fn tip_usage_stays_within_every_rack() {
        let protocol = golden_gate_protocol();
        let plan =
            plan_build(&protocol, &StarTargetProfile::default()).expect("the example compiles");
        for (resource, used) in &plan.tip_usage {
            assert!(
                *used <= 96,
                "rack {resource} reports {used} tips against its 96 capacity"
            );
        }
    }

    #[test]
    fn an_agar_capacity_that_contradicts_the_catalog_is_a_profile_error() {
        let protocol = golden_gate_protocol();
        let mut wrong = StarTargetProfile::default();
        wrong.stages.plating.agar_plate.capacity = 15;
        let error =
            plan_build(&protocol, &wrong).expect_err("the catalog plate holds 96 wells, not 15");
        let message = error.to_string();
        assert!(
            message.contains("15") && message.contains("96"),
            "the error carries both the declared and the actual capacity: {message}"
        );
    }
}
