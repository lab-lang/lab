//! Building a LabOP activity: protocols, actions, pins, and the flows between
//! them.
//!
//! Two rules govern the wiring. Every pin is joined to its own action by an
//! explicit `ObjectFlow`, so a reader never has to infer the connection from
//! containment; and the actions of a protocol are chained by `ControlFlow` from
//! the initial node through to the final node, because a LabOP activity has no
//! other way to say that one step precedes another.

use std::collections::{BTreeSet, HashMap};

use super::identity::{self, Kind, Object};
use super::library::Primitive;
use super::triples::{Graph, Term};
use super::vocabulary::{self as vocab, Unit};

/// A value written directly onto an action rather than flowing into it.
#[derive(Clone, Debug)]
pub(super) enum Value {
    /// A reference to a top-level resource, such as a reagent or a container.
    Reference(String),
    Measure {
        amount: f64,
        unit: Unit,
    },
    Integer(i64),
    Text(String),
}

/// The accumulating document, owning the graph and the resources protocols
/// share. Declaring a resource or a behavior twice is harmless because the
/// graph collapses duplicate statements, but the registries keep the work
/// proportional to the number of distinct resources.
pub(super) struct Document {
    graph: Graph,
    namespace: String,
    resources: BTreeSet<String>,
    behaviors: BTreeSet<String>,
}

impl Document {
    pub(super) fn new(namespace: impl Into<String>) -> Self {
        Self {
            graph: Graph::new(),
            namespace: namespace.into(),
            resources: BTreeSet::new(),
            behaviors: BTreeSet::new(),
        }
    }

    pub(super) fn render(&self) -> String {
        self.graph.render()
    }

    pub(super) fn statement_count(&self) -> usize {
        self.graph.len()
    }

    /// A reagent, strain, or other material named by the build. Returns the IRI
    /// so callers can reference it from a pin.
    pub(super) fn component(&mut self, name: &str) -> String {
        let display_id = identity::display_id(name);
        let iri = format!("{}/{display_id}", self.namespace);
        if self.resources.insert(iri.clone()) {
            let object = identity::top_level(
                &mut self.graph,
                &self.namespace,
                &display_id,
                vocab::SBOL_COMPONENT,
                Kind::Native,
            );
            object.set_name(&mut self.graph, name);
            // SBOL requires a type on a Component; the build does not know
            // whether a named item is DNA or a reagent, so every component is
            // declared with the general-purpose functional entity term.
            self.graph.link(
                object.iri(),
                vocab::SBOL_TYPE,
                "https://identifiers.org/SBO:0000241",
            );
        }
        iri
    }

    /// A container requirement, carried as the Manchester-syntax OWL class
    /// expression LabOP resolves through an external reasoner. Emitting one is
    /// writing a string; only a consumer needs the reasoner.
    pub(super) fn container_spec(&mut self, id: &str, name: &str, query: &str) -> String {
        let display_id = identity::display_id(id);
        let iri = format!("{}/{display_id}", self.namespace);
        if self.resources.insert(iri.clone()) {
            let object = identity::top_level(
                &mut self.graph,
                &self.namespace,
                &display_id,
                vocab::LABOP_CONTAINER_SPEC,
                Kind::TopLevel,
            );
            object.set_name(&mut self.graph, name);
            self.graph
                .push(object.iri(), vocab::LABOP_QUERY_STRING, Term::string(query));
            self.graph.push(
                object.iri(),
                vocab::LABOP_PREFIX_MAP,
                Term::string(PREFIX_MAP),
            );
        }
        iri
    }

    fn declare_behavior(&mut self, primitive: &Primitive) -> String {
        let iri = primitive.iri();
        if self.behaviors.insert(iri.clone()) {
            primitive.declare(&mut self.graph);
        }
        iri
    }

    pub(super) fn protocol(
        &mut self,
        display_id: &str,
        name: &str,
        description: &str,
    ) -> ProtocolBuilder {
        let mut object = identity::top_level(
            &mut self.graph,
            &self.namespace,
            display_id,
            vocab::LABOP_PROTOCOL,
            Kind::TopLevel,
        );
        object.set_name(&mut self.graph, name);
        object.set_description(&mut self.graph, description);

        let initial = object.child(
            &mut self.graph,
            "InitialNode",
            vocab::UML_INITIAL_NODE,
            Kind::Identified,
        );
        let terminal = object.child(
            &mut self.graph,
            "FinalNode",
            vocab::UML_FINAL_NODE,
            Kind::Identified,
        );
        self.graph
            .link(object.iri(), vocab::UML_ACTIVITY_NODE, initial.iri());
        self.graph
            .link(object.iri(), vocab::UML_ACTIVITY_NODE, terminal.iri());

        ProtocolBuilder {
            terminal: terminal.iri().to_owned(),
            previous: initial.iri().to_owned(),
            object,
            forks: HashMap::new(),
        }
    }
}

/// Prefixes a container query is written against, matching the map LabOP's own
/// documents carry.
const PREFIX_MAP: &str = concat!(
    "{\"cont\": \"https://sift.net/container-ontology/container-ontology#\", ",
    "\"om\": \"http://www.ontology-of-units-of-measure.org/resource/om-2/\"}"
);

/// One protocol under construction.
pub(super) struct ProtocolBuilder {
    object: Object,
    terminal: String,
    previous: String,
    forks: HashMap<String, String>,
}

/// An action whose pins are still being attached.
pub(super) struct Action {
    object: Object,
}

impl ProtocolBuilder {
    /// Opens an action calling `primitive`, declaring the behavior if this is
    /// the document's first reference to it.
    pub(super) fn action(&mut self, document: &mut Document, primitive: &Primitive) -> Action {
        let behavior = document.declare_behavior(primitive);
        let object = self.object.child(
            &mut document.graph,
            "CallBehaviorAction",
            vocab::UML_CALL_BEHAVIOR_ACTION,
            Kind::Identified,
        );
        document
            .graph
            .link(object.iri(), vocab::UML_BEHAVIOR, &behavior);
        document
            .graph
            .link(self.object.iri(), vocab::UML_ACTIVITY_NODE, object.iri());
        Action { object }
    }

    /// Attaches a literal value to an action's named parameter.
    pub(super) fn value(
        &mut self,
        document: &mut Document,
        action: &mut Action,
        parameter: &str,
        value: Value,
    ) {
        let pin = action.object.child(
            &mut document.graph,
            "ValuePin",
            vocab::UML_VALUE_PIN,
            Kind::Identified,
        );
        pin.set_name(&mut document.graph, parameter);
        document
            .graph
            .push(pin.iri(), vocab::UML_IS_ORDERED, Term::boolean(true));
        document
            .graph
            .push(pin.iri(), vocab::UML_IS_UNIQUE, Term::boolean(true));
        let mut pin = pin;
        let literal = write_literal(&mut document.graph, &mut pin, value);
        document.graph.link(pin.iri(), vocab::UML_VALUE, &literal);
        document
            .graph
            .link(action.object.iri(), vocab::UML_INPUT, pin.iri());
        self.object_flow(document, pin.iri(), action.object.iri());
    }

    /// Attaches an input pin fed by an upstream output pin.
    pub(super) fn input(
        &mut self,
        document: &mut Document,
        action: &mut Action,
        parameter: &str,
        upstream: &str,
    ) {
        let pin = action.object.child(
            &mut document.graph,
            "InputPin",
            vocab::UML_INPUT_PIN,
            Kind::Identified,
        );
        pin.set_name(&mut document.graph, parameter);
        document
            .graph
            .push(pin.iri(), vocab::UML_IS_ORDERED, Term::boolean(true));
        document
            .graph
            .push(pin.iri(), vocab::UML_IS_UNIQUE, Term::boolean(true));
        document
            .graph
            .link(action.object.iri(), vocab::UML_INPUT, pin.iri());
        let pin_iri = pin.iri().to_owned();
        let source = self.fork_for(document, upstream);
        self.object_flow(document, &source, &pin_iri);
        self.object_flow(document, &pin_iri, action.object.iri());
    }

    /// The node a consumer should draw from. UML treats several object flows
    /// leaving one pin as nondeterministic, so a value with more than one
    /// consumer has to fan out through a `ForkNode`. The consumer count is not
    /// known while building, so the first consumer is routed through the fork
    /// too; a fork with one outgoing edge is well formed.
    fn fork_for(&mut self, document: &mut Document, source: &str) -> String {
        if let Some(existing) = self.forks.get(source) {
            return existing.clone();
        }
        let fork = self.object.child(
            &mut document.graph,
            "ForkNode",
            vocab::UML_FORK_NODE,
            Kind::Identified,
        );
        let fork_iri = fork.iri().to_owned();
        document
            .graph
            .link(self.object.iri(), vocab::UML_ACTIVITY_NODE, &fork_iri);
        self.object_flow(document, source, &fork_iri);
        self.forks.insert(source.to_owned(), fork_iri.clone());
        fork_iri
    }

    /// Adds an output pin and returns its IRI for wiring downstream.
    pub(super) fn output(
        &mut self,
        document: &mut Document,
        action: &mut Action,
        parameter: &str,
    ) -> String {
        let pin = action.object.child(
            &mut document.graph,
            "OutputPin",
            vocab::UML_OUTPUT_PIN,
            Kind::Identified,
        );
        pin.set_name(&mut document.graph, parameter);
        document
            .graph
            .push(pin.iri(), vocab::UML_IS_ORDERED, Term::boolean(true));
        document
            .graph
            .push(pin.iri(), vocab::UML_IS_UNIQUE, Term::boolean(true));
        document
            .graph
            .link(action.object.iri(), vocab::UML_OUTPUT, pin.iri());
        let action_iri = action.object.iri().to_owned();
        let pin_iri = pin.iri().to_owned();
        self.object_flow(document, &action_iri, &pin_iri);
        pin_iri
    }

    /// Closes an action, sequencing it after whatever preceded it.
    pub(super) fn commit(&mut self, document: &mut Document, action: Action) {
        let previous = self.previous.clone();
        let iri = action.object.iri().to_owned();
        self.control_flow(document, &previous, &iri);
        self.previous = iri;
    }

    /// Closes the protocol by running the last action into the final node and
    /// returns its `displayId`. A protocol with no actions still reaches its
    /// final node, which keeps it well formed.
    pub(super) fn finish(mut self, document: &mut Document) -> String {
        let previous = self.previous.clone();
        let terminal = self.terminal.clone();
        self.control_flow(document, &previous, &terminal);
        self.object.display_id().to_owned()
    }

    fn object_flow(&mut self, document: &mut Document, source: &str, target: &str) {
        let edge = self.object.child(
            &mut document.graph,
            "ObjectFlow",
            vocab::UML_OBJECT_FLOW,
            Kind::Identified,
        );
        document.graph.link(edge.iri(), vocab::UML_SOURCE, source);
        document.graph.link(edge.iri(), vocab::UML_TARGET, target);
        document
            .graph
            .link(self.object.iri(), vocab::UML_ACTIVITY_EDGE, edge.iri());
    }

    fn control_flow(&mut self, document: &mut Document, source: &str, target: &str) {
        let edge = self.object.child(
            &mut document.graph,
            "ControlFlow",
            vocab::UML_CONTROL_FLOW,
            Kind::Identified,
        );
        document.graph.link(edge.iri(), vocab::UML_SOURCE, source);
        document.graph.link(edge.iri(), vocab::UML_TARGET, target);
        document
            .graph
            .link(self.object.iri(), vocab::UML_ACTIVITY_EDGE, edge.iri());
    }
}

fn write_literal(graph: &mut Graph, pin: &mut Object, value: Value) -> String {
    match value {
        Value::Reference(target) => {
            let literal = pin.child(
                graph,
                "LiteralReference",
                vocab::UML_LITERAL_REFERENCE,
                Kind::Identified,
            );
            graph.link(literal.iri(), vocab::UML_REFERENCE_VALUE, &target);
            literal.iri().to_owned()
        }
        Value::Measure { amount, unit } => {
            let mut literal = pin.child(
                graph,
                "LiteralIdentified",
                vocab::UML_LITERAL_IDENTIFIED,
                Kind::Identified,
            );
            let measure = literal.child(graph, "Measure", vocab::OM_MEASURE, Kind::Identified);
            graph.push(
                measure.iri(),
                vocab::OM_NUMERICAL_VALUE,
                Term::double(amount),
            );
            graph.link(measure.iri(), vocab::OM_UNIT, unit.iri());
            let measure_iri = measure.iri().to_owned();
            graph.link(literal.iri(), vocab::UML_IDENTIFIED_VALUE, &measure_iri);
            literal.iri().to_owned()
        }
        Value::Integer(amount) => {
            let literal = pin.child(
                graph,
                "LiteralInteger",
                vocab::UML_LITERAL_INTEGER,
                Kind::Identified,
            );
            graph.push(
                literal.iri(),
                vocab::UML_INTEGER_VALUE,
                Term::integer(amount),
            );
            literal.iri().to_owned()
        }
        Value::Text(text) => {
            let literal = pin.child(
                graph,
                "LiteralString",
                vocab::UML_LITERAL_STRING,
                Kind::Identified,
            );
            graph.push(literal.iri(), vocab::UML_STRING_VALUE, Term::string(text));
            literal.iri().to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::labop::library;

    const BASE: &str = "https://lab-lang.org/labop";

    fn build() -> Document {
        let mut document = Document::new(BASE);
        let mut protocol = document.protocol("demo", "Demo", "A demo protocol");
        let mut action = protocol.action(&mut document, &library::PROVISION);
        let water = document.component("water");
        protocol.value(
            &mut document,
            &mut action,
            "resource",
            Value::Reference(water),
        );
        protocol.value(
            &mut document,
            &mut action,
            "amount",
            Value::Measure {
                amount: 10.0,
                unit: Unit::Microlitre,
            },
        );
        protocol.commit(&mut document, action);
        protocol.finish(&mut document);
        document
    }

    #[test]
    fn joins_every_pin_to_its_action_with_an_object_flow() {
        let rendered = build().render();
        let action = format!("{BASE}/demo/CallBehaviorAction1");
        for pin in ["ValuePin1", "ValuePin2"] {
            assert!(
                object_flow_exists(&rendered, &format!("{action}/{pin}"), &action),
                "{pin} is not joined to its action"
            );
        }
    }

    fn object_flow_exists(rendered: &str, source: &str, target: &str) -> bool {
        edges_between(rendered, source, target)
            .iter()
            .any(|edge| edge.contains("/ObjectFlow"))
    }

    fn edges_between(rendered: &str, source: &str, target: &str) -> Vec<String> {
        let subject = |line: &str| {
            line.split_once("> <")
                .expect("statement has a subject")
                .0
                .trim_start_matches('<')
                .to_owned()
        };
        let sources: BTreeSet<String> = rendered
            .lines()
            .filter(|line| {
                line.contains(vocab::UML_SOURCE) && line.ends_with(&format!("<{source}> ."))
            })
            .map(subject)
            .collect();
        rendered
            .lines()
            .filter(|line| {
                line.contains(vocab::UML_TARGET) && line.ends_with(&format!("<{target}> ."))
            })
            .map(subject)
            .filter(|edge| sources.contains(edge))
            .collect()
    }

    #[test]
    fn chains_control_flow_from_initial_through_to_final() {
        let mut document = Document::new(BASE);
        let mut protocol = document.protocol("demo", "Demo", "");
        let first = protocol.action(&mut document, &library::PROVISION);
        protocol.commit(&mut document, first);
        let second = protocol.action(&mut document, &library::SEAL);
        protocol.commit(&mut document, second);
        protocol.finish(&mut document);

        let rendered = document.render();
        let node = |name: &str| format!("{BASE}/demo/{name}");
        assert!(control_flow_exists(
            &rendered,
            &node("InitialNode1"),
            &node("CallBehaviorAction1")
        ));
        assert!(control_flow_exists(
            &rendered,
            &node("CallBehaviorAction1"),
            &node("CallBehaviorAction2")
        ));
        assert!(control_flow_exists(
            &rendered,
            &node("CallBehaviorAction2"),
            &node("FinalNode1")
        ));
    }

    /// True when one control edge carries both `source` and `target`, which is
    /// what sequences the nodes rather than merely placing both in the graph.
    fn control_flow_exists(rendered: &str, source: &str, target: &str) -> bool {
        edges_between(rendered, source, target)
            .iter()
            .any(|edge| edge.contains("/ControlFlow"))
    }

    #[test]
    fn declares_each_referenced_behavior_once() {
        let mut document = Document::new("https://lab-lang.org/labop");
        let mut protocol = document.protocol("demo", "Demo", "");
        for _ in 0..3 {
            let action = protocol.action(&mut document, &library::PROVISION);
            protocol.commit(&mut document, action);
        }
        protocol.finish(&mut document);
        let rendered = document.render();
        let declarations = rendered
            .lines()
            .filter(|line| {
                line.starts_with(&format!("<{}>", library::PROVISION.iri()))
                    && line.contains(vocab::LABOP_PRIMITIVE)
            })
            .count();
        assert_eq!(declarations, 1);
    }

    #[test]
    fn writes_a_measure_with_its_unit() {
        let rendered = build().render();
        assert!(rendered.contains("hasNumericalValue> \"10.0\""));
        assert!(rendered.contains(Unit::Microlitre.iri()));
    }

    #[test]
    fn reuses_a_component_declared_twice() {
        let mut document = Document::new("https://lab-lang.org/labop");
        let first = document.component("water");
        let second = document.component("water");
        assert_eq!(first, second);
        let count = document
            .render()
            .lines()
            .filter(|line| line.contains(vocab::SBOL_COMPONENT) && line.contains(vocab::RDF_TYPE))
            .count();
        assert_eq!(count, 1);
    }
}
