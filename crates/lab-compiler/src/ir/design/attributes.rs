use pliron::derive::pliron_attr;

/// The topology requested by a biological design.
#[pliron_attr(name = "design.topology", format, verifier = "succ")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TopologyAttr {
    Circular,
    Linear,
}
