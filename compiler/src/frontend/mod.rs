//! Lab Lang source frontend for the compiler.

pub mod ast;
mod error;
mod lexer;
mod lowering;
mod parser;
mod source;
mod specification;
mod token;

pub use error::ParseError;
pub use parser::parse_module;
pub use source::{Identifier, Span, Spanned};
pub use specification::{
    AcceptanceCriterion, Artifact, ArtifactSpec, AssemblyMethod, Capability, Concentration,
    DnaSequence, LabProfile, PlasmidSpec, SpecError, Topology, Volume,
};

/// Parse and lower the currently executable plasmid-design subset.
///
/// Use [`parse_module`] to inspect the broader source language without asking
/// the current artifact pipeline to execute it.
pub fn parse(source: &str) -> Result<ArtifactSpec, ParseError> {
    lowering::lower_artifact(parse_module(source)?)
}
