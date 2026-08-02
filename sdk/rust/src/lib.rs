//! Ergonomic Rust API for Lab.

mod api;
mod error;

pub use api::compile_lab_lang;
pub use error::Error;
pub use labc::ParseError;
pub use labc::{
    AcceptanceCriterion, Artifact, ArtifactSpec, AssemblyMethod, Capability, Concentration,
    DnaSequence, LabProfile, PlasmidSpec, SpecError, Topology, Volume,
};
pub use labc::{
    AcceptanceObligation, Compilation, Compiler, CompilerError, ExecutablePlan, OperationKind,
    PlanStep, PlanValue, SimulationError, SimulationEvent, SimulationTrace, ValueKind,
    render_human, simulate,
};
