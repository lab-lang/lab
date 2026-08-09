//! Construction-time enforcement of every semantic rule the protocol engine
//! checks during analysis. Each rejection asserts the exact error.

use crate::v8::builder::*;

const TIPRACK_50: &str = "opentrons_flex_96_tiprack_50ul";
const TIPRACK_1000: &str = "opentrons_flex_96_tiprack_1000ul";
const PCR_PLATE: &str = "nest_96_wellplate_100ul_pcr_full_skirt";

fn builder() -> FlexProtocolBuilder {
    FlexProtocolBuilder::new(Metadata::default())
}

/// A bench with a p50, a tip rack in C2, and a PCR plate in D2, with a tip
/// already attached.
fn bench_with_tip() -> (FlexProtocolBuilder, PipetteId, LabwareId, LabwareId) {
    let mut builder = builder();
    let pipette = builder
        .load_pipette(FlexPipetteName::P50Single, PipetteMount::Left)
        .unwrap();
    let tips = builder.load_labware(TIPRACK_50, FlexSlot::C2).unwrap();
    let plate = builder.load_labware(PCR_PLATE, FlexSlot::D2).unwrap();
    builder.pick_up_tip(pipette, tips, "A1").unwrap();
    (builder, pipette, tips, plate)
}

#[test]
fn a_mount_holds_one_pipette() {
    let mut builder = builder();
    builder
        .load_pipette(FlexPipetteName::P50Single, PipetteMount::Left)
        .unwrap();
    let error = builder
        .load_pipette(FlexPipetteName::P1000Single, PipetteMount::Left)
        .expect_err("the left mount is already occupied");
    assert_eq!(
        error,
        ProtocolError::MountOccupied {
            mount: "left".into(),
            existing: "p50_single_flex".into(),
        }
    );
}

#[test]
fn the_96_channel_loads_on_the_left_mount_only() {
    let error = builder()
        .load_pipette(FlexPipetteName::P1000Channel96, PipetteMount::Right)
        .expect_err("the 96-channel spans both mounts and loads as left");
    assert_eq!(error, ProtocolError::NinetySixChannelNeedsLeftMount);
}

#[test]
fn a_slot_holds_one_item() {
    let mut builder = builder();
    builder.load_labware(PCR_PLATE, FlexSlot::D2).unwrap();
    let error = builder
        .load_labware(TIPRACK_50, FlexSlot::D2)
        .expect_err("slot D2 already holds the plate");
    assert_eq!(
        error,
        ProtocolError::SlotOccupied {
            slot: "D2".into(),
            occupant: "labware 'labware-0'".into(),
        }
    );
}

#[test]
fn the_trash_bin_occupies_its_slot() {
    let mut builder = builder();
    let error = builder
        .load_labware(PCR_PLATE, FlexSlot::A3)
        .expect_err("the factory trash position is A3");
    assert_eq!(
        error,
        ProtocolError::SlotOccupied {
            slot: "A3".into(),
            occupant: "the trash bin".into(),
        }
    );
}

#[test]
fn the_thermocycler_is_addressed_as_b1() {
    let error = builder()
        .load_module::<Thermocycler>(FlexSlot::C1)
        .expect_err("a thermocycler loads at its front-most slot B1");
    assert_eq!(
        error,
        ProtocolError::ThermocyclerSlot { found: "C1".into() }
    );
}

#[test]
fn the_thermocycler_occupies_a1_and_b1() {
    let mut builder = builder();
    builder.load_module::<Thermocycler>(FlexSlot::B1).unwrap();
    let error = builder
        .load_labware(PCR_PLATE, FlexSlot::A1)
        .expect_err("A1 belongs to the installed thermocycler");
    assert_eq!(
        error,
        ProtocolError::SlotOccupied {
            slot: "A1".into(),
            occupant: "module 'module-0'".into(),
        }
    );
}

#[test]
fn the_temperature_module_installs_in_column_1_or_3() {
    let error = builder()
        .load_module::<TemperatureModule>(FlexSlot::C2)
        .expect_err("column 2 has no module caddy");
    assert_eq!(
        error,
        ProtocolError::ModuleSlotInvalid {
            module: "Temperature Module GEN2".into(),
            requirement: "column 1 or 3",
            slot: "C2".into(),
        }
    );
}

#[test]
fn the_absorbance_reader_installs_in_column_3() {
    let error = builder()
        .load_module::<AbsorbanceReader>(FlexSlot::D1)
        .expect_err("the reader's caddy exists only in column 3");
    assert_eq!(
        error,
        ProtocolError::ModuleSlotInvalid {
            module: "Absorbance Plate Reader Module GEN1".into(),
            requirement: "column 3",
            slot: "D1".into(),
        }
    );
}

#[test]
fn unknown_labware_is_rejected_by_name() {
    let error = builder()
        .load_labware("no_such_plate", FlexSlot::D2)
        .expect_err("only embedded definitions resolve");
    assert_eq!(
        error,
        ProtocolError::UnknownLabware {
            load_name: "no_such_plate".into(),
        }
    );
}

#[test]
fn a_module_carries_one_labware() {
    let mut builder = builder();
    let module = builder.load_module::<Thermocycler>(FlexSlot::B1).unwrap();
    builder.load_labware_on_module(PCR_PLATE, module).unwrap();
    let error = builder
        .load_labware_on_module(PCR_PLATE, module)
        .expect_err("the thermocycler already carries a plate");
    assert_eq!(
        error,
        ProtocolError::ModuleOccupied {
            module: "module-0".into(),
            labware: "labware-0".into(),
        }
    );
}

#[test]
fn well_names_come_from_the_definition() {
    let (mut builder, pipette, _, plate) = bench_with_tip();
    let error = builder
        .aspirate(pipette, plate, "H13", 10.0, 35.0, None)
        .expect_err("a 96-well plate has no thirteenth column");
    assert_eq!(
        error,
        ProtocolError::WellDoesNotExist {
            labware: "labware-1".into(),
            well: "H13".into(),
            well_count: 96,
        }
    );
}

#[test]
fn tips_come_only_from_tip_racks() {
    let mut builder = builder();
    let pipette = builder
        .load_pipette(FlexPipetteName::P50Single, PipetteMount::Left)
        .unwrap();
    let plate = builder.load_labware(PCR_PLATE, FlexSlot::D2).unwrap();
    let error = builder
        .pick_up_tip(pipette, plate, "A1")
        .expect_err("a PCR plate holds no tips");
    assert_eq!(
        error,
        ProtocolError::NotATipRack {
            labware: "labware-0".into(),
        }
    );
}

#[test]
fn liquid_is_not_handled_in_a_tip_rack() {
    let (mut builder, pipette, tips, _) = bench_with_tip();
    let error = builder
        .aspirate(pipette, tips, "A1", 10.0, 35.0, None)
        .expect_err("a tip rack is not a liquid container");
    assert_eq!(
        error,
        ProtocolError::IsATipRack {
            labware: "labware-0".into(),
        }
    );
}

#[test]
fn liquid_operations_require_an_attached_tip() {
    let mut builder = builder();
    let pipette = builder
        .load_pipette(FlexPipetteName::P50Single, PipetteMount::Left)
        .unwrap();
    let plate = builder.load_labware(PCR_PLATE, FlexSlot::D2).unwrap();
    let error = builder
        .aspirate(pipette, plate, "A1", 10.0, 35.0, None)
        .expect_err("nothing aspirates without a tip");
    assert_eq!(
        error,
        ProtocolError::TipNotAttached {
            pipette: "pipette-0".into(),
        }
    );
}

#[test]
fn a_second_pick_up_requires_dropping_the_first_tip() {
    let (mut builder, pipette, tips, _) = bench_with_tip();
    let error = builder
        .pick_up_tip(pipette, tips, "B1")
        .expect_err("the pipette already carries a tip");
    assert_eq!(
        error,
        ProtocolError::TipAlreadyAttached {
            pipette: "pipette-0".into(),
        }
    );
}

#[test]
fn an_emptied_tip_well_is_not_picked_again() {
    let (mut builder, pipette, tips, _) = bench_with_tip();
    builder.drop_tip_into_trash(pipette).unwrap();
    let error = builder
        .pick_up_tip(pipette, tips, "A1")
        .expect_err("well A1's tip was already taken");
    assert_eq!(
        error,
        ProtocolError::TipAlreadyUsed {
            labware: "labware-0".into(),
            well: "A1".into(),
        }
    );
}

#[test]
fn aspiration_is_capped_by_the_working_volume() {
    let (mut builder, pipette, _, plate) = bench_with_tip();
    builder
        .aspirate(pipette, plate, "A1", 30.0, 35.0, None)
        .unwrap();
    let error = builder
        .aspirate(pipette, plate, "A1", 30.0, 35.0, None)
        .expect_err("a p50 with a 50 uL tip holds 50 uL");
    assert_eq!(
        error,
        ProtocolError::OverAspiration {
            pipette: "pipette-0".into(),
            requested: 30.0,
            held: 30.0,
            working: 50.0,
        }
    );
}

#[test]
fn the_tip_caps_the_working_volume_below_the_pipette_maximum() {
    let mut builder = builder();
    let pipette = builder
        .load_pipette(FlexPipetteName::P1000Single, PipetteMount::Left)
        .unwrap();
    let tips = builder.load_labware(TIPRACK_50, FlexSlot::C2).unwrap();
    let plate = builder.load_labware(PCR_PLATE, FlexSlot::D2).unwrap();
    builder.pick_up_tip(pipette, tips, "A1").unwrap();
    let error = builder
        .aspirate(pipette, plate, "A1", 80.0, 160.0, None)
        .expect_err("a 50 uL tip caps a p1000 at 50 uL");
    assert_eq!(
        error,
        ProtocolError::OverAspiration {
            pipette: "pipette-0".into(),
            requested: 80.0,
            held: 0.0,
            working: 50.0,
        }
    );
}

#[test]
fn dispensing_is_capped_by_what_the_pipette_holds() {
    let (mut builder, pipette, _, plate) = bench_with_tip();
    builder
        .aspirate(pipette, plate, "A1", 20.0, 35.0, None)
        .unwrap();
    let error = builder
        .dispense(pipette, plate, "B1", 25.0, 57.0, None)
        .expect_err("only 20 uL were aspirated");
    assert_eq!(
        error,
        ProtocolError::OverDispense {
            pipette: "pipette-0".into(),
            requested: 25.0,
            held: 20.0,
        }
    );
}

#[test]
fn a_blowout_empties_the_pipette() {
    let (mut builder, pipette, _, plate) = bench_with_tip();
    builder
        .aspirate(pipette, plate, "A1", 20.0, 35.0, None)
        .unwrap();
    builder.blowout(pipette, plate, "B1", 57.0, None).unwrap();
    let error = builder
        .dispense(pipette, plate, "B1", 1.0, 57.0, None)
        .expect_err("a blowout leaves nothing to dispense");
    assert_eq!(
        error,
        ProtocolError::OverDispense {
            pipette: "pipette-0".into(),
            requested: 1.0,
            held: 0.0,
        }
    );
}

#[test]
fn flow_rates_are_positive() {
    let (mut builder, pipette, _, plate) = bench_with_tip();
    let error = builder
        .aspirate(pipette, plate, "A1", 10.0, 0.0, None)
        .expect_err("a zero flow rate never finishes");
    assert_eq!(error, ProtocolError::NonPositiveFlowRate { found: 0.0 });
}

#[test]
fn configure_for_volume_stays_within_the_pipette_range() {
    let mut builder = builder();
    let pipette = builder
        .load_pipette(FlexPipetteName::P50Single, PipetteMount::Left)
        .unwrap();
    let error = builder
        .configure_for_volume(pipette, 60.0)
        .expect_err("a p50 cannot be configured for 60 uL");
    assert_eq!(
        error,
        ProtocolError::VolumeOutOfRange {
            pipette: "pipette-0".into(),
            volume: 60.0,
            maximum: 50.0,
        }
    );
}

#[test]
fn the_thermocycler_lid_gates_pipetting() {
    let mut builder = builder();
    let pipette = builder
        .load_pipette(FlexPipetteName::P50Single, PipetteMount::Left)
        .unwrap();
    let thermocycler = builder.load_module::<Thermocycler>(FlexSlot::B1).unwrap();
    let plate = builder
        .load_labware_on_module(PCR_PLATE, thermocycler)
        .unwrap();
    let tips = builder.load_labware(TIPRACK_50, FlexSlot::C2).unwrap();
    builder.pick_up_tip(pipette, tips, "A1").unwrap();
    let error = builder
        .aspirate(pipette, plate, "A1", 10.0, 35.0, None)
        .expect_err("the lid has not been opened");
    assert_eq!(
        error,
        ProtocolError::ThermocyclerLidClosed {
            labware: "labware-0".into(),
        }
    );
    builder.thermocycler_open_lid(thermocycler);
    builder
        .aspirate(pipette, plate, "A1", 10.0, 35.0, None)
        .expect("an open lid allows well access");
}

#[test]
fn the_thermocycler_lid_gates_labware_movement() {
    let mut builder = builder();
    let thermocycler = builder.load_module::<Thermocycler>(FlexSlot::B1).unwrap();
    let plate = builder
        .load_labware_on_module(PCR_PLATE, thermocycler)
        .unwrap();
    let error = builder
        .move_labware_to_slot(plate, FlexSlot::D2)
        .expect_err("the gripper cannot reach through a closed lid");
    assert_eq!(
        error,
        ProtocolError::ThermocyclerLidClosed {
            labware: "labware-0".into(),
        }
    );
}

#[test]
fn waiting_for_an_unset_block_target_is_rejected() {
    let mut builder = builder();
    let thermocycler = builder.load_module::<Thermocycler>(FlexSlot::B1).unwrap();
    let error = builder
        .thermocycler_wait_for_block_temperature(thermocycler)
        .expect_err("no target was set");
    assert_eq!(
        error,
        ProtocolError::NoTargetTemperature {
            device: "thermocycler block",
        }
    );
}

#[test]
fn temperature_targets_stay_in_each_devices_range() {
    let mut builder = builder();
    let thermocycler = builder.load_module::<Thermocycler>(FlexSlot::B1).unwrap();
    let temperature = builder
        .load_module::<TemperatureModule>(FlexSlot::C1)
        .unwrap();
    assert_eq!(
        builder
            .thermocycler_set_block_temperature(thermocycler, 120.0, None, None)
            .unwrap_err(),
        ProtocolError::TemperatureOutOfRange {
            device: "thermocycler block",
            celsius: 120.0,
            minimum: 0.0,
            maximum: 99.0,
        }
    );
    assert_eq!(
        builder
            .thermocycler_set_lid_temperature(thermocycler, 20.0)
            .unwrap_err(),
        ProtocolError::TemperatureOutOfRange {
            device: "thermocycler lid",
            celsius: 20.0,
            minimum: 37.0,
            maximum: 110.0,
        }
    );
    assert_eq!(
        builder
            .temperature_module_set_target(temperature, -20.0)
            .unwrap_err(),
        ProtocolError::TemperatureOutOfRange {
            device: "temperature module",
            celsius: -20.0,
            minimum: -9.0,
            maximum: 99.0,
        }
    );
}

#[test]
fn a_profile_is_validated_step_by_step() {
    let mut builder = builder();
    let thermocycler = builder.load_module::<Thermocycler>(FlexSlot::B1).unwrap();
    assert_eq!(
        builder
            .thermocycler_run_profile(thermocycler, &[], None)
            .unwrap_err(),
        ProtocolError::EmptyProfile
    );
    assert_eq!(
        builder
            .thermocycler_run_profile(thermocycler, &[(37.0, 120.0), (105.0, 60.0)], None)
            .unwrap_err(),
        ProtocolError::TemperatureOutOfRange {
            device: "thermocycler block",
            celsius: 105.0,
            minimum: 0.0,
            maximum: 99.0,
        }
    );
}

#[test]
fn the_heater_shaker_latch_and_shaker_interlock() {
    let mut builder = builder();
    let shaker = builder.load_module::<HeaterShaker>(FlexSlot::D1).unwrap();
    builder.heater_shaker_open_labware_latch(shaker).unwrap();
    assert_eq!(
        builder
            .heater_shaker_set_and_wait_for_shake_speed(shaker, 500.0)
            .unwrap_err(),
        ProtocolError::LatchOpenWhileShaking
    );
    builder.heater_shaker_close_labware_latch(shaker);
    assert_eq!(
        builder
            .heater_shaker_set_and_wait_for_shake_speed(shaker, 100.0)
            .unwrap_err(),
        ProtocolError::ShakeSpeedOutOfRange { rpm: 100.0 }
    );
    builder
        .heater_shaker_set_and_wait_for_shake_speed(shaker, 500.0)
        .unwrap();
    assert_eq!(
        builder
            .heater_shaker_open_labware_latch(shaker)
            .unwrap_err(),
        ProtocolError::LatchCannotOpenWhileShaking
    );
    assert_eq!(
        builder
            .heater_shaker_set_target_temperature(shaker, 100.0)
            .unwrap_err(),
        ProtocolError::TemperatureOutOfRange {
            device: "heater-shaker",
            celsius: 100.0,
            minimum: 27.0,
            maximum: 95.0,
        }
    );
}

#[test]
fn a_shaking_heater_shaker_blocks_pipetting_on_and_beside_it() {
    let mut builder = builder();
    let pipette = builder
        .load_pipette(FlexPipetteName::P50Single, PipetteMount::Left)
        .unwrap();
    let shaker = builder.load_module::<HeaterShaker>(FlexSlot::D1).unwrap();
    builder.heater_shaker_open_labware_latch(shaker).unwrap();
    let on_shaker = builder.load_labware_on_module(PCR_PLATE, shaker).unwrap();
    builder.heater_shaker_close_labware_latch(shaker);
    let beside = builder.load_labware(PCR_PLATE, FlexSlot::D2).unwrap();
    let tips = builder.load_labware(TIPRACK_50, FlexSlot::C2).unwrap();
    builder.pick_up_tip(pipette, tips, "A1").unwrap();
    builder
        .heater_shaker_set_and_wait_for_shake_speed(shaker, 500.0)
        .unwrap();
    assert_eq!(
        builder
            .aspirate(pipette, on_shaker, "A1", 10.0, 35.0, None)
            .unwrap_err(),
        ProtocolError::HeaterShakerShaking
    );
    assert_eq!(
        builder
            .aspirate(pipette, beside, "A1", 10.0, 35.0, None)
            .unwrap_err(),
        ProtocolError::HeaterShakerShaking
    );
    builder.heater_shaker_deactivate_shaker(shaker);
    builder
        .aspirate(pipette, beside, "A1", 10.0, 35.0, None)
        .expect("a stopped shaker no longer restricts its neighbors");
}

#[test]
fn the_heater_shaker_latch_gates_movement_and_pipetting_oppositely() {
    let mut builder = builder();
    let pipette = builder
        .load_pipette(FlexPipetteName::P50Single, PipetteMount::Left)
        .unwrap();
    let shaker = builder.load_module::<HeaterShaker>(FlexSlot::D1).unwrap();
    builder.heater_shaker_open_labware_latch(shaker).unwrap();
    let plate = builder.load_labware_on_module(PCR_PLATE, shaker).unwrap();
    let tips = builder.load_labware(TIPRACK_50, FlexSlot::C2).unwrap();
    builder.pick_up_tip(pipette, tips, "A1").unwrap();
    assert_eq!(
        builder
            .aspirate(pipette, plate, "A1", 10.0, 35.0, None)
            .unwrap_err(),
        ProtocolError::HeaterShakerLatchOpenForPipetting {
            labware: "labware-0".into(),
        }
    );
    builder.heater_shaker_close_labware_latch(shaker);
    assert_eq!(
        builder
            .move_labware_to_slot(plate, FlexSlot::D2)
            .unwrap_err(),
        ProtocolError::HeaterShakerLatchClosedForMove {
            labware: "labware-0".into(),
        }
    );
}

#[test]
fn loaded_liquid_fits_the_well() {
    let mut builder = builder();
    let plate = builder.load_labware(PCR_PLATE, FlexSlot::D2).unwrap();
    let water = builder
        .define_liquid("water", "nuclease-free water", None)
        .unwrap();
    let error = builder
        .load_liquid(water, plate, &[("A1", 150.0)])
        .expect_err("a 100 uL PCR well cannot hold 150 uL");
    assert_eq!(
        error,
        ProtocolError::LiquidVolumeExceedsWell {
            labware: "labware-0".into(),
            well: "A1".into(),
            volume: 150.0,
            capacity: 100.0,
        }
    );
}

#[test]
fn display_colors_are_hex_octets() {
    let error = builder()
        .define_liquid("water", "", Some("blue"))
        .expect_err("a display color is hex, not a name");
    assert_eq!(
        error,
        ProtocolError::InvalidDisplayColor {
            found: "blue".into(),
        }
    );
    builder()
        .define_liquid("water", "", Some("#00b0f0"))
        .expect("six hex digits are a color");
}

#[test]
fn disposed_labware_is_gone() {
    let mut builder = builder();
    let pipette = builder
        .load_pipette(FlexPipetteName::P50Single, PipetteMount::Left)
        .unwrap();
    let plate = builder.load_labware(PCR_PLATE, FlexSlot::D2).unwrap();
    let tips = builder.load_labware(TIPRACK_50, FlexSlot::C2).unwrap();
    builder.pick_up_tip(pipette, tips, "A1").unwrap();
    builder.move_labware_to_waste_chute(plate).unwrap();
    let error = builder
        .aspirate(pipette, plate, "A1", 10.0, 35.0, None)
        .expect_err("the plate is in the chute");
    assert_eq!(
        error,
        ProtocolError::LabwareDisposed {
            labware: "labware-0".into(),
        }
    );
}

#[test]
fn the_waste_chute_and_d3_labware_are_mutually_exclusive() {
    let mut builder = builder();
    let plate = builder.load_labware(PCR_PLATE, FlexSlot::D2).unwrap();
    builder.load_labware(TIPRACK_1000, FlexSlot::D3).unwrap();
    let error = builder
        .move_labware_to_waste_chute(plate)
        .expect_err("cutout D3 already holds labware");
    assert_eq!(
        error,
        ProtocolError::WasteChuteBlocked {
            occupant: "labware 'labware-1'".into(),
        }
    );
}

#[test]
fn a_gripper_move_frees_the_source_slot_and_claims_the_target() {
    let mut builder = builder();
    let plate = builder.load_labware(PCR_PLATE, FlexSlot::D2).unwrap();
    builder.move_labware_to_slot(plate, FlexSlot::C2).unwrap();
    builder
        .load_labware(TIPRACK_50, FlexSlot::D2)
        .expect("the vacated slot is free again");
    let error = builder
        .load_labware(TIPRACK_1000, FlexSlot::C2)
        .expect_err("the moved plate now claims C2");
    assert_eq!(
        error,
        ProtocolError::SlotOccupied {
            slot: "C2".into(),
            occupant: "labware 'labware-0'".into(),
        }
    );
}

#[test]
fn dropping_a_tip_in_the_trash_emits_the_flex_idiom() {
    let (mut builder, pipette, _, _) = bench_with_tip();
    builder.drop_tip_into_trash(pipette).unwrap();
    let document = builder.build();
    let commands: Vec<_> = document
        .commands
        .iter()
        .map(|command| serde_json::to_value(command).unwrap())
        .collect();
    let move_to_trash = commands
        .iter()
        .find(|command| command["commandType"] == "moveToAddressableAreaForDropTip")
        .expect("the Flex has no trash labware, so the drop is an addressable-area move");
    assert_eq!(
        move_to_trash["params"]["addressableAreaName"],
        "movableTrashA3"
    );
    assert_eq!(
        commands.last().unwrap()["commandType"],
        "dropTipInPlace",
        "the in-place drop follows the positioning move"
    );
}

#[test]
fn the_built_document_embeds_every_referenced_definition_once() {
    let (builder, _, _, _) = bench_with_tip();
    let document = builder.build();
    assert_eq!(document.schema_version, 8);
    assert_eq!(document.ot_shared_schema, "#/protocol/schemas/8");
    assert_eq!(
        document
            .labware_definitions
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        [
            "opentrons/nest_96_wellplate_100ul_pcr_full_skirt/3",
            "opentrons/opentrons_flex_96_tiprack_50ul/1",
        ]
    );
    assert_eq!(document.robot.deck_id, "ot3_standard");
}
