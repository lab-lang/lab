use std::collections::{BTreeMap, BTreeSet};

use lab_language::{
    Analysis, Diagnostic, ModuleId, SemanticEnvironment, SourceId, analyze_module_in_environment,
    ast, parse_module,
};

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
    /// Synthesized from the source path so an open document can stand in
    /// for a module another open document `use`s (see `synthesize_module_id`).
    module: ModuleId,
    /// Dotted paths named by this document's own `use` items, cached at the
    /// same time as `text` so the dependency graph can be rebuilt without
    /// re-parsing every open document on every edit.
    imports: BTreeSet<String>,
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
        let module = synthesize_module_id(&source);
        let touched = [source.clone()].into_iter().collect();
        self.insert_document(source, version, text, module);
        self.reanalyze_from(touched);
    }

    /// Register a document whose module name the host already knows. A file
    /// inside a package takes its name from that package's manifest, which no
    /// path alone can reveal, so a host that has read the manifest supplies the
    /// name rather than letting it be guessed.
    pub fn set_module_document(
        &mut self,
        source: SourceId,
        version: i64,
        text: String,
        module: ModuleId,
    ) {
        let touched = [source.clone()].into_iter().collect();
        self.insert_document(source, version, text, module);
        self.reanalyze_from(touched);
    }

    /// Register several documents and analyze once, so opening one file in a
    /// package does not re-check the package once per sibling.
    pub fn set_module_documents(
        &mut self,
        documents: impl IntoIterator<Item = (SourceId, i64, String, ModuleId)>,
    ) {
        let mut touched = BTreeSet::new();
        for (source, version, text, module) in documents {
            touched.insert(source.clone());
            self.insert_document(source, version, text, module);
        }
        self.reanalyze_from(touched);
    }

    pub fn contains(&self, source: &SourceId) -> bool {
        self.documents.contains_key(source)
    }

    fn insert_document(&mut self, source: SourceId, version: i64, text: String, module: ModuleId) {
        let imports = parsed_use_paths(&text).into_iter().collect();
        self.documents.insert(
            source,
            Document {
                version,
                text,
                module,
                imports,
                analysis: Analysis {
                    syntax: None,
                    checked: None,
                    diagnostics: Vec::new(),
                },
            },
        );
    }

    pub fn remove_document(&mut self, source: &SourceId) {
        let removed_module = self.documents.remove(source).map(|document| document.module);
        // Anyone who imported the now-gone document needs to be re-checked,
        // since a `use` that resolved before now dangles.
        let touched = removed_module.map_or_else(BTreeSet::new, |module| {
            self.documents
                .iter()
                .filter(|(_, document)| document.imports.contains(module.as_str()))
                .map(|(source, _)| source.clone())
                .collect()
        });
        self.reanalyze_from(touched);
    }

    /// Re-checks `touched` and every open document that transitively depends
    /// on it (directly or through a chain of `use`s), reusing the cached
    /// analysis of everything else instead of re-parsing and re-checking the
    /// whole workspace on every edit.
    ///
    /// This mirrors `lab-project`'s package compiler (topological
    /// compile-and-insert into a shared `SemanticEnvironment`), adapted for
    /// in-memory documents with no manifest: module names come from each
    /// document's own path rather than a package namespace, and an import
    /// cycle — which a real project treats as a hard error — just falls
    /// back to compiling whatever's left against the partial environment,
    /// since a cycle can be a normal transient state while someone is
    /// mid-edit, not necessarily something to stop analyzing over.
    fn reanalyze_from(&mut self, touched: BTreeSet<SourceId>) {
        let by_name: BTreeMap<String, SourceId> = self
            .documents
            .iter()
            .map(|(source, document)| (document.module.as_str().to_owned(), source.clone()))
            .collect();

        let mut dependents: BTreeMap<String, Vec<SourceId>> = BTreeMap::new();
        for (source, document) in &self.documents {
            for name in &document.imports {
                dependents.entry(name.clone()).or_default().push(source.clone());
            }
        }

        // A document whose text just changed is dirty, and so is anyone that
        // (directly or transitively) `use`s it — their checked interface may
        // depend on exports that just moved or disappeared.
        let mut dirty: BTreeSet<SourceId> = BTreeSet::new();
        let mut frontier: Vec<SourceId> = touched.into_iter().collect();
        while let Some(source) = frontier.pop() {
            if !dirty.insert(source.clone()) {
                continue;
            }
            let Some(document) = self.documents.get(&source) else {
                continue;
            };
            if let Some(importers) = dependents.get(document.module.as_str()) {
                frontier.extend(importers.iter().cloned());
            }
        }

        let local_imports: BTreeMap<SourceId, BTreeSet<String>> = self
            .documents
            .iter()
            .map(|(source, document)| {
                let names = document
                    .imports
                    .iter()
                    .filter(|name| {
                        by_name
                            .get(*name)
                            .is_some_and(|dependency| dependency != source)
                    })
                    .cloned()
                    .collect();
                (source.clone(), names)
            })
            .collect();

        let mut environment = SemanticEnvironment::default();
        let mut compiled: BTreeSet<SourceId> = BTreeSet::new();
        let mut remaining: BTreeSet<SourceId> = self.documents.keys().cloned().collect();
        let mut analyses: BTreeMap<SourceId, Analysis> = BTreeMap::new();

        while !remaining.is_empty() {
            let mut ready: Vec<SourceId> = remaining
                .iter()
                .filter(|source| {
                    local_imports[*source]
                        .iter()
                        .all(|name| by_name.get(name).is_some_and(|dep| compiled.contains(dep)))
                })
                .cloned()
                .collect();

            if ready.is_empty() {
                // Nothing left can become ready on its own (an import cycle
                // among the still-open documents) — compile the rest against
                // the environment built so far so each reports its own
                // unresolved imports, rather than looping forever.
                ready = remaining.iter().cloned().collect();
            }

            for source in &ready {
                let document = &self.documents[source];
                let interface = if dirty.contains(source) {
                    let analysis = analyze_module_in_environment(
                        source.clone(),
                        document.module.clone(),
                        &document.text,
                        &environment,
                    );
                    let interface = analysis
                        .checked
                        .as_ref()
                        .map(|checked| checked.interface.clone());
                    analyses.insert(source.clone(), analysis);
                    interface
                } else {
                    // Unaffected by this edit — reuse the interface computed
                    // last time instead of re-parsing and re-checking.
                    document
                        .analysis
                        .checked
                        .as_ref()
                        .map(|checked| checked.interface.clone())
                };
                if let Some(interface) = interface {
                    environment.insert(document.module.as_str(), interface);
                }
                compiled.insert(source.clone());
            }
            for source in &ready {
                remaining.remove(source);
            }
        }

        for (source, analysis) in analyses {
            if let Some(document) = self.documents.get_mut(&source) {
                document.analysis = analysis;
            }
        }
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
                        SymbolKind::Data | SymbolKind::Artifact => CompletionKind::Type,
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

/// Derives a dotted module name from a document's source identity, the way
/// a real package derives one from a file's path under `src/` (see
/// `lab_package::package::module_name`) — minus the leading package
/// namespace segment, since an open document has no package. A leading URI
/// scheme (`file://`, `memory:`) is stripped first so editor- and
/// test-style source ids both produce a sensible name; a `.lab` suffix is
/// stripped, and path segments become dotted, `-`-to-`_` segments, matching
/// what a `use` statement would spell for a same-named on-disk module.
fn synthesize_module_id(source: &SourceId) -> ModuleId {
    let raw = source.0.as_str();
    let without_scheme = match raw.find("://") {
        Some(index) => &raw[index + 3..],
        None => match raw.find(':') {
            Some(index) if raw[..index].chars().all(|c| c.is_ascii_alphabetic()) => {
                &raw[index + 1..]
            }
            _ => raw,
        },
    };
    let trimmed = without_scheme.trim_start_matches('/');
    let without_extension = trimmed.strip_suffix(".lab").unwrap_or(trimmed);
    let name = without_extension
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.replace('-', "_"))
        .collect::<Vec<_>>()
        .join(".");
    ModuleId::new(name)
}

fn use_path_text(path: &ast::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.value.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

/// The dotted paths named by every `use` item in `text`, or empty if the
/// document doesn't parse — a syntax-broken document simply can't gate or
/// satisfy anyone's import yet, which `analyze_module_in_environment` will
/// also report on its own terms.
fn parsed_use_paths(text: &str) -> Vec<String> {
    match parse_module(text) {
        Ok(module) => module
            .items
            .iter()
            .filter_map(|item| match item {
                ast::Item::Use(use_decl) => Some(use_path_text(&use_decl.path)),
                _ => None,
            })
            .collect(),
        Err(_) => Vec::new(),
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
DH5alpha = chassis("DH5alpha")
ampicillin = antibiotic("ampicillin")

plasmid reporter:
  sequence: dna("ACGT")
  backbone: part_receiver
  components: [J23101]
  restriction_enzyme: BsaI
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
            "chassis",
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

    #[test]
    fn resolves_use_of_another_open_document_by_synthesized_module_name() {
        let design = SourceId::new("designs/donor.lab");
        let program = SourceId::new("programs/main.lab");
        let mut workspace = Workspace::new();
        workspace.set_document(
            design.clone(),
            1,
            "plasmid donor:\n  sequence: dna(\"ACGT\")\n  require topology == circular\n"
                .to_owned(),
        );
        workspace.set_document(
            program.clone(),
            1,
            "use designs.donor\n\nselected = donor\n".to_owned(),
        );

        assert!(
            workspace.diagnostics(&design).is_empty(),
            "{:?}",
            workspace.diagnostics(&design)
        );
        assert!(
            workspace.diagnostics(&program).is_empty(),
            "{:?}",
            workspace.diagnostics(&program)
        );
    }

    #[test]
    fn editing_the_imported_document_reanalyzes_the_importer() {
        let design = SourceId::new("designs/donor.lab");
        let program = SourceId::new("programs/main.lab");
        let mut workspace = Workspace::new();
        workspace.set_document(
            design.clone(),
            1,
            "plasmid donor:\n  sequence: dna(\"ACGT\")\n  require topology == circular\n"
                .to_owned(),
        );
        workspace.set_document(
            program.clone(),
            1,
            "use designs.donor\n\nselected = donor\n".to_owned(),
        );
        assert!(workspace.diagnostics(&program).is_empty());

        // Renaming the plasmid in its own file, without touching the
        // importer, should make the importer's reference dangle.
        workspace.set_document(
            design,
            2,
            "plasmid renamed:\n  sequence: dna(\"ACGT\")\n  require topology == circular\n"
                .to_owned(),
        );
        assert!(!workspace.diagnostics(&program).is_empty());
    }

    #[test]
    fn editing_a_document_reanalyzes_transitive_dependents_two_hops_away() {
        let design = SourceId::new("designs/a.lab");
        let middle = SourceId::new("designs/b.lab");
        let program = SourceId::new("programs/main.lab");
        let mut workspace = Workspace::new();
        workspace.set_document(
            design.clone(),
            1,
            "plasmid donor:\n  sequence: dna(\"ACGT\")\n  require topology == circular\n"
                .to_owned(),
        );
        workspace.set_document(
            middle.clone(),
            1,
            "use designs.a\n\nselected = donor\n".to_owned(),
        );
        workspace.set_document(
            program.clone(),
            1,
            "use designs.b\n\npicked = selected\n".to_owned(),
        );
        assert!(workspace.diagnostics(&middle).is_empty());
        assert!(workspace.diagnostics(&program).is_empty());

        // Renaming the plasmid two hops upstream, without touching `middle`
        // or `program` directly, should still make `program` see an error:
        // `middle` fails to resolve `donor`, which makes `program`'s `use`
        // of `middle` unresolved in turn.
        workspace.set_document(
            design,
            2,
            "plasmid renamed:\n  sequence: dna(\"ACGT\")\n  require topology == circular\n"
                .to_owned(),
        );
        assert!(!workspace.diagnostics(&middle).is_empty());
        assert!(!workspace.diagnostics(&program).is_empty());
    }

    #[test]
    fn import_cycle_between_open_documents_terminates_and_reports_diagnostics() {
        let a = SourceId::new("designs/a.lab");
        let b = SourceId::new("designs/b.lab");
        let mut workspace = Workspace::new();
        workspace.set_document(a.clone(), 1, "use designs.b\n".to_owned());
        workspace.set_document(b.clone(), 1, "use designs.a\n".to_owned());

        assert!(!workspace.diagnostics(&a).is_empty());
        assert!(!workspace.diagnostics(&b).is_empty());
    }

    #[test]
    fn synthesizes_module_ids_from_varied_source_id_conventions() {
        assert_eq!(
            synthesize_module_id(&SourceId::new("designs/circuit.lab")).as_str(),
            "designs.circuit"
        );
        assert_eq!(
            synthesize_module_id(&SourceId::new("file:///proj/src/build-plasmid.lab")).as_str(),
            "proj.src.build_plasmid"
        );
        assert_eq!(
            synthesize_module_id(&SourceId::new("memory:test.lab")).as_str(),
            "test"
        );
    }
}
