use std::fs;
use std::path::{Path, PathBuf};

use lab_inventory::InventorySnapshot;
use lab_package::LabPackage;
use sbol_inventory::CandidateQuery;
use sbol_inventory::vocabulary::{LIQUID_HANDLING, Qualification, THERMAL_CYCLING};
use sbol3::{Iri, RdfFormat, Resource};
use tempfile::TempDir;

const FACILITY: &str = "https://example.org/ebef/facility";

fn example_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/ebef")
}

fn load_example() -> InventorySnapshot {
    let package = LabPackage::load(example_root()).unwrap();
    let inventory = &package.manifest.inventory;
    InventorySnapshot::load(
        &package.root,
        inventory.document.as_ref().unwrap(),
        inventory.facility.as_deref(),
    )
    .unwrap()
}

#[test]
fn public_ebef_catalog_is_a_valid_described_facility() {
    let snapshot = load_example();
    let inventory = snapshot.validated();

    assert_eq!(snapshot.facility().as_str(), FACILITY);
    assert_eq!(inventory.facilities().count(), 1);
    assert_eq!(inventory.zones().count(), 12);
    assert_eq!(inventory.assets().count(), 28);
    assert_eq!(inventory.capability_offerings().count(), 30);
    assert_eq!(
        snapshot.source_sha256(),
        "b965b1ed8ed5a02fdffdde591c1532f3dbec1bb6fc40b022941ef7b0e4f0677a"
    );

    let chamber = inventory
        .asset(&Resource::iri(
            "https://example.org/ebef/anaerobic_chamber_1",
        ))
        .unwrap();
    let interior = inventory
        .zone(&Resource::iri(
            "https://example.org/ebef/anaerobic_chamber_1_interior",
        ))
        .unwrap();
    let prep = inventory
        .asset(&Resource::iri("https://example.org/ebef/microlab_prep"))
        .unwrap();
    assert_eq!(
        chamber.established_zone_ids().next(),
        Some(interior.identity())
    );
    assert_eq!(prep.located_in_id(), Some(interior.identity()));

    let thermal = CandidateQuery::new(Iri::from_static(THERMAL_CYCLING), Qualification::Described)
        .within_facility(snapshot.facility().clone());
    assert_eq!(inventory.find_qualified_assets(&thermal).len(), 3);

    let executable_liquid =
        CandidateQuery::new(Iri::from_static(LIQUID_HANDLING), Qualification::Executable);
    assert!(
        inventory
            .find_qualified_assets(&executable_liquid)
            .is_empty(),
        "public equipment descriptions must not become execution claims"
    );
}

#[test]
fn ebef_graph_round_trips_through_every_supported_rdf_format() {
    let snapshot = load_example();
    let output = TempDir::new().unwrap();

    for &format in RdfFormat::ALL {
        let relative = format!("catalog.{}", format.extension());
        let serialized = snapshot.document().write(format).unwrap();
        fs::write(output.path().join(&relative), serialized).unwrap();

        let reread = InventorySnapshot::load(output.path(), &relative, None).unwrap();
        assert_eq!(reread.facility().as_str(), FACILITY, "{format}");
        assert_eq!(reread.validated().assets().count(), 28, "{format}");
    }
}
