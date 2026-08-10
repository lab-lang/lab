//! `lab build` typesets every emitted protocol document to PDF, in-process
//! and hermetically: fonts are embedded in the binary and the documents
//! import only the bundled style sheet, so this runs offline everywhere.

use std::path::{Path, PathBuf};
use std::process::Command;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

#[test]
fn build_typesets_every_document_to_pdf() {
    let example = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/golden-gate");
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("golden-gate");
    copy_dir(&example, &project);

    let output = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["build", project.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let target_root = project.join(".lab/build/opentrons-ot2");
    for document in [
        "manual_protocol.pdf",
        "dependency_report.pdf",
        "wave-001/manual_protocol.pdf",
        "wave-002/manual_protocol.pdf",
    ] {
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
        "build output lists the typeset documents: {human}"
    );
}
