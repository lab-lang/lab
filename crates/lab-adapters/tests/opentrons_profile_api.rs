//! External-crate coverage for the public Opentrons profile construction surface.

use lab_adapters::opentrons::{flex, ot2};

#[test]
fn every_ot2_profile_field_type_is_publicly_nameable() {
    let profile = ot2::Ot2AdapterProfile::parse("external-ot2", "").unwrap();

    let _: &ot2::profile::ProtocolOptions = &profile.protocol;
    let instruments: &ot2::profile::Instruments = &profile.instruments;
    let _: &ot2::profile::Pipette = &instruments.small;
    let _: &ot2::profile::TechniqueCalibration = &profile.techniques;

    let resources: &ot2::profile::Ot2Resources = &profile.resources;
    let sources: &ot2::profile::TemperatureModule = &resources.sources;
    let _: &ot2::profile::Thermocycler = &resources.work;
    let _: &ot2::profile::TipRacks = &resources.small_tips;
    let capacity: ot2::profile::PlateCapacity = sources.capacity;
    assert_eq!(capacity.get(), 24);

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

    let resources: &flex::profile::FlexResources = &profile.resources;
    let sources: &flex::profile::TemperatureModule = &resources.sources;
    let _: &flex::profile::Thermocycler = &resources.work;
    let _: &flex::profile::TipRacks = &resources.large_tips;
    let _: &flex::profile::Trash = &resources.trash;
    let capacity: flex::profile::PlateCapacity = sources.capacity;
    assert_eq!(capacity.get(), 24);

    let error: flex::profile::UnknownPlateGeometry =
        flex::profile::PlateCapacity::new(1).unwrap_err();
    assert_eq!(error.found, 1);
    assert!(flex::profile::supported_plate_capacities().contains(&96));
}
