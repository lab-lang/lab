//! Lab Lang source frontend for the compiler.

pub mod ast;
mod checked;
mod checker;
mod error;
mod lexer;
mod lowering;
mod parser;
mod semantic_error;
mod source;
mod specification;
mod token;

pub use checked::{
    CheckedBinding, CheckedCase, CheckedDeclaration, CheckedField, CheckedMatchCase, CheckedModule,
    CheckedSection, CheckedStatement, CheckedTrigger, ResolvedImport,
};
pub use error::ParseError;
pub use parser::parse_module;
pub use semantic_error::{ModuleError, SemanticError};
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

/// Parse, resolve, type-check, and lower a complete source module into the
/// backend-neutral frontend IR.
pub fn compile_module(source: &str) -> Result<CheckedModule, ModuleError> {
    let module = parse_module(source)?;
    Ok(checker::check_module(source, &module)?)
}
