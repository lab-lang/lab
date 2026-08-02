//! Biological specifications and laboratory target descriptions used by the compiler frontend.

mod capability;
mod model;

pub use capability::{AssemblyMethod, Capability, LabProfile};
pub use model::{
    AcceptanceCriterion, Artifact, ArtifactSpec, Concentration, DnaSequence, PlasmidSpec,
    SpecError, Topology, Volume,
};
