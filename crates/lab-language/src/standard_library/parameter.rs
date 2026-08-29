//! Absolute property-kind IRIs used by bundled durable action parameters.
//!
//! SBOLInventory property kinds are an open vocabulary. Lab uses explicit terms in the same
//! capability namespace as its operation vocabulary so requirements and facility offerings can
//! join without comparing source argument names.

pub(crate) const TEMPERATURE: &str = "https://sbol.io/ns/capability#Temperature";
pub(crate) const DURATION: &str = "https://sbol.io/ns/capability#Duration";
pub(crate) const COUNT: &str = "https://sbol.io/ns/capability#Count";
