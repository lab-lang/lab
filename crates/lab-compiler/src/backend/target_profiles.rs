//! Machine-readable target-profile discovery and validation.
//!
//! This module is the compiler-owned contract for control planes and editors.
//! Consumers discover schemas, defaults, catalog choices, and station kinds
//! here instead of copying Rust profile structs into another codebase.

use std::collections::BTreeSet;

use schemars::{JsonSchema, schema_for};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::backend::hamilton::star::catalog::{CARRIERS, LABWARE, LabwareRole};
use crate::backend::hamilton::star::{StarBackend, StarTargetProfile};
use crate::backend::opentrons::flex::{FlexBackend, FlexTargetProfile};
use crate::backend::opentrons::ot2::{Ot2Backend, Ot2TargetProfile};
use crate::backend::workcell::{StationKind, WorkcellProfile};
use crate::backend::{Backend, BackendDescriptor};

pub const CAPABILITIES_FORMAT: &str = "lab.target-capabilities.v0";
pub const PROFILE_SCHEMA_VERSION: &str = "lab.target-profile.v0";
pub const VALIDATION_FORMAT: &str = "lab.target-profile-validation.v0";

pub const KNOWN_BACKENDS: [&str; 4] = [
    "opentrons.ot2",
    "opentrons.flex",
    "hamilton.star",
    "workcell",
];

/// All target schemas and station kinds shipped by this compiler build.
#[derive(Clone, Debug, Serialize)]
pub struct TargetCapabilitiesDocument {
    pub format: &'static str,
    pub compiler_version: &'static str,
    pub profile_schema_version: &'static str,
    pub targets: Vec<TargetCapability>,
    pub station_kinds: Vec<StationCapability>,
}

/// One concrete value accepted by `[target] backend`.
#[derive(Clone, Debug, Serialize)]
pub struct TargetCapability {
    pub backend: &'static str,
    pub display_name: String,
    pub manufacturer: Option<String>,
    pub kind: TargetKind,
    pub capabilities: BTreeSet<String>,
    pub schema: Value,
    pub default_profile: ValidatedTargetProfile,
    /// Editor hints backed by the same catalogs and validators planning uses.
    pub catalog: Value,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TargetKind {
    LiquidHandler,
    Workcell,
}

/// A station kind a workcell may declare.
#[derive(Clone, Debug, Serialize)]
pub struct StationCapability {
    pub kind: &'static str,
    pub display_name: &'static str,
    pub manufacturer: &'static str,
    pub capabilities: BTreeSet<String>,
    pub runtime_executor: bool,
    pub planner_assigns_work: bool,
}

/// The canonical result of parsing and semantically validating one profile.
#[derive(Clone, Debug, Serialize)]
pub struct ValidatedTargetProfile {
    pub format: &'static str,
    pub schema_version: &'static str,
    pub compiler_version: &'static str,
    pub name: String,
    pub backend: &'static str,
    pub canonical_toml: String,
    pub canonical_json: Value,
    pub sha256: String,
}

#[derive(Debug, Error)]
pub enum TargetProfileContractError {
    #[error("failed to parse target profile header: {0}")]
    Header(#[from] toml::de::Error),
    #[error(
        "target profile declares backend '{found}', which this compiler does not provide; known backends are {known}"
    )]
    UnknownBackend { found: String, known: String },
    #[error("invalid {backend} target profile: {message}")]
    Invalid {
        backend: &'static str,
        message: String,
    },
    #[error("failed to serialize the validated target profile: {0}")]
    Serialize(String),
}

pub enum TargetProfile {
    Ot2(Ot2TargetProfile),
    Flex(FlexTargetProfile),
    Star(StarTargetProfile),
    Workcell(WorkcellProfile),
}

impl TargetProfile {
    pub fn backend(&self) -> &'static str {
        match self {
            Self::Ot2(_) => "opentrons.ot2",
            Self::Flex(_) => "opentrons.flex",
            Self::Star(_) => "hamilton.star",
            Self::Workcell(_) => "workcell",
        }
    }

    pub fn canonical(
        &self,
        name: &str,
    ) -> Result<ValidatedTargetProfile, TargetProfileContractError> {
        match self {
            Self::Ot2(profile) => canonical_profile(name, self.backend(), profile),
            Self::Flex(profile) => canonical_profile(name, self.backend(), profile),
            Self::Star(profile) => canonical_profile(name, self.backend(), profile),
            Self::Workcell(profile) => canonical_profile(name, self.backend(), profile),
        }
    }
}

/// Describe every target and station kind this exact compiler binary provides.
pub fn target_capabilities() -> Result<TargetCapabilitiesDocument, TargetProfileContractError> {
    Ok(TargetCapabilitiesDocument {
        format: CAPABILITIES_FORMAT,
        compiler_version: env!("CARGO_PKG_VERSION"),
        profile_schema_version: PROFILE_SCHEMA_VERSION,
        targets: vec![
            liquid_handler_capability::<Ot2TargetProfile>(
                "opentrons.ot2",
                Ot2Backend::default().descriptor(),
                ot2_catalog(),
            )?,
            liquid_handler_capability::<FlexTargetProfile>(
                "opentrons.flex",
                FlexBackend::default().descriptor(),
                flex_catalog(),
            )?,
            liquid_handler_capability::<StarTargetProfile>(
                "hamilton.star",
                StarBackend::default().descriptor(),
                star_catalog(),
            )?,
            TargetCapability {
                backend: "workcell",
                display_name: "Multi-station workcell".to_string(),
                manufacturer: None,
                kind: TargetKind::Workcell,
                capabilities: BTreeSet::from([
                    "coordination_plan".to_string(),
                    "human_handoff".to_string(),
                    "liquid_transfer".to_string(),
                    "thermocycler".to_string(),
                ]),
                schema: schema_value::<WorkcellProfile>()?,
                default_profile: default_target_profile("workcell", "workcell")?,
                catalog: json!({
                    "station_kinds": [
                        "hamilton.star",
                        "inheco.odtc",
                        "byonoy.absorbance96"
                    ],
                    "transport": ["human"],
                    "constraints": {
                        "liquid_handlers": { "kind": "hamilton.star", "exactly": 1 },
                        "instruments_of_each_kind": { "at_most": 1 }
                    }
                }),
            },
        ],
        station_kinds: station_capabilities(),
    })
}

/// Parse and semantically validate a profile, returning its canonical form.
pub fn validate_target_profile(
    name: &str,
    contents: &str,
) -> Result<ValidatedTargetProfile, TargetProfileContractError> {
    parse_target_profile(name, contents)?.canonical(name)
}

/// Return the complete canonical reference profile for one backend.
pub fn default_target_profile(
    backend: &str,
    name: &str,
) -> Result<ValidatedTargetProfile, TargetProfileContractError> {
    let profile = match backend {
        "opentrons.ot2" => TargetProfile::Ot2(
            Ot2TargetProfile::parse(name, "").map_err(|error| invalid("opentrons.ot2", error))?,
        ),
        "opentrons.flex" => TargetProfile::Flex(
            FlexTargetProfile::parse(name, "").map_err(|error| invalid("opentrons.flex", error))?,
        ),
        "hamilton.star" => TargetProfile::Star(
            StarTargetProfile::parse(name, "").map_err(|error| invalid("hamilton.star", error))?,
        ),
        "workcell" => TargetProfile::Workcell(
            WorkcellProfile::parse(
                name,
                r#"[target]
backend = "workcell"

[[station]]
name = "star-1"
kind = "hamilton.star"
profile = "hamilton-star"

[transport]
between = "human"
"#,
            )
            .map_err(|error| invalid("workcell", error))?,
        ),
        other => return Err(unknown_backend(other)),
    };
    profile.canonical(name)
}

pub fn parse_target_profile(
    name: &str,
    contents: &str,
) -> Result<TargetProfile, TargetProfileContractError> {
    let table = contents.parse::<toml::Table>()?;
    let backend = table
        .get("target")
        .and_then(|target| target.get("backend"))
        .and_then(toml::Value::as_str)
        .unwrap_or("opentrons.ot2");
    match backend {
        "opentrons.ot2" => Ot2TargetProfile::parse(name, contents)
            .map(TargetProfile::Ot2)
            .map_err(|error| invalid("opentrons.ot2", error)),
        "opentrons.flex" => FlexTargetProfile::parse(name, contents)
            .map(TargetProfile::Flex)
            .map_err(|error| invalid("opentrons.flex", error)),
        "hamilton.star" => StarTargetProfile::parse(name, contents)
            .map(TargetProfile::Star)
            .map_err(|error| invalid("hamilton.star", error)),
        "workcell" => WorkcellProfile::parse(name, contents)
            .map(TargetProfile::Workcell)
            .map_err(|error| invalid("workcell", error)),
        other => Err(unknown_backend(other)),
    }
}

fn liquid_handler_capability<T>(
    backend: &'static str,
    descriptor: BackendDescriptor,
    catalog: Value,
) -> Result<TargetCapability, TargetProfileContractError>
where
    T: JsonSchema,
{
    let target = descriptor
        .targets
        .into_iter()
        .next()
        .expect("each shipped machine backend has one target descriptor");
    Ok(TargetCapability {
        backend,
        display_name: target.display_name,
        manufacturer: descriptor.manufacturer,
        kind: TargetKind::LiquidHandler,
        capabilities: target.capabilities,
        schema: schema_value::<T>()?,
        default_profile: default_target_profile(backend, backend)?,
        catalog,
    })
}

fn schema_value<T: JsonSchema>() -> Result<Value, TargetProfileContractError> {
    let mut schema = serde_json::to_value(schema_for!(T))
        .map_err(|error| TargetProfileContractError::Serialize(error.to_string()))?;
    sanitize_schema_defaults(&mut schema);
    Ok(schema)
}

/// Schemars sees the loader-supplied profile name during `Default`
/// serialization even though serde correctly omits it from deserialization.
/// Remove any such value from closed-object defaults so every advertised
/// default validates against the schema that advertises it.
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

fn canonical_profile<T: Serialize>(
    name: &str,
    backend: &'static str,
    profile: &T,
) -> Result<ValidatedTargetProfile, TargetProfileContractError> {
    let mut canonical_json = serde_json::to_value(profile)
        .map_err(|error| TargetProfileContractError::Serialize(error.to_string()))?;
    remove_derived_name(&mut canonical_json);

    let mut toml_value = toml::Value::try_from(profile)
        .map_err(|error| TargetProfileContractError::Serialize(error.to_string()))?;
    remove_derived_toml_name(&mut toml_value);
    let mut canonical_toml = toml::to_string_pretty(&toml_value)
        .map_err(|error| TargetProfileContractError::Serialize(error.to_string()))?;
    if !canonical_toml.ends_with('\n') {
        canonical_toml.push('\n');
    }
    let sha256 = Sha256::digest(canonical_toml.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(ValidatedTargetProfile {
        format: VALIDATION_FORMAT,
        schema_version: PROFILE_SCHEMA_VERSION,
        compiler_version: env!("CARGO_PKG_VERSION"),
        name: name.to_string(),
        backend,
        canonical_toml,
        canonical_json,
        sha256,
    })
}

fn remove_derived_name(value: &mut Value) {
    if let Some(target) = value.get_mut("target").and_then(Value::as_object_mut) {
        target.remove("name");
    }
}

fn remove_derived_toml_name(value: &mut toml::Value) {
    if let Some(target) = value.get_mut("target").and_then(toml::Value::as_table_mut) {
        target.remove("name");
    }
}

fn invalid(backend: &'static str, error: impl std::fmt::Display) -> TargetProfileContractError {
    TargetProfileContractError::Invalid {
        backend,
        message: error.to_string(),
    }
}

fn unknown_backend(found: &str) -> TargetProfileContractError {
    TargetProfileContractError::UnknownBackend {
        found: found.to_string(),
        known: KNOWN_BACKENDS
            .iter()
            .map(|backend| format!("'{backend}'"))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

fn station_capabilities() -> Vec<StationCapability> {
    vec![
        StationCapability {
            kind: StationKind::HamiltonStar.as_str(),
            display_name: "Hamilton STAR/STARlet",
            manufacturer: "Hamilton",
            capabilities: BTreeSet::from(["liquid_transfer".to_string()]),
            runtime_executor: true,
            planner_assigns_work: true,
        },
        StationCapability {
            kind: StationKind::InhecoOdtc.as_str(),
            display_name: "Inheco ODTC",
            manufacturer: "Inheco",
            capabilities: BTreeSet::from(["thermocycler".to_string()]),
            runtime_executor: true,
            planner_assigns_work: true,
        },
        StationCapability {
            kind: StationKind::ByonoyAbsorbance96.as_str(),
            display_name: "Byonoy Absorbance 96",
            manufacturer: "Byonoy",
            capabilities: BTreeSet::from(["absorbance_plate_read".to_string()]),
            runtime_executor: false,
            planner_assigns_work: false,
        },
    ]
}

fn ot2_catalog() -> Value {
    json!({
        "slots": ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11"],
        "mounts": ["left", "right"],
        "reference_pipettes": ["p20_single_gen2", "p300_single_gen2"],
        "reference_modules": ["temperature module gen2", "thermocycler module gen2"],
        "fixed_thermocycler_slots": ["7", "8", "10", "11"]
    })
}

fn flex_catalog() -> Value {
    json!({
        "slots": ["A1", "A2", "A3", "B1", "B2", "B3", "C1", "C2", "C3", "D1", "D2", "D3"],
        "mounts": ["left", "right"],
        "pipettes": [
            "p50_single_flex",
            "p50_multi_flex",
            "p1000_single_flex",
            "p1000_multi_flex",
            "p1000_96"
        ],
        "trash_areas": [
            "movableTrashA1", "movableTrashB1", "movableTrashC1", "movableTrashD1",
            "movableTrashA3", "movableTrashB3", "movableTrashC3", "movableTrashD3"
        ],
        "fixed_thermocycler_slots": ["A1", "B1"]
    })
}

fn star_catalog() -> Value {
    let carriers = CARRIERS
        .iter()
        .map(|carrier| {
            json!({
                "id": carrier.id,
                "display_name": carrier.hamilton_model,
                "width_rails": carrier.width_rails,
                "sites": carrier.sites.len()
            })
        })
        .collect::<Vec<_>>();
    let labware = LABWARE
        .iter()
        .map(|labware| {
            json!({
                "id": labware.id,
                "display_name": labware.display,
                "capacity": labware.capacity,
                "role": match labware.role {
                    LabwareRole::Vessel { .. } => "vessel",
                    LabwareRole::TipRack { .. } => "tip-rack",
                }
            })
        })
        .collect::<Vec<_>>();
    json!({
        "machine_variants": [
            { "id": "starlet", "rails": 32 },
            { "id": "star", "rails": 56 }
        ],
        "channel_counts": [8],
        "lld_policies": ["off", "gamma"],
        "carriers": carriers,
        "labware": labware
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_names_every_backend_and_station_kind() {
        let document = target_capabilities().unwrap();
        assert_eq!(
            document
                .targets
                .iter()
                .map(|target| target.backend)
                .collect::<Vec<_>>(),
            KNOWN_BACKENDS
        );
        assert_eq!(document.station_kinds.len(), 3);
        assert!(
            document
                .station_kinds
                .iter()
                .any(|station| station.kind == "byonoy.absorbance96"
                    && !station.runtime_executor
                    && !station.planner_assigns_work)
        );
    }

    #[test]
    fn every_default_is_complete_canonical_and_revalidates() {
        for backend in KNOWN_BACKENDS {
            let profile = default_target_profile(backend, "bench").unwrap();
            assert_eq!(profile.backend, backend);
            assert_eq!(profile.sha256.len(), 64);
            let canonical = profile.canonical_toml.parse::<toml::Table>().unwrap();
            assert!(
                canonical
                    .get("target")
                    .and_then(toml::Value::as_table)
                    .and_then(|target| target.get("name"))
                    .is_none()
            );
            let round_trip = validate_target_profile("bench", &profile.canonical_toml).unwrap();
            assert_eq!(round_trip.sha256, profile.sha256);
            assert_eq!(round_trip.canonical_json, profile.canonical_json);
        }
    }

    #[test]
    fn validation_runs_backend_semantics_not_only_toml_parsing() {
        let error = validate_target_profile(
            "bad-flex",
            r#"[target]
backend = "opentrons.flex"

[instruments.small]
model = "p20_single_gen2"
mount = "left"
"#,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("not a Flex pipette"), "{error}");
    }

    #[test]
    fn schemas_reject_unknown_fields() {
        let document = target_capabilities().unwrap();
        for target in document.targets {
            assert_eq!(
                target.schema["additionalProperties"], false,
                "{}",
                target.backend
            );
            let metadata = &target.schema["$defs"]["TargetMetadata"];
            assert!(metadata["properties"].get("name").is_none());
            assert!(
                target.schema["properties"]["target"]["default"]
                    .get("name")
                    .is_none()
            );
        }
    }
}
