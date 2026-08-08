//! Source-aware analysis results shared by native and embedded tooling.

use serde::{Deserialize, Serialize};

use crate::source::{LineIndex, char_boundary};
use crate::{
    CheckedModule, ModuleError, ModuleId, ParseError, SemanticEnvironment, Span, ast,
    compile_parsed_module, parse_module,
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
    /// Suggested ways forward, carrying no source range because they describe
    /// what the author could write instead of what they wrote.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub help: Vec<String>,
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
    analyze_module_in_environment(
        source_id,
        ModuleId::standalone(),
        text,
        &SemanticEnvironment::default(),
    )
}

/// Analyze a module against a caller-supplied module identity and import
/// environment, for hosts that resolve imports across several modules held
/// in memory together (an editor's open documents, a package's sources).
pub fn analyze_module_in_environment(
    source_id: SourceId,
    module_id: ModuleId,
    text: &str,
    environment: &SemanticEnvironment,
) -> Analysis {
    match parse_module(text) {
        Err(error) => Analysis {
            syntax: None,
            checked: None,
            diagnostics: vec![diagnostic_from_parse(source_id, error)],
        },
        Ok(syntax) => match compile_parsed_module(module_id, environment, &syntax) {
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

/// One underlined range and the note that goes beside it. The primary range
/// carries no note, because its note is the headline.
type Mark<'a> = (Span, Option<&'a str>);

/// Render a diagnostic against the source it came from, as an excerpt with the
/// offending range underlined.
///
/// Spans that share a line are underlined beneath one copy of it, so an error
/// about two operands reads as one sentence about one line rather than as two
/// separate complaints.
pub fn render_diagnostic(source: &str, diagnostic: &Diagnostic) -> String {
    let index = LineIndex::new(source);
    let severity = match diagnostic.severity {
        DiagnosticSeverity::Error => "error",
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Information => "note",
        DiagnosticSeverity::Hint => "hint",
    };

    // The primary span carries no label: its message is already the headline.
    // A related note about that same range replaces it rather than underlining
    // the same text twice.
    let related = diagnostic
        .related
        .iter()
        .filter(|related| related.source == diagnostic.source)
        .collect::<Vec<_>>();
    let mut marks = Vec::new();
    if !related.iter().any(|other| other.span == diagnostic.span) {
        marks.push((diagnostic.span, None));
    }
    marks.extend(
        related
            .iter()
            .map(|related| (related.span, Some(related.message.as_str()))),
    );

    let gutter = marks
        .iter()
        .map(|(span, _)| index.line(char_boundary(source, span.start)) + 1)
        .max()
        .unwrap_or(1)
        .to_string()
        .len();
    let bar = format!("{:gutter$} |", "");

    let (line, column) = index.position(source, diagnostic.span.start);
    let mut out = vec![
        format!("{severity}: {}", diagnostic.message),
        format!(
            "{:gutter$}--> {}:{}:{}",
            "",
            diagnostic.source.0,
            line + 1,
            column + 1
        ),
        bar.clone(),
    ];

    // Group by line, keeping the order the lines were first mentioned so the
    // primary span leads.
    let mut grouped: Vec<(usize, Vec<Mark<'_>>)> = Vec::new();
    for (span, label) in marks {
        let line = index.line(char_boundary(source, span.start));
        match grouped.iter_mut().find(|(existing, _)| *existing == line) {
            Some((_, entries)) => entries.push((span, label)),
            None => grouped.push((line, vec![(span, label)])),
        }
    }

    for (position, (line, mut entries)) in grouped.into_iter().enumerate() {
        if position != 0 {
            out.push(bar.clone());
        }
        let text = index.line_text(source, line);
        out.push(format!("{:>gutter$} | {text}", line + 1));
        entries.sort_by_key(|(span, _)| span.start);
        for (span, label) in entries {
            let (_, column) = index.position(source, span.start);
            // A span reaching past this line underlines to its end; a
            // zero-width span still gets one caret to point at.
            let line_end = index.start(line) + text.len();
            let end = char_boundary(source, span.end.clamp(span.start, line_end));
            let width = source[char_boundary(source, span.start)..end]
                .chars()
                .count()
                .max(1);
            let carets = "^".repeat(width);
            let label = label.map(|label| format!(" {label}")).unwrap_or_default();
            out.push(format!("{:gutter$} | {:column$}{carets}{label}", "", ""));
        }
    }
    out.push(bar);

    for help in &diagnostic.help {
        out.push(format!("{:gutter$} = help: {help}", ""));
    }
    // Related information in another file cannot be excerpted here, but it must
    // not silently vanish.
    for related in &diagnostic.related {
        if related.source != diagnostic.source {
            out.push(format!(
                "{:gutter$} = note: {} ({})",
                "", related.message, related.source.0
            ));
        }
    }
    out.join("\n")
}

fn diagnostic_from_parse(source: SourceId, error: ParseError) -> Diagnostic {
    let (span, code, message) = match error {
        ParseError::Syntax { span, message } => (span, DiagnosticCode::Syntax, message),
    };
    Diagnostic {
        source,
        span,
        severity: DiagnosticSeverity::Error,
        code,
        message,
        related: Vec::new(),
        help: Vec::new(),
    }
}

fn diagnostic_from_module(source: SourceId, fallback: Span, error: ModuleError) -> Diagnostic {
    match error {
        ModuleError::Parse(error) => diagnostic_from_parse(source, error),
        ModuleError::Semantic(error) => Diagnostic {
            related: error
                .related
                .into_iter()
                .map(|related| DiagnosticRelatedInformation {
                    source: source.clone(),
                    span: related.span,
                    message: related.message,
                })
                .collect(),
            help: error.help,
            source,
            span: error.span,
            severity: DiagnosticSeverity::Error,
            code: DiagnosticCode::Semantic,
            message: error.message,
        },
        ModuleError::MaterialFlow(error) => Diagnostic {
            source,
            span: fallback,
            severity: DiagnosticSeverity::Error,
            code: DiagnosticCode::MaterialFlow,
            message: error.to_string(),
            related: Vec::new(),
            help: Vec::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostics::*;

    #[test]
    fn preserves_syntax_for_semantic_errors() {
        let analysis = analyze_module(SourceId::new("memory:test.lab"), "use nowhere\n");
        assert!(analysis.syntax.is_some());
        assert!(analysis.checked.is_none());
        assert_eq!(analysis.diagnostics[0].code, DiagnosticCode::Semantic);
        assert_eq!(analysis.diagnostics[0].source.0, "memory:test.lab");
    }

    fn diagnostic(source: &str, span: Span, message: &str) -> Diagnostic {
        Diagnostic {
            source: SourceId::new(source),
            span,
            severity: DiagnosticSeverity::Error,
            code: DiagnosticCode::Semantic,
            message: message.to_owned(),
            related: Vec::new(),
            help: Vec::new(),
        }
    }

    fn related(source: &str, span: Span, message: &str) -> DiagnosticRelatedInformation {
        DiagnosticRelatedInformation {
            source: SourceId::new(source),
            span,
            message: message.to_owned(),
        }
    }

    #[test]
    fn underlines_one_span_with_its_line_and_column() {
        let source = "workflow grow() -> Integer:\n  return mystery\n";
        let span = Span::new(37, 44);
        assert_eq!(&source[span.start..span.end], "mystery");

        let rendered = render_diagnostic(
            source,
            &diagnostic("panel.lab", span, "unknown value 'mystery'"),
        );

        assert_eq!(
            rendered,
            [
                "error: unknown value 'mystery'",
                " --> panel.lab:2:10",
                "  |",
                "2 |   return mystery",
                "  |          ^^^^^^^",
                "  |",
            ]
            .join("\n")
        );
    }

    #[test]
    fn underlines_spans_that_share_a_line_beneath_one_copy_of_it() {
        let source = "workflow main() -> Evidence:\n  evidence <- characterize tet arabinose\n";
        let circuit = Span::new(56, 59);
        let inducer = Span::new(60, 69);
        assert_eq!(&source[circuit.start..circuit.end], "tet");
        assert_eq!(&source[inducer.start..inducer.end], "arabinose");

        let mut error = diagnostic(
            "panel.lab",
            circuit,
            "'S' cannot be both Tetracycline and Arabinose",
        );
        error.related = vec![
            related("panel.lab", circuit, "fixes S = Tetracycline"),
            related("panel.lab", inducer, "requires S = Arabinose"),
        ];

        let rendered = render_diagnostic(source, &error);

        assert_eq!(
            rendered,
            [
                "error: 'S' cannot be both Tetracycline and Arabinose".to_owned(),
                " --> panel.lab:2:28".to_owned(),
                "  |".to_owned(),
                "2 |   evidence <- characterize tet arabinose".to_owned(),
                format!("  | {}^^^ fixes S = Tetracycline", " ".repeat(27)),
                format!("  | {}^^^^^^^^^ requires S = Arabinose", " ".repeat(31)),
                "  |".to_owned(),
            ]
            .join("\n"),
            "both operands are underlined under one excerpt:\n{rendered}"
        );
    }

    #[test]
    fn separates_spans_on_different_lines_and_appends_help() {
        let source = "circuit sense(\n  promoter: Promoter<S: Signal>,\n) -> Circuit<T>:\n";
        let unbound = Span::new(61, 62);
        let introduced = Span::new(36, 37);
        assert_eq!(&source[unbound.start..unbound.end], "T");
        assert_eq!(&source[introduced.start..introduced.end], "S");

        let mut error = diagnostic("sense.lab", unbound, "unknown type 'T'");
        error.related = vec![related(
            "sense.lab",
            introduced,
            "'S' is introduced by parameter 'promoter'",
        )];
        error.help = vec!["did you mean 'S'?".to_owned()];

        let rendered = render_diagnostic(source, &error);

        assert_eq!(
            rendered,
            [
                "error: unknown type 'T'".to_owned(),
                " --> sense.lab:3:14".to_owned(),
                "  |".to_owned(),
                "3 | ) -> Circuit<T>:".to_owned(),
                format!("  | {}^", " ".repeat(13)),
                "  |".to_owned(),
                "2 |   promoter: Promoter<S: Signal>,".to_owned(),
                format!(
                    "  | {}^ 'S' is introduced by parameter 'promoter'",
                    " ".repeat(21)
                ),
                "  |".to_owned(),
                "  = help: did you mean 'S'?".to_owned(),
            ]
            .join("\n"),
            "\n{rendered}"
        );
    }

    #[test]
    fn renders_a_zero_width_span_and_a_span_reaching_past_its_line() {
        let source = "x = 1\ny = 2\n";
        let at_end = render_diagnostic(
            source,
            &diagnostic("a.lab", Span::at(5), "expected a value"),
        );
        assert!(
            at_end.contains("1 | x = 1\n  |      ^"),
            "a zero-width span still points somewhere:\n{at_end}"
        );

        let across = render_diagnostic(
            source,
            &diagnostic("a.lab", Span::new(0, 11), "this whole module"),
        );
        assert!(
            across.contains("1 | x = 1\n  | ^^^^^"),
            "a multi-line span underlines to the end of its first line:\n{across}"
        );
    }

    /// The whole path, from a checker error carrying two spans through to the
    /// rendered excerpt a person reads.
    #[test]
    fn carries_a_checker_errors_second_span_through_to_the_rendered_excerpt() {
        let source =
            "workflow grow() -> Integer:\n  return 1\n\nworkflow grow() -> Integer:\n  return 2\n";
        let analysis = analyze_module(SourceId::new("grow.lab"), source);

        let [error] = analysis.diagnostics.as_slice() else {
            panic!("expected one diagnostic: {:?}", analysis.diagnostics);
        };
        assert_eq!(error.message, "duplicate declaration 'grow'");
        assert_eq!(error.related.len(), 1, "{error:?}");
        assert_eq!(error.related[0].message, "'grow' is already declared here");

        let rendered = render_diagnostic(source, error);
        assert!(
            rendered.contains("4 | workflow grow() -> Integer:"),
            "the second declaration is the primary span:\n{rendered}"
        );
        assert!(
            rendered.contains("1 | workflow grow() -> Integer:"),
            "the first declaration is shown too:\n{rendered}"
        );
    }

    #[test]
    fn notes_related_information_in_another_file_rather_than_dropping_it() {
        let source = "use other\n";
        let mut error = diagnostic("here.lab", Span::new(4, 9), "'shared' is ambiguous");
        error.related = vec![related("there.lab", Span::new(0, 6), "also exported here")];

        let rendered = render_diagnostic(source, &error);

        assert!(
            rendered.contains("= note: also exported here (there.lab)"),
            "{rendered}"
        );
    }
}
