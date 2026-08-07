//! The Lab Compiler biological compilation pipeline.

pub mod artifact;
pub mod backend;
pub mod lair;
pub mod planning;
#[cfg(test)]
mod test_support;

pub use artifact::{ArtifactBundle, ArtifactError, GeneratedArtifact};
pub use lab_language::{
    CheckedModule, MaterialFlowError, ModuleError, ParseError, SemanticError, compile_module,
    parse_module, render_checked_module,
};
pub use lair::pipeline::{PassInfo, PassPipeline, PassPipelineError, registered_passes};
pub use lair::program::{
    PortableLairError, PortableLairProgram, ProtocolLairError, ProtocolLairProgram,
};
pub use lair::session::{CompilerSession, SessionError, SessionOptions};
pub use lair::stage::{IrStage, StageContract};
