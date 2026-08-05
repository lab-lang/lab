use crate::ArtifactBundle;

use super::BackendDescriptor;

/// Typed compilation boundary implemented by a concrete robot backend.
///
/// The generic input and associated program preserve backend-specific IRs. A
/// future runtime registry can erase this boundary once multiple backends have
/// demonstrated which parts actually need dynamic dispatch.
pub trait Backend<Input> {
    type Program;
    type Error: std::error::Error + Send + Sync + 'static;

    fn descriptor(&self) -> BackendDescriptor;
    fn compile(&self, input: &Input) -> Result<Self::Program, Self::Error>;
}

/// Artifact emission is separate from planning so simulation and validation
/// can consume a backend program without first rendering files.
pub trait BackendEmitter<Program> {
    type Error: std::error::Error + Send + Sync + 'static;

    fn emit(&self, program: &Program) -> Result<ArtifactBundle, Self::Error>;
}
