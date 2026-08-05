//! Lab Lang source frontend for the compiler.

pub mod ast;
mod checked;
mod checker;
mod diagnostics;
mod error;
mod lexer;
mod material_flow;
mod parser;
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
    CheckedPattern, CheckedPatternField, CheckedProperty, CheckedSection, CheckedState,
    CheckedStatement, CheckedTrigger, CheckedType, OwnershipMode, ResolvedAction, ResolvedImport,
    TypedExpression,
};
pub use diagnostics::{
    Analysis, Diagnostic, DiagnosticCode, DiagnosticRelatedInformation, DiagnosticSeverity,
    SourceId, analyze_module,
};
pub use error::ParseError;
pub use material_flow::MaterialFlowError;
pub use parser::parse_module;
pub use render::render_checked_module;
pub use semantic_error::{ModuleError, SemanticError};
pub use semantics::{
    CallableSignature, DefinitionId, ExportKind, ModuleExport, ModuleId, ModuleInterface,
    SemanticEnvironment,
};
pub use source::{Identifier, Span, Spanned};

/// Generate reference documentation from the same Rust specifications used by
/// name resolution and type checking.
pub fn standard_library_markdown() -> String {
    standard_library::render_markdown()
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

fn compile_parsed_module(
    module_id: ModuleId,
    environment: &SemanticEnvironment,
    module: &ast::Module,
) -> Result<CheckedModule, ModuleError> {
    let checked = checker::check_module(module_id, environment, module)?;
    material_flow::verify_module(&checked)?;
    Ok(checked)
}
