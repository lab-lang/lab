//! Implementation-independent contracts for composing laboratory adapters.
//!
//! Adapter authors depend on this crate for immutable descriptors, operational-profile
//! validation, planning feasibility, invocation records, artifact bundles, and lowering
//! registration. Concrete device dependencies and optional runtime execution belong to adapter
//! implementation crates.

mod artifact;
mod invocation;
mod registration;

pub use artifact::{ArtifactBundle, ArtifactError, GeneratedArtifact};
pub use invocation::{
    ADAPTER_INVOCATIONS_SCHEMA_VERSION, AdapterInvocation, AdapterInvocationError,
    AdapterInvocationPlan, AdapterInvocationValidationError, adapter_invocation_id, hex_sha256,
    is_sha256,
};
pub use registration::{
    ADAPTER_API_VERSION, ADAPTER_CATALOG_FORMAT, ADAPTER_PROFILE_FORMAT,
    ADAPTER_PROFILE_SCHEMA_VERSION, AdapterInvocationDocument, AdapterInvocationLowering,
    AdapterLoweringError, AdapterProfileContractError, AdapterRegistration, AdapterRegistry,
    InvocationLowerer, ProfileValidator, ProgramFeasibility, canonical_adapter_profile,
    validate_adapter_profile_record,
};

// Adapter registrations exchange these values in their public callbacks. Re-export the focused
// semantic model here so an adapter implementation never needs to know which compiler crate owns
// a canonical Procedure or facility-allocation record.
pub use lab_capability::{
    CapabilityKind, ControlMode, ExactDecimal, ExactInteger, OperationId, ProcedureContractId,
    ProcedureImplementationId, PropertyConstraint, PropertyKind, PropertyValue, QualificationLevel,
    ScalarValue, UnitIri,
};
pub use lab_compiler::allocation::{
    AllocatedMethod, AllocatedProcedureTask, AllocatedProgram, AllocatedRequirementBinding,
    InvocationAdapter,
};
pub use lab_compiler::method::{LocalId, PortType, ProcedureValue};
pub use lab_compiler::planning::{
    PlanningCapabilityRequirement, PlanningMaterialInput, PlanningMaterialSource,
    PlanningProcedureParameter, PlanningProcedureTask, PlanningTaskInput, PlanningTaskOutput,
    PlanningValueSource, SelectedCapabilityParameter, SelectedMaterialBinding,
    SelectedMaterialSource,
};
pub use lab_compiler::procedure::vocabulary::{
    AIR_GAP_HANDLING, CONTROLLED_TEMPERATURE_RAMP, DEGREE_CELSIUS, DEGREE_CELSIUS_PER_SECOND, GRAM,
    GRAM_PER_LITRE, HEATED_LID_TEMPERATURE_CONTROL, IN_WELL_MIXING, LIQUID_LEVEL_AWARE_ASPIRATION,
    MAXIMUM_AIR_GAP_VOLUME, MAXIMUM_BLOCK_TEMPERATURE, MAXIMUM_LID_TEMPERATURE, MAXIMUM_MIX_VOLUME,
    MAXIMUM_RAMP_RATE, MAXIMUM_SAMPLE_COUNT, MAXIMUM_TEMPERATURE, MAXIMUM_THERMAL_SAMPLE_VOLUME,
    MAXIMUM_TRANSFER_VOLUME, METERED_LIQUID_TRANSFER, MICROLITRE, MILLIMETRE,
    MINIMUM_BLOCK_TEMPERATURE, MINIMUM_LID_TEMPERATURE, MINIMUM_TEMPERATURE,
    MINIMUM_THERMAL_SAMPLE_VOLUME, MINIMUM_TRANSFER_VOLUME, PIPETTING_PROGRAM_V1,
    POST_DISPENSE_BLOWOUT, PROGRAMMED_BLOCK_TEMPERATURE_CONTROL, SECOND,
    TEMPERATURE_CONTROLLED_STAGING, THERMAL_PROGRAM_V1, TOUCH_TIP, VESSEL_RELATIVE_LIQUID_ACCESS,
};
pub use lab_compiler::procedure::{
    AspirationStrategy, BindingScope, DispenseStrategy, Duration, FluidPathPolicy, Length,
    LiquidLedger, Location, Mass, MassConcentration, MaterialInput, MaterialOutput, MixTechnique,
    PipettingConstraints, PipettingProgramV1, PipettingProgramValidationError, PipettingStep,
    ProcedureContractRegistry, ProcedureLocalId, ProcedureProgram, ProcedureProgramDecodeError,
    ProcedureProgramValidationError, ProgramFeature, Temperature, TemperatureRampRate,
    TemperatureRange, ThermalLoad, ThermalProgramV1, ThermalProgramValidationError, ThermalStage,
    ThermalStep, TransferTechnique, ValidatedPipettingProgramV1, ValidatedProcedureProgram,
    ValidatedThermalProgramV1, Vessel, VesselRole, Volume, VolumeConflict,
};

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Which parts of the compile-to-run path an adapter implementation provides.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterServices {
    pub planning: bool,
    pub lowering: bool,
    pub simulation: bool,
    pub runtime: bool,
}

/// One implementation of a versioned, device-neutral Procedure contract.
///
/// Biological operation names are deliberately absent. If the registered feature set covers a
/// validated program and the profile feasibility check accepts it, the implementation claims it
/// can lower that program regardless of which scientific Method produced it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProcedureImplementationDescriptor {
    pub id: ProcedureImplementationId,
    pub contract: ProcedureContractId,
    pub capability_kinds: BTreeSet<CapabilityKind>,
    pub control_modes: BTreeSet<ControlMode>,
    pub accepted_run_formats: BTreeSet<String>,
    pub emitted_run_formats: BTreeSet<String>,
    pub program_features: BTreeSet<ProgramFeature>,
    pub services: AdapterServices,
}

/// Public facts for one concrete adapter integration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterDescriptor {
    pub id: String,
    pub display_name: String,
    pub manufacturer: Option<String>,
    /// Product facts that are never used for semantic or runtime routing.
    pub features: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub procedure_implementations: Vec<ProcedureImplementationDescriptor>,
    pub profile_schema: Value,
    pub default_profile: ValidatedAdapterProfile,
}

/// Canonical, digest-bound result of one concrete adapter's profile parser.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValidatedAdapterProfile {
    pub format: String,
    pub schema_version: String,
    pub api_version: String,
    pub name: String,
    pub driver: String,
    pub canonical_toml: String,
    pub canonical_json: Value,
    pub sha256: String,
}

/// Serializable description of the adapters linked into one application composition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterCatalog {
    pub format: String,
    pub api_version: String,
    pub profile_schema_version: String,
    pub adapters: Vec<AdapterDescriptor>,
}

/// Deterministically indexed adapter descriptions injected into facility binding.
#[derive(Clone, Debug)]
pub struct AdapterDescriptorRegistry {
    registrations: BTreeMap<String, AdapterDescriptor>,
}

/// Pure planning-time feasibility supplied by the adapters linked into an application.
///
/// Facility planning owns selection, but it must not select a profile that the eventual lowerer
/// already knows cannot realize the canonical program. Implementations receive only immutable,
/// validated data and must not allocate resources, write artifacts, or contact hardware.
pub trait AdapterProgramFeasibility: Send + Sync {
    fn check_program(
        &self,
        adapter: &str,
        implementation: &ProcedureImplementationId,
        profile: &ValidatedAdapterProfile,
        task: &PlanningProcedureTask,
        contracts: &ProcedureContractRegistry,
    ) -> Result<(), String>;
}

impl AdapterDescriptorRegistry {
    pub fn new(
        registrations: impl IntoIterator<Item = AdapterDescriptor>,
    ) -> Result<Self, AdapterDescriptorRegistryError> {
        let mut by_id = BTreeMap::new();
        let mut implementation_ids = BTreeSet::new();
        for descriptor in registrations {
            if descriptor.id.is_empty()
                || descriptor
                    .id
                    .chars()
                    .any(|character| character.is_whitespace() || character.is_control())
            {
                return Err(AdapterDescriptorRegistryError::InvalidAdapterId { id: descriptor.id });
            }
            if descriptor.default_profile.driver != descriptor.id {
                return Err(
                    AdapterDescriptorRegistryError::DefaultProfileDriverMismatch {
                        adapter: descriptor.id,
                        profile_driver: descriptor.default_profile.driver,
                    },
                );
            }
            for implementation in &descriptor.procedure_implementations {
                if !implementation_ids.insert(implementation.id.clone()) {
                    return Err(AdapterDescriptorRegistryError::DuplicateImplementation {
                        id: implementation.id.clone(),
                    });
                }
            }
            let id = descriptor.id.clone();
            if by_id.insert(id.clone(), descriptor).is_some() {
                return Err(AdapterDescriptorRegistryError::DuplicateAdapter { id });
            }
        }
        Ok(Self {
            registrations: by_id,
        })
    }

    pub fn descriptor(&self, id: &str) -> Option<&AdapterDescriptor> {
        self.registrations.get(id)
    }

    pub fn descriptors(&self) -> impl Iterator<Item = &AdapterDescriptor> {
        self.registrations.values()
    }

    pub fn catalog(
        &self,
        format: impl Into<String>,
        api_version: impl Into<String>,
        profile_schema_version: impl Into<String>,
    ) -> AdapterCatalog {
        AdapterCatalog {
            format: format.into(),
            api_version: api_version.into(),
            profile_schema_version: profile_schema_version.into(),
            adapters: self.registrations.values().cloned().collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum AdapterDescriptorRegistryError {
    #[error("adapter ID `{id}` must be non-empty and contain no whitespace or controls")]
    InvalidAdapterId { id: String },
    #[error("adapter `{id}` is registered more than once")]
    DuplicateAdapter { id: String },
    #[error("Procedure implementation `{id}` is registered more than once")]
    DuplicateImplementation { id: ProcedureImplementationId },
    #[error("adapter `{adapter}` has a default profile for different driver `{profile_driver}`")]
    DefaultProfileDriverMismatch {
        adapter: String,
        profile_driver: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_profile(
        name: &str,
        contents: &str,
    ) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
        if !contents.is_empty() {
            return Err(AdapterProfileContractError::Invalid {
                driver: "adapter".to_owned(),
                message: "this fixture accepts an empty profile".to_owned(),
            });
        }
        Ok(profile("adapter", name))
    }

    fn wrong_digest_profile(
        name: &str,
        _contents: &str,
    ) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
        let mut profile = profile("adapter", name);
        profile.sha256 = "0".repeat(64);
        Ok(profile)
    }

    fn wrong_format_profile(
        name: &str,
        _contents: &str,
    ) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
        let mut profile = profile("adapter", name);
        profile.format = "adapter.profile.custom".to_owned();
        Ok(profile)
    }

    fn wrong_schema_profile(
        name: &str,
        _contents: &str,
    ) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
        let mut profile = profile("adapter", name);
        profile.schema_version = "lab.adapter-profile.v999".to_owned();
        Ok(profile)
    }

    fn wrong_api_profile(
        name: &str,
        _contents: &str,
    ) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
        let mut profile = profile("adapter", name);
        profile.api_version = "999.0.0".to_owned();
        Ok(profile)
    }

    fn profile(driver: &str, name: &str) -> ValidatedAdapterProfile {
        ValidatedAdapterProfile {
            format: ADAPTER_PROFILE_FORMAT.to_owned(),
            schema_version: ADAPTER_PROFILE_SCHEMA_VERSION.to_owned(),
            api_version: ADAPTER_API_VERSION.to_owned(),
            name: name.to_owned(),
            driver: driver.to_owned(),
            canonical_toml: String::new(),
            canonical_json: serde_json::json!({}),
            sha256: hex_sha256(b""),
        }
    }

    fn descriptor(id: &str) -> AdapterDescriptor {
        AdapterDescriptor {
            id: id.to_owned(),
            display_name: id.to_owned(),
            manufacturer: None,
            features: BTreeSet::new(),
            procedure_implementations: Vec::new(),
            profile_schema: serde_json::json!({}),
            default_profile: profile(id, "default"),
        }
    }

    #[test]
    fn registry_order_is_stable_and_duplicate_ids_fail() {
        let registry = AdapterDescriptorRegistry::new([descriptor("z"), descriptor("a")]).unwrap();
        assert_eq!(
            registry
                .descriptors()
                .map(|descriptor| descriptor.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "z"]
        );
        assert!(matches!(
            AdapterDescriptorRegistry::new([descriptor("same"), descriptor("same")]),
            Err(AdapterDescriptorRegistryError::DuplicateAdapter { .. })
        ));
    }

    #[test]
    fn default_profiles_cannot_select_a_different_driver() {
        let mut invalid = descriptor("adapter");
        invalid.default_profile.driver = "other".to_owned();
        assert!(matches!(
            AdapterDescriptorRegistry::new([invalid]),
            Err(AdapterDescriptorRegistryError::DefaultProfileDriverMismatch { .. })
        ));
    }

    #[test]
    fn complete_registration_uses_only_the_focused_api() {
        let registry = AdapterRegistry::new([AdapterRegistration::new(
            descriptor("adapter"),
            valid_profile,
            |_profile, _implementation, _task, _contracts| Ok(()),
            |_profile, _plan, _invocation, _contracts| {
                Ok(AdapterInvocationLowering {
                    artifacts: ArtifactBundle::new(),
                    documents: Vec::new(),
                })
            },
        )])
        .unwrap();
        let checked = registry.validate_profile("adapter", "bench", "").unwrap();
        assert_eq!(checked.sha256, hex_sha256(b""));
    }

    #[test]
    fn registry_rejects_a_validator_that_lies_about_the_digest() {
        let error = AdapterRegistry::new([AdapterRegistration::new(
            descriptor("adapter"),
            wrong_digest_profile,
            |_profile, _implementation, _task, _contracts| Ok(()),
            |_profile, _plan, _invocation, _contracts| {
                Err(AdapterLoweringError::UnsupportedInvocation {
                    driver: "adapter".to_owned(),
                })
            },
        )])
        .unwrap_err();
        assert!(error.to_string().contains("does not digest canonical_toml"));
    }

    #[test]
    fn api_canonicalization_owns_version_and_empty_profile_encoding() {
        let profile =
            canonical_adapter_profile("adapter", "empty", &serde_json::json!({})).unwrap();
        assert_eq!(profile.format, ADAPTER_PROFILE_FORMAT);
        assert_eq!(profile.schema_version, ADAPTER_PROFILE_SCHEMA_VERSION);
        assert_eq!(profile.api_version, ADAPTER_API_VERSION);
        assert_eq!(profile.canonical_toml, "");
        assert_eq!(profile.canonical_json, serde_json::json!({}));
        assert_eq!(
            profile.sha256,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn registry_rejects_noncanonical_profile_metadata_versions() {
        let rejected = |validator: ProfileValidator| {
            AdapterRegistry::new([AdapterRegistration::new(
                descriptor("adapter"),
                validator,
                |_profile, _implementation, _task, _contracts| Ok(()),
                |_profile, _plan, _invocation, _contracts| {
                    Err(AdapterLoweringError::UnsupportedInvocation {
                        driver: "adapter".to_owned(),
                    })
                },
            )])
            .unwrap_err()
        };

        let error = rejected(wrong_format_profile);
        assert!(
            error.to_string().contains(ADAPTER_PROFILE_FORMAT),
            "{error}"
        );

        let error = rejected(wrong_schema_profile);
        assert!(
            error.to_string().contains(ADAPTER_PROFILE_SCHEMA_VERSION),
            "{error}"
        );

        let error = rejected(wrong_api_profile);
        assert!(error.to_string().contains(ADAPTER_API_VERSION), "{error}");
    }
}
