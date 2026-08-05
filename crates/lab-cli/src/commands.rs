use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lab_compiler::{CheckedModule, compile_module};
use lab_package::{LabPackage, PackageManifest};
use serde::Serialize;

use crate::Output;

pub(crate) fn new_project(path: PathBuf, name: Option<String>, output: &Output) -> Result<()> {
    if path.exists() && fs::read_dir(&path)?.next().is_some() {
        bail!("{} already exists and is not empty", path.display());
    }
    let package_name = name.unwrap_or_else(|| {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("lab-project")
            .to_owned()
    });
    validate_package_name(&package_name)?;

    let programs = path.join("src").join("programs");
    fs::create_dir_all(&programs)
        .with_context(|| format!("failed to create {}", programs.display()))?;
    let manifest = format!(
        "[package]\nname = \"{package_name}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[build]\nentry = \"src/programs/main.lab\"\n"
    );
    write_new(&path.join("lab.toml"), &manifest)?;
    write_new(
        &programs.join("main.lab"),
        r#"plasmid starter:
  sequence: dna("ATGCGTACGTTAGCTA")
  require topology == circular
  accept sequence == design.sequence
"#,
    )?;
    write_new(&path.join(".gitignore"), ".lab/\n")?;

    output.success(
        "created",
        ProjectCreated {
            package: package_name,
            root: path.clone(),
            entry: PathBuf::from("src/programs/main.lab"),
        },
        format!(
            "Created Lab project in {}\n  Next: cd {} && lab check",
            path.display(),
            path.display()
        ),
    )
}

pub(crate) fn check(path: PathBuf, output: &Output) -> Result<()> {
    if path.is_file() && path.extension().is_some_and(|extension| extension == "lab") {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        compile_module(&text).with_context(|| format!("failed to check {}", path.display()))?;
        return output.success(
            "checked",
            FileChecked {
                source: path.clone(),
            },
            format!("Checked {}", path.display()),
        );
    }

    let package = load_package(&path)?;
    reject_unresolved_dependencies(&package)?;
    let modules = compile_package(&package)?;
    output.success(
        "checked",
        PackageChecked {
            package: package.manifest.package.name.clone(),
            version: package.manifest.package.version.clone(),
            modules: modules.len(),
        },
        format!(
            "Checked {} {} ({} modules)",
            package.manifest.package.name,
            package.manifest.package.version,
            modules.len()
        ),
    )
}

pub(crate) fn build(path: PathBuf, out_dir: Option<PathBuf>, output: &Output) -> Result<()> {
    let package = load_package(&path)?;
    reject_unresolved_dependencies(&package)?;
    let modules = compile_package(&package)?;
    let output_root = match out_dir {
        Some(path) if path.is_absolute() => path,
        Some(path) => package.root.join(path),
        None => package.root.join(".lab").join("build"),
    };
    fs::create_dir_all(&output_root)
        .with_context(|| format!("failed to create {}", output_root.display()))?;

    let mut artifacts = Vec::new();
    for (source, module) in package.sources.iter().zip(&modules) {
        let relative_artifact = PathBuf::from("modules")
            .join(source.module.replace('.', "/"))
            .with_extension("module.json");
        let artifact_path = output_root.join(&relative_artifact);
        if let Some(parent) = artifact_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let mut json = serde_json::to_string_pretty(module)?;
        json.push('\n');
        fs::write(&artifact_path, json)
            .with_context(|| format!("failed to write {}", artifact_path.display()))?;
        artifacts.push(BuildModule {
            module: source.module.clone(),
            source: source.relative_path.clone(),
            artifact: relative_artifact,
        });
    }

    let index = BuildIndex {
        schema_version: 1,
        package: package.manifest.package.name.clone(),
        version: package.manifest.package.version.clone(),
        edition: package.manifest.package.edition.clone(),
        entry: package.manifest.build.entry.clone(),
        modules: artifacts,
    };
    let index_path = output_root.join("package.json");
    let mut json = serde_json::to_string_pretty(&index)?;
    json.push('\n');
    fs::write(&index_path, json)
        .with_context(|| format!("failed to write {}", index_path.display()))?;

    output.success(
        "built",
        BuildCompleted {
            package: index.package.clone(),
            version: index.version.clone(),
            modules: index.modules.len(),
            output: output_root.clone(),
        },
        format!(
            "Built {} {} ({} modules)\n  Artifacts: {}",
            index.package,
            index.version,
            index.modules.len(),
            output_root.display()
        ),
    )
}

pub(crate) fn metadata(path: PathBuf, output: &Output) -> Result<()> {
    let package = load_package(&path)?;
    let metadata = PackageMetadataOutput {
        root: package.root.clone(),
        manifest: package.manifest.clone(),
        modules: package
            .sources
            .iter()
            .map(|source| SourceMetadata {
                module: source.module.clone(),
                source: source.relative_path.clone(),
            })
            .collect(),
    };
    let human = format!(
        "{} {}\n{}",
        metadata.manifest.package.name,
        metadata.manifest.package.version,
        metadata
            .modules
            .iter()
            .map(|module| format!("  {}  {}", module.module, module.source.display()))
            .collect::<Vec<_>>()
            .join("\n")
    );
    output.success("metadata", metadata, human)
}

fn load_package(path: &Path) -> Result<LabPackage> {
    LabPackage::discover(path)
        .with_context(|| format!("failed to load package from {}", path.display()))
}

fn reject_unresolved_dependencies(package: &LabPackage) -> Result<()> {
    if package.manifest.dependencies.is_empty() {
        return Ok(());
    }
    bail!(
        "package '{}' declares dependencies, but dependency resolution is not implemented yet; no dependency was silently ignored",
        package.manifest.package.name
    )
}

fn compile_package(package: &LabPackage) -> Result<Vec<CheckedModule>> {
    package
        .sources
        .iter()
        .map(|source| {
            let text = fs::read_to_string(&source.path)
                .with_context(|| format!("failed to read {}", source.path.display()))?;
            compile_module(&text).with_context(|| {
                format!(
                    "failed to check module '{}' ({})",
                    source.module,
                    source.path.display()
                )
            })
        })
        .collect()
}

fn validate_package_name(name: &str) -> Result<()> {
    let manifest = format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n");
    let parsed = PackageManifest::parse(&manifest)?;
    if parsed.package.name != name {
        bail!("invalid package name '{name}'");
    }
    let mut characters = name.chars();
    if !characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic())
        || !characters.all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        bail!("invalid package name '{name}'");
    }
    Ok(())
}

fn write_new(path: &Path, contents: &str) -> Result<()> {
    if path.exists() {
        bail!("refusing to overwrite {}", path.display());
    }
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))
}

#[derive(Serialize)]
struct ProjectCreated {
    package: String,
    root: PathBuf,
    entry: PathBuf,
}

#[derive(Serialize)]
struct FileChecked {
    source: PathBuf,
}

#[derive(Serialize)]
struct PackageChecked {
    package: String,
    version: String,
    modules: usize,
}

#[derive(Serialize)]
struct BuildCompleted {
    package: String,
    version: String,
    modules: usize,
    output: PathBuf,
}

#[derive(Serialize)]
struct PackageMetadataOutput {
    root: PathBuf,
    manifest: PackageManifest,
    modules: Vec<SourceMetadata>,
}

#[derive(Serialize)]
struct SourceMetadata {
    module: String,
    source: PathBuf,
}

#[derive(Serialize)]
struct BuildIndex {
    schema_version: u32,
    package: String,
    version: String,
    edition: String,
    entry: Option<PathBuf>,
    modules: Vec<BuildModule>,
}

#[derive(Serialize)]
struct BuildModule {
    module: String,
    source: PathBuf,
    artifact: PathBuf,
}
