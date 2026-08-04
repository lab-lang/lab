//! The Lab Compiler biological compilation pipeline.

mod analyses;
pub mod frontend;
mod ir;
mod output;
mod passes;
mod pipeline;
mod plan;
mod session;
mod stages;
mod translations;

pub use frontend::{
    Artifact, ArtifactSpec, AssemblyMethod, Capability, CheckedModule, Concentration, DnaSequence,
    LabProfile, MaterialFlowError, ModuleError, ParseError, PlasmidSpec, SemanticError, SpecError,
    Topology, Volume, compile_module, parse, parse_module,
};
pub use output::{
    SimulationError, SimulationEvent, SimulationTrace, render_checked_module, render_human,
    simulate,
};
pub use pipeline::{Compilation, Compiler, CompilerError};
pub use plan::{
    AcceptanceCriterion, AcceptanceObligation, ExecutablePlan, OperationKind, PlanError, PlanStep,
    PlanValue, ValueKind,
};
pub use session::{
    CompilerSession, PassInfo, PassPipeline, PassPipelineError, SessionError, SessionOptions,
    registered_passes,
};
pub use stages::{IrStage, StageContract};
