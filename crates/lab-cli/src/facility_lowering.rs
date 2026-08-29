//! Facility-derived adapter lowering and immutable artifact staging.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lab_compiler::backend::{
    AdapterLoweringScope, adapter_catalog, lower_adapter_invocation_with_adapter,
    lower_allocated_dependency_build_with_adapter,
};
use lab_compiler::planning::{
    AdapterInvocation, AdapterInvocationPlan, BuildInventory, FACILITY_LOWERING_SCHEMA_VERSION,
    FacilityLoweredArtifact, FacilityLoweredArtifactRole, FacilityLoweredRequirement,
    FacilityLoweringManifest, FacilityLoweringRoute, FacilityLoweringScope,
    MaterialLotBuildInventory,
};
use lab_compiler::{AllocatedLairProgram, ArtifactBundle, CheckedModule};
use lab_inventory::InventorySnapshot;
use lab_package::LabPackage;
use lab_runfmt::ReviewedRunDocument;
use sha2::{Digest, Sha256};

pub(crate) struct FacilityLoweringOutput {
    pub(crate) manifest: FacilityLoweringManifest,
    pub(crate) protocols: Vec<PathBuf>,
    pub(crate) documents: Vec<PathBuf>,
    pub(crate) reviewed_documents: BTreeMap<String, ReviewedRunDocument>,
}

struct LowerableInvocation {
    invocation: AdapterInvocation,
    scope: AdapterLoweringScope,
    requirements: Vec<FacilityLoweredRequirement>,
    whole_program_format: Option<String>,
}

struct WrittenFacilityArtifacts {
    artifacts: Vec<FacilityLoweredArtifact>,
    protocols: Vec<PathBuf>,
    documents: Vec<PathBuf>,
    reviewed_documents: BTreeMap<String, ReviewedRunDocument>,
}

/// Derives concrete backend invocations from exact facility allocations.
///
/// A package never selects a device implementation here. Each route exists only because a reachable semantic
/// requirement was allocated to an offering, that offering belongs to an exact Asset, and the
/// Asset has an explicit local adapter binding whose implementation provides lowering.
pub(crate) fn lower_adapter_invocations(
    package: &LabPackage,
    inventory: &InventorySnapshot,
    allocated: &AllocatedLairProgram,
    invocation_plan: &AdapterInvocationPlan,
    output_root: &Path,
) -> Result<FacilityLoweringOutput> {
    invocation_plan
        .validate()
        .context("allocated adapter invocations are invalid")?;
    if invocation_plan.inventory_sha256 != inventory.source_sha256()
        || invocation_plan.facility != inventory.facility().as_str()
    {
        bail!("adapter invocations and the selected inventory snapshot do not match");
    }
    let catalog = adapter_catalog().context("failed to load the compiler adapter catalog")?;
    let descriptors = catalog
        .adapters
        .iter()
        .map(|descriptor| (descriptor.id.as_str(), descriptor))
        .collect::<BTreeMap<_, _>>();
    let requirements = invocation_plan
        .methods
        .iter()
        .flat_map(|method| &method.tasks)
        .flat_map(|task| &task.requirements)
        .map(|requirement| (requirement.id.clone(), requirement))
        .collect::<BTreeMap<_, _>>();

    let mut lowerable = Vec::new();
    for invocation in &invocation_plan.invocations {
        let descriptor = descriptors
            .get(invocation.adapter.driver.as_str())
            .with_context(|| {
                format!(
                    "allocated adapter '{}' is not present in this compiler build",
                    invocation.adapter.driver
                )
            })?;
        let Some(scope) = descriptor.services.lowering else {
            continue;
        };
        let whole_program_format = if scope == AdapterLoweringScope::WholeProgram {
            let mut emitted_formats = descriptor.emitted_run_formats.iter();
            let format = emitted_formats.next().cloned().with_context(|| {
                format!(
                    "adapter '{}' provides whole-program lowering but declares no emitted run-document format",
                    descriptor.id
                )
            })?;
            if emitted_formats.next().is_some() {
                bail!(
                    "adapter '{}' provides whole-program lowering with several emitted run-document formats; the compatibility API cannot identify each artifact format",
                    descriptor.id
                );
            }
            Some(format)
        } else {
            None
        };
        let mut lowered_requirements = invocation
            .requirements
            .iter()
            .map(|requirement_id| {
                let requirement = requirements
                    .get(requirement_id)
                    .expect("validated invocation Requirement exists");
                FacilityLoweredRequirement {
                    requirement_instance: requirement.id.to_string(),
                    capability_kind: requirement.capability_kind.to_string(),
                    offering: requirement.offering.clone(),
                }
            })
            .collect::<Vec<_>>();
        lowered_requirements
            .sort_by(|left, right| left.requirement_instance.cmp(&right.requirement_instance));
        lowerable.push(LowerableInvocation {
            invocation: invocation.clone(),
            scope,
            requirements: lowered_requirements,
            whole_program_format,
        });
    }
    let whole_program_count = lowerable
        .iter()
        .filter(|lowering| lowering.scope == AdapterLoweringScope::WholeProgram)
        .count();
    if whole_program_count > 1 || (whole_program_count == 1 && lowerable.len() > 1) {
        let routes = lowerable
            .iter()
            .map(|lowering| {
                format!(
                    "{} through {} ({:?})",
                    lowering.invocation.asset, lowering.invocation.adapter.driver, lowering.scope
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "facility allocation mixes a transitional whole-program lowerer with another route ({routes}); migrate that backend to invocation-scoped lowering before composing it with another Asset"
        );
    }
    let mut lowering_directories =
        facility_lowering_directories(lowerable.iter().map(|lowering| {
            (
                lowering.invocation.asset.as_str(),
                lowering.invocation.adapter.driver.as_str(),
            )
        }));

    let mut routes = Vec::new();
    let mut protocols = Vec::new();
    let mut documents = Vec::new();
    let mut reviewed_documents = BTreeMap::new();
    if !lowerable.is_empty() {
        if whole_program_count == 1
            && invocation_plan.methods.iter().any(|method| {
                method.source_operation.as_str() == "std.bio.build.realize"
                    && method.method.as_str()
                        != "https://www.lab-compiler.org/ns/method#automated-golden-gate"
            })
        {
            bail!(
                "the selected realization Method is not supported by the current dependency-build adapter bridge"
            );
        }
        let build_inventory =
            BuildInventory::MaterialLots(invocation_plan.material_inventory.clone());

        for lowering in lowerable {
            let invocation = lowering.invocation;
            let asset = invocation.asset.clone();
            let driver = invocation.adapter.driver.clone();
            let source_profile_path = invocation.adapter.profile_path.clone();
            let profile_sha256 = invocation.adapter.profile_sha256.clone();
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
            let (bundle, invocation_documents) = match lowering.scope {
                AdapterLoweringScope::WholeProgram => (
                    lower_allocated_dependency_build_with_adapter(
                        &driver,
                        &profile.name,
                        &profile.canonical_toml,
                        allocated,
                        &build_inventory,
                    )
                    .with_context(|| {
                        format!(
                            "failed to lower the allocated program for Asset '{}' through adapter '{}'",
                            asset, driver
                        )
                    })?,
                    BTreeMap::new(),
                ),
                AdapterLoweringScope::Invocation => {
                    let lowered = lower_adapter_invocation_with_adapter(
                        &driver,
                        invocation_plan,
                        &invocation,
                    )
                    .with_context(|| {
                        format!(
                            "failed to lower invocation '{}' for Asset '{}' through adapter '{}'",
                            invocation.id, asset, driver
                        )
                    })?;
                    let documents = lowered
                        .documents
                        .into_iter()
                        .map(|document| {
                            (
                                document.path,
                                (document.requirement.to_string(), document.format),
                            )
                        })
                        .collect();
                    (lowered.artifacts, documents)
                }
            };
            let relative_output = lowering_directories
                .remove(&(asset.clone(), driver.clone()))
                .expect("every lowerable Asset and adapter has an output directory");
            let written = write_facility_artifacts(
                &bundle,
                output_root,
                &relative_output,
                lowering.whole_program_format.as_deref(),
                &invocation_documents,
            )?;
            protocols.extend(written.protocols);
            documents.extend(written.documents);
            for (requirement, document) in written.reviewed_documents {
                if reviewed_documents
                    .insert(requirement.clone(), document)
                    .is_some()
                {
                    bail!("several adapter routes implement requirement '{requirement}'");
                }
            }
            routes.push(FacilityLoweringRoute {
                id: facility_lowering_id(&asset, &driver),
                asset,
                driver: driver.clone(),
                profile_path: staged_adapter_profile_path(&driver, &profile_sha256),
                profile_sha256,
                scope: match lowering.scope {
                    AdapterLoweringScope::WholeProgram => FacilityLoweringScope::WholeProgram,
                    AdapterLoweringScope::Invocation => FacilityLoweringScope::Invocation,
                },
                requirements: lowering.requirements,
                output: relative_output,
                artifacts: written.artifacts,
            });
        }
    }
    routes.sort_by(|left, right| (&left.asset, &left.driver).cmp(&(&right.asset, &right.driver)));
    protocols.sort();
    documents.sort();
    Ok(FacilityLoweringOutput {
        manifest: FacilityLoweringManifest {
            schema_version: FACILITY_LOWERING_SCHEMA_VERSION.to_owned(),
            inventory_sha256: invocation_plan.inventory_sha256.clone(),
            facility: invocation_plan.facility.clone(),
            routes,
        },
        protocols,
        documents,
        reviewed_documents,
    })
}

pub(crate) fn semantic_material_inventory(
    modules: &[&CheckedModule],
    snapshot: &InventorySnapshot,
) -> Result<MaterialLotBuildInventory> {
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
    let inventory = BuildInventory::from_material_lots(
        modules,
        snapshot.source_sha256(),
        snapshot.facility().as_str(),
        &lots_by_component,
    )
    .context("failed to bind checked designs to SBOLInventory MaterialLots")?;
    let BuildInventory::MaterialLots(inventory) = inventory else {
        unreachable!("SBOLInventory material binding always creates semantic inventory")
    };
    Ok(inventory)
}

fn facility_lowering_directories<'a>(
    routes: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> BTreeMap<(String, String), PathBuf> {
    let routes = routes
        .into_iter()
        .map(|(asset, driver)| (asset, driver, facility_asset_name(asset)))
        .collect::<Vec<_>>();
    let mut name_counts = BTreeMap::<String, usize>::new();
    for (_, _, name) in &routes {
        *name_counts.entry(name.clone()).or_default() += 1;
    }
    routes
        .into_iter()
        .map(|(asset, driver, name)| {
            let directory = if name_counts[&name] == 1 {
                name
            } else {
                let identity = format!("{asset}\0{driver}");
                format!("{name}-{}", &sha256_hex(identity.as_bytes())[..8])
            };
            (
                (asset.to_owned(), driver.to_owned()),
                PathBuf::from("assets").join(directory),
            )
        })
        .collect()
}

fn facility_asset_name(asset: &str) -> String {
    let raw_name = asset
        .rsplit(['/', '#'])
        .find(|segment| !segment.is_empty())
        .unwrap_or("asset");
    let mut name = String::new();
    for character in raw_name.chars() {
        if character.is_ascii_alphanumeric() {
            name.push(character.to_ascii_lowercase());
        } else if matches!(character, '-' | '_') {
            name.push(character);
        } else if !name.ends_with('-') {
            name.push('-');
        }
    }
    let name = name.trim_matches('-');
    if name.is_empty() {
        "asset".to_owned()
    } else {
        name.to_owned()
    }
}

fn facility_lowering_id(asset: &str, driver: &str) -> String {
    let asset_hash = sha256_hex(asset.as_bytes());
    format!("{}-{}", driver.replace('.', "-"), &asset_hash[..12])
}

fn write_facility_artifacts(
    bundle: &ArtifactBundle,
    output_root: &Path,
    relative_output: &Path,
    whole_program_format: Option<&str>,
    invocation_documents: &BTreeMap<String, (String, String)>,
) -> Result<WrittenFacilityArtifacts> {
    let route_root = output_root.join(relative_output);
    if let Some(path) = invocation_documents
        .keys()
        .find(|path| bundle.get(path).is_none())
    {
        bail!("adapter names missing reviewed invocation artifact '{path}'");
    }
    let mut artifacts = Vec::new();
    let mut protocols = Vec::new();
    let mut documents = Vec::new();
    let mut reviewed_documents = BTreeMap::new();
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
        let invocation_document = invocation_documents.get(artifact.path());
        let role = if invocation_document.is_some()
            || (whole_program_format.is_some() && is_automation_protocol(&path))
        {
            protocols.push(path);
            FacilityLoweredArtifactRole::AutomationProtocol
        } else {
            FacilityLoweredArtifactRole::Support
        };
        if artifact.media_type() == "text/x-typst" && is_typeset_document(artifact.path()) {
            typst_sources.push(relative_path.clone());
        }
        let sha256 = sha256_hex(artifact.contents());
        let format = invocation_document
            .map(|(_, format)| format.clone())
            .or_else(|| {
                if role == FacilityLoweredArtifactRole::AutomationProtocol {
                    whole_program_format.map(str::to_owned)
                } else {
                    None
                }
            });
        if let Some((requirement, format)) = invocation_document {
            let reviewed = ReviewedRunDocument {
                path: relative_output
                    .join(&relative_path)
                    .to_str()
                    .context("reviewed invocation document paths must be UTF-8")?
                    .to_owned(),
                format: format.clone(),
                sha256: sha256.clone(),
            };
            if reviewed_documents
                .insert(requirement.clone(), reviewed)
                .is_some()
            {
                bail!("several adapter artifacts implement requirement '{requirement}'");
            }
        }
        artifacts.push(FacilityLoweredArtifact {
            path: relative_path,
            media_type: artifact.media_type().to_owned(),
            sha256,
            role,
            format,
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
    Ok(WrittenFacilityArtifacts {
        artifacts,
        protocols,
        documents,
        reviewed_documents,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unique_asset_gets_a_short_readable_directory() {
        let directories = facility_lowering_directories([(
            "https://example.org/facility/Opentrons_OT2",
            "opentrons.ot2",
        )]);

        assert_eq!(
            directories[&(
                "https://example.org/facility/Opentrons_OT2".to_owned(),
                "opentrons.ot2".to_owned()
            )],
            PathBuf::from("assets/opentrons_ot2")
        );
    }

    #[test]
    fn colliding_asset_names_get_only_the_hash_they_need() {
        let directories = facility_lowering_directories([
            ("https://example.org/room-a/reader", "reader.alpha"),
            ("https://example.org/room-b/reader", "reader.beta"),
        ]);
        let first = &directories[&(
            "https://example.org/room-a/reader".to_owned(),
            "reader.alpha".to_owned(),
        )];
        let second = &directories[&(
            "https://example.org/room-b/reader".to_owned(),
            "reader.beta".to_owned(),
        )];

        assert_ne!(first, second);
        assert!(first.to_string_lossy().starts_with("assets/reader-"));
        assert!(second.to_string_lossy().starts_with("assets/reader-"));
    }
}
