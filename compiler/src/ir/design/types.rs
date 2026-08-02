use pliron::derive::pliron_type;

/// A declarative biological artifact design. Design values are freely reusable.
#[pliron_type(
    name = "design.artifact",
    format,
    generate_get = true,
    verifier = "succ"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DesignType;
