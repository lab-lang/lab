use std::collections::BTreeSet;

use lab_language::{Span, ast};

use crate::{DocumentSymbol, SemanticToken, SemanticTokenKind, SymbolKind};

pub(crate) const KEYWORDS: &[&str] = &[
    "use",
    "circuit",
    "plasmid",
    "strain",
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

pub(crate) fn declaration(item: &ast::Item) -> Option<(&str, SymbolKind, Span)> {
    match item {
        ast::Item::Use(_) => None,
        ast::Item::Circuit(item) => Some((&item.name.value, SymbolKind::Circuit, item.name.span)),
        ast::Item::Artifact(item) => Some((&item.name.value, SymbolKind::Artifact, item.name.span)),
        ast::Item::Data(item) => Some((&item.name.value, SymbolKind::Data, item.name.span)),
        ast::Item::Workflow(item) => Some((&item.name.value, SymbolKind::Workflow, item.name.span)),
        ast::Item::Binding(item) => item
            .names
            .first()
            .map(|name| (&*name.value, SymbolKind::Variable, name.span)),
    }
}

pub(crate) fn symbol_from_item(item: &ast::Item) -> DocumentSymbol {
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
            .chain(match &workflow.outputs {
                ast::WorkflowOutputs::Single { .. } => [].as_slice().iter(),
                ast::WorkflowOutputs::Named { fields } => fields.as_slice().iter(),
            })
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

pub(crate) fn valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        && !KEYWORDS.contains(&name)
}

pub(crate) fn identifier_at(text: &str, offset: usize) -> Option<(&str, Span)> {
    identifier_spans(text).find(|(_, span)| span.start <= offset && offset <= span.end)
}

pub(crate) fn identifier_spans(text: &str) -> impl Iterator<Item = (&str, Span)> {
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

#[derive(Default)]
struct SemanticNames {
    values: BTreeSet<String>,
    types: BTreeSet<String>,
    functions: BTreeSet<String>,
}

fn semantic_names(module: Option<&ast::Module>) -> SemanticNames {
    let mut names = SemanticNames::default();
    let Some(module) = module else {
        return names;
    };
    for item in &module.items {
        match item {
            ast::Item::Use(_) => {}
            ast::Item::Circuit(declaration) => {
                names.functions.insert(declaration.name.value.clone());
                names.types.extend(
                    declaration
                        .parameters
                        .iter()
                        .map(|parameter| parameter.name.value.clone()),
                );
            }
            ast::Item::Artifact(declaration) => {
                names.values.insert(declaration.name.value.clone());
            }
            ast::Item::Data(declaration) => {
                names.types.insert(declaration.name.value.clone());
                names.types.extend(
                    declaration
                        .parameters
                        .iter()
                        .map(|parameter| parameter.name.value.clone()),
                );
            }
            ast::Item::Workflow(declaration) => {
                names.functions.insert(declaration.name.value.clone());
            }
            ast::Item::Binding(binding) => {
                names
                    .values
                    .extend(binding.names.iter().map(|name| name.value.clone()));
            }
        }
    }
    names
}

pub(crate) fn scan_semantic_tokens(text: &str, module: Option<&ast::Module>) -> Vec<SemanticToken> {
    let names = semantic_names(module);
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
                } else if names.values.contains(word) {
                    SemanticTokenKind::Variable
                } else if names.types.contains(word) {
                    SemanticTokenKind::Type
                } else if names.functions.contains(word) {
                    SemanticTokenKind::Function
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
