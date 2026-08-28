//! Compiler backend contracts, the planning every backend shares, and the
//! concrete execution targets.
//!
//! Three layers live here. The contracts — [`Backend`], [`BackendEmitter`],
//! [`AdapterDescriptor`], [`TargetConstraintError`] — say what a backend is.
//! The modules beside them are planning that holds for any liquid handler and
//! names no robot: provenance analysis over Protocol LAIR, projection into the
//! target-independent build graph, SBS plate geometry and well allocation, the
//! labware groupings every bench profile declares, and the rendering of a
//! dependency-driven build. Backend identity enters that planning only as a
//! parameter, so a capacity error names the machine that planned the build.
//!
//! Below both sits one module per vendor family — [`opentrons`] and
//! [`hamilton`] — holding one module per machine. A target implementation
//! owns the selection from
//! verified LAIR into a target IR, target validation and resource planning,
//! and concrete emitters. The language frontend and generic output renderers
//! deliberately do not depend on any target module.

mod adapters;
mod constraints;
mod document;
mod error;
mod graph;
pub mod hamilton;
mod markdown;
pub mod opentrons;
mod package;
mod profile;
mod resources;
mod target_profiles;
mod trace;
mod traits;
mod typst;

pub use adapters::{
    ADAPTER_CATALOG_FORMAT, ADAPTER_PROFILE_SCHEMA_VERSION, AdapterCatalog, AdapterDescriptor,
    AdapterLoweringError, AdapterProfileContractError, AdapterServices, ValidatedAdapterProfile,
    adapter_catalog, default_adapter_profile, lower_dependency_build_with_adapter,
    validate_adapter_profile,
};
pub use constraints::TargetConstraintError;
pub use target_profiles::{
    CAPABILITIES_FORMAT, KNOWN_BACKENDS, PROFILE_SCHEMA_VERSION, TargetCapabilitiesDocument,
    TargetCapability, TargetKind, TargetProfile, TargetProfileContractError, VALIDATION_FORMAT,
    ValidatedTargetProfile, default_target_profile, parse_target_profile, target_capabilities,
    validate_target_profile,
};
pub use traits::{Backend, BackendEmitter};
