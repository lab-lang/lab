//! Source-aware analysis results shared by native and embedded tooling.

use serde::{Deserialize, Serialize};

use super::{
    CheckedModule, ModuleError, ParseError, Span, ast, compile_parsed_module, parse_module,
};

/// An opaque source identity chosen by the host (URI, virtual path, or handle).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourceId(pub String);

impl SourceId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    Syntax,
    Unsupported,
    Specification,
    Semantic,
    MaterialFlow,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticRelatedInformation {
    pub source: SourceId,
    pub span: Span,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub source: SourceId,
    pub span: Span,
    pub severity: DiagnosticSeverity,
    pub code: DiagnosticCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<DiagnosticRelatedInformation>,
}

/// All useful products from one source analysis, including partial syntax.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub syntax: Option<ast::Module>,
    pub checked: Option<CheckedModule>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Analysis {
    pub fn is_valid(&self) -> bool {
        self.diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != DiagnosticSeverity::Error)
    }
}

/// Analyze a module without throwing away the syntax tree when later phases fail.
///
/// The result is deliberately diagnostic-oriented. Parser recovery can add more
/// syntax diagnostics later without changing this host-facing contract.
pub fn analyze_module(source_id: SourceId, text: &str) -> Analysis {
    match parse_module(text) {
        Err(error) => Analysis {
            syntax: None,
            checked: None,
            diagnostics: vec![diagnostic_from_parse(source_id, error)],
        },
        Ok(syntax) => match compile_parsed_module(&syntax) {
            Ok(checked) => Analysis {
                syntax: Some(syntax),
                checked: Some(checked),
                diagnostics: Vec::new(),
            },
            Err(error) => Analysis {
                syntax: Some(syntax.clone()),
                checked: None,
                diagnostics: vec![diagnostic_from_module(source_id, syntax.span, error)],
            },
        },
    }
}

fn diagnostic_from_parse(source: SourceId, error: ParseError) -> Diagnostic {
    let (span, code, message) = match error {
        ParseError::Syntax { span, message } => (span, DiagnosticCode::Syntax, message),
        ParseError::Unsupported { span, feature } => (
            span,
            DiagnosticCode::Unsupported,
            format!("unsupported language feature: {feature}"),
        ),
        ParseError::Specification(error) => (
            Span::at(0),
            DiagnosticCode::Specification,
            error.to_string(),
        ),
    };
    Diagnostic {
        source,
        span,
        severity: DiagnosticSeverity::Error,
        code,
        message,
        related: Vec::new(),
    }
}

fn diagnostic_from_module(source: SourceId, fallback: Span, error: ModuleError) -> Diagnostic {
    match error {
        ModuleError::Parse(error) => diagnostic_from_parse(source, error),
        ModuleError::Semantic(error) => Diagnostic {
            source,
            span: error.span,
            severity: DiagnosticSeverity::Error,
            code: DiagnosticCode::Semantic,
            message: error.message,
            related: Vec::new(),
        },
        ModuleError::MaterialFlow(error) => Diagnostic {
            source,
            span: fallback,
            severity: DiagnosticSeverity::Error,
            code: DiagnosticCode::MaterialFlow,
            message: error.to_string(),
            related: Vec::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_syntax_for_semantic_errors() {
        let analysis = analyze_module(SourceId::new("memory:test.lab"), "use nowhere\n");
        assert!(analysis.syntax.is_some());
        assert!(analysis.checked.is_none());
        assert_eq!(analysis.diagnostics[0].code, DiagnosticCode::Semantic);
        assert_eq!(analysis.diagnostics[0].source.0, "memory:test.lab");
    }
}
