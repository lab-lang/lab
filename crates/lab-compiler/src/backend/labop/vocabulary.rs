//! The IRIs a LabOP document is built from.
//!
//! LabOP splits its identifiers across two hosts that differ by one character:
//! ontology classes live under `http://bioprotocols.org/labop#` while the
//! primitive library lives under `https://bioprotocols.org/labop/primitives/`.
//! Confusing the two produces a document that parses and silently references
//! nothing, so every identifier this backend writes is named here rather than
//! spelled inline.

pub(super) const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

pub(super) const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
pub(super) const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
pub(super) const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";

pub(super) const SBOL_TOP_LEVEL: &str = "http://sbols.org/v3#TopLevel";
pub(super) const SBOL_IDENTIFIED: &str = "http://sbols.org/v3#Identified";
pub(super) const SBOL_COMPONENT: &str = "http://sbols.org/v3#Component";
pub(super) const SBOL_DISPLAY_ID: &str = "http://sbols.org/v3#displayId";
pub(super) const SBOL_NAMESPACE: &str = "http://sbols.org/v3#hasNamespace";
pub(super) const SBOL_NAME: &str = "http://sbols.org/v3#name";
pub(super) const SBOL_DESCRIPTION: &str = "http://sbols.org/v3#description";
pub(super) const SBOL_TYPE: &str = "http://sbols.org/v3#type";

pub(super) const LABOP_PROTOCOL: &str = "http://bioprotocols.org/labop#Protocol";
pub(super) const LABOP_PRIMITIVE: &str = "http://bioprotocols.org/labop#Primitive";
pub(super) const LABOP_CONTAINER_SPEC: &str = "http://bioprotocols.org/labop#ContainerSpec";
pub(super) const LABOP_SAMPLE_ARRAY: &str = "http://bioprotocols.org/labop#SampleArray";
pub(super) const LABOP_SAMPLE_COLLECTION: &str = "http://bioprotocols.org/labop#SampleCollection";
pub(super) const LABOP_QUERY_STRING: &str = "http://bioprotocols.org/labop#queryString";
pub(super) const LABOP_PREFIX_MAP: &str = "http://bioprotocols.org/labop#prefixMap";

/// UML classes an emitted activity is built from. A class name reaches the
/// document as an `rdf:type`, where a misspelling produces a document that
/// parses and references a class no reader knows, so each is named once here
/// rather than spelled at the point of use.
pub(super) const UML_INITIAL_NODE: &str = "http://bioprotocols.org/uml#InitialNode";
pub(super) const UML_FINAL_NODE: &str = "http://bioprotocols.org/uml#FinalNode";
pub(super) const UML_FORK_NODE: &str = "http://bioprotocols.org/uml#ForkNode";
pub(super) const UML_CALL_BEHAVIOR_ACTION: &str = "http://bioprotocols.org/uml#CallBehaviorAction";
pub(super) const UML_INPUT_PIN: &str = "http://bioprotocols.org/uml#InputPin";
pub(super) const UML_OUTPUT_PIN: &str = "http://bioprotocols.org/uml#OutputPin";
pub(super) const UML_VALUE_PIN: &str = "http://bioprotocols.org/uml#ValuePin";
pub(super) const UML_OBJECT_FLOW: &str = "http://bioprotocols.org/uml#ObjectFlow";
pub(super) const UML_CONTROL_FLOW: &str = "http://bioprotocols.org/uml#ControlFlow";
pub(super) const UML_PARAMETER: &str = "http://bioprotocols.org/uml#Parameter";
pub(super) const UML_ORDERED_PROPERTY_VALUE: &str =
    "http://bioprotocols.org/uml#OrderedPropertyValue";
pub(super) const UML_LITERAL_INTEGER: &str = "http://bioprotocols.org/uml#LiteralInteger";
pub(super) const UML_LITERAL_STRING: &str = "http://bioprotocols.org/uml#LiteralString";
pub(super) const UML_LITERAL_REFERENCE: &str = "http://bioprotocols.org/uml#LiteralReference";
pub(super) const UML_LITERAL_IDENTIFIED: &str = "http://bioprotocols.org/uml#LiteralIdentified";

pub(super) const UML_ACTIVITY_NODE: &str = "http://bioprotocols.org/uml#node";
pub(super) const UML_ACTIVITY_EDGE: &str = "http://bioprotocols.org/uml#edge";
pub(super) const UML_BEHAVIOR: &str = "http://bioprotocols.org/uml#behavior";
pub(super) const UML_INPUT: &str = "http://bioprotocols.org/uml#input";
pub(super) const UML_OUTPUT: &str = "http://bioprotocols.org/uml#output";
pub(super) const UML_SOURCE: &str = "http://bioprotocols.org/uml#source";
pub(super) const UML_TARGET: &str = "http://bioprotocols.org/uml#target";
pub(super) const UML_VALUE: &str = "http://bioprotocols.org/uml#value";
pub(super) const UML_IS_ORDERED: &str = "http://bioprotocols.org/uml#isOrdered";
pub(super) const UML_IS_UNIQUE: &str = "http://bioprotocols.org/uml#isUnique";
pub(super) const UML_DIRECTION: &str = "http://bioprotocols.org/uml#direction";
pub(super) const UML_DIRECTION_IN: &str = "http://bioprotocols.org/uml#in";
pub(super) const UML_DIRECTION_OUT: &str = "http://bioprotocols.org/uml#out";
pub(super) const UML_TYPE: &str = "http://bioprotocols.org/uml#type";
pub(super) const UML_LOWER_VALUE: &str = "http://bioprotocols.org/uml#lowerValue";
pub(super) const UML_UPPER_VALUE: &str = "http://bioprotocols.org/uml#upperValue";
pub(super) const UML_OWNED_PARAMETER: &str = "http://bioprotocols.org/uml#ownedParameter";
pub(super) const UML_INDEX_VALUE: &str = "http://bioprotocols.org/uml#indexValue";
pub(super) const UML_PROPERTY_VALUE: &str = "http://bioprotocols.org/uml#propertyValue";
pub(super) const UML_INTEGER_VALUE: &str = "http://bioprotocols.org/uml#integerValue";
pub(super) const UML_STRING_VALUE: &str = "http://bioprotocols.org/uml#stringValue";
pub(super) const UML_IDENTIFIED_VALUE: &str = "http://bioprotocols.org/uml#identifiedValue";
pub(super) const UML_REFERENCE_VALUE: &str = "http://bioprotocols.org/uml#referenceValue";
pub(super) const UML_VALUE_SPECIFICATION: &str = "http://bioprotocols.org/uml#ValueSpecification";

pub(super) const OM_MEASURE: &str =
    "http://www.ontology-of-units-of-measure.org/resource/om-2/Measure";
pub(super) const OM_NUMERICAL_VALUE: &str =
    "http://www.ontology-of-units-of-measure.org/resource/om-2/hasNumericalValue";
pub(super) const OM_UNIT: &str =
    "http://www.ontology-of-units-of-measure.org/resource/om-2/hasUnit";

/// Units of measure this backend emits, each an OM-2 resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Unit {
    Microlitre,
    Celsius,
    Minute,
}

impl Unit {
    pub(super) fn iri(self) -> &'static str {
        match self {
            Self::Microlitre => {
                "http://www.ontology-of-units-of-measure.org/resource/om-2/microlitre"
            }
            Self::Celsius => {
                "http://www.ontology-of-units-of-measure.org/resource/om-2/degreeCelsius"
            }
            Self::Minute => "http://www.ontology-of-units-of-measure.org/resource/om-2/minute-Time",
        }
    }
}

/// The base of the published primitive libraries. Note the `https` scheme and
/// the absence of a fragment separator, both of which differ from the
/// ontology host the `LABOP_*` classes are named under.
pub(super) const PRIMITIVE_BASE: &str = "https://bioprotocols.org/labop/primitives";

/// Namespace for the primitives Lab defines because LabOP's library has no
/// counterpart for them. `labop:Primitive` is a `sbol:TopLevel`, so a document
/// may carry its own behavior definitions alongside the published ones.
pub(super) const LAB_PRIMITIVE_BASE: &str = "https://lab-lang.org/labop/primitives";

/// Namespace for the protocols and resources a Lab build emits.
pub(super) const LAB_NAMESPACE: &str = "https://lab-lang.org/labop";
