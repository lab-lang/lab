//! JSON protocol authoring for the three build stages.
//!
//! Each stage renders one Opentrons JSON protocol (schema v8) through the
//! checked `opentrons-protocol` builder, reproducing the liquid-handling
//! choreography of the OT-2 Python protocols: reagent additions with a fresh
//! tip per transfer, thermocycler digest/ligate cycling, cold-hold heat-shock
//! transformation, and serial dilution with agar spotting. Plates stay in
//! place across the three runs — the thermocycler plate carries reactions
//! from assembly through plating — so no gripper move is needed inside a
//! stage; between-run handling belongs to the manual protocol.

use opentrons_protocol::schema::{Metadata, WellLocation, WellOrigin};
use opentrons_protocol::{
    FlexPipetteName, FlexProtocolBuilder, FlexSlot, LabwareId, ModuleId, PipetteId, PipetteMount,
    ProtocolError, TemperatureModule, Thermocycler, standard_definition,
};

use crate::backend::opentrons::flex::plan::{FlexEmissionError, FlexExecutionPlan};
use crate::backend::opentrons::flex::profile::{FlexTargetProfile, TipRacks};
use crate::backend::resources::plate_wells;

pub(in crate::backend::opentrons::flex) fn render_assembly_protocol(
    plan: &FlexExecutionPlan,
) -> Result<String, FlexEmissionError> {
    let profile = &plan.deck;
    let deck = &profile.deck;
    let stage = &profile.stages.assembly;
    let mut builder = stage_builder(profile, "Lab Golden Gate assembly");

    let temperature =
        builder.load_module::<TemperatureModule>(slot(&deck.temperature_module.slot))?;
    let sources = builder.load_labware_on_module(&deck.temperature_module.labware, temperature)?;
    let thermocycler = builder.load_module::<Thermocycler>(FlexSlot::B1)?;
    let reaction_plate =
        builder.load_labware_on_module(&deck.thermocycler.labware, thermocycler)?;
    let mut tips = TipFeeder::load(&mut builder, &stage.small_tips)?;
    let pipette = load_instrument(&mut builder, &profile.instruments.small)?;

    builder.temperature_module_set_target(temperature, 4.0)?;
    builder.temperature_module_wait_for_temperature(temperature)?;
    builder.thermocycler_open_lid(thermocycler);

    for construct in &plan.assemblies {
        let chemistry = &construct.chemistry;
        let part_volume = f64::from(chemistry.part_volume_ul);
        let mut additions: Vec<(String, f64)> = vec![
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
            (format!("dna:{}", construct.backbone), part_volume),
        ];
        additions.extend(
            construct
                .components
                .iter()
                .map(|component| (format!("dna:{component}"), part_volume)),
        );
        for destination in &construct.assembly_wells {
            for (source_key, volume) in &additions {
                let source_well = source_well(plan, Stage::Assembly, source_key);
                transfer(
                    &mut builder,
                    &mut tips,
                    &pipette,
                    (sources, source_well),
                    (reaction_plate, destination),
                    *volume,
                    None,
                    None,
                )?;
            }
            let (rack, well) = tips.next();
            builder.pick_up_tip(pipette.id, rack, &well)?;
            builder.mix(
                pipette.id,
                reaction_plate,
                destination,
                3,
                15.0,
                pipette.flow_rate,
            )?;
            builder.drop_tip_into_trash(pipette.id)?;
        }
    }

    // Every assembly in a batch shares one thermal profile, so the first
    // construct's chemistry drives the block. The digest/ligate cycles and
    // the final 50 C / 80 C soaks run as one unrolled profile.
    if let Some(construct) = plan.assemblies.first() {
        let chemistry = &construct.chemistry;
        builder.thermocycler_close_lid(thermocycler);
        builder.thermocycler_set_lid_temperature(thermocycler, 105.0)?;
        builder.thermocycler_wait_for_lid_temperature(thermocycler)?;
        let mut steps = Vec::with_capacity(usize::from(chemistry.cycles) * 2 + 2);
        for _ in 0..chemistry.cycles {
            steps.push((
                f64::from(chemistry.digest_temperature_c),
                f64::from(chemistry.digest_minutes) * 60.0,
            ));
            steps.push((
                f64::from(chemistry.ligate_temperature_c),
                f64::from(chemistry.ligate_minutes) * 60.0,
            ));
        }
        steps.push((50.0, 300.0));
        steps.push((80.0, 600.0));
        builder.thermocycler_run_profile(
            thermocycler,
            &steps,
            Some(f64::from(chemistry.reaction_volume_ul)),
        )?;
        builder.thermocycler_set_block_temperature(thermocycler, 4.0, None, None)?;
        builder.thermocycler_deactivate_lid(thermocycler);
        builder.thermocycler_open_lid(thermocycler);
    }
    builder.comment(
        "Assembly complete. Preserve the reaction plate for transformation_protocol.json.",
    );
    render(builder)
}

pub(in crate::backend::opentrons::flex) fn render_transformation_protocol(
    plan: &FlexExecutionPlan,
) -> Result<String, FlexEmissionError> {
    let profile = &plan.deck;
    let deck = &profile.deck;
    let stage = &profile.stages.transformation;
    let mut builder = stage_builder(profile, "Lab heat-shock transformation");

    let temperature =
        builder.load_module::<TemperatureModule>(slot(&deck.temperature_module.slot))?;
    let sources = builder.load_labware_on_module(&deck.temperature_module.labware, temperature)?;
    let thermocycler = builder.load_module::<Thermocycler>(FlexSlot::B1)?;
    let reaction_plate =
        builder.load_labware_on_module(&deck.thermocycler.labware, thermocycler)?;
    let dna_plates = load_plates(
        &mut builder,
        &stage.dna_plate.labware,
        &stage.dna_plate.slots,
    )?;
    let mut small_tips = TipFeeder::load(&mut builder, &stage.small_tips)?;
    let mut large_tips = TipFeeder::load(&mut builder, &stage.large_tips)?;
    let small = load_instrument(&mut builder, &profile.instruments.small)?;
    let large = load_instrument(&mut builder, &profile.instruments.large)?;

    builder.temperature_module_set_target(temperature, 4.0)?;
    builder.temperature_module_wait_for_temperature(temperature)?;
    builder.thermocycler_open_lid(thermocycler);

    for construct in &plan.strains {
        let chemistry = &construct.chemistry;
        let cells_well = source_well(
            plan,
            Stage::Transformation,
            &format!("cells:{}", construct.host),
        );
        for reaction in &construct.transformations {
            transfer(
                &mut builder,
                &mut small_tips,
                &small,
                (sources, cells_well),
                (reaction_plate, &reaction.culture_well),
                f64::from(chemistry.cell_volume_ul),
                None,
                None,
            )?;
            for source in &reaction.source_wells {
                transfer(
                    &mut builder,
                    &mut small_tips,
                    &small,
                    (dna_plates[source.plate], &source.well),
                    (reaction_plate, &reaction.culture_well),
                    f64::from(chemistry.dna_volume_ul),
                    Some((3, 15.0)),
                    None,
                )?;
            }
        }
    }

    // Every strain in a batch shares one heat-shock profile.
    if let Some(strain) = plan.strains.first() {
        let shock = &strain.chemistry;
        builder.thermocycler_close_lid(thermocycler);
        hold_block(
            &mut builder,
            thermocycler,
            4.0,
            f64::from(shock.cold_minutes) * 60.0,
        )?;
        hold_block(
            &mut builder,
            thermocycler,
            f64::from(shock.heat_shock_temperature_c),
            f64::from(shock.heat_shock_minutes) * 60.0,
        )?;
        hold_block(&mut builder, thermocycler, 4.0, 120.0)?;
        builder.thermocycler_open_lid(thermocycler);

        let recovery_well = source_well(plan, Stage::Transformation, "reagent:recovery_medium");
        for construct in &plan.strains {
            for reaction in &construct.transformations {
                transfer(
                    &mut builder,
                    &mut large_tips,
                    &large,
                    (sources, recovery_well),
                    (reaction_plate, &reaction.culture_well),
                    f64::from(construct.chemistry.recovery_volume_ul),
                    None,
                    None,
                )?;
            }
        }
        builder.thermocycler_close_lid(thermocycler);
        hold_block(
            &mut builder,
            thermocycler,
            f64::from(shock.recovery_temperature_c),
            f64::from(shock.recovery_minutes) * 60.0,
        )?;
        builder.thermocycler_set_block_temperature(thermocycler, 4.0, None, None)?;
        builder.thermocycler_open_lid(thermocycler);
    }
    builder
        .comment("Transformation complete. Preserve the reaction plate for plating_protocol.json.");
    render(builder)
}

pub(in crate::backend::opentrons::flex) fn render_plating_protocol(
    plan: &FlexExecutionPlan,
) -> Result<String, FlexEmissionError> {
    let profile = &plan.deck;
    let deck = &profile.deck;
    let stage = &profile.stages.plating;
    let mut builder = stage_builder(profile, "Lab serial dilution and plating");

    let thermocycler = builder.load_module::<Thermocycler>(FlexSlot::B1)?;
    let cultures = builder.load_labware_on_module(&deck.thermocycler.labware, thermocycler)?;
    let mut small_tips = TipFeeder::load(&mut builder, &stage.small_tips)?;
    let mut large_tips = TipFeeder::load(&mut builder, &stage.large_tips)?;
    let small = load_instrument(&mut builder, &profile.instruments.small)?;
    let large = load_instrument(&mut builder, &profile.instruments.large)?;
    let dilution_plates = load_plates(
        &mut builder,
        &stage.dilution_plate.labware,
        &stage.dilution_plate.slots,
    )?;
    let agar_plates = load_plates(
        &mut builder,
        &stage.agar_plate.labware,
        &stage.agar_plate.slots,
    )?;
    let media_rack =
        builder.load_labware(&stage.media_rack.labware, slot(&stage.media_rack.slot))?;

    builder.thermocycler_set_block_temperature(thermocycler, 4.0, None, None)?;
    builder.thermocycler_wait_for_block_temperature(thermocycler)?;
    builder.thermocycler_open_lid(thermocycler);

    if let Some(strain) = plan.strains.first() {
        // The dilution-well pre-load happens once for the whole batch with a
        // single tip, multi-dispensing as many wells per aspirate as the
        // working volume allows.
        let medium_volume = f64::from(strain.chemistry.medium_volume_ul);
        let all_dilution_wells: Vec<(LabwareId, String)> = plan
            .strains
            .iter()
            .flat_map(|construct| &construct.plating)
            .flat_map(|layout| &layout.dilution_wells)
            .map(|well| (dilution_plates[well.plate], well.well.clone()))
            .collect();
        let (rack, well) = large_tips.next();
        builder.pick_up_tip(large.id, rack, &well)?;
        let working = large.max_volume.min(large_tips.tip_volume);
        let wells_per_aspirate = ((working / medium_volume).floor() as usize).max(1);
        for chunk in all_dilution_wells.chunks(wells_per_aspirate) {
            builder.aspirate(
                large.id,
                media_rack,
                &stage.media_rack.medium_well,
                medium_volume * chunk.len() as f64,
                large.flow_rate,
                None,
            )?;
            for (plate, well) in chunk {
                builder.dispense(large.id, *plate, well, medium_volume, large.flow_rate, None)?;
            }
        }
        builder.drop_tip_into_trash(large.id)?;
    }

    for construct in &plan.strains {
        let chemistry = &construct.chemistry;
        builder.comment(&format!(
            "Plating {} on {}",
            construct.artifact, construct.selection
        ));
        for layout in &construct.plating {
            let mut source: (LabwareId, String) = (cultures, layout.culture_well.clone());
            for (dilution_index, dilution_well) in layout.dilution_wells.iter().enumerate() {
                let dilution = (
                    dilution_plates[dilution_well.plate],
                    dilution_well.well.clone(),
                );
                transfer(
                    &mut builder,
                    &mut small_tips,
                    &small,
                    (source.0, &source.1),
                    (dilution.0, &dilution.1),
                    f64::from(chemistry.culture_volume_ul),
                    Some((5, 19.0)),
                    None,
                )?;
                for agar_well in &layout.agar_wells[dilution_index] {
                    // Spotting dispenses 8 mm below the well top so the drop
                    // lands on the agar surface rather than the rim.
                    transfer(
                        &mut builder,
                        &mut small_tips,
                        &small,
                        (dilution.0, &dilution.1),
                        (agar_plates[agar_well.plate], &agar_well.well),
                        f64::from(chemistry.colony_volume_ul),
                        None,
                        Some(WellLocation::with_offset(WellOrigin::Top, 0.0, 0.0, -8.0)),
                    )?;
                }
                source = dilution;
            }
        }
    }

    builder.comment(
        "Plating complete. Incubate the selective agar under host-appropriate conditions.",
    );
    render(builder)
}

/// Which stage's source-rack layout a reagent key resolves against.
#[derive(Clone, Copy)]
enum Stage {
    Assembly,
    Transformation,
}

/// A loaded pipette together with the numbers transfers plan around: its
/// default flow rate and its maximum volume. The working volume of one
/// transfer is this maximum capped by the tip in use.
struct Instrument {
    id: PipetteId,
    flow_rate: f64,
    max_volume: f64,
}

fn stage_builder(profile: &FlexTargetProfile, protocol_name: &str) -> FlexProtocolBuilder {
    FlexProtocolBuilder::with_trash(
        Metadata {
            protocol_name: Some(protocol_name.into()),
            author: Some("Lab Compiler".into()),
            description: Some("Generated concept protocol".into()),
            ..Metadata::default()
        },
        profile.trash_area(),
    )
}

fn slot(name: &str) -> FlexSlot {
    FlexSlot::parse(name).expect("profile validation accepted only Flex slot names")
}

fn source_well<'plan>(plan: &'plan FlexExecutionPlan, stage: Stage, key: &str) -> &'plan str {
    let wells = match stage {
        Stage::Assembly => &plan.assembly_source_wells,
        Stage::Transformation => &plan.transformation_source_wells,
    };
    wells
        .get(key)
        .expect("planning assigned every reagent, DNA, enzyme, and cell source a rack well")
}

fn load_instrument(
    builder: &mut FlexProtocolBuilder,
    pipette: &crate::backend::opentrons::flex::profile::Pipette,
) -> Result<Instrument, FlexEmissionError> {
    let name = FlexPipetteName::parse(&pipette.model)
        .expect("profile validation accepted only Flex pipette models");
    let mount = match pipette.mount.as_str() {
        "left" => PipetteMount::Left,
        "right" => PipetteMount::Right,
        _ => unreachable!("profile validation accepted only left and right mounts"),
    };
    let id = builder.load_pipette(name, mount)?;
    Ok(Instrument {
        id,
        flow_rate: name.default_flow_rate_ul_s(),
        max_volume: name.max_volume_ul(),
    })
}

fn load_plates(
    builder: &mut FlexProtocolBuilder,
    labware: &str,
    slots: &[String],
) -> Result<Vec<LabwareId>, FlexEmissionError> {
    slots
        .iter()
        .map(|name| Ok(builder.load_labware(labware, slot(name))?))
        .collect()
}

/// Hands out tip-rack wells across every rack a stage declares, in the same
/// column-major order planning uses for plates.
struct TipFeeder {
    racks: Vec<LabwareId>,
    wells: Vec<String>,
    tip_volume: f64,
    cursor: usize,
}

impl TipFeeder {
    fn load(
        builder: &mut FlexProtocolBuilder,
        racks: &TipRacks,
    ) -> Result<Self, FlexEmissionError> {
        let loaded = load_plates(builder, &racks.labware, &racks.slots)?;
        let tip_volume = standard_definition(&racks.labware)
            .and_then(|definition| definition.well_volume_ul("A1"))
            .expect("loading the tip racks verified an embedded definition with wells");
        Ok(Self {
            racks: loaded,
            wells: plate_wells(racks.capacity),
            tip_volume,
            cursor: 0,
        })
    }

    fn next(&mut self) -> (LabwareId, String) {
        let rack = self.racks[self.cursor / self.wells.len()];
        let well = self.wells[self.cursor % self.wells.len()].clone();
        self.cursor += 1;
        (rack, well)
    }
}

/// One OT-2-style `transfer(new_tip="always")`: a fresh tip, the volume split
/// into equal chunks no larger than the working volume, an optional
/// destination mix with the same tip, and the tip dropped into the trash.
#[expect(
    clippy::too_many_arguments,
    reason = "a transfer names its bench: pipette, tips, endpoints, volume, and shape"
)]
fn transfer(
    builder: &mut FlexProtocolBuilder,
    tips: &mut TipFeeder,
    instrument: &Instrument,
    source: (LabwareId, &str),
    destination: (LabwareId, &str),
    volume: f64,
    mix_after: Option<(u32, f64)>,
    dispense_location: Option<WellLocation>,
) -> Result<(), ProtocolError> {
    let (rack, well) = tips.next();
    builder.pick_up_tip(instrument.id, rack, &well)?;
    let working = instrument.max_volume.min(tips.tip_volume);
    let chunks = (volume / working).ceil().max(1.0);
    let chunk_volume = volume / chunks;
    for _ in 0..chunks as usize {
        builder.aspirate(
            instrument.id,
            source.0,
            source.1,
            chunk_volume,
            instrument.flow_rate,
            None,
        )?;
        builder.dispense(
            instrument.id,
            destination.0,
            destination.1,
            chunk_volume,
            instrument.flow_rate,
            dispense_location,
        )?;
    }
    if let Some((repetitions, mix_volume)) = mix_after {
        builder.mix(
            instrument.id,
            destination.0,
            destination.1,
            repetitions,
            mix_volume.min(working),
            instrument.flow_rate,
        )?;
    }
    builder.drop_tip_into_trash(instrument.id)?;
    Ok(())
}

/// Set the block target, wait until it is reached, then hold for a duration.
fn hold_block(
    builder: &mut FlexProtocolBuilder,
    thermocycler: ModuleId<Thermocycler>,
    celsius: f64,
    seconds: f64,
) -> Result<(), ProtocolError> {
    builder.thermocycler_set_block_temperature(thermocycler, celsius, None, None)?;
    builder.thermocycler_wait_for_block_temperature(thermocycler)?;
    builder.wait_for_duration(seconds, None);
    Ok(())
}

fn render(builder: FlexProtocolBuilder) -> Result<String, FlexEmissionError> {
    builder
        .build()
        .to_json_pretty()
        .map_err(|error| FlexEmissionError::Serialization(error.to_string()))
}
