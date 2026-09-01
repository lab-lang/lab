//! The Lab Compiler biological compilation pipeline.

pub mod allocation;
pub mod artifact;
pub mod backend;
pub(crate) mod capability;
pub(crate) mod design;
pub(crate) mod ir;
pub mod method;
pub mod pipeline;
pub mod planning;
pub mod procedure;
pub mod program;
pub mod session;
pub mod stage;
pub(crate) mod workflow;
pub use artifact::{ArtifactBundle, ArtifactError, GeneratedArtifact};
pub use lab_language::{
    Analysis, CheckedDeclaration, CheckedModule, Diagnostic, DiagnosticSeverity, MaterialFlowError,
    ModuleError, ModuleId, ModuleInterface, ParseError, SemanticEnvironment, SemanticError,
    SourceId, analyze_module, analyze_module_in_environment, compile_module,
    compile_module_in_environment, manifest, parse_module, render_checked_module,
    render_diagnostic, standard_library_manifest,
};
pub use lab_runfmt as runfmt;
