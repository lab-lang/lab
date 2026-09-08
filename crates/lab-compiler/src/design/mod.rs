//! Reusable, package-neutral design identities and their LAIR operations.

pub(crate) mod ir;
mod model;

#[cfg(test)]
pub(crate) use model::synthetic_artifact_design;
pub use model::{ARTIFACT_DESIGN_SCHEMA_VERSION, ArtifactDesign};
