use crate::ArtifactBundle;

/// Typed compilation boundary implemented by a concrete robot backend.
///
/// The generic input and associated program preserve backend-specific IRs. A
/// future runtime registry can erase this boundary once multiple backends have
/// demonstrated which parts actually need dynamic dispatch.
pub trait Backend<Input> {
    type Program;
    type Error: std::error::Error + Send + Sync + 'static;

    fn compile(&self, input: &Input) -> Result<Self::Program, Self::Error>;
}

/// Artifact emission is separate from planning so validation and other
/// consumers can inspect a backend program without first rendering files.
pub trait BackendEmitter<Program> {
    type Error: std::error::Error + Send + Sync + 'static;

    fn emit(&self, program: &Program) -> Result<ArtifactBundle, Self::Error>;
}
