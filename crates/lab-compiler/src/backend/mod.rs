//! Compiler backend contracts, shared device planning, and concrete adapter implementations.
//!
//! Three layers live here. The contracts, including [`Backend`], [`BackendEmitter`], [`AdapterDescriptor`], and [`AdapterConstraintError`], define implementation behavior. The modules beside them hold planning shared by liquid-handler adapters: provenance analysis over Protocol LAIR, projection into the portable build graph, SBS plate geometry and well allocation, checked resource groupings, and dependency-build rendering. Adapter identity enters that planning only as a parameter, so a capacity error names the implementation that planned the device run.
//!
//! Below both sits one module per vendor family, [`opentrons`] and [`hamilton`], holding one module per adapter. An adapter owns selection from verified LAIR into a device IR, implementation validation and resource planning, and concrete emitters. The language frontend and generic output renderers deliberately do not depend on a device implementation.

mod adapters;
mod constraints;
mod document;
mod error;
mod graph;
pub mod hamilton;
mod invocation;
mod markdown;
pub mod opentrons;
mod package;
mod profile;
mod resources;
mod trace;
mod traits;
mod typst;

pub use adapters::{
    ADAPTER_CATALOG_FORMAT, ADAPTER_PROFILE_SCHEMA_VERSION, AdapterCatalog, AdapterDescriptor,
    AdapterInvocationDocument, AdapterInvocationLowering, AdapterLoweringError,
    AdapterLoweringScope, AdapterProfileContractError, AdapterServices, ValidatedAdapterProfile,
    adapter_catalog, default_adapter_profile, lower_adapter_invocation_with_adapter,
    lower_allocated_dependency_build_with_adapter, lower_dependency_build_with_adapter,
    validate_adapter_profile,
};
pub use constraints::AdapterConstraintError;
pub use traits::{Backend, BackendEmitter};
