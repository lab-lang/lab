//! The complete planning and artifact-lowering extension contract for laboratory adapters.

use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    AdapterCatalog, AdapterDescriptor, AdapterDescriptorRegistry, AdapterInvocation,
    AdapterInvocationPlan, ArtifactBundle, LocalId, PlanningProcedureTask,
    ProcedureContractRegistry, ProcedureImplementationDescriptor, ProcedureImplementationId,
    ProgramFeature, ValidatedAdapterProfile,
};

pub const ADAPTER_CATALOG_FORMAT: &str = "lab.adapter-catalog.v4";
pub const ADAPTER_PROFILE_FORMAT: &str = "lab.adapter-profile-validation.v2";
pub const ADAPTER_PROFILE_SCHEMA_VERSION: &str = "lab.adapter-profile.v2";
pub const ADAPTER_API_VERSION: &str = env!("CARGO_PKG_VERSION");

pub type ProfileValidator =
    fn(&str, &str) -> Result<ValidatedAdapterProfile, AdapterProfileContractError>;
pub type ProgramFeasibility = fn(
    &ValidatedAdapterProfile,
    &ProcedureImplementationDescriptor,
    &PlanningProcedureTask,
    &ProcedureContractRegistry,
) -> Result<(), String>;
pub type InvocationLowerer = fn(
    &ValidatedAdapterProfile,
    &AdapterInvocationPlan,
    &AdapterInvocation,
    &ProcedureContractRegistry,
) -> Result<AdapterInvocationLowering, AdapterLoweringError>;

/// Canonicalize one typed operational profile into the record accepted by the registry.
///
/// Adapter validators should deserialize and semantically validate their private profile type,
/// then return this function's result. This keeps TOML formatting, JSON projection, schema
/// identity, and digest construction out of each integration.
pub fn canonical_adapter_profile<T: Serialize>(
    driver: &str,
    name: &str,
    profile: &T,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    let toml_value = toml::Value::try_from(profile)
        .map_err(|error| AdapterProfileContractError::Contract(error.to_string()))?;
    // Project JSON from the TOML value rather than serializing `profile` twice. TOML omits
    // `Option::None`, while serde_json normally renders it as `null`; one source value prevents
    // those two canonical forms from disagreeing.
    let canonical_json = serde_json::to_value(&toml_value)
        .map_err(|error| AdapterProfileContractError::Contract(error.to_string()))?;
    let mut canonical_toml = toml::to_string_pretty(&toml_value)
        .map_err(|error| AdapterProfileContractError::Contract(error.to_string()))?;
    if !canonical_toml.is_empty() && !canonical_toml.ends_with('\n') {
        canonical_toml.push('\n');
    }
    let profile = ValidatedAdapterProfile {
        format: ADAPTER_PROFILE_FORMAT.to_owned(),
        schema_version: ADAPTER_PROFILE_SCHEMA_VERSION.to_owned(),
        api_version: ADAPTER_API_VERSION.to_owned(),
        name: name.to_owned(),
        driver: driver.to_owned(),
        sha256: hex_sha256(canonical_toml.as_bytes()),
        canonical_toml,
        canonical_json,
    };
    validate_adapter_profile_record(driver, name, &profile)?;
    Ok(profile)
}

/// One reviewed run document emitted for exact allocated requirements.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterInvocationDocument {
    pub requirements: Vec<LocalId>,
    pub path: String,
    pub format: String,
}

/// All artifacts and reviewed documents emitted by one invocation lowerer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterInvocationLowering {
    pub artifacts: ArtifactBundle,
    pub documents: Vec<AdapterInvocationDocument>,
}

/// Every planning and lowering operation contributed by one adapter implementation.
///
/// Runtime integrations may attach type-erased records without making this focused API depend on
/// hardware or executor crates. Planning and lowering consumers never inspect those extensions.
#[derive(Clone)]
pub struct AdapterRegistration {
    pub descriptor: AdapterDescriptor,
    validate_profile: ProfileValidator,
    feasible: ProgramFeasibility,
    lower: InvocationLowerer,
    extensions: Vec<Arc<dyn Any + Send + Sync>>,
}

impl fmt::Debug for AdapterRegistration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdapterRegistration")
            .field("descriptor", &self.descriptor)
            .field("extensions", &self.extensions.len())
            .finish_non_exhaustive()
    }
}

impl AdapterRegistration {
    pub fn new(
        descriptor: AdapterDescriptor,
        validate_profile: ProfileValidator,
        feasible: ProgramFeasibility,
        lower: InvocationLowerer,
    ) -> Self {
        Self {
            descriptor,
            validate_profile,
            feasible,
            lower,
            extensions: Vec::new(),
        }
    }

    /// Attach optional integration-layer data while keeping the core contract dependency-light.
    pub fn with_extension<T: Any + Send + Sync>(mut self, extension: T) -> Self {
        self.extensions.push(Arc::new(extension));
        self
    }

    /// Iterate optional records of one integration-layer type.
    pub fn extensions<T: Any + Send + Sync>(&self) -> impl Iterator<Item = &T> {
        self.extensions
            .iter()
            .filter_map(|extension| extension.downcast_ref::<T>())
    }
}

/// One deterministic adapter composition shared by profile loading, planning, and lowering.
#[derive(Clone, Debug)]
pub struct AdapterRegistry {
    descriptors: AdapterDescriptorRegistry,
    registrations: BTreeMap<String, AdapterRegistration>,
}

impl AdapterRegistry {
    pub fn new(
        registrations: impl IntoIterator<Item = AdapterRegistration>,
    ) -> Result<Self, AdapterProfileContractError> {
        let mut by_id = BTreeMap::new();
        for registration in registrations {
            validate_registration(&registration)?;
            let id = registration.descriptor.id.clone();
            if by_id.insert(id.clone(), registration).is_some() {
                return Err(AdapterProfileContractError::Contract(format!(
                    "adapter '{id}' is registered more than once"
                )));
            }
        }
        let descriptors = AdapterDescriptorRegistry::new(
            by_id
                .values()
                .map(|registration| registration.descriptor.clone()),
        )
        .map_err(|error| AdapterProfileContractError::Contract(error.to_string()))?;
        Ok(Self {
            descriptors,
            registrations: by_id,
        })
    }

    pub fn descriptors(&self) -> &AdapterDescriptorRegistry {
        &self.descriptors
    }

    pub fn registrations(&self) -> impl Iterator<Item = &AdapterRegistration> {
        self.registrations.values()
    }

    pub fn registration(&self, id: &str) -> Option<&AdapterRegistration> {
        self.registrations.get(id)
    }

    pub fn with_registration(
        &self,
        registration: AdapterRegistration,
    ) -> Result<Self, AdapterProfileContractError> {
        Self::new(
            self.registrations()
                .cloned()
                .chain(std::iter::once(registration)),
        )
    }

    pub fn catalog(&self) -> AdapterCatalog {
        self.descriptors.catalog(
            ADAPTER_CATALOG_FORMAT,
            ADAPTER_API_VERSION,
            ADAPTER_PROFILE_SCHEMA_VERSION,
        )
    }

    /// Parse and centrally verify one operational profile.
    pub fn validate_profile(
        &self,
        driver: &str,
        name: &str,
        contents: &str,
    ) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
        let registration = self.registrations.get(driver).ok_or_else(|| {
            AdapterProfileContractError::UnknownDriver {
                found: driver.to_owned(),
                known: self
                    .registrations
                    .keys()
                    .map(|known| format!("'{known}'"))
                    .collect::<Vec<_>>()
                    .join(", "),
            }
        })?;
        let profile = (registration.validate_profile)(name, contents)?;
        validate_adapter_profile_record(driver, name, &profile)?;
        Ok(profile)
    }

    /// Validate and lower one exact member of an immutable invocation plan.
    pub fn lower_invocation(
        &self,
        profile: &ValidatedAdapterProfile,
        invocation_plan: &AdapterInvocationPlan,
        invocation: &AdapterInvocation,
        contracts: &ProcedureContractRegistry,
    ) -> Result<AdapterInvocationLowering, AdapterLoweringError> {
        let registration = self.registrations.get(&profile.driver).ok_or_else(|| {
            AdapterLoweringError::UnsupportedInvocation {
                driver: profile.driver.clone(),
            }
        })?;
        validate_invocation(
            self,
            registration,
            profile,
            invocation_plan,
            invocation,
            contracts,
        )?;
        (registration.lower)(profile, invocation_plan, invocation, contracts)
    }
}

impl crate::AdapterProgramFeasibility for AdapterRegistry {
    fn check_program(
        &self,
        adapter: &str,
        implementation: &ProcedureImplementationId,
        profile: &ValidatedAdapterProfile,
        task: &PlanningProcedureTask,
        contracts: &ProcedureContractRegistry,
    ) -> Result<(), String> {
        let registration = self
            .registrations
            .get(adapter)
            .ok_or_else(|| format!("adapter '{adapter}' is not registered"))?;
        if let Err(error) = validate_adapter_profile_record(adapter, &profile.name, profile) {
            return Err(error.to_string());
        }
        let implementation = registration
            .descriptor
            .procedure_implementations
            .iter()
            .find(|candidate| &candidate.id == implementation)
            .ok_or_else(|| {
                format!(
                    "Procedure implementation '{implementation}' is not registered by '{adapter}'"
                )
            })?;
        (registration.feasible)(profile, implementation, task, contracts)
    }
}

fn validate_registration(
    registration: &AdapterRegistration,
) -> Result<(), AdapterProfileContractError> {
    let expected = &registration.descriptor.default_profile;
    validate_adapter_profile_record(&registration.descriptor.id, &expected.name, expected)?;
    let reparsed = (registration.validate_profile)(&expected.name, &expected.canonical_toml)?;
    validate_adapter_profile_record(&registration.descriptor.id, &expected.name, &reparsed)?;
    if &reparsed != expected {
        return Err(AdapterProfileContractError::Contract(format!(
            "adapter '{}' default profile is not the exact canonical result of its validator",
            registration.descriptor.id
        )));
    }
    Ok(())
}

/// Revalidate an owned profile record at any persisted-data boundary.
///
/// Adapter registries call this automatically. Other consumers retaining a profile alongside
/// planning evidence can use the same API-owned format, version, canonicalization, and digest
/// checks instead of implementing a weaker copy.
pub fn validate_adapter_profile_record(
    expected_driver: &str,
    expected_name: &str,
    profile: &ValidatedAdapterProfile,
) -> Result<(), AdapterProfileContractError> {
    let invalid = |message: String| AdapterProfileContractError::Invalid {
        driver: expected_driver.to_owned(),
        message,
    };
    if profile.driver != expected_driver {
        return Err(invalid(format!(
            "validator returned driver '{}' instead of '{expected_driver}'",
            profile.driver
        )));
    }
    if profile.name != expected_name {
        return Err(invalid(format!(
            "validator returned profile name '{}' instead of '{expected_name}'",
            profile.name
        )));
    }
    if profile.format != ADAPTER_PROFILE_FORMAT {
        return Err(invalid(format!(
            "validator returned profile format '{}' instead of '{ADAPTER_PROFILE_FORMAT}'",
            profile.format
        )));
    }
    if profile.schema_version != ADAPTER_PROFILE_SCHEMA_VERSION {
        return Err(invalid(format!(
            "validator returned profile schema version '{}' instead of '{ADAPTER_PROFILE_SCHEMA_VERSION}'",
            profile.schema_version
        )));
    }
    if profile.api_version != ADAPTER_API_VERSION {
        return Err(invalid(format!(
            "validator returned adapter API version '{}' instead of '{ADAPTER_API_VERSION}'",
            profile.api_version
        )));
    }
    if !crate::is_sha256(&profile.sha256) {
        return Err(invalid(
            "sha256 must be exactly 64 lowercase hexadecimal characters".to_owned(),
        ));
    }
    let expected_sha256 = hex_sha256(profile.canonical_toml.as_bytes());
    if profile.sha256 != expected_sha256 {
        return Err(invalid(format!(
            "sha256 '{}' does not digest canonical_toml (expected '{expected_sha256}')",
            profile.sha256
        )));
    }
    let parsed: toml::Value = toml::from_str(&profile.canonical_toml)
        .map_err(|error| invalid(format!("canonical_toml is invalid TOML: {error}")))?;
    let mut canonical_toml = toml::to_string_pretty(&parsed)
        .map_err(|error| invalid(format!("canonical_toml cannot be serialized: {error}")))?;
    if !canonical_toml.is_empty() && !canonical_toml.ends_with('\n') {
        canonical_toml.push('\n');
    }
    if profile.canonical_toml != canonical_toml {
        return Err(invalid(
            "canonical_toml is not the canonical TOML representation".to_owned(),
        ));
    }
    let canonical_json = serde_json::to_value(parsed).map_err(|error| {
        invalid(format!(
            "canonical TOML cannot be represented as JSON: {error}"
        ))
    })?;
    if profile.canonical_json != canonical_json {
        return Err(invalid(
            "canonical_json does not describe the canonical TOML value".to_owned(),
        ));
    }
    Ok(())
}

fn validate_invocation(
    registry: &AdapterRegistry,
    registration: &AdapterRegistration,
    profile: &ValidatedAdapterProfile,
    invocation_plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
    contracts: &ProcedureContractRegistry,
) -> Result<(), AdapterLoweringError> {
    let driver = profile.driver.as_str();
    invocation_plan.validate(contracts).map_err(|error| {
        AdapterLoweringError::InvalidInvocation {
            driver: driver.to_owned(),
            message: error.to_string(),
        }
    })?;
    validate_adapter_profile_record(driver, &profile.name, profile).map_err(|error| {
        AdapterLoweringError::InvalidProfile {
            driver: driver.to_owned(),
            message: error.to_string(),
        }
    })?;
    let revalidated = registry
        .validate_profile(driver, &profile.name, &profile.canonical_toml)
        .map_err(|error| AdapterLoweringError::InvalidProfile {
            driver: driver.to_owned(),
            message: error.to_string(),
        })?;
    if revalidated != *profile {
        return Err(AdapterLoweringError::InvalidProfile {
            driver: driver.to_owned(),
            message: "validated profile differs from its canonical adapter representation"
                .to_owned(),
        });
    }
    if invocation.adapter.driver != driver
        || invocation.adapter.profile_sha256 != profile.sha256
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
    validate_invocation_implementation(
        &registration.descriptor,
        invocation_plan,
        invocation,
        contracts,
    )
    .map_err(|message| AdapterLoweringError::InvalidInvocation {
        driver: driver.to_owned(),
        message,
    })
}

fn validate_invocation_implementation(
    descriptor: &AdapterDescriptor,
    invocation_plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
    contracts: &ProcedureContractRegistry,
) -> Result<(), String> {
    let invocation_tasks = invocation.tasks.iter().collect::<BTreeSet<_>>();
    let invocation_requirements = invocation.requirements.iter().collect::<BTreeSet<_>>();
    for task in invocation_plan
        .allocated
        .methods
        .iter()
        .flat_map(|method| &method.tasks)
        .filter(|task| invocation_tasks.contains(&task.id))
    {
        let selected_requirements = task
            .requirements
            .iter()
            .filter(|requirement| invocation_requirements.contains(&requirement.id));
        let program = task.program.as_ref().ok_or_else(|| {
            format!(
                "adapter-bound task '{}' has no canonical Procedure program",
                task.id
            )
        })?;
        for requirement in selected_requirements {
            let implementation_id = requirement
                .procedure_implementation
                .as_ref()
                .expect("validated adapter work names an exact Procedure implementation");
            let implementation = descriptor
                .procedure_implementations
                .iter()
                .find(|implementation| &implementation.id == implementation_id)
                .ok_or_else(|| {
                    format!(
                        "Procedure implementation '{}' is not provided by adapter '{}'",
                        implementation_id, invocation.adapter.driver
                    )
                })?;
            if !implementation.services.lowering {
                return Err(format!(
                    "Procedure implementation '{}' does not provide lowering",
                    implementation.id
                ));
            }
            if implementation.contract != program.contract
                || !implementation
                    .capability_kinds
                    .contains(&requirement.capability_kind)
            {
                return Err(format!(
                    "Procedure implementation '{}' does not implement task '{}' contract '{}' for capability '{}'",
                    implementation.id, task.id, program.contract, requirement.capability_kind
                ));
            }
            let validated = program.clone().validate(contracts).map_err(|error| {
                format!(
                    "Procedure task '{}' has an invalid normalized program: {error}",
                    task.id
                )
            })?;
            let missing = validated
                .features()
                .difference(&implementation.program_features)
                .map(ProgramFeature::to_string)
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                return Err(format!(
                    "Procedure implementation '{}' cannot realize task '{}' contract '{}': its normalized program requires unsupported features {}",
                    implementation.id,
                    task.id,
                    program.contract,
                    missing.join(", ")
                ));
            }
        }
    }
    Ok(())
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AdapterLoweringError {
    #[error("adapter '{driver}' does not provide requirement-scoped lowering")]
    UnsupportedInvocation { driver: String },
    #[error("invalid invocation for adapter '{driver}': {message}")]
    InvalidInvocation { driver: String, message: String },
    #[error("invalid operational profile for adapter '{driver}': {message}")]
    InvalidProfile { driver: String, message: String },
    #[error("adapter '{driver}' could not lower the allocated program: {message}")]
    Lowering { driver: String, message: String },
}
