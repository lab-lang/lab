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
pub use source::{Identifier, Span, Spanned};
/// Parse, resolve, type-check, and lower a complete source module into the
/// backend-neutral frontend IR.
pub fn compile_module(source: &str) -> Result<CheckedModule, ModuleError> {
    let module = parse_module(source)?;
    compile_parsed_module(&module)
}

fn compile_parsed_module(module: &ast::Module) -> Result<CheckedModule, ModuleError> {
    let checked = checker::check_module(module)?;
    material_flow::verify_module(&checked)?;
    Ok(checked)
}
