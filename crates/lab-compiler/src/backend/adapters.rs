//! Compiler-owned adapter discovery and profile validation.
//!
//! An adapter is a Lab implementation, never a facility Asset. The manifest binds an adapter ID
//! to an exact SBOLInventory Asset IRI; this registry states which semantic capability offerings
//! and control modes that implementation can use. Product features stay separate from semantic
//! capability kinds so neither manufacturer nor model can silently select a driver.

use std::collections::BTreeSet;

use sbol_inventory::vocabulary::{
    ABSORBANCE_MEASUREMENT, ControlMode, LIQUID_HANDLING, THERMAL_CYCLING,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use crate::backend::hamilton::star::StarTargetProfile;
use crate::backend::opentrons::flex::FlexTargetProfile;
use crate::backend::opentrons::ot2::Ot2TargetProfile;
use crate::backend::target_profiles::{TargetProfile, TargetProfileContractError, schema_value};
use lab_runfmt::{STAR_RUN_FORMAT, THERMOCYCLE_RUN_FORMAT};

pub const ADAPTER_CATALOG_FORMAT: &str = "lab.adapter-catalog.v1";
pub const ADAPTER_PROFILE_SCHEMA_VERSION: &str = "lab.adapter-profile.v1";

const OPENTRONS_PYTHON_PROTOCOL: &str = "opentrons.python-protocol";
const OPENTRONS_PROTOCOL_DESIGNER: &str = "opentrons.protocol-designer-json";

const KNOWN_ADAPTERS: [&str; 5] = [
    "opentrons.ot2",
    "opentrons.flex",
    "hamilton.star",
    "inheco.odtc",
    "byonoy.absorbance96",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterServices {
    pub planning: bool,
    pub simulation: bool,
    pub runtime: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AdapterDescriptor {
    pub id: String,
    pub display_name: String,
    pub manufacturer: Option<String>,
    /// Exact SBOLInventory `fac:capabilityKind` IRIs this implementation supports.
    pub capabilities: BTreeSet<String>,
    /// Implementation facts that must never be used as semantic capability kinds.
    pub features: BTreeSet<String>,
    /// Exact closed SBOLInventory control-mode IRIs this implementation supports.
    pub control_modes: BTreeSet<String>,
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
    Contract(#[from] TargetProfileContractError),
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
                    simulation: false,
                    runtime: false,
                },
                schema_value::<Ot2TargetProfile>()?,
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
                    simulation: false,
                    runtime: false,
                },
                schema_value::<FlexTargetProfile>()?,
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
                    simulation: true,
                    runtime: true,
                },
                schema_value::<StarTargetProfile>()?,
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
                    simulation: false,
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
        capabilities: strings(capabilities),
        features: strings(features),
        control_modes: control_modes
            .into_iter()
            .map(|mode| mode.iri().to_owned())
            .collect(),
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
    let target = match driver {
        "opentrons.ot2" => Ot2TargetProfile::parse(name, contents)
            .map(TargetProfile::Ot2)
            .map_err(|error| invalid(driver, error))?,
        "opentrons.flex" => FlexTargetProfile::parse(name, contents)
            .map(TargetProfile::Flex)
            .map_err(|error| invalid(driver, error))?,
        "hamilton.star" => StarTargetProfile::parse(name, contents)
            .map(TargetProfile::Star)
            .map_err(|error| invalid(driver, error))?,
        "inheco.odtc" | "byonoy.absorbance96" => {
            let _: EmptyAdapterProfile = toml::from_str(contents)?;
            return Ok(empty_profile(driver, name));
        }
        other => return Err(unknown_driver(other)),
    };
    let profile = target.canonical(name)?;
    Ok(ValidatedAdapterProfile {
        format: "lab.adapter-profile-validation.v1".to_owned(),
        schema_version: ADAPTER_PROFILE_SCHEMA_VERSION.to_owned(),
        compiler_version: profile.compiler_version.to_owned(),
        name: profile.name,
        driver: driver.to_owned(),
        canonical_toml: profile.canonical_toml,
        canonical_json: profile.canonical_json,
        sha256: profile.sha256,
    })
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EmptyAdapterProfile {}

fn empty_profile(driver: &str, name: &str) -> ValidatedAdapterProfile {
    let canonical_toml = String::new();
    let sha256 = sha256(canonical_toml.as_bytes());
    ValidatedAdapterProfile {
        format: "lab.adapter-profile-validation.v1".to_owned(),
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
        assert_eq!(star.capabilities, strings([LIQUID_HANDLING]));
        assert!(star.features.contains("eight-channel"));
        assert!(!star.capabilities.contains("eight-channel"));
        assert!(star.control_modes.contains(ControlMode::Api.iri()));
        assert!(star.accepted_run_formats.contains(STAR_RUN_FORMAT));
        assert!(star.services.runtime);
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
        assert!(wrong.contains("opentrons.ot2"), "{wrong}");

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
