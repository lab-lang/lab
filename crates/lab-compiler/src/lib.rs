//! The Lab Compiler biological compilation pipeline.

pub mod artifact;
pub mod backend;
mod compiler;
pub mod execution;
pub mod lair;
pub mod planning;
pub mod render;
pub mod simulation;

pub use artifact::{ArtifactBundle, ArtifactError, GeneratedArtifact};
pub use compiler::{BackendCompilation, Compiler};
pub use execution::{ExecutionDependency, ExecutionGraph, ExecutionOperation};
pub use lab_language::{
    CheckedModule, MaterialFlowError, ModuleError, ParseError, SemanticError, compile_module,
    parse_module, render_checked_module,
};
pub use lair::session::{
    CompilerSession, PassInfo, PassPipeline, PassPipelineError, SessionError, SessionOptions,
    registered_passes,
};
pub use lair::stage::{IrStage, StageContract};
pub use planning::{
    AcceptanceCriterion, AcceptanceObligation, OperationKind, PlanError, PlanStep, PlanValue,
    ProtocolPlan, ValueKind,
};
pub use render::render_human;
pub use simulation::{
    LabState, SimulatedValue, SimulationError, SimulationEvent, SimulationTrace, simulate,
};
