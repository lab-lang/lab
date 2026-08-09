//! Golden and round-trip coverage for the emitted document shape.
//!
//! The fixture mirrors the shape of the official `simpleFlexV8.json` protocol
//! fixture: one pipette, a tip rack, a plate, one declared liquid, and a
//! pick-up/transfer/trash sequence. Regenerate it by running with
//! `LAB_BLESS_GOLDEN=1` after an intentional emission change.

use lab_opentrons_protocol::schema::{Metadata, ProtocolDocument};
use lab_opentrons_protocol::{FlexPipetteName, FlexProtocolBuilder, FlexSlot, PipetteMount};

const GOLDEN: &str = include_str!("fixtures/simple_flex_v8.json");

fn simple_flex_protocol() -> ProtocolDocument {
    let mut builder = FlexProtocolBuilder::new(Metadata {
        protocol_name: Some("Simple Flex transfer".into()),
        author: Some("lab-opentrons-protocol".into()),
        description: Some("One transfer from a reservoir well to a plate well".into()),
        ..Metadata::default()
    });
    let pipette = builder
        .load_pipette(FlexPipetteName::P1000Single, PipetteMount::Left)
        .unwrap();
    let tips = builder
        .load_labware("opentrons_flex_96_tiprack_1000ul", FlexSlot::D1)
        .unwrap();
    let plate = builder
        .load_labware("nest_96_wellplate_100ul_pcr_full_skirt", FlexSlot::D3)
        .unwrap();
    let water = builder
        .define_liquid("Aqueous solution", "H₂O", Some("#738ee6"))
        .unwrap();
    builder.load_liquid(water, plate, &[("A1", 100.0)]).unwrap();
    builder.pick_up_tip(pipette, tips, "A1").unwrap();
    builder
        .aspirate(pipette, plate, "A1", 50.0, 160.0, None)
        .unwrap();
    builder
        .dispense(pipette, plate, "B1", 50.0, 160.0, None)
        .unwrap();
    builder.drop_tip_into_trash(pipette).unwrap();
    builder.build()
}

#[test]
fn the_simple_flex_protocol_matches_its_golden_fixture() {
    let document = simple_flex_protocol();
    let rendered = document.to_json_pretty().unwrap();
    if std::env::var_os("LAB_BLESS_GOLDEN").is_some() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/simple_flex_v8.json");
        std::fs::write(&path, &rendered).unwrap();
    }
    assert_eq!(
        rendered, GOLDEN,
        "emission changed; rerun with LAB_BLESS_GOLDEN=1 if the change is intentional"
    );
}

#[test]
fn the_golden_fixture_round_trips_through_the_schema_model() {
    let parsed: ProtocolDocument = serde_json::from_str(GOLDEN).unwrap();
    assert_eq!(parsed, simple_flex_protocol());
    let rendered = parsed.to_json_pretty().unwrap();
    assert_eq!(rendered, GOLDEN);
}

#[test]
fn the_document_declares_every_v8_schema_id() {
    let document = simple_flex_protocol();
    assert_eq!(document.ot_shared_schema, "#/protocol/schemas/8");
    assert_eq!(document.schema_version, 8);
    assert_eq!(
        document.labware_definition_schema_id,
        "opentronsLabwareSchemaV2"
    );
    assert_eq!(document.command_schema_id, "opentronsCommandSchemaV8");
    assert_eq!(
        document.command_annotation_schema_id,
        "opentronsCommandAnnotationSchemaV1"
    );
    assert_eq!(document.liquid_schema_id, "opentronsLiquidSchemaV1");
    assert!(document.command_annotations.is_empty());
}
