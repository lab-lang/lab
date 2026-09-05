//! Project-level facility planning across the complete compiler pipeline.
//!
//! This module is the filesystem-aware application service shared by the CLI and language
//! bindings. It loads one validated SBOLInventory snapshot and local adapter overlay, then drives
//! portable LAIR through Method refinement, global facility allocation, allocated Procedure LAIR,
//! and exact adapter invocations. Backends consume only the result of this service.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use lab_adapters::{
    AdapterInvocationError, AdapterInvocationPlan, AdapterProfileContractError,
    validate_adapter_profile,
};
use lab_capability::CapabilityKind;
use lab_capability::MethodId;
use lab_compiler::method::{IntentOperationId, LocalId, MethodRegistry};
use lab_compiler::planning::{
    AdapterRequirement, AssetPin, AssetPinSelector, FacilityPlanningPolicy,
    FacilityPlanningSolution, MethodPin, MethodPinSelector, PlanningProblemExtractionError,
};
use lab_compiler::program::{
    AllocatedLairError, AllocatedLairProgram, PortableLairError, PortableLairProgram,
    RefinedLairError,
};
use lab_facility::{
    AdapterBindingError, AdapterBindingRequest, AdapterBindingSnapshot,
    AllocatedMaterialInventoryValidationError, FacilityPlanningError, MaterialLotInventory,
    MaterialLotInventoryError, build_material_lot_inventory, explain_facility_planning_error,
    solve_facility_planning, validate_allocated_material_inventory,
};
use lab_inventory::{InventoryLoadError, InventorySnapshot, MaterialLotCatalogError};
use lab_package::{LabPackage, PlanningAdapterRequirement};
use thiserror::Error;

use crate::{CompiledProject, LabProject};

/// One complete, immutable facility-planning result for a compiled Lab package.
///
/// The textual refined IR is retained as review evidence. The allocated IR remains an owned,
/// verifier-valid Pliron program so exact backends cannot bypass allocation by reconstructing
/// work from frontend declarations.
pub struct FacilityPlanningResult {
    pub package: String,
    pub version: String,
    pub inventory: InventorySnapshot,
    pub material_inventory: MaterialLotInventory,
    pub adapter_bindings: Option<AdapterBindingSnapshot>,
    pub refined_lair: String,
    pub problem: lab_compiler::planning::PlanningProblem,
    pub solution: FacilityPlanningSolution,
    pub allocated: AllocatedLairProgram,
    pub adapter_invocations: AdapterInvocationPlan,
}

impl FacilityPlanningResult {
    pub fn problem(&self) -> &lab_compiler::planning::PlanningProblem {
        &self.problem
    }

    pub fn solution(&self) -> &FacilityPlanningSolution {
        &self.solution
    }
}

#[derive(Debug, Error)]
pub enum FacilityProjectError {
    #[error(
        "package '{package}' is a library with no build.entry; a facility plan needs an exact main workflow"
    )]
    MissingEntry { package: String },
    #[error(
        "package '{package}' has no inventory.document; facility planning consumes a validated SBOLInventory document"
    )]
    MissingInventory { package: String },
    #[error("failed to load inventory for package '{package}'")]
    Inventory {
        package: String,
        #[source]
        source: InventoryLoadError,
    },
    #[error("failed to canonicalize package root {path}")]
    CanonicalizePackageRoot {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("asset '{asset}' binds adapter '{driver}', but its profile cannot be read at {path}")]
    ResolveAdapterProfile {
        asset: String,
        driver: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("adapter profile '{profile}' resolves outside package '{package}'")]
    AdapterProfileOutsidePackage { profile: PathBuf, package: String },
    #[error("failed to read adapter profile {path}")]
    ReadAdapterProfile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("adapter profile {path} needs a UTF-8 file name")]
    InvalidAdapterProfileName { path: PathBuf },
    #[error("asset '{asset}' has invalid '{driver}' adapter profile {path}")]
    InvalidAdapterProfile {
        asset: String,
        driver: String,
        path: PathBuf,
        #[source]
        source: Box<AdapterProfileContractError>,
    },
    #[error("failed to bind configured adapters to SBOLInventory capability offerings")]
    AdapterBindings(#[source] AdapterBindingError),
    #[error("invalid package planning policy: {0}")]
    InvalidPlanningPolicy(String),
    #[error("failed to index active SBOLInventory MaterialLots")]
    MaterialLots(#[source] MaterialLotCatalogError),
    #[error("failed to bind checked designs to SBOLInventory MaterialLots")]
    MaterialInventory(#[source] MaterialLotInventoryError),
    #[error("failed to lower the checked program into Design and Intent LAIR")]
    PortableLair(#[source] PortableLairError),
    #[error("failed to refine workflow intent into Method alternatives")]
    RefinedLair(#[source] RefinedLairError),
    #[error("failed to project the verified Method graph into a planning problem")]
    PlanningProblem(#[source] PlanningProblemExtractionError),
    // The rendered message already contains the solver's full explanation, so this variant does
    // not also expose a `source`; printing the chain would repeat the summary after the detail.
    #[error("{}", facility_planning_message(.0))]
    FacilityPlanning(FacilityPlanningError),
    #[error("failed to apply the facility solution to refined LAIR")]
    Allocation(#[source] AllocatedLairError),
    #[error("failed to project allocated LAIR into adapter invocations")]
    AdapterInvocations(#[source] AdapterInvocationError),
    #[error("allocated LAIR does not match the retained material inventory")]
    AllocatedMaterialInventory(#[source] AllocatedMaterialInventoryValidationError),
}

impl LabProject {
    /// Plans the default runnable package against its selected facility using an explicit Method registry.
    pub fn plan_facility(
        &self,
        compiled: &CompiledProject,
        methods: &MethodRegistry,
    ) -> Result<FacilityPlanningResult, FacilityProjectError> {
        let package = self.default_package();
        if package.entry_source().is_none() {
            return Err(FacilityProjectError::MissingEntry {
                package: package.manifest.package.name.clone(),
            });
        }
        let inventory = load_package_inventory(package)?.ok_or_else(|| {
            FacilityProjectError::MissingInventory {
                package: package.manifest.package.name.clone(),
            }
        })?;
        let program_packages = self.program_packages();
        let modules = compiled
            .modules
            .iter()
            .filter(|module| program_packages.contains(&module.package))
            .map(|module| &module.module)
            .collect::<Vec<_>>();
        plan_modules_with_inventory(package, &modules, methods, inventory, None)
    }

    /// Plans with the standard and package-contributed Methods captured during compilation.
    pub fn plan_facility_with_package_methods(
        &self,
        compiled: &CompiledProject,
    ) -> Result<FacilityPlanningResult, FacilityProjectError> {
        self.plan_facility(compiled, &compiled.methods)
    }

    /// Plans one program of the default runnable package: the build the named
    /// entry module's `main` reaches through workflow calls. What the workspace
    /// declares beyond that is a library and stays unplanned.
    pub fn plan_facility_program(
        &self,
        compiled: &CompiledProject,
        entry_module: &str,
    ) -> Result<FacilityPlanningResult, FacilityProjectError> {
        let package = self.default_package();
        let inventory = load_package_inventory(package)?.ok_or_else(|| {
            FacilityProjectError::MissingInventory {
                package: package.manifest.package.name.clone(),
            }
        })?;
        let program_packages = self.program_packages();
        let modules = compiled
            .modules
            .iter()
            .filter(|module| program_packages.contains(&module.package))
            .map(|module| &module.module)
            .collect::<Vec<_>>();
        plan_modules_with_inventory(
            package,
            &modules,
            &compiled.methods,
            inventory,
            Some(entry_module),
        )
    }
}

/// Plans an explicitly supplied, already checked program against one package's facility context.
///
/// This is the embedding boundary used by non-file frontends such as Python. The caller owns
/// frontend checking and module order; the package contributes only inventory selection, planning
/// policy, and local adapter configuration.
pub fn plan_modules_for_package(
    package: &LabPackage,
    modules: &[&lab_language::CheckedModule],
    methods: &MethodRegistry,
) -> Result<FacilityPlanningResult, FacilityProjectError> {
    let inventory =
        load_package_inventory(package)?.ok_or_else(|| FacilityProjectError::MissingInventory {
            package: package.manifest.package.name.clone(),
        })?;
    plan_modules_with_inventory(package, modules, methods, inventory, None)
}

fn plan_modules_with_inventory(
    package: &LabPackage,
    modules: &[&lab_language::CheckedModule],
    methods: &MethodRegistry,
    inventory: InventorySnapshot,
    entry: Option<&str>,
) -> Result<FacilityPlanningResult, FacilityProjectError> {
    let portable = PortableLairProgram::lower_program_rooted(modules, entry)
        .map_err(FacilityProjectError::PortableLair)?;
    let refined = portable
        .refine_methods(methods)
        .map_err(FacilityProjectError::RefinedLair)?;
    let refined_lair = refined.ir();
    let problem = refined
        .planning_problem()
        .map_err(FacilityProjectError::PlanningProblem)?;
    let adapter_bindings = resolve_package_adapter_bindings(package, &inventory)?;
    let material_inventory = semantic_material_inventory(modules, &inventory)?;
    let solution = solve_facility_planning(
        &problem,
        &inventory,
        &material_inventory,
        adapter_bindings.as_ref(),
        facility_planning_policy(package)?,
    )
    .map_err(FacilityProjectError::FacilityPlanning)?;
    let allocated = refined
        .allocate(&solution)
        .map_err(FacilityProjectError::Allocation)?;
    let adapter_invocations = AdapterInvocationPlan::from_allocated_lair(&allocated)
        .map_err(FacilityProjectError::AdapterInvocations)?;
    validate_allocated_material_inventory(&adapter_invocations.allocated, &material_inventory)
        .map_err(FacilityProjectError::AllocatedMaterialInventory)?;

    Ok(FacilityPlanningResult {
        package: package.manifest.package.name.clone(),
        version: package.manifest.package.version.clone(),
        inventory,
        material_inventory,
        adapter_bindings,
        refined_lair,
        problem,
        solution,
        allocated,
        adapter_invocations,
    })
}

/// Loads and validates the package's selected SBOLInventory document, if configured.
pub fn load_package_inventory(
    package: &LabPackage,
) -> Result<Option<InventorySnapshot>, FacilityProjectError> {
    let inventory = &package.manifest.inventory;
    let Some(document) = inventory.document.as_ref() else {
        return Ok(None);
    };
    InventorySnapshot::load(&package.root, document, inventory.facility.as_deref())
        .map(Some)
        .map_err(|source| FacilityProjectError::Inventory {
            package: package.manifest.package.name.clone(),
            source,
        })
}

/// Resolves local operational configuration against exact Assets and offerings in the catalog.
pub fn resolve_package_adapter_bindings(
    package: &LabPackage,
    inventory: &InventorySnapshot,
) -> Result<Option<AdapterBindingSnapshot>, FacilityProjectError> {
    if package.manifest.execution.adapters.is_empty() {
        return Ok(None);
    }
    let canonical_root = fs::canonicalize(&package.root).map_err(|source| {
        FacilityProjectError::CanonicalizePackageRoot {
            path: package.root.clone(),
            source,
        }
    })?;
    let mut requests = Vec::new();
    for binding in &package.manifest.execution.adapters {
        let joined = canonical_root.join(&binding.profile);
        let profile_path = fs::canonicalize(&joined).map_err(|source| {
            FacilityProjectError::ResolveAdapterProfile {
                asset: binding.asset.clone(),
                driver: binding.driver.clone(),
                path: joined.clone(),
                source,
            }
        })?;
        if !profile_path.starts_with(&canonical_root) {
            return Err(FacilityProjectError::AdapterProfileOutsidePackage {
                profile: binding.profile.clone(),
                package: package.manifest.package.name.clone(),
            });
        }
        let contents = fs::read_to_string(&profile_path).map_err(|source| {
            FacilityProjectError::ReadAdapterProfile {
                path: profile_path.clone(),
                source,
            }
        })?;
        let name = binding
            .profile
            .file_stem()
            .and_then(|name| name.to_str())
            .ok_or_else(|| FacilityProjectError::InvalidAdapterProfileName {
                path: binding.profile.clone(),
            })?;
        let profile =
            validate_adapter_profile(&binding.driver, name, &contents).map_err(|source| {
                FacilityProjectError::InvalidAdapterProfile {
                    asset: binding.asset.clone(),
                    driver: binding.driver.clone(),
                    path: binding.profile.clone(),
                    source: Box::new(source),
                }
            })?;
        requests.push(AdapterBindingRequest {
            asset: binding.asset.clone(),
            driver: binding.driver.clone(),
            profile_path: binding.profile.clone(),
            profile,
        });
    }
    AdapterBindingSnapshot::resolve(inventory, requests)
        .map(Some)
        .map_err(FacilityProjectError::AdapterBindings)
}

fn semantic_material_inventory(
    modules: &[&lab_language::CheckedModule],
    snapshot: &InventorySnapshot,
) -> Result<MaterialLotInventory, FacilityProjectError> {
    let material_lots = snapshot
        .active_material_lots()
        .map_err(FacilityProjectError::MaterialLots)?;
    let lots_by_component = material_lots
        .components()
        .map(|(component, lots)| {
            (
                component.as_str().to_owned(),
                lots.iter().map(|lot| lot.as_str().to_owned()).collect(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    build_material_lot_inventory(
        modules,
        snapshot.source_sha256(),
        snapshot.facility().as_str(),
        &lots_by_component,
    )
    .map_err(FacilityProjectError::MaterialInventory)
}

fn facility_planning_policy(
    package: &LabPackage,
) -> Result<FacilityPlanningPolicy, FacilityProjectError> {
    let method_pins = package
        .manifest
        .planning
        .methods
        .iter()
        .map(|pin| {
            let selector = match (&pin.source_operation, &pin.choice) {
                (Some(source_operation), None) => MethodPinSelector::SourceOperation {
                    source_operation: IntentOperationId::new(source_operation.clone())
                        .map_err(|error| error.to_string())?,
                },
                (None, Some(choice)) => MethodPinSelector::Choice {
                    choice: LocalId::new(choice.clone()).map_err(|error| error.to_string())?,
                },
                _ => unreachable!("package validation requires exactly one method selector"),
            };
            Ok(MethodPin {
                selector,
                method: MethodId::new(pin.method.clone()).map_err(|error| error.to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()
        .map_err(FacilityProjectError::InvalidPlanningPolicy)?;
    let asset_pins = package
        .manifest
        .planning
        .assets
        .iter()
        .map(|pin| {
            let selector = match (&pin.capability_kind, &pin.requirement) {
                (Some(capability_kind), None) => AssetPinSelector::CapabilityKind {
                    capability_kind: CapabilityKind::new(capability_kind.clone())
                        .map_err(|error| error.to_string())?,
                },
                (None, Some(requirement)) => AssetPinSelector::Requirement {
                    requirement: LocalId::new(requirement.clone())
                        .map_err(|error| error.to_string())?,
                },
                (None, None) => AssetPinSelector::AnyRequirement,
                _ => unreachable!("package validation rejects two asset selectors"),
            };
            Ok(AssetPin {
                selector,
                asset: pin.asset.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()
        .map_err(FacilityProjectError::InvalidPlanningPolicy)?;
    Ok(FacilityPlanningPolicy {
        method_pins,
        asset_pins,
        adapter_requirement: match package.manifest.planning.adapter_requirement {
            PlanningAdapterRequirement::Optional => AdapterRequirement::Optional,
            PlanningAdapterRequirement::NonManual => AdapterRequirement::NonManual,
        },
    })
}

/// Prefers the solver's full explanation over its one-line summary.
///
/// The solver records why every candidate was rejected and how two complete plans differ. Printing
/// only the summary leaves a user with a verdict and no way to act on it.
fn facility_planning_message(error: &FacilityPlanningError) -> String {
    explain_facility_planning_error(error).unwrap_or_else(|| {
        format!(
            "failed to solve Method, material, and facility choices as one complete plan: {error}"
        )
    })
}
