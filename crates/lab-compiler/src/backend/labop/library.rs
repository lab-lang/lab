//! Behavior definitions a emitted document must carry.
//!
//! A `CallBehaviorAction` names its behavior by IRI, and nothing in the
//! document says what that behavior expects unless the behavior's own subgraph
//! travels with it. A reader that only has the protocol cannot tell a misnamed
//! pin from a correct one, so this module restates every referenced primitive:
//! the published LabOP ones with the parameters their library declares, and the
//! Lab-defined ones for procedures the published libraries do not name.

use super::sbol::{self, Kind, Object};
use super::triples::{Graph, Term};
use super::vocabulary as vocab;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Direction {
    In,
    Out,
}

/// How many values a parameter accepts. LabOP writes no upper bound at all for
/// an unbounded parameter rather than writing an explicit unlimited value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Cardinality {
    Required,
    Optional,
    Unbounded,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Parameter {
    pub(super) name: &'static str,
    pub(super) type_iri: &'static str,
    pub(super) direction: Direction,
    pub(super) cardinality: Cardinality,
}

impl Parameter {
    const fn input(name: &'static str, type_iri: &'static str) -> Self {
        Self {
            name,
            type_iri,
            direction: Direction::In,
            cardinality: Cardinality::Required,
        }
    }

    const fn optional(name: &'static str, type_iri: &'static str) -> Self {
        Self {
            name,
            type_iri,
            direction: Direction::In,
            cardinality: Cardinality::Optional,
        }
    }

    const fn unbounded(name: &'static str, type_iri: &'static str) -> Self {
        Self {
            name,
            type_iri,
            direction: Direction::In,
            cardinality: Cardinality::Unbounded,
        }
    }

    const fn output(name: &'static str, type_iri: &'static str) -> Self {
        Self {
            name,
            type_iri,
            direction: Direction::Out,
            cardinality: Cardinality::Required,
        }
    }
}

/// Where a behavior definition comes from, which decides its namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Origin {
    /// A primitive published in a LabOP library, referenced by its own IRI.
    Labop(&'static str),
    /// A primitive Lab defines because no published one names this procedure.
    Lab,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Primitive {
    pub(super) name: &'static str,
    pub(super) origin: Origin,
    pub(super) description: &'static str,
    pub(super) parameters: &'static [Parameter],
}

impl Primitive {
    pub(super) fn namespace(&self) -> String {
        match self.origin {
            Origin::Labop(library) => format!("{}/{library}", vocab::PRIMITIVE_BASE),
            Origin::Lab => vocab::LAB_PRIMITIVE_BASE.to_owned(),
        }
    }

    pub(super) fn iri(&self) -> String {
        format!("{}/{}", self.namespace(), self.name)
    }

    /// Writes this behavior and its parameter list into the document.
    pub(super) fn declare(&self, graph: &mut Graph) {
        let namespace = self.namespace();
        let mut behavior = sbol::top_level(
            graph,
            &namespace,
            self.name,
            vocab::LABOP_PRIMITIVE,
            Kind::TopLevel,
        );
        behavior.set_description(graph, self.description);
        for (index, parameter) in self.parameters.iter().enumerate() {
            let ordered = behavior.child(
                graph,
                "OrderedPropertyValue",
                &format!("{}OrderedPropertyValue", vocab::UML),
                Kind::Identified,
            );
            graph.push(
                ordered.iri(),
                vocab::UML_INDEX_VALUE,
                Term::integer(index as i64),
            );
            graph.link(behavior.iri(), vocab::UML_OWNED_PARAMETER, ordered.iri());
            declare_parameter(graph, ordered, *parameter);
        }
    }
}

fn declare_parameter(graph: &mut Graph, mut ordered: Object, parameter: Parameter) {
    let mut owned = ordered.child(
        graph,
        "Parameter",
        &format!("{}Parameter", vocab::UML),
        Kind::Identified,
    );
    graph.link(ordered.iri(), vocab::UML_PROPERTY_VALUE, owned.iri());
    owned.set_name(graph, parameter.name);
    graph.link(owned.iri(), vocab::UML_TYPE, parameter.type_iri);
    graph.link(
        owned.iri(),
        vocab::UML_DIRECTION,
        match parameter.direction {
            Direction::In => vocab::UML_DIRECTION_IN,
            Direction::Out => vocab::UML_DIRECTION_OUT,
        },
    );
    graph.push(owned.iri(), vocab::UML_IS_ORDERED, Term::boolean(true));
    graph.push(owned.iri(), vocab::UML_IS_UNIQUE, Term::boolean(true));

    // The upper bound is written first, so an unbounded parameter's lower bound
    // is still `LiteralInteger1` and a bounded one's is `LiteralInteger2`.
    if !matches!(parameter.cardinality, Cardinality::Unbounded) {
        let upper = literal_integer(graph, &mut owned, 1);
        graph.link(owned.iri(), vocab::UML_UPPER_VALUE, &upper);
    }
    let lower = literal_integer(
        graph,
        &mut owned,
        match parameter.cardinality {
            Cardinality::Optional => 0,
            _ => 1,
        },
    );
    graph.link(owned.iri(), vocab::UML_LOWER_VALUE, &lower);
}

fn literal_integer(graph: &mut Graph, parent: &mut Object, value: i64) -> String {
    let literal = parent.child(
        graph,
        "LiteralInteger",
        &format!("{}LiteralInteger", vocab::UML),
        Kind::Identified,
    );
    graph.push(
        literal.iri(),
        vocab::UML_INTEGER_VALUE,
        Term::integer(value),
    );
    literal.iri().to_owned()
}

const SAMPLE_ARRAY: &str = vocab::LABOP_SAMPLE_ARRAY;
const SAMPLE_COLLECTION: &str = vocab::LABOP_SAMPLE_COLLECTION;
const COMPONENT: &str = vocab::SBOL_COMPONENT;
const MEASURE: &str = vocab::OM_MEASURE;
const VALUE_SPEC: &str = vocab::UML_VALUE_SPECIFICATION;
const CONTAINER_SPEC: &str = vocab::LABOP_CONTAINER_SPEC;
const IDENTIFIED: &str = vocab::SBOL_IDENTIFIED;

pub(super) const EMPTY_CONTAINER: Primitive = Primitive {
    name: "EmptyContainer",
    origin: Origin::Labop("sample_arrays"),
    description: "Allocate a sample array with size and type based on an empty container",
    parameters: &[
        Parameter::input("specification", IDENTIFIED),
        Parameter::optional("sample_array", SAMPLE_ARRAY),
        Parameter::output("samples", SAMPLE_ARRAY),
    ],
};

pub(super) const PROVISION: Primitive = Primitive {
    name: "Provision",
    origin: Origin::Labop("liquid_handling"),
    description: "Place a measured amount (mass or volume) of a specified component into a location, where it may then be used in executing the protocol.",
    parameters: &[
        Parameter::input("resource", COMPONENT),
        Parameter::input("destination", SAMPLE_COLLECTION),
        Parameter::input("amount", MEASURE),
        Parameter::optional("dispenseVelocity", MEASURE),
    ],
};

pub(super) const TRANSFER: Primitive = Primitive {
    name: "Transfer",
    origin: Origin::Labop("liquid_handling"),
    description: "Move a measured volume taken from a collection of source samples to a location whose shape can contain them in a destination locations",
    parameters: &[
        Parameter::input("source", SAMPLE_COLLECTION),
        Parameter::input("destination", SAMPLE_COLLECTION),
        Parameter::optional("coordinates", VALUE_SPEC),
        Parameter::optional("replicates", VALUE_SPEC),
        Parameter::optional("temperature", MEASURE),
        Parameter::input("amount", MEASURE),
        Parameter::optional("dispenseVelocity", MEASURE),
    ],
};

pub(super) const SERIAL_DILUTION: Primitive = Primitive {
    name: "SerialDilution",
    origin: Origin::Labop("liquid_handling"),
    description: "Serial Dilution",
    parameters: &[
        Parameter::input("samples", SAMPLE_COLLECTION),
        Parameter::input("direction", VALUE_SPEC),
        Parameter::input("diluent", COMPONENT),
        Parameter::input("amount", MEASURE),
        Parameter::optional("dilution_factor", MEASURE),
    ],
};

pub(super) const PIPETTE_MIX: Primitive = Primitive {
    name: "PipetteMix",
    origin: Origin::Labop("liquid_handling"),
    description: "Mix by cycling a measured volume of liquid in and out at an array of samples a fixed number of times",
    parameters: &[
        Parameter::input("samples", SAMPLE_COLLECTION),
        Parameter::input("amount", MEASURE),
        Parameter::optional("dispenseVelocity", MEASURE),
        Parameter::optional("cycleCount", MEASURE),
    ],
};

pub(super) const SEAL: Primitive = Primitive {
    name: "Seal",
    origin: Origin::Labop("plate_handling"),
    description: "Seal a collection of samples fixing the seal using a user-selected method, in order to guarantee isolation from the external environment",
    parameters: &[
        Parameter::input("location", SAMPLE_ARRAY),
        Parameter::input("specification", CONTAINER_SPEC),
    ],
};

pub(super) const INCUBATE: Primitive = Primitive {
    name: "Incubate",
    origin: Origin::Labop("plate_handling"),
    description: "Incubate a set of samples under specified conditions for a fixed period of time",
    parameters: &[
        Parameter::input("location", SAMPLE_ARRAY),
        Parameter::input("duration", MEASURE),
        Parameter::input("temperature", MEASURE),
        Parameter::optional("shakingFrequency", MEASURE),
    ],
};

pub(super) const TRANSFORM: Primitive = Primitive {
    name: "Transform",
    origin: Origin::Labop("culturing"),
    description: "Transform competent cells.",
    parameters: &[
        Parameter::input("host", COMPONENT),
        Parameter::unbounded("dna", COMPONENT),
        Parameter::optional("amount", MEASURE),
        Parameter::input("selection_medium", COMPONENT),
        Parameter::input("destination", SAMPLE_ARRAY),
        Parameter::output("transformants", SAMPLE_ARRAY),
    ],
};

pub(super) const CULTURE_PLATES: Primitive = Primitive {
    name: "CulturePlates",
    origin: Origin::Labop("culturing"),
    description: "Create a new sample collection of culture plates using a particular media type",
    parameters: &[
        Parameter::input("quantity", VALUE_SPEC),
        Parameter::input("specification", CONTAINER_SPEC),
        Parameter::optional("replicates", VALUE_SPEC),
        Parameter::input("growth_medium", COMPONENT),
        Parameter::output("samples", SAMPLE_ARRAY),
    ],
};

/// Golden Gate assembly has no counterpart in the published libraries. The
/// thermal program is carried by the surrounding `Incubate` actions rather than
/// by this behavior, because a LabOP activity cannot express the cycling.
pub(super) const ASSEMBLE: Primitive = Primitive {
    name: "GoldenGateAssembly",
    origin: Origin::Lab,
    description: "Assemble a circular plasmid from a backbone and parts by Golden Gate cycling between digestion and ligation.",
    parameters: &[
        Parameter::input("reaction", SAMPLE_ARRAY),
        Parameter::input("backbone", COMPONENT),
        Parameter::unbounded("parts", COMPONENT),
        Parameter::input("restriction_enzyme", COMPONENT),
        Parameter::output("product", SAMPLE_ARRAY),
    ],
};

/// The judgement that a material meets its acceptance criteria. LabOP records
/// measurements but has no operation that decides whether they suffice, so the
/// criteria travel as parameters of a Lab-defined behavior.
pub(super) const ACCEPT: Primitive = Primitive {
    name: "AcceptArtifact",
    origin: Origin::Lab,
    description: "Accept a material as the named artifact when its measured concentration and volume meet the declared minima.",
    parameters: &[
        Parameter::input("samples", SAMPLE_COLLECTION),
        Parameter::input("artifact", VALUE_SPEC),
        Parameter::optional("minimum_concentration", MEASURE),
        Parameter::optional("minimum_volume", MEASURE),
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separates_the_primitive_host_from_the_ontology_host() {
        assert_eq!(
            PROVISION.iri(),
            "https://bioprotocols.org/labop/primitives/liquid_handling/Provision"
        );
        assert!(PROVISION.iri().starts_with("https://"));
        assert!(vocab::LABOP_PRIMITIVE.starts_with("http://"));
    }

    #[test]
    fn declares_lab_primitives_outside_the_labop_namespace() {
        assert!(ASSEMBLE.iri().starts_with(vocab::LAB_PRIMITIVE_BASE));
    }

    #[test]
    fn writes_bounds_matching_each_cardinality() {
        let mut graph = Graph::new();
        TRANSFORM.declare(&mut graph);
        let rendered = graph.render();
        // `dna` is unbounded, so it carries a lower bound and no upper bound.
        let dna_parameter = rendered
            .lines()
            .find(|line| line.contains("#name> \"dna\""))
            .expect("dna parameter present");
        let subject = dna_parameter
            .split_once("> <")
            .expect("statement has a subject")
            .0
            .trim_start_matches('<');
        assert!(
            !rendered.contains(&format!("<{subject}> <{}>", vocab::UML_UPPER_VALUE)),
            "an unbounded parameter carries no upper bound"
        );
        assert!(rendered.contains(&format!("<{subject}> <{}>", vocab::UML_LOWER_VALUE)));
    }

    #[test]
    fn indexes_parameters_in_declaration_order() {
        let mut graph = Graph::new();
        PROVISION.declare(&mut graph);
        let rendered = graph.render();
        assert!(rendered.contains(
            "/Provision/OrderedPropertyValue1> <http://bioprotocols.org/uml#indexValue> \"0\""
        ));
        assert!(rendered.contains(
            "/Provision/OrderedPropertyValue4> <http://bioprotocols.org/uml#indexValue> \"3\""
        ));
    }
}
