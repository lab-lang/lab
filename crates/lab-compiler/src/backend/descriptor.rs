use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendDescriptor {
    pub id: String,
    pub display_name: String,
    pub manufacturer: Option<String>,
    pub targets: Vec<BackendTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendTarget {
    pub id: String,
    pub display_name: String,
    /// Stable capability identifiers used for discovery and preflight. The
    /// backend remains responsible for detailed constraint validation.
    pub capabilities: BTreeSet<String>,
}
