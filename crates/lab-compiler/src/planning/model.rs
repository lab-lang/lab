use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// Materials and already-realized artifacts available before planning begins.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildInventory {
    #[serde(default)]
    pub available_materials: BTreeSet<String>,
    #[serde(default)]
    pub available_artifacts: BTreeSet<String>,
}

impl BuildInventory {
    /// This inventory narrowed to what the graph can consume. A package
    /// manifest declares exactly the stock its build uses, and resolution
    /// rejects any surplus; a facility stocks a whole lab, so a build
    /// drawing on one narrows the stock to the graph's demands first and
    /// keeps that rejection meaningful for manifests.
    pub fn restricted_to(&self, graph: &BuildGraph) -> BuildInventory {
        let required: BTreeSet<&String> = graph
            .nodes
            .values()
            .flat_map(|node| node.required_materials.iter())
            .collect();
        let produced: BTreeSet<&String> = graph.nodes.keys().collect();
        BuildInventory {
            available_materials: self
                .available_materials
                .iter()
                .filter(|name| required.contains(name))
                .cloned()
                .collect(),
            available_artifacts: self
                .available_artifacts
                .iter()
                .filter(|name| produced.contains(name))
                .cloned()
                .collect(),
        }
    }
}

/// A target-neutral artifact dependency graph.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildGraph {
    pub nodes: BTreeMap<String, BuildGraphNode>,
}

/// Planning facts supplied by a frontend or backend specialization.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildGraphNode {
    pub dependencies: BTreeSet<String>,
    pub steps: Vec<String>,
    pub required_materials: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyBuildStatus {
    Complete,
    Partial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactResolution {
    Existing,
    Generated,
    Blocked,
    Cyclic,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DependencyNode {
    pub artifact: String,
    pub dependencies: Vec<String>,
    pub steps: Vec<String>,
    pub inventory_materials: Vec<String>,
    pub resolution: ArtifactResolution,
    pub generated_in_iteration: Option<usize>,
    pub missing_dependencies: Vec<String>,
    pub missing_materials: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DependencyEdge {
    pub artifact: String,
    pub depends_on: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BuildAttempt {
    pub iteration: usize,
    pub artifact: String,
    pub outcome: ArtifactResolution,
    pub missing_dependencies: Vec<String>,
    pub missing_materials: Vec<String>,
}

/// Serializable dependency-resolution result. Rendering and package emission
/// remain the responsibility of consumers such as robot backends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DependencyBuildManifest {
    pub schema_version: String,
    pub status: DependencyBuildStatus,
    pub roots: Vec<String>,
    pub nodes: Vec<DependencyNode>,
    pub edges: Vec<DependencyEdge>,
    pub attempts: Vec<BuildAttempt>,
    pub generated_artifacts: Vec<String>,
    pub existing_artifacts: Vec<String>,
}
