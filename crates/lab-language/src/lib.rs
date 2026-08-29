//! Lab Lang source frontend for the compiler.

pub mod ast;
mod checked;
mod checker;
mod diagnostics;
mod error;
mod lexer;
mod material_flow;
mod parser;
pub mod provenance;
mod render;
mod semantic_error;
mod semantics;
mod source;
mod standard_library;
mod token;
mod type_system;

pub use checked::{
    CheckedActionArgument, CheckedArgument, CheckedBinding, CheckedCase, CheckedDeclaration,
    CheckedExpression, CheckedField, CheckedFieldValue, CheckedMatchCase, CheckedModule,
    CheckedPattern, CheckedPatternField, CheckedPresence, CheckedProperty, CheckedSection,
    CheckedState, CheckedStatement, CheckedTrigger, CheckedType, OwnershipMode, ResolvedAction,
    ResolvedImport, TypedExpression,
};
pub use diagnostics::{
    Analysis, Diagnostic, DiagnosticCode, DiagnosticRelatedInformation, DiagnosticSeverity,
    SourceId, analyze_module, analyze_module_in_environment, render_diagnostic,
};
pub use error::ParseError;
pub use lab_capability::is_absolute_iri;
pub use material_flow::MaterialFlowError;
pub use parser::parse_module;
pub use render::render_checked_module;
pub use semantic_error::{ModuleError, RelatedSpan, SemanticError};
pub use semantics::{
    ArtifactSchema, CallableSignature, DefinitionId, ExportKind, Grounding, ModuleExport, ModuleId,
    ModuleInterface, SemanticEnvironment, TypeParameters,
};
pub use source::{Identifier, LineIndex, Span, Spanned};
pub use standard_library::manifest;

/// Generate reference documentation from the same Rust specifications used by
/// name resolution and type checking.
pub fn standard_library_markdown() -> String {
    standard_library::render_markdown()
}

/// Describe the bundled standard library for tooling outside the compiler.
///
/// An editor completing a word and a host-language SDK mirroring the library
/// need the same answer name resolution gives, which is why this is derived
/// from the catalog rather than maintained beside it.
pub fn standard_library_manifest() -> standard_library::manifest::Library {
    standard_library::manifest()
}
/// Parse, resolve, type-check, and lower a complete source module into the
/// backend-neutral frontend IR.
pub fn compile_module(source: &str) -> Result<CheckedModule, ModuleError> {
    compile_module_with_id(ModuleId::standalone(), source)
}

/// Compile a source module with an identity supplied by a project host.
pub fn compile_module_with_id(
    module_id: ModuleId,
    source: &str,
) -> Result<CheckedModule, ModuleError> {
    let module = parse_module(source)?;
    compile_parsed_module(module_id, &SemanticEnvironment::default(), &module)
}

/// Compile in an explicitly supplied package/module environment.
pub fn compile_module_in_environment(
    module_id: ModuleId,
    source: &str,
    environment: &SemanticEnvironment,
) -> Result<CheckedModule, ModuleError> {
    let module = parse_module(source)?;
    compile_parsed_module(module_id, environment, &module)
}

/// Compile against an explicitly supplied standard library.
///
/// Only the standard library's own bootstrap needs this: a module written in
/// Lab must compile against the modules that precede it, not against a library
/// that is still being built.
pub(crate) fn compile_module_with_library(
    module_id: ModuleId,
    source: &str,
    library: standard_library::StandardLibrary,
) -> Result<CheckedModule, ModuleError> {
    let module = parse_module(source)?;
    let checked = checker::check_module_with_library(
        module_id,
        &SemanticEnvironment::default(),
        &module,
        library,
    )?;
    material_flow::verify_module(&checked, &SemanticEnvironment::default())?;
    Ok(checked)
}

/// Check a module that was built rather than parsed.
///
/// Designs read from an SBOL document arrive as declarations, not as text, and
/// they still have to satisfy every rule a written module satisfies. Compiling
/// them through the same checker is what makes that true by construction: an
/// undeclared property, an incomplete schema, or a type that does not fit is
/// rejected the same way whichever way the module was produced, and no second
/// implementation of those rules exists to drift.
pub fn compile_ast_module(
    module_id: ModuleId,
    environment: &SemanticEnvironment,
    module: &ast::Module,
) -> Result<CheckedModule, ModuleError> {
    compile_parsed_module(module_id, environment, module)
}

fn compile_parsed_module(
    module_id: ModuleId,
    environment: &SemanticEnvironment,
    module: &ast::Module,
) -> Result<CheckedModule, ModuleError> {
    let checked = checker::check_module(module_id, environment, module)?;
    material_flow::verify_module(&checked, environment)?;
    Ok(checked)
}
