use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// The inventory evidence available to dependency planning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildInventory {
    /// Exact SBOL Component-to-MaterialLot candidates from one validated facility snapshot.
    MaterialLots(MaterialLotBuildInventory),
    /// Temporary compatibility for packages that still carry symbolic manifest arrays.
    LegacySymbols(LegacyBuildInventory),
}

impl Default for BuildInventory {
    fn default() -> Self {
        Self::LegacySymbols(LegacyBuildInventory::default())
    }
}

impl BuildInventory {
    pub fn legacy(
        available_materials: impl IntoIterator<Item = String>,
        available_artifacts: impl IntoIterator<Item = String>,
    ) -> Self {
        Self::LegacySymbols(LegacyBuildInventory {
            available_materials: available_materials.into_iter().collect(),
            available_artifacts: available_artifacts.into_iter().collect(),
        })
    }

    pub fn as_legacy_mut(&mut self) -> Option<&mut LegacyBuildInventory> {
        match self {
            Self::LegacySymbols(inventory) => Some(inventory),
            Self::MaterialLots(_) => None,
        }
    }
}

/// Symbolic inventory accepted only while existing package manifests migrate.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyBuildInventory {
    #[serde(default)]
    pub available_materials: BTreeSet<String>,
    #[serde(default)]
    pub available_artifacts: BTreeSet<String>,
}

/// Exact candidate lots for the checked declarations in one program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialLotBuildInventory {
    pub(crate) source_sha256: String,
    pub(crate) facility: String,
    pub(crate) materials: BTreeMap<String, MaterialLotCandidates>,
    pub(crate) artifacts: BTreeMap<String, MaterialLotCandidates>,
}

impl MaterialLotBuildInventory {
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }

    pub fn facility(&self) -> &str {
        &self.facility
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialLotCandidates {
    Unidentified,
    Identified {
        component: String,
        material_lots: Vec<String>,
    },
}

/// A frozen exact binding from a workflow symbol through its Component to one physical lot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MaterialLotBinding {
    pub symbol: String,
    pub component: String,
    pub material_lot: String,
}

/// A facility-independent artifact dependency graph.
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
    pub material_lot_bindings: Vec<MaterialLotBinding>,
    pub existing_material_lot: Option<MaterialLotBinding>,
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
    pub inventory: DependencyInventorySource,
    pub status: DependencyBuildStatus,
    pub roots: Vec<String>,
    pub nodes: Vec<DependencyNode>,
    pub edges: Vec<DependencyEdge>,
    pub attempts: Vec<BuildAttempt>,
    pub generated_artifacts: Vec<String>,
    pub existing_artifacts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DependencyInventorySource {
    SbolInventory {
        source_sha256: String,
        facility: String,
    },
    LegacySymbols,
}
