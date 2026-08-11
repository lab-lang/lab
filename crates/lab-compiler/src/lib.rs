//! The Lab Compiler biological compilation pipeline.

pub mod artifact;
pub mod backend;
pub mod lair;
pub mod planning;
pub use lab_runfmt as runfmt;
#[cfg(test)]
mod test_support;

pub use artifact::{ArtifactBundle, ArtifactError, GeneratedArtifact};
pub use lab_language::{
    CheckedModule, Diagnostic, DiagnosticSeverity, MaterialFlowError, ModuleError, ParseError,
    SemanticError, SourceId, analyze_module, compile_module, parse_module, render_checked_module,
    render_diagnostic,
};
pub use lair::pipeline::{PassInfo, PassPipeline, PassPipelineError, registered_passes};
pub use lair::program::{
    PortableLairError, PortableLairProgram, ProtocolLairError, ProtocolLairProgram,
};
pub use lair::session::{CompilerSession, SessionError, SessionOptions};
pub use lair::stage::{IrStage, StageContract};
