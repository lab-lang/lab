//! The Lab Compiler biological compilation pipeline.

pub mod artifact;
pub mod backend;
pub mod lair;
pub mod planning;
pub mod runfmt;
#[cfg(test)]
mod test_support;

pub use artifact::{ArtifactBundle, ArtifactError, GeneratedArtifact};
pub use lab_language::{
    Analysis, CheckedModule, Diagnostic, DiagnosticSeverity, MaterialFlowError, ModuleError,
    ModuleId, ModuleInterface, ParseError, SemanticEnvironment, SemanticError, SourceId,
    analyze_module, analyze_module_in_environment, compile_module, compile_module_in_environment,
    manifest, parse_module, render_checked_module, render_diagnostic, standard_library_manifest,
};
pub use lair::pipeline::{PassInfo, PassPipeline, PassPipelineError, registered_passes};
pub use lair::program::{
    PortableLairError, PortableLairProgram, ProtocolLairError, ProtocolLairProgram,
};
pub use lair::session::{CompilerSession, SessionError, SessionOptions};
pub use lair::stage::{IrStage, StageContract};
