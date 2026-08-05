//! Generated compiler artifacts independent of persistence or transport.
//!
//! Backends and renderers return bundles. Applications such as `labc` decide
//! whether those artifacts are written to disk, uploaded, or inspected in
//! memory.

mod bundle;

pub use bundle::{ArtifactBundle, ArtifactError, GeneratedArtifact};
