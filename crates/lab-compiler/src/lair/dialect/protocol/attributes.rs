use pliron::derive::pliron_attr;

/// A DNA assembly strategy selected for the target laboratory.
#[pliron_attr(name = "protocol.assembly_method", format, verifier = "succ")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AssemblyMethodAttr {
    Gibson,
    GoldenGate,
}
