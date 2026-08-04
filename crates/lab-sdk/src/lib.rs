//! Ergonomic Rust API for Lab.

mod api;
mod error;

pub use api::compile_lab_lang;
pub use error::Error;
pub use lab_compiler::ParseError;
pub use lab_compiler::{
    AcceptanceCriterion, Artifact, ArtifactSpec, AssemblyMethod, Capability, Concentration,
    DnaSequence, LabProfile, PlasmidSpec, SpecError, Topology, Volume,
};
pub use lab_compiler::{
    AcceptanceObligation, Compilation, Compiler, CompilerError, ExecutablePlan, OperationKind,
    PlanStep, PlanValue, SimulationError, SimulationEvent, SimulationTrace, ValueKind,
    render_human, simulate,
};
