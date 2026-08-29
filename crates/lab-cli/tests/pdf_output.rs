//! Facility builds typeset every emitted protocol document to PDF, in-process
//! and hermetically: fonts are embedded in the binary and the documents
//! import only the bundled style sheet, so this runs offline everywhere.

use std::path::{Path, PathBuf};
use std::process::Command;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() == ".lab" {
            continue;
        }
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

#[test]
fn facility_plan_typesets_every_document_to_pdf() {
    let example = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/golden-gate");
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("golden-gate");
    copy_dir(&example, &project);

    let output = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["plan", project.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "facility plan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let plan_root = project.join(".lab/plan");
    let lowering: serde_json::Value =
        serde_json::from_slice(&std::fs::read(plan_root.join("facility_lowering.json")).unwrap())
            .unwrap();
    let target_root = plan_root.join(lowering["routes"][0]["output"].as_str().unwrap());
    let documents = lowering["routes"][0]["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|artifact| artifact["role"] == "operator_document")
        .map(|artifact| artifact["path"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(documents.len(), 8);
    for document in documents {
        let path = target_root.join(document);
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("missing {}: {error}", path.display()));
        assert!(bytes.starts_with(b"%PDF-"), "{document} is not a PDF");
        // The .typ source stays beside the PDF, and with the bundled style
        // sheet the directory re-typesets standalone.
        assert!(path.with_extension("typ").is_file());
        assert!(path.parent().unwrap().join("lab-style.typ").is_file());
    }

    let human = String::from_utf8_lossy(&output.stdout);
    assert!(
        human.contains("Documents:"),
        "plan output lists the typeset documents: {human}"
    );
}
