//! Naming of the objects a LabOP document is made of.
//!
//! `sbol3` validates and assembles identities — [`DisplayId`], [`Namespace`],
//! and [`SbolIdentity`] are used here for exactly that — but its object
//! builders cannot serve this backend. Each one is keyed to a variant of the
//! closed `SbolClass` enum (`SubComponent`, `ComponentReference`, and the rest
//! of the SBOL 3 vocabulary), and a LabOP document is built almost entirely
//! from classes that enum does not contain: `labop:Protocol`,
//! `uml:CallBehaviorAction`, `uml:ObjectFlow`. What remains here is only what
//! the library has no representation for.
//!
//! Two conventions, inherited from pySBOL3, decide whether a document another
//! tool reads resolves or dangles. A `displayId` is the class name followed by
//! a counter that restarts within each parent, so the second pin of an action
//! is `InputPin2` even though the previous action also has an `InputPin1`. And
//! a class from an ontology layered over SBOL carries two `rdf:type`
//! statements, its own and the SBOL class it specializes, while a native SBOL3
//! class carries only its own.

use std::collections::HashMap;

use sbol3::{DisplayId, Namespace, SbolIdentity};

use super::triples::{Graph, Term};
use super::vocabulary as vocab;

/// Whether an object's class comes from SBOL3 itself or from an ontology
/// layered over it, which decides how many `rdf:type` statements it carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    /// A native SBOL3 class, typed only as itself.
    Native,
    /// A generated class that is also an `sbol:TopLevel`.
    TopLevel,
    /// A generated class that is also an `sbol:Identified` child.
    Identified,
}

/// An object under construction, holding the identity every statement about it
/// is written against.
#[derive(Clone, Debug)]
pub(super) struct Object {
    iri: String,
    display_id: String,
    counters: HashMap<&'static str, u32>,
}

impl Object {
    pub(super) fn iri(&self) -> &str {
        &self.iri
    }

    pub(super) fn display_id(&self) -> &str {
        &self.display_id
    }

    /// Allocates the next `displayId` for `class` within this parent and
    /// returns the child object, writing the child's identity statements.
    ///
    /// A child's IRI extends its parent's, so the parent stands in for a
    /// namespace. `DisplayId` still validates the allocated name, which fails
    /// only if a class name reaches here that is not a bare identifier.
    pub(super) fn child(
        &mut self,
        graph: &mut Graph,
        class: &'static str,
        class_iri: &str,
        kind: Kind,
    ) -> Object {
        let counter = self.counters.entry(class).or_insert(0);
        *counter += 1;
        let display_id = DisplayId::new(format!("{class}{counter}"))
            .expect("a UML or LabOP class name is a valid displayId");
        let child = Object {
            iri: format!("{}/{}", self.iri, display_id.as_str()),
            display_id: display_id.into_string(),
            counters: HashMap::new(),
        };
        child.declare(graph, class_iri, kind, None);
        child
    }

    fn declare(&self, graph: &mut Graph, class_iri: &str, kind: Kind, namespace: Option<&str>) {
        graph.link(&self.iri, vocab::RDF_TYPE, class_iri);
        match kind {
            Kind::Native => {}
            Kind::TopLevel => graph.link(&self.iri, vocab::RDF_TYPE, vocab::SBOL_TOP_LEVEL),
            Kind::Identified => graph.link(&self.iri, vocab::RDF_TYPE, vocab::SBOL_IDENTIFIED),
        }
        graph.push(
            &self.iri,
            vocab::SBOL_DISPLAY_ID,
            Term::string(&self.display_id),
        );
        if let Some(namespace) = namespace {
            graph.link(&self.iri, vocab::SBOL_NAMESPACE, namespace);
        }
    }

    pub(super) fn set_name(&self, graph: &mut Graph, name: &str) {
        graph.push(&self.iri, vocab::SBOL_NAME, Term::string(name));
    }

    pub(super) fn set_description(&self, graph: &mut Graph, description: &str) {
        graph.push(
            &self.iri,
            vocab::SBOL_DESCRIPTION,
            Term::string(description),
        );
    }
}

/// A top-level object, whose IRI is its namespace extended by a caller-chosen
/// `displayId` rather than a counter.
///
/// The identity is assembled by [`SbolIdentity`], so an invalid namespace or
/// `displayId` is refused here rather than surfacing as a validation error
/// against the finished document.
pub(super) fn top_level(
    graph: &mut Graph,
    namespace: &str,
    display_id: &str,
    class_iri: &str,
    kind: Kind,
) -> Object {
    let namespace = Namespace::new(namespace).expect("backend namespaces are valid IRIs");
    let display_id =
        DisplayId::new(display_id).expect("display identifiers are encoded before they reach here");
    let identity = SbolIdentity::new(namespace.clone(), display_id.clone());
    let object = Object {
        iri: identity.to_iri().into_string(),
        display_id: display_id.into_string(),
        counters: HashMap::new(),
    };
    object.declare(graph, class_iri, kind, Some(namespace.as_str()));
    object
}

/// Converts an arbitrary Lab identifier into a `displayId` satisfying SBOL rule
/// sbol3-10201: an ASCII letter or underscore first, then ASCII alphanumerics
/// and underscores.
///
/// `sbol3::sanitize_display_id` also satisfies that rule but replaces every
/// invalid character with `_`, which maps `pUC19-A` and `pUC19_A` onto one
/// identifier. Two artifacts sharing an IRI would silently become one object,
/// so an invalid character is encoded here rather than replaced.
pub(super) fn display_id(name: &str) -> String {
    let mut output = String::with_capacity(name.len());
    for character in name.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            output.push(character);
        } else {
            // Encoding keeps `pUC19-A` and `pUC19_A` distinguishable.
            for byte in character.to_string().as_bytes() {
                output.push_str(&format!("_x{byte:02x}"));
            }
        }
    }
    if output
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_digit())
    {
        output.insert(0, '_');
    }
    if output.is_empty() {
        output.push('_');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restarts_child_counters_within_each_parent() {
        let mut graph = Graph::new();
        let mut first = top_level(
            &mut graph,
            "https://example.org",
            "First",
            vocab::LABOP_PROTOCOL,
            Kind::TopLevel,
        );
        let mut second = top_level(
            &mut graph,
            "https://example.org",
            "Second",
            vocab::LABOP_PROTOCOL,
            Kind::TopLevel,
        );
        let one = first.child(&mut graph, "InputPin", "urn:pin", Kind::Identified);
        let two = first.child(&mut graph, "InputPin", "urn:pin", Kind::Identified);
        let other = second.child(&mut graph, "InputPin", "urn:pin", Kind::Identified);
        assert_eq!(one.display_id(), "InputPin1");
        assert_eq!(two.display_id(), "InputPin2");
        assert_eq!(other.display_id(), "InputPin1");
        assert_eq!(other.iri(), "https://example.org/Second/InputPin1");
    }

    #[test]
    fn counts_each_class_separately() {
        let mut graph = Graph::new();
        let mut parent = top_level(
            &mut graph,
            "https://example.org",
            "P",
            vocab::LABOP_PROTOCOL,
            Kind::TopLevel,
        );
        let input = parent.child(&mut graph, "InputPin", "urn:in", Kind::Identified);
        let value = parent.child(&mut graph, "ValuePin", "urn:value", Kind::Identified);
        assert_eq!(input.display_id(), "InputPin1");
        assert_eq!(value.display_id(), "ValuePin1");
    }

    #[test]
    fn writes_one_type_for_native_classes_and_two_for_generated_ones() {
        let mut graph = Graph::new();
        top_level(
            &mut graph,
            "https://example.org",
            "Reagent",
            vocab::SBOL_COMPONENT,
            Kind::Native,
        );
        top_level(
            &mut graph,
            "https://example.org",
            "Run",
            vocab::LABOP_PROTOCOL,
            Kind::TopLevel,
        );
        let rendered = graph.render();
        assert!(!rendered.contains("<https://example.org/Reagent> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://sbols.org/v3#TopLevel>"));
        assert!(rendered.contains("<https://example.org/Run> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://sbols.org/v3#TopLevel>"));
    }

    #[test]
    fn keeps_distinct_names_distinct_after_encoding() {
        assert_eq!(display_id("pUC19_A"), "pUC19_A");
        assert_ne!(display_id("pUC19-A"), display_id("pUC19_A"));
        assert_eq!(display_id("5prime"), "_5prime");
    }

    /// The encoding must still satisfy sbol3-10201, the rule the library's own
    /// sanitizer enforces, even though it resolves collisions differently.
    #[test]
    fn encoded_identifiers_satisfy_the_display_id_rule() {
        for name in ["pUC19-A", "5prime", "", "a b/c", "µL", "p.gfp:1"] {
            let encoded = display_id(name);
            assert_eq!(
                encoded,
                sbol3::design::sanitize_display_id(&encoded),
                "'{name}' encoded as '{encoded}', which the spec rule would rewrite"
            );
        }
    }
}
