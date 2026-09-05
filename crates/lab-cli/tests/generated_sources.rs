//! A package whose manifest declares `build.generate` compiles what its
//! generator writes, so a workspace emitted by another frontend plans with
//! the same commands a native one does.

use std::process::Command;

const MANIFEST: &str = "[package]\nname = \"generated\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[build]\nentry = \"src/programs/main.lab\"\ngenerate = \"mkdir -p src/programs && cp seed.lab src/programs/main.lab\"\n";

const SEED: &str = "use std.bio.designs\nuse std.bio.build\n\nbuild medium LB_broth:\n  components = [\n    Ingredient { substance: \"tryptone\", concentration: 10 g/L },\n  ]\n\nworkflow main() -> Material<Medium>:\n  product <- realize LB_broth\n  return product\n";

#[test]
fn check_runs_the_source_generator_first() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("generated");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("lab.toml"), MANIFEST).unwrap();
    std::fs::write(root.join("seed.lab"), SEED).unwrap();

    // The entry source does not exist until the generator writes it.
    let output = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["check", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(root.join("src/programs/main.lab").is_file());
}

#[test]
fn a_failing_generator_names_its_command() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("generated");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("lab.toml"),
        MANIFEST.replace(
            "generate = \"mkdir -p src/programs && cp seed.lab src/programs/main.lab\"",
            "generate = \"echo the exporter broke >&2 && false\"",
        ),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["check", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("build.generate"), "{stderr}");
    assert!(stderr.contains("the exporter broke"), "{stderr}");
}
