use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn specimen(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("docs")
        .join("language")
        .join("specimens")
        .join(name)
}

fn compile(path: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_labc"))
        .arg(path)
        .args(arguments)
        .output()
        .unwrap()
}

fn module_ir(name: &str) -> Value {
    let output = compile(&specimen(name), &["--emit", "module-ir"]);
    assert!(
        output.status.success(),
        "{name} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn plasmid_design_compiles_through_the_portable_module_boundary() {
    let module = module_ir("plasmid-design.lab");
    assert_eq!(module["imports"].as_array().unwrap().len(), 3);
    let declarations = module["declarations"].as_array().unwrap();
    assert!(declarations.iter().any(|declaration| {
        declaration["kind"] == "binding"
            && declaration["targets"][0]["type"]["name"] == "Circuit"
            && declaration["targets"][0]["type"]["arguments"][0]["name"] == "Tetracycline"
            && declaration["targets"][0]["type"]["arguments"][1]["name"]
                == "GreenFluorescentProtein"
    }));
    assert!(declarations.iter().any(|declaration| {
        declaration["kind"] == "artifact"
            && declaration["artifact"] == "plasmid"
            && declaration["name"] == "p_tet_reporter"
            && declaration["requirements"].as_array().unwrap().len() == 3
            && declaration["acceptance"].as_array().unwrap().len() == 3
    }));

    let human = compile(&specimen("plasmid-design.lab"), &[]);
    assert!(human.status.success());
    assert!(
        String::from_utf8(human.stdout)
            .unwrap()
            .contains("plasmid p_tet_reporter")
    );
}

/// The specimen that carries the type system's whole argument: a circuit
/// generic over its trigger, a panel that forgets which trigger, and a
/// characterization that refuses to.
#[test]
fn sensor_panel_compiles_roles_generics_and_forgotten_arguments() {
    let module = module_ir("sensor-panel.lab");
    let declarations = module["declarations"].as_array().unwrap();

    // The circuit introduces its parameters inside its own signature.
    let circuit = declarations
        .iter()
        .find(|declaration| declaration["kind"] == "circuit")
        .unwrap();
    assert_eq!(circuit["parameters"][0], "Trigger");
    assert_eq!(circuit["parameters"][1], "Product");
    assert_eq!(circuit["bounds"]["Trigger"]["name"], "Signal");
    assert_eq!(circuit["bounds"]["Product"]["name"], "Protein");

    // Two circuits with different triggers and the same product.
    for (name, trigger) in [
        ("tet_reporter", "Tetracycline"),
        ("ara_reporter", "Arabinose"),
    ] {
        let binding = declarations
            .iter()
            .find(|declaration| {
                declaration["kind"] == "binding" && declaration["targets"][0]["name"] == name
            })
            .unwrap_or_else(|| panic!("missing {name}"));
        let ty = &binding["targets"][0]["type"];
        assert_eq!(ty["name"], "Circuit");
        assert_eq!(ty["arguments"][0]["name"], trigger);
        assert_eq!(ty["arguments"][1]["name"], "GreenFluorescentProtein");
    }

    // The panel forgets the trigger and pins the product.
    let panel = declarations
        .iter()
        .find(|declaration| {
            declaration["kind"] == "binding" && declaration["targets"][0]["name"] == "panel"
        })
        .unwrap();
    let element = &panel["targets"][0]["type"]["element"];
    assert_eq!(element["arguments"][0]["kind"], "any");
    assert_eq!(element["arguments"][0]["role"], "Signal");
    assert_eq!(element["arguments"][1]["name"], "GreenFluorescentProtein");

    // Characterization keeps the signal named, which is what links its operands.
    let characterize = declarations
        .iter()
        .find(|declaration| declaration["name"] == "characterize")
        .unwrap();
    assert_eq!(characterize["parameters"][0], "S");
    assert_eq!(characterize["bounds"]["S"]["name"], "Signal");

    // A role declared by a standard module written in Lab bounds a type the
    // specimen declares itself.
    let reading = declarations
        .iter()
        .find(|declaration| declaration["name"] == "Reading")
        .unwrap();
    assert_eq!(reading["bounds"]["Of"]["name"], "Reporter");
    assert_eq!(
        reading["roles"][0], "Evidential",
        "a reading may be offered in support of a claim"
    );
    assert!(
        module["imports"]
            .as_array()
            .unwrap()
            .iter()
            .any(|import| import["module"] == "std.bio.reporters"),
        "{:?}",
        module["imports"]
    );
}

#[test]
fn plasmid_build_compiles_typed_effects_and_reactive_handlers() {
    let module = module_ir("plasmid-build.lab");
    let declarations = module["declarations"].as_array().unwrap();
    let workflow = declarations
        .iter()
        .find(|declaration| declaration["name"] == "build_plasmid")
        .unwrap();
    assert_eq!(workflow["outputs"][0]["name"], "outcome");
    assert_eq!(workflow["outputs"][0]["type"]["kind"], "union");
    assert_eq!(
        workflow["outputs"][0]["type"]["alternatives"][0]["name"],
        "Accepted"
    );
    assert_eq!(
        workflow["outputs"][0]["type"]["alternatives"][1]["name"],
        "Rejected"
    );
    let serialized = serde_json::to_string(workflow).unwrap();
    assert!(serialized.contains("workflow.await_colonies"));
    assert!(serialized.contains("std.lab.plasmid.split"));
    assert!(serialized.contains("\"mode\":\"take\""));
    assert!(serialized.contains("\"mode\":\"borrow\""));

    let await_colonies = declarations
        .iter()
        .find(|declaration| declaration["name"] == "await_colonies")
        .unwrap();
    let serialized = serde_json::to_string(await_colonies).unwrap();
    assert_eq!(await_colonies["state"][0]["name"], "observations");
    assert!(serialized.contains("\"kind\":\"state_update\""));
    assert!(serialized.contains("\"kind\":\"every\""));
    assert!(serialized.contains("\"kind\":\"after\""));

    let human = compile(&specimen("plasmid-build.lab"), &[]);
    assert!(human.status.success());
    assert!(
        String::from_utf8(human.stdout)
            .unwrap()
            .contains("workflow build_plasmid")
    );
}

#[test]
fn inventory_specimen_preserves_properties_and_resolved_operations() {
    let module = module_ir("inventory-plasmid.lab");
    let declarations = module["declarations"].as_array().unwrap();
    let reporter = declarations
        .iter()
        .find(|declaration| declaration["name"] == "reporter")
        .unwrap();
    assert_eq!(reporter["kind"], "artifact");
    assert_eq!(reporter["artifact"], "plasmid");
    assert!(reporter.get("bindings").is_none());
    // Each component keeps the kind its catalogue entry was declared with, so
    // the list records a promoter driving a coding sequence rather than
    // flattening every element to the one kind they have in common.
    let components = reporter["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|property| property["name"] == "components")
        .expect("the design states what it is assembled from");
    let alternatives = components["value"]["type"]["element"]["alternatives"]
        .as_array()
        .expect("a heterogeneous component list is a union of its kinds")
        .iter()
        .map(|alternative| alternative["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(alternatives, ["Promoter", "Part", "CDS"]);

    // A catalogued name carries its supplier's identifier as a field, so a
    // backend reads it directly rather than recognizing a call shape.
    let catalogued = declarations
        .iter()
        .find(|declaration| declaration["kind"] == "catalog" && declaration["name"] == "J23101")
        .expect("the specimen catalogues its parts");
    assert_eq!(catalogued["supplier_identity"], "J23101");
    assert_eq!(catalogued["type"]["name"], "Promoter");

    let serialized = serde_json::to_string(&module).unwrap();
    assert!(serialized.contains("std.bio.build.realize"));
    assert!(serialized.contains("https://sbol.io/ns/capability#ArtifactRealization"));
}

#[test]
fn dependency_specimen_preserves_typed_material_edges_without_levels() {
    let module = module_ir("dependency-build.lab");
    let declarations = module["declarations"].as_array().unwrap();
    let reporter = declarations
        .iter()
        .find(|declaration| declaration["name"] == "reporter_region")
        .unwrap();
    let components = reporter["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|property| property["name"] == "components")
        .unwrap();
    let alternatives = components["value"]["type"]["element"]["alternatives"]
        .as_array()
        .unwrap();
    assert_eq!(alternatives[0]["name"], "Plasmid");
    assert_eq!(alternatives[1]["name"], "Part");

    let assembly = declarations
        .iter()
        .find(|declaration| declaration["name"] == "assemble_reporter_region")
        .unwrap();
    assert_eq!(assembly["inputs"][0]["name"], "promoter_carrier");
    assert_eq!(assembly["inputs"][0]["type"]["name"], "Material");
    assert_eq!(assembly["outputs"][0]["name"], "outcome");
    let serialized = serde_json::to_string(assembly).unwrap();
    assert!(serialized.contains("std.bio.build.realize"));
    assert!(!serialized.contains("level1"));
    assert!(!serialized.contains("level2"));

    let host = declarations
        .iter()
        .find(|declaration| declaration["name"] == "build_reporter_host")
        .unwrap();
    assert_eq!(host["outputs"][0]["name"], "strain");
    assert_eq!(host["outputs"][1]["name"], "plate");
    assert!(
        serde_json::to_string(host)
            .unwrap()
            .contains("std.lab.plasmid.transform")
    );
}
