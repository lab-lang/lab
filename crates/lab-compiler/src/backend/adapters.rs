//! Compiler-owned adapter discovery and profile validation.
//!
//! An adapter is a Lab implementation, never a facility Asset. The manifest binds an adapter ID
//! to an exact SBOLInventory Asset IRI; this registry states which semantic capability offerings
//! and control modes that implementation can use. Product features stay separate from semantic
//! capability kinds so neither manufacturer nor model can silently select a driver.

use std::collections::BTreeSet;

use lab_capability::{CapabilityKind, ControlMode};
use sbol_inventory::vocabulary::{
    ABSORBANCE_MEASUREMENT, INCUBATION, LIQUID_HANDLING, THERMAL_CYCLING,
};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use crate::backend::hamilton::star::StarAdapterProfile;
use crate::backend::opentrons::flex::FlexAdapterProfile;
use crate::backend::opentrons::ot2::Ot2AdapterProfile;
use crate::planning::{AdapterInvocation, AdapterInvocationPlan, BuildInventory};
use crate::{AllocatedLairProgram, ArtifactBundle, ProtocolLairProgram};
use lab_method::LocalId;
use lab_runfmt::{
    SIMULATION_RUN_FORMAT, STAR_RUN_FORMAT, SimulationRunDocument, THERMOCYCLE_RUN_FORMAT,
};

pub const ADAPTER_CATALOG_FORMAT: &str = "lab.adapter-catalog.v2";
pub const ADAPTER_PROFILE_SCHEMA_VERSION: &str = "lab.adapter-profile.v2";

const OPENTRONS_PYTHON_PROTOCOL: &str = "opentrons.python-protocol";
const OPENTRONS_PROTOCOL_DESIGNER: &str = "opentrons.protocol-designer-json";

const KNOWN_ADAPTERS: [&str; 6] = [
    "opentrons.ot2",
    "opentrons.flex",
    "hamilton.star",
    "inheco.odtc",
    "byonoy.absorbance96",
    "lab.simulator",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterServices {
    pub planning: bool,
    /// How this adapter consumes allocated Procedure work, when it provides lowering.
    pub lowering: Option<AdapterLoweringScope>,
    pub simulation: bool,
    pub runtime: bool,
}

/// The semantic unit accepted by an adapter lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterLoweringScope {
    /// Transitional built-ins that still consume the complete allocated dependency build.
    WholeProgram,
    /// The adapter consumes only the tasks and requirements in one exact invocation.
    Invocation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AdapterDescriptor {
    pub id: String,
    pub display_name: String,
    pub manufacturer: Option<String>,
    /// Exact SBOLInventory `fac:capabilityKind` IRIs this implementation supports.
    pub capabilities: BTreeSet<CapabilityKind>,
    /// Implementation facts that must never be used as semantic capability kinds.
    pub features: BTreeSet<String>,
    /// Exact closed SBOLInventory control-mode IRIs this implementation supports.
    pub control_modes: BTreeSet<ControlMode>,
    pub accepted_run_formats: BTreeSet<String>,
    pub emitted_run_formats: BTreeSet<String>,
    pub services: AdapterServices,
    pub profile_schema: Value,
    pub default_profile: ValidatedAdapterProfile,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AdapterCatalog {
    pub format: String,
    pub compiler_version: String,
    pub profile_schema_version: String,
    pub adapters: Vec<AdapterDescriptor>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ValidatedAdapterProfile {
    pub format: String,
    pub schema_version: String,
    pub compiler_version: String,
    pub name: String,
    pub driver: String,
    pub canonical_toml: String,
    pub canonical_json: Value,
    pub sha256: String,
}

#[derive(Debug, Error)]
pub enum AdapterProfileContractError {
    #[error(
        "adapter driver '{found}' is not provided by this compiler; known adapters are {known}"
    )]
    UnknownDriver { found: String, known: String },
    #[error("invalid {driver} adapter profile: {message}")]
    Invalid { driver: String, message: String },
    #[error("failed to describe adapter profiles: {0}")]
    Contract(String),
    #[error("failed to parse adapter profile TOML: {0}")]
    Toml(#[from] toml::de::Error),
}

/// Describes every concrete adapter in this compiler build.
pub fn adapter_catalog() -> Result<AdapterCatalog, AdapterProfileContractError> {
    Ok(AdapterCatalog {
        format: ADAPTER_CATALOG_FORMAT.to_owned(),
        compiler_version: env!("CARGO_PKG_VERSION").to_owned(),
        profile_schema_version: ADAPTER_PROFILE_SCHEMA_VERSION.to_owned(),
        adapters: vec![
            descriptor(
                "opentrons.ot2",
                "Opentrons OT-2",
                Some("Opentrons"),
                [LIQUID_HANDLING, THERMAL_CYCLING],
                ["on-deck-modules", "python-protocol-api", "single-channel"],
                [ControlMode::ReviewedFile],
                [],
                [OPENTRONS_PYTHON_PROTOCOL],
                AdapterServices {
                    planning: true,
                    lowering: Some(AdapterLoweringScope::WholeProgram),
                    simulation: false,
                    runtime: false,
                },
                schema_value::<Ot2AdapterProfile>()?,
            )?,
            descriptor(
                "opentrons.flex",
                "Opentrons Flex",
                Some("Opentrons"),
                [LIQUID_HANDLING, THERMAL_CYCLING],
                ["on-deck-modules", "protocol-designer-json"],
                [ControlMode::ReviewedFile],
                [],
                [OPENTRONS_PROTOCOL_DESIGNER],
                AdapterServices {
                    planning: true,
                    lowering: Some(AdapterLoweringScope::WholeProgram),
                    simulation: false,
                    runtime: false,
                },
                schema_value::<FlexAdapterProfile>()?,
            )?,
            descriptor(
                "hamilton.star",
                "Hamilton STAR/STARlet",
                Some("Hamilton"),
                [LIQUID_HANDLING],
                ["eight-channel", "firmware-frames", "live-usb"],
                [ControlMode::ReviewedFile, ControlMode::Api],
                [STAR_RUN_FORMAT],
                [STAR_RUN_FORMAT],
                AdapterServices {
                    planning: true,
                    lowering: Some(AdapterLoweringScope::WholeProgram),
                    simulation: true,
                    runtime: true,
                },
                schema_value::<StarAdapterProfile>()?,
            )?,
            descriptor(
                "inheco.odtc",
                "Inheco ODTC",
                Some("Inheco"),
                [THERMAL_CYCLING],
                ["network-session", "thermal-profile"],
                [ControlMode::Sila2],
                [THERMOCYCLE_RUN_FORMAT],
                [THERMOCYCLE_RUN_FORMAT],
                AdapterServices {
                    planning: true,
                    lowering: None,
                    simulation: true,
                    runtime: true,
                },
                schema_value::<EmptyAdapterProfile>()?,
            )?,
            descriptor(
                "byonoy.absorbance96",
                "Byonoy Absorbance 96",
                Some("Byonoy"),
                [ABSORBANCE_MEASUREMENT],
                ["hid", "plate-reader"],
                [ControlMode::Api],
                [],
                [],
                AdapterServices {
                    planning: false,
                    lowering: None,
                    simulation: false,
                    runtime: false,
                },
                schema_value::<EmptyAdapterProfile>()?,
            )?,
            descriptor(
                "lab.simulator",
                "Lab semantic capability simulator",
                None,
                [
                    LIQUID_HANDLING,
                    THERMAL_CYCLING,
                    INCUBATION,
                    ABSORBANCE_MEASUREMENT,
                ],
                ["no-hardware", "semantic-simulation"],
                [ControlMode::ReviewedFile],
                [SIMULATION_RUN_FORMAT],
                [SIMULATION_RUN_FORMAT],
                AdapterServices {
                    planning: true,
                    lowering: Some(AdapterLoweringScope::Invocation),
                    simulation: true,
                    runtime: false,
                },
                schema_value::<EmptyAdapterProfile>()?,
            )?,
        ],
    })
}

#[allow(clippy::too_many_arguments)]
fn descriptor<const C: usize, const F: usize, const M: usize, const A: usize, const E: usize>(
    id: &'static str,
    display_name: &'static str,
    manufacturer: Option<&'static str>,
    capabilities: [&'static str; C],
    features: [&'static str; F],
    control_modes: [ControlMode; M],
    accepted_run_formats: [&'static str; A],
    emitted_run_formats: [&'static str; E],
    services: AdapterServices,
    profile_schema: Value,
) -> Result<AdapterDescriptor, AdapterProfileContractError> {
    Ok(AdapterDescriptor {
        id: id.to_owned(),
        display_name: display_name.to_owned(),
        manufacturer: manufacturer.map(str::to_owned),
        capabilities: capabilities
            .into_iter()
            .map(|kind| {
                CapabilityKind::new(kind).map_err(|error| {
                    AdapterProfileContractError::Contract(format!(
                        "adapter '{id}' declares invalid capability kind: {error}"
                    ))
                })
            })
            .collect::<Result<_, _>>()?,
        features: strings(features),
        control_modes: control_modes.into_iter().collect(),
        accepted_run_formats: strings(accepted_run_formats),
        emitted_run_formats: strings(emitted_run_formats),
        services,
        profile_schema,
        default_profile: default_adapter_profile(id, id)?,
    })
}

fn strings<const N: usize>(values: [&'static str; N]) -> BTreeSet<String> {
    values.into_iter().map(str::to_owned).collect()
}

/// Returns the canonical empty or reference profile for one adapter.
pub fn default_adapter_profile(
    driver: &str,
    name: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    validate_adapter_profile(driver, name, "")
}

/// Parses a profile with the schema selected by the explicit adapter ID.
///
/// The profile cannot select another driver. In particular, an omitted or misleading
/// manufacturer/model value never changes which parser runs.
pub fn validate_adapter_profile(
    driver: &str,
    name: &str,
    contents: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    match driver {
        "opentrons.ot2" => {
            let profile =
                Ot2AdapterProfile::parse(name, contents).map_err(|error| invalid(driver, error))?;
            canonical_adapter_profile(driver, name, &profile)
        }
        "opentrons.flex" => {
            let profile = FlexAdapterProfile::parse(name, contents)
                .map_err(|error| invalid(driver, error))?;
            canonical_adapter_profile(driver, name, &profile)
        }
        "hamilton.star" => {
            let profile = StarAdapterProfile::parse(name, contents)
                .map_err(|error| invalid(driver, error))?;
            canonical_adapter_profile(driver, name, &profile)
        }
        "inheco.odtc" | "byonoy.absorbance96" | "lab.simulator" => {
            let _: EmptyAdapterProfile = toml::from_str(contents)?;
            Ok(empty_profile(driver, name))
        }
        other => Err(unknown_driver(other)),
    }
}

/// Lowers one complete checked program through an explicitly selected adapter.
///
/// Selection has already happened through facility allocation. This function cannot infer a
/// driver from an Asset's manufacturer or model and cannot select a different adapter. The
/// profile is private operational configuration for the exact Asset binding, not a second facility model.
pub fn lower_dependency_build_with_adapter(
    driver: &str,
    name: &str,
    contents: &str,
    protocol: &ProtocolLairProgram,
    inventory: &BuildInventory,
) -> Result<ArtifactBundle, AdapterLoweringError> {
    match driver {
        "opentrons.ot2" => {
            let profile = Ot2AdapterProfile::parse(name, contents).map_err(|error| {
                AdapterLoweringError::InvalidProfile {
                    driver: driver.to_owned(),
                    message: error.to_string(),
                }
            })?;
            lab_compiler_ot2(protocol, &profile, inventory).map_err(|message| {
                AdapterLoweringError::Lowering {
                    driver: driver.to_owned(),
                    message,
                }
            })
        }
        "opentrons.flex" => {
            let profile = FlexAdapterProfile::parse(name, contents).map_err(|error| {
                AdapterLoweringError::InvalidProfile {
                    driver: driver.to_owned(),
                    message: error.to_string(),
                }
            })?;
            lab_compiler_flex(protocol, &profile, inventory).map_err(|message| {
                AdapterLoweringError::Lowering {
                    driver: driver.to_owned(),
                    message,
                }
            })
        }
        "hamilton.star" => {
            let profile = StarAdapterProfile::parse(name, contents).map_err(|error| {
                AdapterLoweringError::InvalidProfile {
                    driver: driver.to_owned(),
                    message: error.to_string(),
                }
            })?;
            lab_compiler_star(protocol, &profile, inventory).map_err(|message| {
                AdapterLoweringError::Lowering {
                    driver: driver.to_owned(),
                    message,
                }
            })
        }
        _ => Err(AdapterLoweringError::Unsupported {
            driver: driver.to_owned(),
        }),
    }
}

/// Lowers one exact allocated Procedure program through its already selected adapter.
///
/// The compatibility Protocol IR is derived from selected Procedure tasks and their exact values.
/// It does not revisit Workflow Intent or perform method selection.
pub fn lower_allocated_dependency_build_with_adapter(
    driver: &str,
    name: &str,
    contents: &str,
    allocated: &AllocatedLairProgram,
    inventory: &BuildInventory,
) -> Result<ArtifactBundle, AdapterLoweringError> {
    let protocol =
        allocated
            .dependency_build_protocol()
            .map_err(|error| AdapterLoweringError::Lowering {
                driver: driver.to_owned(),
                message: error.to_string(),
            })?;
    lower_dependency_build_with_adapter(driver, name, contents, &protocol, inventory)
}

/// One reviewed run document emitted for an exact allocated requirement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterInvocationDocument {
    pub requirement: LocalId,
    pub path: String,
    pub format: String,
}

/// Artifacts emitted by a requirement-scoped adapter invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterInvocationLowering {
    pub artifacts: ArtifactBundle,
    pub documents: Vec<AdapterInvocationDocument>,
}

/// Lower one exact allocated invocation without exposing LAIR or the rest of the experiment.
pub fn lower_adapter_invocation_with_adapter(
    driver: &str,
    invocation_plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
) -> Result<AdapterInvocationLowering, AdapterLoweringError> {
    invocation_plan
        .validate()
        .map_err(|error| AdapterLoweringError::InvalidInvocation {
            driver: driver.to_owned(),
            message: error.to_string(),
        })?;
    if invocation.adapter.driver != driver
        || !invocation_plan
            .invocations
            .iter()
            .any(|candidate| candidate == invocation)
    {
        return Err(AdapterLoweringError::InvalidInvocation {
            driver: driver.to_owned(),
            message: "the invocation is not an exact member of its validated plan".to_owned(),
        });
    }
    match driver {
        "lab.simulator" => lower_simulator_invocation(invocation_plan, invocation),
        _ => Err(AdapterLoweringError::UnsupportedInvocation {
            driver: driver.to_owned(),
        }),
    }
}

fn lower_simulator_invocation(
    invocation_plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
) -> Result<AdapterInvocationLowering, AdapterLoweringError> {
    let requirements = invocation_plan
        .methods
        .iter()
        .flat_map(|method| &method.tasks)
        .flat_map(|task| {
            task.requirements
                .iter()
                .map(move |requirement| (requirement.id.clone(), task))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut artifacts = ArtifactBundle::new();
    let mut documents = Vec::new();
    for (ordinal, requirement_id) in invocation.requirements.iter().enumerate() {
        let task = requirements
            .get(requirement_id)
            .expect("validated invocation requirements belong to exact tasks");
        let requirement = task
            .requirements
            .iter()
            .find(|requirement| &requirement.id == requirement_id)
            .expect("the requirement index preserves the owning requirement");
        let document = SimulationRunDocument {
            format: SIMULATION_RUN_FORMAT.to_owned(),
            id: requirement_id.to_string(),
            title: format!("Simulate {}", task.operation),
            capability_kind: requirement.capability_kind.to_string(),
            assumptions: vec![
                "Semantic simulation only; no physical hardware is contacted.".to_owned(),
                format!("Allocated Asset: {}", invocation.asset),
                format!("Procedure operation: {}", task.operation),
            ],
        };
        let path = format!(
            "requirement-{:03}-{}.simulation.json",
            ordinal + 1,
            short_digest(requirement_id.as_str())
        );
        let mut contents = serde_json::to_string_pretty(&document).map_err(|error| {
            AdapterLoweringError::Lowering {
                driver: invocation.adapter.driver.clone(),
                message: error.to_string(),
            }
        })?;
        contents.push('\n');
        artifacts
            .insert_text(&path, "application/json", contents)
            .map_err(|error| AdapterLoweringError::Lowering {
                driver: invocation.adapter.driver.clone(),
                message: error.to_string(),
            })?;
        documents.push(AdapterInvocationDocument {
            requirement: requirement_id.clone(),
            path,
            format: SIMULATION_RUN_FORMAT.to_owned(),
        });
    }
    Ok(AdapterInvocationLowering {
        artifacts,
        documents,
    })
}

fn short_digest(value: &str) -> String {
    sha256(value.as_bytes())[..8].to_owned()
}

fn lab_compiler_ot2(
    protocol: &ProtocolLairProgram,
    profile: &Ot2AdapterProfile,
    inventory: &BuildInventory,
) -> Result<ArtifactBundle, String> {
    crate::backend::opentrons::ot2::compile_dependency_build(protocol, profile, inventory)
        .map(|bundle| bundle.artifacts().clone())
        .map_err(|error| error.to_string())
}

fn lab_compiler_flex(
    protocol: &ProtocolLairProgram,
    profile: &FlexAdapterProfile,
    inventory: &BuildInventory,
) -> Result<ArtifactBundle, String> {
    crate::backend::opentrons::flex::compile_dependency_build(protocol, profile, inventory)
        .map(|bundle| bundle.artifacts().clone())
        .map_err(|error| error.to_string())
}

fn lab_compiler_star(
    protocol: &ProtocolLairProgram,
    profile: &StarAdapterProfile,
    inventory: &BuildInventory,
) -> Result<ArtifactBundle, String> {
    crate::backend::hamilton::star::compile_dependency_build(protocol, profile, inventory)
        .map(|bundle| bundle.artifacts().clone())
        .map_err(|error| error.to_string())
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AdapterLoweringError {
    #[error("adapter '{driver}' does not provide whole-program lowering")]
    Unsupported { driver: String },
    #[error("adapter '{driver}' does not provide requirement-scoped lowering")]
    UnsupportedInvocation { driver: String },
    #[error("invalid invocation for adapter '{driver}': {message}")]
    InvalidInvocation { driver: String, message: String },
    #[error("invalid operational profile for adapter '{driver}': {message}")]
    InvalidProfile { driver: String, message: String },
    #[error("adapter '{driver}' could not lower the allocated program: {message}")]
    Lowering { driver: String, message: String },
}

fn schema_value<T: JsonSchema>() -> Result<Value, AdapterProfileContractError> {
    let mut schema = serde_json::to_value(schema_for!(T))
        .map_err(|error| AdapterProfileContractError::Contract(error.to_string()))?;
    sanitize_schema_defaults(&mut schema);
    Ok(schema)
}

fn sanitize_schema_defaults(value: &mut Value) {
    let definitions = value.get("$defs").cloned().unwrap_or(Value::Null);
    sanitize_schema_node(value, &definitions);
}

fn sanitize_schema_node(value: &mut Value, definitions: &Value) {
    if let Some(object) = value.as_object_mut() {
        let property_names = closed_object_properties(object, definitions);
        if let (Some(property_names), Some(default)) = (
            property_names,
            object.get_mut("default").and_then(Value::as_object_mut),
        ) {
            default.retain(|name, _| property_names.contains(name));
        }
        for child in object.values_mut() {
            sanitize_schema_node(child, definitions);
        }
    } else if let Some(array) = value.as_array_mut() {
        for child in array {
            sanitize_schema_node(child, definitions);
        }
    }
}

fn closed_object_properties(
    object: &serde_json::Map<String, Value>,
    definitions: &Value,
) -> Option<BTreeSet<String>> {
    let closed_object = if object.get("additionalProperties") == Some(&Value::Bool(false)) {
        Some(object)
    } else {
        object
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|reference| reference.strip_prefix("#/$defs/"))
            .and_then(|name| definitions.get(name))
            .and_then(Value::as_object)
            .filter(|definition| {
                definition.get("additionalProperties") == Some(&Value::Bool(false))
            })
    }?;
    closed_object
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| properties.keys().cloned().collect())
}

fn canonical_adapter_profile<T: Serialize>(
    driver: &str,
    name: &str,
    profile: &T,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    let canonical_json = serde_json::to_value(profile)
        .map_err(|error| AdapterProfileContractError::Contract(error.to_string()))?;

    let toml_value = toml::Value::try_from(profile)
        .map_err(|error| AdapterProfileContractError::Contract(error.to_string()))?;
    let mut canonical_toml = toml::to_string_pretty(&toml_value)
        .map_err(|error| AdapterProfileContractError::Contract(error.to_string()))?;
    if !canonical_toml.ends_with('\n') {
        canonical_toml.push('\n');
    }
    Ok(ValidatedAdapterProfile {
        format: "lab.adapter-profile-validation.v2".to_owned(),
        schema_version: ADAPTER_PROFILE_SCHEMA_VERSION.to_owned(),
        compiler_version: env!("CARGO_PKG_VERSION").to_owned(),
        name: name.to_owned(),
        driver: driver.to_owned(),
        sha256: sha256(canonical_toml.as_bytes()),
        canonical_toml,
        canonical_json,
    })
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EmptyAdapterProfile {}

fn empty_profile(driver: &str, name: &str) -> ValidatedAdapterProfile {
    let canonical_toml = String::new();
    let sha256 = sha256(canonical_toml.as_bytes());
    ValidatedAdapterProfile {
        format: "lab.adapter-profile-validation.v2".to_owned(),
        schema_version: ADAPTER_PROFILE_SCHEMA_VERSION.to_owned(),
        compiler_version: env!("CARGO_PKG_VERSION").to_owned(),
        name: name.to_owned(),
        driver: driver.to_owned(),
        canonical_toml,
        canonical_json: json!({}),
        sha256,
    }
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn invalid(driver: &str, error: impl std::fmt::Display) -> AdapterProfileContractError {
    AdapterProfileContractError::Invalid {
        driver: driver.to_owned(),
        message: error.to_string(),
    }
}

fn unknown_driver(found: &str) -> AdapterProfileContractError {
    AdapterProfileContractError::UnknownDriver {
        found: found.to_owned(),
        known: KNOWN_ADAPTERS
            .iter()
            .map(|driver| format!("'{driver}'"))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::planning::{
        AllocatedMethod, AllocatedProcedureTask, AllocatedRequirementBinding, InvocationAdapter,
        MaterialLotBuildInventory,
    };
    use lab_capability::{MethodId, OperationId, QualificationLevel};
    use lab_method::IntentOperationId;

    #[test]
    fn registry_separates_semantic_capabilities_from_features() {
        let catalog = adapter_catalog().unwrap();

        assert_eq!(catalog.format, ADAPTER_CATALOG_FORMAT);
        assert_eq!(catalog.adapters.len(), KNOWN_ADAPTERS.len());
        let star = catalog
            .adapters
            .iter()
            .find(|adapter| adapter.id == "hamilton.star")
            .unwrap();
        assert_eq!(
            star.capabilities,
            [CapabilityKind::new(LIQUID_HANDLING).unwrap()]
                .into_iter()
                .collect()
        );
        assert!(star.features.contains("eight-channel"));
        assert!(
            !star
                .capabilities
                .iter()
                .any(|kind| kind.as_str() == "eight-channel")
        );
        assert!(star.control_modes.contains(&ControlMode::Api));
        assert!(star.accepted_run_formats.contains(STAR_RUN_FORMAT));
        assert_eq!(
            star.services.lowering,
            Some(AdapterLoweringScope::WholeProgram)
        );
        assert!(star.services.runtime);

        let simulator = catalog
            .adapters
            .iter()
            .find(|adapter| adapter.id == "lab.simulator")
            .unwrap();
        assert!(simulator.services.simulation);
        assert_eq!(
            simulator.services.lowering,
            Some(AdapterLoweringScope::Invocation)
        );
        assert!(!simulator.services.runtime);
        assert!(
            simulator
                .accepted_run_formats
                .contains(SIMULATION_RUN_FORMAT)
        );
        assert_eq!(
            simulator.capabilities,
            [
                LIQUID_HANDLING,
                THERMAL_CYCLING,
                INCUBATION,
                ABSORBANCE_MEASUREMENT,
            ]
            .into_iter()
            .map(|kind| CapabilityKind::new(kind).unwrap())
            .collect()
        );
    }

    #[test]
    fn simulator_lowers_each_exact_invocation_without_receiving_other_assets_work() {
        let adapter = InvocationAdapter {
            driver: "lab.simulator".to_owned(),
            profile_path: "adapters/simulator.toml".into(),
            profile_sha256: "b".repeat(64),
            features: BTreeSet::from(["no-hardware".to_owned(), "semantic-simulation".to_owned()]),
            accepted_run_formats: BTreeSet::from([SIMULATION_RUN_FORMAT.to_owned()]),
            emitted_run_formats: BTreeSet::from([SIMULATION_RUN_FORMAT.to_owned()]),
        };
        let make_task =
            |choice: &str, operation: &str, capability: &str, asset: &str, offering: &str| {
                let task_id = LocalId::new(format!("{choice}::task")).unwrap();
                let requirement_id =
                    LocalId::new(format!("{choice}::task::requirement::capability")).unwrap();
                let requirement = AllocatedRequirementBinding {
                    id: requirement_id.clone(),
                    capability_kind: CapabilityKind::new(capability).unwrap(),
                    minimum_qualification: QualificationLevel::Simulatable,
                    accepted_control_modes: BTreeSet::from([ControlMode::ReviewedFile]),
                    offering: offering.to_owned(),
                    asset: asset.to_owned(),
                    observed_qualification: QualificationLevel::Simulatable.to_string(),
                    control_mode: ControlMode::ReviewedFile.to_string(),
                    parameters: Vec::new(),
                    adapter: Some(adapter.clone()),
                };
                (
                    AllocatedMethod {
                        choice: LocalId::new(choice).unwrap(),
                        source_operation: IntentOperationId::new(format!(
                            "https://example.org/intent/{choice}"
                        ))
                        .unwrap(),
                        method: MethodId::new(format!("https://example.org/method/{choice}"))
                            .unwrap(),
                        tasks: vec![AllocatedProcedureTask {
                            id: task_id.clone(),
                            operation: OperationId::new(operation).unwrap(),
                            inputs: Vec::new(),
                            outputs: Vec::new(),
                            parameters: Vec::new(),
                            requirements: vec![requirement],
                        }],
                    },
                    task_id,
                    requirement_id,
                )
            };
        let first_asset = "https://example.org/facility/liquid-handler";
        let second_asset = "https://example.org/facility/reader";
        let (first_method, first_task, first_requirement) = make_task(
            "prepare",
            "https://example.org/procedure/prepare-plate",
            LIQUID_HANDLING,
            first_asset,
            "https://example.org/facility/liquid-handler/liquid-handling",
        );
        let (second_method, second_task, second_requirement) = make_task(
            "measure",
            "https://example.org/procedure/measure-plate",
            ABSORBANCE_MEASUREMENT,
            second_asset,
            "https://example.org/facility/reader/absorbance",
        );
        let make_invocation = |asset: &str, task: LocalId, requirement: LocalId| {
            let mut invocation = AdapterInvocation {
                id: String::new(),
                asset: asset.to_owned(),
                adapter: adapter.clone(),
                tasks: vec![task],
                requirements: vec![requirement],
            };
            invocation.id =
                crate::planning::adapter_invocation_id(&invocation.asset, &invocation.adapter);
            invocation
        };
        let first = make_invocation(first_asset, first_task, first_requirement.clone());
        let second = make_invocation(second_asset, second_task, second_requirement.clone());
        let plan = AdapterInvocationPlan {
            schema_version: crate::planning::ADAPTER_INVOCATIONS_SCHEMA_VERSION.to_owned(),
            problem_sha256: "a".repeat(64),
            allocated_lair_sha256: "c".repeat(64),
            inventory_sha256: "d".repeat(64),
            facility: "https://example.org/facility".to_owned(),
            material_inventory: MaterialLotBuildInventory {
                source_sha256: "d".repeat(64),
                facility: "https://example.org/facility".to_owned(),
                materials: BTreeMap::new(),
                artifacts: BTreeMap::new(),
            },
            methods: vec![first_method, second_method],
            invocations: vec![first.clone(), second.clone()],
        };
        plan.validate().unwrap();

        let first_lowered =
            lower_adapter_invocation_with_adapter("lab.simulator", &plan, &first).unwrap();
        let second_lowered =
            lower_adapter_invocation_with_adapter("lab.simulator", &plan, &second).unwrap();
        assert_eq!(first_lowered.documents[0].requirement, first_requirement);
        assert_eq!(second_lowered.documents[0].requirement, second_requirement);
        assert_eq!(first_lowered.artifacts.len(), 1);
        assert_eq!(second_lowered.artifacts.len(), 1);
        let first_document: SimulationRunDocument =
            serde_json::from_slice(first_lowered.artifacts.iter().next().unwrap().contents())
                .unwrap();
        let second_document: SimulationRunDocument =
            serde_json::from_slice(second_lowered.artifacts.iter().next().unwrap().contents())
                .unwrap();
        assert_eq!(first_document.capability_kind, LIQUID_HANDLING);
        assert_eq!(second_document.capability_kind, ABSORBANCE_MEASUREMENT);
        assert!(
            first_document
                .assumptions
                .iter()
                .all(|assumption| !assumption.contains(second_asset))
        );
    }

    #[test]
    fn explicit_driver_selects_the_profile_schema() {
        let wrong = validate_adapter_profile(
            "hamilton.star",
            "star-1",
            "[target]\nbackend = \"opentrons.ot2\"\n",
        )
        .unwrap_err()
        .to_string();
        assert!(wrong.contains("hamilton.star"), "{wrong}");
        assert!(wrong.contains("target"), "{wrong}");

        let flex = validate_adapter_profile("opentrons.flex", "flex-1", "").unwrap();
        assert_eq!(flex.driver, "opentrons.flex");
        assert!(flex.canonical_json.get("target").is_none());
        assert!(!flex.canonical_toml.contains("[target]"));
        let flex_descriptor = adapter_catalog()
            .unwrap()
            .adapters
            .into_iter()
            .find(|adapter| adapter.id == "opentrons.flex")
            .unwrap();
        assert!(
            flex_descriptor.profile_schema["properties"]
                .get("target")
                .is_none()
        );

        let profile = validate_adapter_profile("inheco.odtc", "cycler-1", "").unwrap();
        assert_eq!(profile.driver, "inheco.odtc");
        assert_eq!(profile.canonical_json, json!({}));
        assert_eq!(profile.sha256.len(), 64);
    }

    #[test]
    fn empty_profiles_reject_unknown_operational_configuration() {
        let error = validate_adapter_profile(
            "inheco.odtc",
            "cycler-1",
            "endpoint = \"192.0.2.10:8080\"\n",
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("unknown field `endpoint`"), "{error}");
    }
}
