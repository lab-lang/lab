//! LAIR Protocol dialect for target-selected biological procedures and evidence.

mod attributes;
mod operations;
mod types;

pub(crate) use attributes::AssemblyMethodAttr;
#[cfg(test)]
pub use operations::{AssembleOp, SynthesizeOp};
pub use types::{EvidenceType, MaterialType};
