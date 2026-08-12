//! Statement building over the `sbol-rdf` term model, and the canonical
//! N-Triples serialization LabOP documents use.
//!
//! Terms, escaping, and serialization come from `sbol_rdf`, the same library
//! that reads the result back, so the emitter cannot disagree with a real SBOL
//! implementation about how a term is written.
//!
//! Ordering is this module's own responsibility, because the serializer emits
//! statements in insertion order. LabOP's documents are sorted byte-wise over
//! the rendered line, which places a child's statements before its parent's
//! (`/` precedes `>`). Sorting the serialized lines reproduces that exactly;
//! sorting structured triples would not, since a shorter IRI compares before
//! its own extensions.

use sbol3::{Iri, Literal, RdfFormat, RdfGraph, Resource, Term as RdfTerm, Triple};

/// An object term, built with the datatypes SBOL documents use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Term(RdfTerm);

impl Term {
    pub(super) fn iri(value: impl Into<String>) -> Self {
        Self(RdfTerm::Resource(Resource::iri(value)))
    }

    pub(super) fn string(value: impl Into<String>) -> Self {
        Self(RdfTerm::Literal(Literal::simple(value)))
    }

    pub(super) fn integer(value: i64) -> Self {
        Self::typed(value.to_string(), super::vocabulary::XSD_INTEGER)
    }

    pub(super) fn boolean(value: bool) -> Self {
        Self::typed(value.to_string(), super::vocabulary::XSD_BOOLEAN)
    }

    /// A double in the lexical form SBOL documents carry, which always shows a
    /// decimal point so a whole number is not mistaken for an integer.
    pub(super) fn double(value: f64) -> Self {
        let lexical = if value.fract() == 0.0 {
            format!("{value:.1}")
        } else {
            let mut rendered = format!("{value}");
            if !rendered.contains('.') {
                rendered.push_str(".0");
            }
            rendered
        };
        Self::typed(lexical, super::vocabulary::XSD_DOUBLE)
    }

    fn typed(lexical: String, datatype: &'static str) -> Self {
        Self(RdfTerm::Literal(Literal::new(
            lexical,
            Iri::new_unchecked(datatype),
            None,
        )))
    }
}

/// An accumulating set of statements that renders as canonical N-Triples.
#[derive(Debug, Default)]
pub(super) struct Graph {
    triples: Vec<Triple>,
}

impl Graph {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn push(&mut self, subject: &str, predicate: &str, object: Term) {
        self.triples.push(Triple {
            subject: Resource::iri(subject),
            predicate: Iri::new_unchecked(predicate),
            object: object.0,
        });
    }

    /// Convenience for the common case of an object that is itself a resource.
    pub(super) fn link(&mut self, subject: &str, predicate: &str, object: &str) {
        self.push(subject, predicate, Term::iri(object));
    }

    /// Statements written so far, before duplicates are collapsed.
    pub(super) fn len(&self) -> usize {
        self.rendered().len()
    }

    pub(super) fn render(&self) -> String {
        let mut output = self.rendered().join("\n");
        output.push('\n');
        output
    }

    /// Serializes through `sbol_rdf`, then orders and deduplicates the lines.
    fn rendered(&self) -> Vec<String> {
        let serialized = RdfGraph::new(self.triples.clone())
            .write(RdfFormat::NTriples)
            .expect("N-Triples serialization of well-formed terms cannot fail");
        let mut lines: Vec<String> = serialized
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_owned)
            .collect();
        lines.sort_unstable();
        lines.dedup();
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_statements_in_byte_order() {
        let mut graph = Graph::new();
        graph.link("https://example.org/b", "https://example.org/p", "urn:z");
        graph.link("https://example.org/a", "https://example.org/p", "urn:y");
        let rendered = graph.render();
        let lines = rendered.lines().collect::<Vec<_>>();
        assert_eq!(
            lines,
            vec![
                "<https://example.org/a> <https://example.org/p> <urn:y> .",
                "<https://example.org/b> <https://example.org/p> <urn:z> .",
            ]
        );
    }

    /// A child IRI sorts before its parent because `/` precedes `>`, which is
    /// the ordering LabOP's own documents exhibit. Sorting structured triples
    /// would place the parent first, so this is the property that decides the
    /// serialization order is line-based.
    #[test]
    fn orders_children_before_their_parent() {
        let mut graph = Graph::new();
        graph.link("https://example.org/a", "https://example.org/p", "urn:one");
        graph.link(
            "https://example.org/a/child",
            "https://example.org/p",
            "urn:two",
        );
        let rendered = graph.render();
        let child = rendered.find("a/child").expect("child statement present");
        let parent = rendered.find("<https://example.org/a> ").expect("parent");
        assert!(child < parent);
    }

    #[test]
    fn collapses_duplicate_statements() {
        let mut graph = Graph::new();
        graph.link("urn:s", "urn:p", "urn:o");
        graph.link("urn:s", "urn:p", "urn:o");
        assert_eq!(graph.len(), 1);
    }

    #[test]
    fn escapes_literal_control_characters() {
        let mut graph = Graph::new();
        graph.push("urn:s", "urn:p", Term::string("a\"b\\c\nd"));
        assert!(graph.render().contains(r#""a\"b\\c\nd""#));
    }

    #[test]
    fn writes_doubles_with_a_decimal_point() {
        let mut graph = Graph::new();
        graph.push("urn:s", "urn:p", Term::double(100.0));
        assert!(graph.render().contains("\"100.0\""));
    }

    #[test]
    fn writes_typed_literals_with_their_datatype() {
        let mut graph = Graph::new();
        graph.push("urn:s", "urn:p", Term::integer(3));
        graph.push("urn:s", "urn:q", Term::boolean(true));
        let rendered = graph.render();
        assert!(rendered.contains("\"3\"^^<http://www.w3.org/2001/XMLSchema#integer>"));
        assert!(rendered.contains("\"true\"^^<http://www.w3.org/2001/XMLSchema#boolean>"));
    }
}
