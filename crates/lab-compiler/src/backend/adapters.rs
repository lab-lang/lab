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
use crate::planning::BuildInventory;
use crate::{AllocatedLairProgram, ArtifactBundle, ProtocolLairProgram};
use lab_runfmt::{SIMULATION_RUN_FORMAT, STAR_RUN_FORMAT, THERMOCYCLE_RUN_FORMAT};

pub const ADAPTER_CATALOG_FORMAT: &str = "lab.adapter-catalog.v1";
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
    /// This adapter can lower a complete checked Lab program into device artifacts.
    pub lowering: bool,
    pub simulation: bool,
    pub runtime: bool,
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
                    lowering: true,
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
                    lowering: true,
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
                    lowering: true,
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
                    lowering: false,
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
                    lowering: false,
                    simulation: false,
                    runtime: false,
                },
                schema_value::<EmptyAdapterProfile>()?,
            )?,
            descriptor(
                "lab.simulator",
                "Lab semantic capability simulator",
                None,
                [LIQUID_HANDLING, INCUBATION, ABSORBANCE_MEASUREMENT],
                ["no-hardware", "semantic-simulation"],
                [ControlMode::ReviewedFile],
                [SIMULATION_RUN_FORMAT],
                [SIMULATION_RUN_FORMAT],
                AdapterServices {
                    planning: true,
                    lowering: false,
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
    use super::*;

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
        assert!(star.services.lowering);
        assert!(star.services.runtime);

        let simulator = catalog
            .adapters
            .iter()
            .find(|adapter| adapter.id == "lab.simulator")
            .unwrap();
        assert!(simulator.services.simulation);
        assert!(!simulator.services.lowering);
        assert!(!simulator.services.runtime);
        assert!(
            simulator
                .accepted_run_formats
                .contains(SIMULATION_RUN_FORMAT)
        );
        assert_eq!(
            simulator.capabilities,
            [LIQUID_HANDLING, INCUBATION, ABSORBANCE_MEASUREMENT]
                .into_iter()
                .map(|kind| CapabilityKind::new(kind).unwrap())
                .collect()
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
