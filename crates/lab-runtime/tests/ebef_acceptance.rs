use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use lab_runfmt::{
    EXECUTION_PLAN_FILE, EXECUTION_PLAN_FORMAT, ExecutionAdapterBinding,
    ExecutionInventoryReference, ExecutionMaterialBinding, ExecutionPlanAction,
    ExecutionPlanDocument, ExecutionPlanNode, ExecutionRequirementBinding, ReviewedRunDocument,
    SIMULATION_RUN_FORMAT, SimulationRunDocument,
};
use lab_runtime::clock::Clock;
use lab_runtime::device_executors::ReviewedDocumentSimulationExecutor;
use lab_runtime::events::{RecordingSink, RunEvent};
use lab_runtime::execution::{
    ExecutionOutcome, ExecutionRunConfig, ExecutorRegistry, load_execution_directory,
    render_execution_dry_run, run_execution_plan,
};
use lab_runtime::mode::ExecutionMode;
use lab_runtime::operator::AutoOperator;
use lab_runtime::provenance::{SIMULATION_INVENTORY_RESULT_FILE, write_inventory_result};
use sbol_inventory::InventoryDocument;
use sbol_inventory::vocabulary::{
    ABSORBANCE_MEASUREMENT, ControlMode, INCUBATION, LIQUID_HANDLING, PROV_ENTITY,
    PROV_QUALIFIED_USAGE, Qualification,
};
use sbol3::{Iri, RdfFormat, Resource};
use sha2::{Digest, Sha256};

const FACILITY: &str = "https://example.org/ebef/facility";
const PHYSICAL_MICROLAB: &str = "https://example.org/ebef/microlab_prep";
const PHYSICAL_EPOCH: &str = "https://example.org/ebef/biotek_epoch_2";
const SIMULATED_MICROLAB: &str = "https://example.org/ebef-acceptance/microlab_prep_simulator";
const SIMULATED_EPOCH: &str = "https://example.org/ebef-acceptance/epoch_2_simulator";
const ASSAY_COMPONENT: &str = "https://example.org/ebef-acceptance/assay_plate_design";
const ASSAY_LOT: &str = "https://example.org/ebef-acceptance/assay_plate_lot";
const SIMULATOR: &str = "lab.simulator";

struct FixedClock;

impl Clock for FixedClock {
    fn now_unix(&self) -> u64 {
        1_725_000_000
    }
}

#[test]
fn ebef_derived_facility_composes_three_capabilities_without_claiming_hardware_control() {
    let directory = materialize_reviewed_simulation();
    let source_path = directory.path().join("inventory-source.ttl");
    let source_before = fs::read(&source_path).unwrap();
    let loaded = load_execution_directory(directory.path()).unwrap();

    for asset in [PHYSICAL_MICROLAB, PHYSICAL_EPOCH] {
        let physical = loaded.inventory.facility_asset(asset).unwrap();
        assert!(
            physical
                .offerings
                .iter()
                .all(|offering| offering.qualification == Qualification::Described)
        );
        assert!(
            physical
                .offerings
                .iter()
                .all(|offering| offering.control_mode == ControlMode::Unspecified)
        );
    }
    assert!(loaded.is_ready(ExecutionMode::Simulation));
    assert!(!loaded.is_ready(ExecutionMode::Live));

    let narration = render_execution_dry_run(&loaded);
    for expected in [
        LIQUID_HANDLING,
        INCUBATION,
        ABSORBANCE_MEASUREMENT,
        SIMULATED_MICROLAB,
        SIMULATED_EPOCH,
        "move assay_plate",
    ] {
        assert!(
            narration.contains(expected),
            "missing {expected}:\n{narration}"
        );
    }

    let mut registry = simulation_registry();
    let mut events = RecordingSink::default();
    let outcome = run_execution_plan(
        &loaded,
        ExecutionRunConfig {
            assume_yes: true,
            resume: false,
            mode: ExecutionMode::Simulation,
        },
        &mut registry,
        &mut AutoOperator { answer: true },
        &mut events,
        &FixedClock,
    )
    .unwrap();
    assert_eq!(
        outcome,
        ExecutionOutcome::Completed {
            executed: 5,
            skipped: 0,
            started_at_unix_seconds: 1_725_000_000,
            ended_at_unix_seconds: 1_725_000_000,
        }
    );
    assert_eq!(
        events
            .events
            .iter()
            .filter(|event| matches!(event, RunEvent::DocumentStarted { .. }))
            .count(),
        3
    );
    assert_eq!(
        events
            .events
            .iter()
            .filter(|event| matches!(event, RunEvent::LabwareMoved { .. }))
            .count(),
        2
    );

    let result = write_inventory_result(
        &loaded,
        ExecutionMode::Simulation,
        1_725_000_000,
        1_725_000_000,
    )
    .unwrap();
    assert_eq!(
        result.path,
        loaded.directory.join(SIMULATION_INVENTORY_RESULT_FILE)
    );
    assert!(result.output_materials.is_empty());
    assert_eq!(fs::read(&source_path).unwrap(), source_before);

    let result_text = fs::read_to_string(&result.path).unwrap();
    let result_document = InventoryDocument::read(&result_text, RdfFormat::Turtle).unwrap();
    result_document.check().unwrap();
    let graph = result_document.as_sbol_document();
    let activity = graph
        .get(&Resource::Iri(Iri::new(result.activity).unwrap()))
        .unwrap();
    let used = activity
        .resources(PROV_QUALIFIED_USAGE)
        .filter_map(|usage| graph.get(usage))
        .flat_map(|usage| usage.resources(PROV_ENTITY))
        .map(|entity| entity.to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        used,
        BTreeSet::from([
            ASSAY_LOT.to_owned(),
            SIMULATED_EPOCH.to_owned(),
            SIMULATED_MICROLAB.to_owned(),
        ])
    );

    let resumed = run_execution_plan(
        &loaded,
        ExecutionRunConfig {
            assume_yes: true,
            resume: true,
            mode: ExecutionMode::Simulation,
        },
        &mut registry,
        &mut AutoOperator { answer: true },
        &mut RecordingSink::default(),
        &FixedClock,
    )
    .unwrap();
    assert_eq!(
        resumed,
        ExecutionOutcome::Completed {
            executed: 0,
            skipped: 5,
            started_at_unix_seconds: 1_725_000_000,
            ended_at_unix_seconds: 1_725_000_000,
        }
    );
}

fn materialize_reviewed_simulation() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir_all(directory.path().join("adapters")).unwrap();
    fs::create_dir_all(directory.path().join("runs")).unwrap();

    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/ebef");
    let mut inventory = fs::read_to_string(examples.join("inventory/ebef.ttl")).unwrap();
    inventory.push('\n');
    inventory.push_str(
        &fs::read_to_string(examples.join("acceptance/simulation-extension.ttl")).unwrap(),
    );
    InventoryDocument::read(&inventory, RdfFormat::Turtle)
        .unwrap()
        .check()
        .unwrap();
    fs::write(directory.path().join("inventory-source.ttl"), &inventory).unwrap();
    fs::write(directory.path().join("adapters/lab-simulator.toml"), b"").unwrap();

    let runs = [
        (
            "liquid-handling.simulation.json",
            "liquid-handling",
            "Simulate anaerobic plate preparation",
            LIQUID_HANDLING,
        ),
        (
            "incubation.simulation.json",
            "incubation",
            "Simulate plate growth",
            INCUBATION,
        ),
        (
            "absorbance.simulation.json",
            "absorbance",
            "Simulate absorbance acquisition",
            ABSORBANCE_MEASUREMENT,
        ),
    ];
    let reviewed = runs
        .into_iter()
        .map(|(file, id, title, capability)| {
            let document = SimulationRunDocument {
                format: SIMULATION_RUN_FORMAT.to_owned(),
                id: id.to_owned(),
                title: title.to_owned(),
                capability_kind: capability.to_owned(),
                assumptions: vec![
                    "No physical EBEF hardware is contacted.".to_owned(),
                    "The step establishes architecture and provenance behavior only.".to_owned(),
                ],
            };
            let path = format!("runs/{file}");
            let bytes = write_json(&directory.path().join(&path), &document);
            (
                capability,
                ReviewedRunDocument {
                    path,
                    format: SIMULATION_RUN_FORMAT.to_owned(),
                    sha256: sha256_hex(&bytes),
                },
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();

    let plan = ExecutionPlanDocument {
        format: EXECUTION_PLAN_FORMAT.to_owned(),
        inventory: ExecutionInventoryReference {
            document: "inventory-source.ttl".to_owned(),
            source_sha256: sha256_hex(inventory.as_bytes()),
            facility: FACILITY.to_owned(),
        },
        planning: None,
        requirements: vec![
            requirement(
                "assay/liquid-handling",
                LIQUID_HANDLING,
                "https://example.org/ebef-acceptance/microlab_prep_simulator/liquid_handling",
                SIMULATED_MICROLAB,
            ),
            requirement(
                "assay/incubation",
                INCUBATION,
                "https://example.org/ebef-acceptance/epoch_2_simulator/incubation",
                SIMULATED_EPOCH,
            ),
            requirement(
                "assay/absorbance",
                ABSORBANCE_MEASUREMENT,
                "https://example.org/ebef-acceptance/epoch_2_simulator/absorbance_measurement",
                SIMULATED_EPOCH,
            ),
        ],
        materials: vec![ExecutionMaterialBinding {
            id: "assay_plate".to_owned(),
            component: ASSAY_COMPONENT.to_owned(),
            material_lot: ASSAY_LOT.to_owned(),
        }],
        outputs: Vec::new(),
        nodes: vec![
            ExecutionPlanNode {
                id: "move-to-liquid-handler".to_owned(),
                after: Vec::new(),
                action: ExecutionPlanAction::MoveMaterial {
                    material: "assay_plate".to_owned(),
                    from: "https://example.org/ebef/microbiology_lab".to_owned(),
                    to: SIMULATED_MICROLAB.to_owned(),
                    instructions: "Place the simulated assay plate at the liquid-handler twin."
                        .to_owned(),
                },
            },
            execute_node(
                "simulate-liquid-handling",
                &["move-to-liquid-handler"],
                "assay/liquid-handling",
                reviewed.get(LIQUID_HANDLING).unwrap().clone(),
            ),
            ExecutionPlanNode {
                id: "move-to-reader".to_owned(),
                after: vec!["simulate-liquid-handling".to_owned()],
                action: ExecutionPlanAction::MoveMaterial {
                    material: "assay_plate".to_owned(),
                    from: SIMULATED_MICROLAB.to_owned(),
                    to: SIMULATED_EPOCH.to_owned(),
                    instructions: "Move the simulated assay plate between the two exact twins."
                        .to_owned(),
                },
            },
            execute_node(
                "simulate-incubation",
                &["move-to-reader"],
                "assay/incubation",
                reviewed.get(INCUBATION).unwrap().clone(),
            ),
            execute_node(
                "simulate-absorbance",
                &["simulate-incubation"],
                "assay/absorbance",
                reviewed.get(ABSORBANCE_MEASUREMENT).unwrap().clone(),
            ),
        ],
    };
    write_json(&directory.path().join(EXECUTION_PLAN_FILE), &plan);
    directory
}

fn requirement(
    instance: &str,
    capability: &str,
    offering: &str,
    asset: &str,
) -> ExecutionRequirementBinding {
    ExecutionRequirementBinding {
        requirement_instance: instance.to_owned(),
        requirement_template: format!("ebef-acceptance::{instance}"),
        capability_kind: capability.to_owned(),
        offering: offering.to_owned(),
        asset: asset.to_owned(),
        minimum_qualification: Qualification::Simulatable.iri().to_owned(),
        observed_qualification: Qualification::Simulatable.iri().to_owned(),
        control_mode: ControlMode::ReviewedFile.iri().to_owned(),
        parameters: Vec::new(),
        adapter: Some(ExecutionAdapterBinding {
            driver: SIMULATOR.to_owned(),
            profile_path: "adapters/lab-simulator.toml".to_owned(),
            profile_sha256: sha256_hex(b""),
        }),
    }
}

fn execute_node(
    id: &str,
    after: &[&str],
    requirement: &str,
    document: ReviewedRunDocument,
) -> ExecutionPlanNode {
    ExecutionPlanNode {
        id: id.to_owned(),
        after: after.iter().map(|id| (*id).to_owned()).collect(),
        action: ExecutionPlanAction::Execute {
            requirement: requirement.to_owned(),
            document: Some(document),
        },
    }
}

fn simulation_registry() -> ExecutorRegistry {
    let mut registry = ExecutorRegistry::new();
    for asset in [SIMULATED_MICROLAB, SIMULATED_EPOCH] {
        registry
            .register(
                asset,
                SIMULATOR,
                SIMULATION_RUN_FORMAT,
                Box::<ReviewedDocumentSimulationExecutor>::default(),
            )
            .unwrap();
    }
    registry
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(value).unwrap();
    bytes.push(b'\n');
    fs::write(path, &bytes).unwrap();
    bytes
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
