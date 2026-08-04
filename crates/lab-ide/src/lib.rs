//! Platform-neutral editor intelligence for native, browser, and embedded hosts.

use std::collections::{BTreeMap, BTreeSet};

use lab_language::{Analysis, Diagnostic, SourceId, Span, analyze_module, ast};
use serde::{Deserialize, Serialize};

const KEYWORDS: &[&str] = &[
    "use",
    "circuit",
    "plasmid",
    "record",
    "material",
    "observation",
    "evidence",
    "event",
    "outcome",
    "workflow",
    "input",
    "output",
    "state",
    "require",
    "accept",
    "if",
    "else",
    "for",
    "in",
    "match",
    "case",
    "return",
    "when",
    "every",
    "after",
    "emit",
    "and",
    "or",
    "not",
];

#[derive(Clone, Debug)]
struct Document {
    version: i64,
    text: String,
    analysis: Analysis,
}

#[derive(Clone, Debug, Default)]
pub struct Workspace {
    documents: BTreeMap<SourceId, Document>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Module,
    Circuit,
    Plasmid,
    Data,
    Workflow,
    Variable,
    Field,
    Case,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub span: Span,
    pub selection_span: Span,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DocumentSymbol>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionKind {
    Keyword,
    Type,
    Value,
    Function,
    Module,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hover {
    pub span: Span,
    pub markdown: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub source: SourceId,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEdit {
    pub source: SourceId,
    pub span: Span,
    pub new_text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticTokenKind {
    Comment,
    Keyword,
    String,
    Number,
    Type,
    Function,
    Variable,
    Operator,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticToken {
    pub span: Span,
    pub kind: SemanticTokenKind,
}

impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_document(&mut self, source: SourceId, version: i64, text: String) {
        let analysis = analyze_module(source.clone(), &text);
        self.documents.insert(
            source,
            Document {
                version,
                text,
                analysis,
            },
        );
    }

    pub fn remove_document(&mut self, source: &SourceId) {
        self.documents.remove(source);
    }

    pub fn version(&self, source: &SourceId) -> Option<i64> {
        self.documents.get(source).map(|document| document.version)
    }

    pub fn text(&self, source: &SourceId) -> Option<&str> {
        self.documents
            .get(source)
            .map(|document| document.text.as_str())
    }

    pub fn diagnostics(&self, source: &SourceId) -> &[Diagnostic] {
        self.documents
            .get(source)
            .map_or(&[], |document| document.analysis.diagnostics.as_slice())
    }

    pub fn document_symbols(&self, source: &SourceId) -> Vec<DocumentSymbol> {
        let Some(module) = self
            .documents
            .get(source)
            .and_then(|document| document.analysis.syntax.as_ref())
        else {
            return Vec::new();
        };
        module.items.iter().map(symbol_from_item).collect()
    }

    pub fn completions(&self, _source: &SourceId, _offset: usize) -> Vec<CompletionItem> {
        let mut items = KEYWORDS
            .iter()
            .map(|keyword| CompletionItem {
                label: (*keyword).to_owned(),
                kind: CompletionKind::Keyword,
                detail: Some("Lab keyword".to_owned()),
            })
            .collect::<Vec<_>>();
        let mut seen = BTreeSet::new();
        for (name, kind, _) in self.declarations() {
            if seen.insert(name.clone()) {
                items.push(CompletionItem {
                    label: name,
                    kind: match kind {
                        SymbolKind::Circuit | SymbolKind::Workflow => CompletionKind::Function,
                        SymbolKind::Data | SymbolKind::Plasmid => CompletionKind::Type,
                        _ => CompletionKind::Value,
                    },
                    detail: Some(format!("Lab {kind:?}").to_lowercase()),
                });
            }
        }
        items
    }

    pub fn hover(&self, source: &SourceId, offset: usize) -> Option<Hover> {
        let document = self.documents.get(source)?;
        let (name, span) = identifier_at(&document.text, offset)?;
        let (_, kind, location) = self
            .declarations()
            .into_iter()
            .find(|(candidate, _, _)| candidate == name)?;
        Some(Hover {
            span,
            markdown: format!(
                "```lab\n{kind:?} {name}\n```\n\nDefined in `{}`.",
                location.source.0
            ),
        })
    }

    pub fn definition(&self, source: &SourceId, offset: usize) -> Option<Location> {
        let document = self.documents.get(source)?;
        let (name, _) = identifier_at(&document.text, offset)?;
        self.declarations()
            .into_iter()
            .find_map(|(candidate, _, location)| (candidate == name).then_some(location))
    }

    pub fn references(&self, source: &SourceId, offset: usize) -> Vec<Location> {
        let Some(document) = self.documents.get(source) else {
            return Vec::new();
        };
        let Some((name, _)) = identifier_at(&document.text, offset) else {
            return Vec::new();
        };
        self.documents
            .iter()
            .flat_map(|(source, document)| {
                identifier_spans(&document.text)
                    .filter(|(candidate, _)| *candidate == name)
                    .map(|(_, span)| Location {
                        source: source.clone(),
                        span,
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    pub fn rename(&self, source: &SourceId, offset: usize, new_name: &str) -> Vec<TextEdit> {
        if !valid_identifier(new_name) {
            return Vec::new();
        }
        self.references(source, offset)
            .into_iter()
            .map(|location| TextEdit {
                source: location.source,
                span: location.span,
                new_text: new_name.to_owned(),
            })
            .collect()
    }

    pub fn semantic_tokens(&self, source: &SourceId) -> Vec<SemanticToken> {
        self.documents
            .get(source)
            .map_or_else(Vec::new, |document| scan_semantic_tokens(&document.text))
    }

    /// A deliberately conservative formatter: it only removes trailing space,
    /// preserves layout, and establishes one final newline.
    pub fn format_document(&self, source: &SourceId) -> Option<String> {
        let text = &self.documents.get(source)?.text;
        let mut formatted = text
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n");
        formatted.push('\n');
        Some(formatted)
    }

    fn declarations(&self) -> Vec<(String, SymbolKind, Location)> {
        self.documents
            .iter()
            .flat_map(|(source, document)| {
                document
                    .analysis
                    .syntax
                    .as_ref()
                    .into_iter()
                    .flat_map(|module| module.items.iter())
                    .filter_map(|item| {
                        declaration(item).map(|(name, kind, span)| {
                            (
                                name.to_owned(),
                                kind,
                                Location {
                                    source: source.clone(),
                                    span,
                                },
                            )
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}

fn declaration(item: &ast::Item) -> Option<(&str, SymbolKind, Span)> {
    match item {
        ast::Item::Use(_) => None,
        ast::Item::Circuit(item) => Some((&item.name.value, SymbolKind::Circuit, item.name.span)),
        ast::Item::Plasmid(item) => Some((&item.name.value, SymbolKind::Plasmid, item.name.span)),
        ast::Item::Data(item) => Some((&item.name.value, SymbolKind::Data, item.name.span)),
        ast::Item::Workflow(item) => Some((&item.name.value, SymbolKind::Workflow, item.name.span)),
        ast::Item::Binding(item) => item
            .names
            .first()
            .map(|name| (&*name.value, SymbolKind::Variable, name.span)),
    }
}

fn symbol_from_item(item: &ast::Item) -> DocumentSymbol {
    let (name, kind, selection_span) =
        declaration(item).unwrap_or(("use", SymbolKind::Module, item.span()));
    let children = match item {
        ast::Item::Data(data) => data
            .fields
            .iter()
            .map(|field| DocumentSymbol {
                name: field.name.value.clone(),
                kind: SymbolKind::Field,
                span: field.span,
                selection_span: field.name.span,
                children: Vec::new(),
            })
            .chain(data.cases.iter().map(|case| DocumentSymbol {
                name: case.name.value.clone(),
                kind: SymbolKind::Case,
                span: case.span,
                selection_span: case.name.span,
                children: Vec::new(),
            }))
            .collect(),
        ast::Item::Workflow(workflow) => workflow
            .inputs
            .iter()
            .map(|field| DocumentSymbol {
                name: field.name.value.clone(),
                kind: SymbolKind::Field,
                span: field.span,
                selection_span: field.name.span,
                children: Vec::new(),
            })
            .collect(),
        _ => Vec::new(),
    };
    DocumentSymbol {
        name: name.to_owned(),
        kind,
        span: item.span(),
        selection_span,
        children,
    }
}

fn valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        && !KEYWORDS.contains(&name)
}

fn identifier_at(text: &str, offset: usize) -> Option<(&str, Span)> {
    identifier_spans(text).find(|(_, span)| span.start <= offset && offset <= span.end)
}

fn identifier_spans(text: &str) -> impl Iterator<Item = (&str, Span)> {
    let bytes = text.as_bytes();
    let mut cursor = 0;
    std::iter::from_fn(move || {
        loop {
            while cursor < bytes.len()
                && !(bytes[cursor] == b'_'
                    || bytes[cursor].is_ascii_alphabetic()
                    || bytes[cursor] == b'"'
                    || bytes[cursor] == b'#'
                    || (bytes[cursor] == b'/' && bytes.get(cursor + 1) == Some(&b'/')))
            {
                cursor += 1;
            }
            if cursor == bytes.len() {
                return None;
            }
            if bytes[cursor] == b'#'
                || (bytes[cursor] == b'/' && bytes.get(cursor + 1) == Some(&b'/'))
            {
                while cursor < bytes.len() && bytes[cursor] != b'\n' {
                    cursor += 1;
                }
                continue;
            }
            if bytes[cursor] == b'"' {
                cursor += 1;
                while cursor < bytes.len() {
                    if bytes[cursor] == b'\\' {
                        cursor = (cursor + 2).min(bytes.len());
                    } else if bytes[cursor] == b'"' {
                        cursor += 1;
                        break;
                    } else {
                        cursor += 1;
                    }
                }
                continue;
            }
            let start = cursor;
            cursor += 1;
            while cursor < bytes.len()
                && (bytes[cursor] == b'_' || bytes[cursor].is_ascii_alphanumeric())
            {
                cursor += 1;
            }
            return Some((&text[start..cursor], Span::new(start, cursor)));
        }
    })
}

fn scan_semantic_tokens(text: &str) -> Vec<SemanticToken> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let start = cursor;
        match bytes[cursor] {
            b'#' => {
                cursor += 1;
                while cursor < bytes.len() && bytes[cursor] != b'\n' {
                    cursor += 1;
                }
                tokens.push(token(start, cursor, SemanticTokenKind::Comment));
            }
            b'/' if bytes.get(cursor + 1) == Some(&b'/') => {
                cursor += 2;
                while cursor < bytes.len() && bytes[cursor] != b'\n' {
                    cursor += 1;
                }
                tokens.push(token(start, cursor, SemanticTokenKind::Comment));
            }
            b'"' => {
                cursor += 1;
                while cursor < bytes.len() {
                    if bytes[cursor] == b'\\' {
                        cursor = (cursor + 2).min(bytes.len());
                    } else if bytes[cursor] == b'"' {
                        cursor += 1;
                        break;
                    } else {
                        cursor += 1;
                    }
                }
                tokens.push(token(start, cursor, SemanticTokenKind::String));
            }
            byte if byte.is_ascii_digit() => {
                cursor += 1;
                while cursor < bytes.len()
                    && (bytes[cursor].is_ascii_digit() || bytes[cursor] == b'.')
                {
                    cursor += 1;
                }
                tokens.push(token(start, cursor, SemanticTokenKind::Number));
            }
            byte if byte == b'_' || byte.is_ascii_alphabetic() => {
                cursor += 1;
                while cursor < bytes.len()
                    && (bytes[cursor] == b'_' || bytes[cursor].is_ascii_alphanumeric())
                {
                    cursor += 1;
                }
                let word = &text[start..cursor];
                let kind = if KEYWORDS.contains(&word) {
                    SemanticTokenKind::Keyword
                } else if word.as_bytes().first().is_some_and(u8::is_ascii_uppercase) {
                    SemanticTokenKind::Type
                } else if text[cursor..].trim_start().starts_with('(') {
                    SemanticTokenKind::Function
                } else {
                    SemanticTokenKind::Variable
                };
                tokens.push(token(start, cursor, kind));
            }
            b'<' | b'>' | b'=' | b'!' | b'+' | b'-' | b'*' | b'|' => {
                cursor += 1;
                if cursor < bytes.len()
                    && matches!(
                        (bytes[start], bytes[cursor]),
                        (b'<', b'-')
                            | (b'-', b'>')
                            | (b'=', b'=')
                            | (b'!', b'=')
                            | (b'<', b'=')
                            | (b'>', b'=')
                    )
                {
                    cursor += 1;
                }
                tokens.push(token(start, cursor, SemanticTokenKind::Operator));
            }
            _ => cursor += 1,
        }
    }
    tokens
}

fn token(start: usize, end: usize, kind: SemanticTokenKind) -> SemanticToken {
    SemanticToken {
        span: Span::new(start, end),
        kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_symbols_and_renames_across_documents() {
        let source = SourceId::new("file:///design.lab");
        let mut workspace = Workspace::new();
        workspace.set_document(
            source.clone(),
            1,
            "plasmid reporter:\n  sequence = dna(\"ACGT\")\n".to_owned(),
        );
        assert_eq!(workspace.document_symbols(&source)[0].name, "reporter");
        let offset = workspace.text(&source).unwrap().find("reporter").unwrap();
        assert_eq!(workspace.rename(&source, offset, "sensor").len(), 1);
    }

    #[test]
    fn semantic_tokens_include_lab_constructs() {
        let source = SourceId::new("memory:test.lab");
        let mut workspace = Workspace::new();
        workspace.set_document(
            source.clone(),
            1,
            "workflow build:\n  # observe\n  return None\n".to_owned(),
        );
        let tokens = workspace.semantic_tokens(&source);
        assert!(
            tokens
                .iter()
                .any(|token| token.kind == SemanticTokenKind::Keyword)
        );
        assert!(
            tokens
                .iter()
                .any(|token| token.kind == SemanticTokenKind::Comment)
        );
    }

    #[test]
    fn rename_does_not_touch_comments_or_strings() {
        let source = SourceId::new("memory:test.lab");
        let mut workspace = Workspace::new();
        workspace.set_document(
            source.clone(),
            1,
            "plasmid reporter:\n  # reporter\n  label = \"reporter\"\n".to_owned(),
        );
        let offset = workspace.text(&source).unwrap().find("reporter").unwrap();
        assert_eq!(workspace.rename(&source, offset, "sensor").len(), 1);
    }
}
