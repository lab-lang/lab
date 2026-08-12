//! RDF terms and the canonical N-Triples serialization LabOP documents use.
//!
//! A LabOP document is exchanged as sorted N-Triples: one statement per line,
//! byte-ordered over the rendered line. Sorting is what makes two emissions of
//! the same protocol comparable, so the graph stores rendered lines in a
//! [`BTreeSet`] and duplicate statements collapse rather than accumulate.

use std::collections::BTreeSet;
use std::fmt::Write as _;

/// An RDF term in subject, predicate, or object position.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Term {
    Iri(String),
    /// A plain string literal, serialized without a datatype suffix.
    Plain(String),
    Typed {
        lexical: String,
        datatype: &'static str,
    },
}

impl Term {
    pub(super) fn iri(value: impl Into<String>) -> Self {
        Self::Iri(value.into())
    }

    pub(super) fn string(value: impl Into<String>) -> Self {
        Self::Plain(value.into())
    }

    pub(super) fn integer(value: i64) -> Self {
        Self::Typed {
            lexical: value.to_string(),
            datatype: super::vocabulary::XSD_INTEGER,
        }
    }

    pub(super) fn boolean(value: bool) -> Self {
        Self::Typed {
            lexical: value.to_string(),
            datatype: super::vocabulary::XSD_BOOLEAN,
        }
    }

    /// A double in the lexical form pySBOL3 writes, which always carries a
    /// decimal point so `100` round-trips as `100.0` rather than an integer.
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
        Self::Typed {
            lexical,
            datatype: super::vocabulary::XSD_DOUBLE,
        }
    }

    fn render(&self) -> String {
        match self {
            Self::Iri(value) => format!("<{}>", escape_iri(value)),
            Self::Plain(value) => format!("\"{}\"", escape_literal(value)),
            Self::Typed { lexical, datatype } => {
                format!("\"{}\"^^<{datatype}>", escape_literal(lexical))
            }
        }
    }
}

/// Escapes the characters N-Triples forbids inside an IRI reference. Generated
/// IRIs are built from identifiers that are already safe, so this guards
/// against a name reaching the emitter with a character that would otherwise
/// produce an unparseable document.
fn escape_iri(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '<' => output.push_str("%3C"),
            '>' => output.push_str("%3E"),
            '"' => output.push_str("%22"),
            '{' => output.push_str("%7B"),
            '}' => output.push_str("%7D"),
            '|' => output.push_str("%7C"),
            '^' => output.push_str("%5E"),
            '`' => output.push_str("%60"),
            '\\' => output.push_str("%5C"),
            ' ' => output.push_str("%20"),
            character if (character as u32) <= 0x20 => {
                let _ = write!(output, "%{:02X}", character as u32);
            }
            character => output.push(character),
        }
    }
    output
}

fn escape_literal(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character => output.push(character),
        }
    }
    output
}

/// An accumulating set of statements that renders as canonical N-Triples.
#[derive(Debug, Default)]
pub(super) struct Graph {
    lines: BTreeSet<String>,
}

impl Graph {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn push(&mut self, subject: &str, predicate: &str, object: Term) {
        let line = format!(
            "<{}> <{}> {} .",
            escape_iri(subject),
            escape_iri(predicate),
            object.render()
        );
        self.lines.insert(line);
    }

    /// Convenience for the common case of an object that is itself a resource.
    pub(super) fn link(&mut self, subject: &str, predicate: &str, object: &str) {
        self.push(subject, predicate, Term::iri(object));
    }

    pub(super) fn len(&self) -> usize {
        self.lines.len()
    }

    pub(super) fn render(&self) -> String {
        let mut output = String::new();
        for line in &self.lines {
            output.push_str(line);
            output.push('\n');
        }
        output
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
    /// the ordering LabOP's own documents exhibit.
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
        let Term::Typed { lexical, .. } = Term::double(100.0) else {
            panic!("double is a typed literal");
        };
        assert_eq!(lexical, "100.0");
    }
}
