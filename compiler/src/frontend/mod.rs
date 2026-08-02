//! Lab Lang source frontend for the compiler.

mod error;
mod lexer;
mod parser;
mod specification;
mod token;

pub use error::ParseError;
pub use parser::parse;
pub use specification::{
    AcceptanceCriterion, Artifact, ArtifactSpec, AssemblyMethod, Capability, Concentration,
    DnaSequence, LabProfile, PlasmidSpec, SpecError, Topology, Volume,
};
