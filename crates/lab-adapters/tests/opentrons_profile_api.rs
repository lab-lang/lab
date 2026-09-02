//! External-crate coverage for the public Opentrons profile construction surface.

use lab_adapters::opentrons::{flex, ot2};

#[test]
fn every_ot2_profile_field_type_is_publicly_nameable() {
    let profile = ot2::Ot2AdapterProfile::parse("external-ot2", "").unwrap();

    let _: &ot2::profile::ProtocolOptions = &profile.protocol;
    let instruments: &ot2::profile::Instruments = &profile.instruments;
    let _: &ot2::profile::Pipette = &instruments.small;
    let _: &ot2::profile::TechniqueCalibration = &profile.techniques;

    let deck: &ot2::profile::SharedDeck = &profile.deck;
    let temperature_module: &ot2::profile::TemperatureModule = &deck.temperature_module;
    let _: &ot2::profile::Thermocycler = &deck.thermocycler;
    let capacity: ot2::profile::PlateCapacity = temperature_module.capacity;
    assert_eq!(capacity.get(), 24);

    let stages: &ot2::profile::Stages = &profile.stages;
    let assembly: &ot2::profile::AssemblyStage = &stages.assembly;
    let _: &ot2::profile::TipRacks = &assembly.small_tips;
    let transformation: &ot2::profile::TransformationStage = &stages.transformation;
    let _: &ot2::profile::Plates = &transformation.dna_plate;
    let _: &ot2::profile::SourceRack = &transformation.source_rack;
    let plating: &ot2::profile::PlatingStage = &stages.plating;
    let _: &ot2::profile::MediaRack = &plating.media_rack;

    let error: ot2::profile::UnknownPlateGeometry =
        ot2::profile::PlateCapacity::new(1).unwrap_err();
    assert_eq!(error.found, 1);
    assert!(ot2::profile::supported_plate_capacities().contains(&96));
}

#[test]
fn every_flex_profile_field_type_is_publicly_nameable() {
    let profile = flex::FlexAdapterProfile::parse("external-flex", "").unwrap();

    let instruments: &flex::profile::Instruments = &profile.instruments;
    let _: &flex::profile::Pipette = &instruments.small;
    let _: &flex::profile::FlexTechniqueCalibration = &profile.techniques;

    let deck: &flex::profile::FlexDeck = &profile.deck;
    let temperature_module: &flex::profile::TemperatureModule = &deck.temperature_module;
    let _: &flex::profile::Thermocycler = &deck.thermocycler;
    let _: &flex::profile::Trash = &deck.trash;
    let capacity: flex::profile::PlateCapacity = temperature_module.capacity;
    assert_eq!(capacity.get(), 24);

    let stages: &flex::profile::Stages = &profile.stages;
    let assembly: &flex::profile::AssemblyStage = &stages.assembly;
    let _: &flex::profile::TipRacks = &assembly.small_tips;
    let transformation: &flex::profile::TransformationStage = &stages.transformation;
    let _: &flex::profile::Plates = &transformation.dna_plate;
    let plating: &flex::profile::PlatingStage = &stages.plating;
    let _: &flex::profile::MediaRack = &plating.media_rack;

    let error: flex::profile::UnknownPlateGeometry =
        flex::profile::PlateCapacity::new(1).unwrap_err();
    assert_eq!(error.found, 1);
    assert!(flex::profile::supported_plate_capacities().contains(&96));
}
