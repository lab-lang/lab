//! Facility-derived adapter lowering and immutable artifact staging.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lab_compiler::backend::{adapter_catalog, lower_dependency_build_with_adapter};
use lab_compiler::planning::{
    AdapterBindingSnapshot, BuildInventory, FACILITY_LOWERING_SCHEMA_VERSION, FacilityAllocation,
    FacilityLoweredArtifact, FacilityLoweredArtifactRole, FacilityLoweredRequirement,
    FacilityLoweringManifest, FacilityLoweringRoute,
};
use lab_compiler::{ArtifactBundle, CheckedModule, PortableLairProgram};
use lab_inventory::InventorySnapshot;
use lab_package::LabPackage;
use sha2::{Digest, Sha256};

pub(crate) struct FacilityLoweringOutput {
    pub(crate) manifest: FacilityLoweringManifest,
    pub(crate) protocols: Vec<PathBuf>,
    pub(crate) documents: Vec<PathBuf>,
}

/// Derives concrete backend invocations from exact facility allocations.
///
/// A package never names a target here. Each route exists only because a reachable semantic
/// requirement was allocated to an offering, that offering belongs to an exact Asset, and the
/// Asset has an explicit local adapter binding whose implementation provides lowering.
pub(crate) fn lower_allocated_adapters(
    package: &LabPackage,
    modules: &[&CheckedModule],
    inventory: &InventorySnapshot,
    allocation: &FacilityAllocation,
    bindings: Option<&AdapterBindingSnapshot>,
    output_root: &Path,
) -> Result<FacilityLoweringOutput> {
    let catalog = adapter_catalog().context("failed to load the compiler adapter catalog")?;
    let descriptors = catalog
        .adapters
        .iter()
        .map(|descriptor| (descriptor.id.as_str(), descriptor))
        .collect::<BTreeMap<_, _>>();
    let mut grouped =
        BTreeMap::<(String, String, PathBuf, String), Vec<FacilityLoweredRequirement>>::new();
    for selected in &allocation.allocations {
        let Some(adapter) = selected.adapter.as_ref() else {
            continue;
        };
        grouped
            .entry((
                selected.asset.clone(),
                adapter.driver.clone(),
                adapter.profile_path.clone(),
                adapter.profile_sha256.clone(),
            ))
            .or_default()
            .push(FacilityLoweredRequirement {
                requirement_instance: selected.requirement_instance.clone(),
                capability_kind: selected.capability_kind.clone(),
                offering: selected.offering.clone(),
            });
    }

    if let Some(bindings) = bindings
        && (bindings.inventory_sha256 != allocation.inventory_sha256
            || bindings.facility != allocation.facility)
    {
        bail!(
            "adapter bindings and facility allocation do not describe the same inventory snapshot"
        );
    }

    let mut lowerable = Vec::new();
    for (key, mut requirements) in grouped {
        let descriptor = descriptors.get(key.1.as_str()).with_context(|| {
            format!(
                "allocated adapter '{}' is not present in this compiler build",
                key.1
            )
        })?;
        if !descriptor.services.lowering {
            continue;
        }
        let mut emitted_formats = descriptor.emitted_run_formats.iter();
        let automation_format = emitted_formats.next().cloned().with_context(|| {
            format!(
                "adapter '{}' provides lowering but declares no emitted run-document format",
                descriptor.id
            )
        })?;
        if emitted_formats.next().is_some() {
            bail!(
                "adapter '{}' provides whole-program lowering with several emitted run-document formats; the lowering API must identify each artifact format explicitly",
                descriptor.id
            );
        }
        requirements
            .sort_by(|left, right| left.requirement_instance.cmp(&right.requirement_instance));
        lowerable.push((key, requirements, automation_format));
    }
    if lowerable.len() > 1 {
        let routes = lowerable
            .iter()
            .map(|((asset, driver, _, _), _, _)| format!("{asset} through {driver}"))
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "facility allocation selects several whole-program lowerers ({routes}); these legacy backends cannot yet partition one program by requirement"
        );
    }

    let mut routes = Vec::new();
    let mut protocols = Vec::new();
    let mut documents = Vec::new();
    if !lowerable.is_empty() {
        let lair = PortableLairProgram::lower_program(modules)
            .context("failed to lower the allocated program for facility adapters")?;
        let protocol = lair
            .select_protocol()
            .context("failed to select a concrete protocol for facility adapter lowering")?;
        let build_inventory = semantic_build_inventory(modules, inventory)?;

        for (
            (asset, driver, source_profile_path, profile_sha256),
            requirements,
            automation_format,
        ) in lowerable
        {
            let source = package.root.join(&source_profile_path);
            let profile =
                crate::adapters::load_and_validate(&driver, &source).with_context(|| {
                    format!(
                        "failed to load operational profile for Asset '{}' adapter '{}'",
                        asset, driver
                    )
                })?;
            if profile.sha256 != profile_sha256 {
                bail!(
                    "operational profile {} changed after adapter allocation for Asset '{}'",
                    source.display(),
                    asset
                );
            }
            let bundle = lower_dependency_build_with_adapter(
                &driver,
                &profile.name,
                &profile.canonical_toml,
                &protocol,
                &build_inventory,
            )
            .with_context(|| {
                format!(
                    "failed to lower the allocated program for Asset '{}' through adapter '{}'",
                    asset, driver
                )
            })?;
            let relative_output = facility_lowering_directory(&asset, &driver);
            let written = write_facility_artifacts(
                &bundle,
                output_root,
                &relative_output,
                &automation_format,
                &mut protocols,
                &mut documents,
            )?;
            routes.push(FacilityLoweringRoute {
                id: facility_lowering_id(&asset, &driver),
                asset,
                driver: driver.clone(),
                profile_path: staged_adapter_profile_path(&driver, &profile_sha256),
                profile_sha256,
                requirements,
                output: relative_output,
                artifacts: written,
            });
        }
    }
    routes.sort_by(|left, right| (&left.asset, &left.driver).cmp(&(&right.asset, &right.driver)));
    protocols.sort();
    documents.sort();
    Ok(FacilityLoweringOutput {
        manifest: FacilityLoweringManifest {
            schema_version: FACILITY_LOWERING_SCHEMA_VERSION.to_owned(),
            inventory_sha256: allocation.inventory_sha256.clone(),
            facility: allocation.facility.clone(),
            routes,
        },
        protocols,
        documents,
    })
}

fn semantic_build_inventory(
    modules: &[&CheckedModule],
    snapshot: &InventorySnapshot,
) -> Result<BuildInventory> {
    let material_lots = snapshot
        .active_material_lots()
        .context("failed to index active SBOLInventory MaterialLots")?;
    let lots_by_component = material_lots
        .components()
        .map(|(component, lots)| {
            (
                component.as_str().to_owned(),
                lots.iter().map(|lot| lot.as_str().to_owned()).collect(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    BuildInventory::from_material_lots(
        modules,
        snapshot.source_sha256(),
        snapshot.facility().as_str(),
        &lots_by_component,
    )
    .context("failed to bind checked designs to SBOLInventory MaterialLots")
}

fn facility_lowering_directory(asset: &str, driver: &str) -> PathBuf {
    let raw_name = asset
        .rsplit(['/', '#'])
        .find(|segment| !segment.is_empty())
        .unwrap_or("asset");
    let mut asset_name = raw_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if asset_name.is_empty() {
        asset_name.push_str("asset");
    }
    let asset_hash = sha256_hex(asset.as_bytes());
    let driver_name = driver.replace('.', "-");
    PathBuf::from("lowerings")
        .join(format!("{asset_name}-{}", &asset_hash[..12]))
        .join(driver_name)
}

fn facility_lowering_id(asset: &str, driver: &str) -> String {
    let asset_hash = sha256_hex(asset.as_bytes());
    format!("{}-{}", driver.replace('.', "-"), &asset_hash[..12])
}

fn write_facility_artifacts(
    bundle: &ArtifactBundle,
    output_root: &Path,
    relative_output: &Path,
    automation_format: &str,
    protocols: &mut Vec<PathBuf>,
    documents: &mut Vec<PathBuf>,
) -> Result<Vec<FacilityLoweredArtifact>> {
    let route_root = output_root.join(relative_output);
    let mut artifacts = Vec::new();
    let mut typst_sources = Vec::new();
    for artifact in bundle.iter() {
        let relative_path = PathBuf::from(artifact.path());
        let path = route_root.join(&relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&path, artifact.contents())
            .with_context(|| format!("failed to write {}", path.display()))?;
        let role = if is_automation_protocol(&path) {
            protocols.push(path);
            FacilityLoweredArtifactRole::AutomationProtocol
        } else {
            FacilityLoweredArtifactRole::Support
        };
        if artifact.media_type() == "text/x-typst" && is_typeset_document(artifact.path()) {
            typst_sources.push(relative_path.clone());
        }
        artifacts.push(FacilityLoweredArtifact {
            path: relative_path,
            media_type: artifact.media_type().to_owned(),
            sha256: sha256_hex(artifact.contents()),
            role,
            format: (role == FacilityLoweredArtifactRole::AutomationProtocol)
                .then(|| automation_format.to_owned()),
        });
    }

    typst_sources.sort();
    let typesetter = crate::typeset::Typesetter::new();
    for source in typst_sources {
        let source_text = source
            .to_str()
            .context("a generated Typst source path must be UTF-8")?;
        let pdf_bytes = typesetter
            .compile_pdf(&route_root, source_text)
            .with_context(|| format!("failed to typeset {}", source.display()))?;
        let pdf_relative = source.with_extension("pdf");
        let pdf_path = route_root.join(&pdf_relative);
        fs::write(&pdf_path, &pdf_bytes)
            .with_context(|| format!("failed to write {}", pdf_path.display()))?;
        documents.push(pdf_path);
        artifacts.push(FacilityLoweredArtifact {
            path: pdf_relative,
            media_type: "application/pdf".to_owned(),
            sha256: sha256_hex(&pdf_bytes),
            role: FacilityLoweredArtifactRole::OperatorDocument,
            format: None,
        });
    }
    artifacts.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(artifacts)
}

pub(crate) fn staged_adapter_profile_path(driver: &str, profile_sha256: &str) -> PathBuf {
    PathBuf::from("adapters").join(format!("{driver}-{}.toml", &profile_sha256[..12]))
}

fn is_automation_protocol(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with("_protocol.py")
                || name.ends_with("_protocol.json")
                || name.ends_with(".star.json")
                || name.ends_with(".odtc.json")
                || name.ends_with(".read.json")
        })
}

fn is_typeset_document(path: &str) -> bool {
    !path.ends_with("lab-style.typ")
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
