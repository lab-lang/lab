//! The Lab Compiler biological compilation pipeline.

pub mod artifact;
pub mod backend;
pub mod lair;
pub mod planning;
pub mod render;
pub mod simulation;

pub use artifact::{ArtifactBundle, ArtifactError, GeneratedArtifact};
pub use lab_language::{
    CheckedModule, MaterialFlowError, ModuleError, ParseError, SemanticError, compile_module,
    parse_module, render_checked_module,
};
pub use lair::pipeline::{PassInfo, PassPipeline, PassPipelineError, registered_passes};
pub use lair::session::{CompilerSession, SessionError, SessionOptions};
pub use lair::stage::{IrStage, StageContract};
pub use planning::{
    AcceptanceCriterion, AcceptanceObligation, OperationKind, PlanError, PlanStep, PlanValue,
    ProtocolPlan, ValueKind,
};
pub use render::render_human;
pub use simulation::{
    ExecutionDependency, ExecutionGraph, ExecutionOperation, LabState, SimulatedValue,
    SimulationError, SimulationEvent, SimulationTrace, simulate,
};
