//! Statically linked application extensions shared by the CLI and Python SDK.
//!
//! Add a crate dependency to `lab-project` and compose its registration in
//! `application_extensions`. Both frontends then use that exact build composition
//! for discovery, checking, refinement, planning, lowering, and reviewed execution.

use lab_adapter_api::{AdapterProfileContractError, AdapterRegistry};
use lab_compiler::procedure::{ProcedureCompiler, ProcedureCompilerError};
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct ApplicationExtensions {
    pub adapters: AdapterRegistry,
    pub procedures: ProcedureCompiler,
}

#[derive(Debug, Error)]
pub enum ApplicationExtensionError {
    #[error("invalid adapter composition: {0}")]
    Adapters(#[from] AdapterProfileContractError),
    #[error("invalid Procedure composition: {0}")]
    Procedures(#[from] ProcedureCompilerError),
}

/// The single composition point for the installed Lab application.
/// Embedders may construct a different value and call `load_with_extensions`.
pub fn application_extensions() -> Result<ApplicationExtensions, ApplicationExtensionError> {
    Ok(ApplicationExtensions {
        adapters: lab_adapters::builtin_adapter_registry()?,
        procedures: lab_compiler::procedure::builtin_procedure_compiler().clone(),
    })
}
