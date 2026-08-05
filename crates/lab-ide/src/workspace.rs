use std::collections::{BTreeMap, BTreeSet};

use lab_language::{Analysis, Diagnostic, SourceId, analyze_module};

use crate::semantic::{
    KEYWORDS, declaration, identifier_at, identifier_spans, scan_semantic_tokens, symbol_from_item,
    valid_identifier,
};
use crate::{
    CompletionItem, CompletionKind, DocumentSymbol, Hover, Location, SemanticToken, SymbolKind,
    TextEdit,
};

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
            .map_or_else(Vec::new, |document| {
                scan_semantic_tokens(&document.text, document.analysis.syntax.as_ref())
            })
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

#[cfg(test)]
mod tests {
    use crate::SemanticTokenKind;
    use crate::workspace::*;

    #[test]
    fn indexes_symbols_and_renames_across_documents() {
        let source = SourceId::new("file:///design.lab");
        let mut workspace = Workspace::new();
        workspace.set_document(
            source.clone(),
            1,
            "plasmid reporter:\n  sequence: dna(\"ACGT\")\n".to_owned(),
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
            "workflow build() -> None:\n  # observe\n  return None\n".to_owned(),
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
    fn semantic_tokens_classify_inventory_symbols_as_values_not_types() {
        let source = SourceId::new("memory:inventory.lab");
        let text = r#"use std.bio.inventory

J23101 = part("J23101")
part_receiver = backbone("part_receiver")
BsaI = restriction_enzyme("BsaI")
DH5alpha = strain("DH5alpha")
ampicillin = antibiotic("ampicillin")

plasmid reporter:
  sequence: dna("ACGT")
  backbone: part_receiver
  components: [J23101]
  restriction_enzyme: BsaI
  host: DH5alpha
  selection: ampicillin
  require topology == circular
  accept sequence == design.sequence
"#;
        let mut workspace = Workspace::new();
        workspace.set_document(source.clone(), 1, text.to_owned());

        let tokens = workspace.semantic_tokens(&source);
        for name in ["J23101", "part_receiver", "BsaI", "DH5alpha", "ampicillin"] {
            let matching = tokens
                .iter()
                .filter(|token| &text[token.span.start..token.span.end] == name)
                .collect::<Vec<_>>();
            assert!(!matching.is_empty(), "expected semantic tokens for {name}");
            assert!(
                matching
                    .iter()
                    .all(|token| token.kind == SemanticTokenKind::Variable),
                "{name} should be a value everywhere, found {matching:?}"
            );
        }

        for constructor in [
            "part",
            "backbone",
            "restriction_enzyme",
            "strain",
            "antibiotic",
        ] {
            let token = tokens
                .iter()
                .find(|token| &text[token.span.start..token.span.end] == constructor)
                .unwrap();
            assert_eq!(token.kind, SemanticTokenKind::Function);
        }
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
