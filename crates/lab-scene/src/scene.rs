//! `lab.scene.v0`: the neutral scene graph document.
//!
//! Coordinates are millimeters in the lab frame: X right along the deck,
//! Y toward the back, Z up. A node's translation is relative to its
//! parent. A `Box` sits with its minimum corner at the node origin; a
//! `Cylinder` stands on the node origin, centered on it.

use serde::{Deserialize, Serialize};

/// The format string every `lab.scene.v0` document declares.
pub const SCENE_FORMAT: &str = "lab.scene.v0";

#[derive(Debug, thiserror::Error)]
pub enum SceneError {
    #[error("the profile places carrier '{name}' as '{catalog}', which the catalog does not have")]
    UnknownCarrier { name: String, catalog: String },
    #[error("the profile loads labware '{labware}', which the catalog does not have")]
    UnknownLabware { labware: String },
    #[error("'{address}' is not a <carrier>/<site> address")]
    BadSiteAddress { address: String },
    #[error("{context}: {message}")]
    Resolve { context: String, message: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    /// Always [`SCENE_FORMAT`]; readers reject any other value.
    pub format: String,
    /// What was rendered: a target name, facility name, or wave.
    pub name: String,
    pub root: SceneNode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneNode {
    /// Stable identity a trace event can bind to. Labware nodes use the
    /// plan's resource names (`reaction_plate`, `dna_plate/1`).
    pub id: String,
    pub semantic: Semantic,
    /// Millimeters, relative to the parent node.
    pub translation: [f64; 3],
    /// Degrees counterclockwise about the node's Z axis, applied after
    /// translation. Zero for everything except placed stations.
    #[serde(default, skip_serializing_if = "rotation_is_zero")]
    pub rotation_z_deg: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Geometry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SceneNode>,
}

fn rotation_is_zero(rotation: &f64) -> bool {
    *rotation == 0.0
}

impl SceneNode {
    pub fn new(id: impl Into<String>, semantic: Semantic, translation: [f64; 3]) -> Self {
        Self {
            id: id.into(),
            semantic,
            translation,
            rotation_z_deg: 0.0,
            geometry: None,
            children: Vec::new(),
        }
    }

    pub fn with_geometry(mut self, geometry: Geometry) -> Self {
        self.geometry = Some(geometry);
        self
    }

    /// Depth-first traversal over the node and everything under it.
    ///
    /// Origins accumulate translations only: a node under a rotated
    /// ancestor reports where it would sit unrotated. Renderers compose
    /// full transforms natively; use this for counting and for scenes
    /// whose rotated nodes have no children that need exact origins.
    pub fn walk(&self, visit: &mut dyn FnMut(&SceneNode, [f64; 3])) {
        fn inner(node: &SceneNode, origin: [f64; 3], visit: &mut dyn FnMut(&SceneNode, [f64; 3])) {
            let here = [
                origin[0] + node.translation[0],
                origin[1] + node.translation[1],
                origin[2] + node.translation[2],
            ];
            visit(node, here);
            for child in &node.children {
                inner(child, here, visit);
            }
        }
        inner(self, [0.0, 0.0, 0.0], visit);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Semantic {
    /// The room a workcell's stations stand in.
    Room,
    /// One instrument, by its station kind string.
    Station { station_kind: String },
    /// A liquid handler's deck plate.
    Deck,
    /// A carrier seated on rails, by catalog id.
    Carrier { catalog: String },
    /// One seat on a carrier, 0-based.
    Site { index: usize },
    /// A piece of labware, by catalog id.
    Labware { catalog: String },
    /// One well or tip position within its labware.
    Well { name: String },
    /// A cosmetic piece of an instrument or bench, carrying its material
    /// name: `frame`, `panel`, `glass`, or `accent`. Parts never bind to
    /// trace events.
    Part { material: String },
}

/// Extents in millimeters. Positions in the scene are exact; extents are
/// nominal (see `dims`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "kebab-case")]
pub enum Geometry {
    /// Minimum corner at the node origin.
    Box { x: f64, y: f64, z: f64 },
    /// Standing on the node origin, centered on it.
    Cylinder { diameter: f64, height: f64 },
    /// A real asset, resolved from the facility's assets directory. Paths
    /// are relative to the scene file once bundled. The extents remain
    /// the honest fallback for any consumer that cannot load the mesh.
    Mesh {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gltf: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usd: Option<String>,
        fallback: [f64; 3],
    },
}

#[cfg(test)]
mod tests {
    use super::{SceneNode, Semantic};

    fn traversal_tree() -> SceneNode {
        let grandchild = SceneNode::new(
            "grandchild",
            Semantic::Well { name: "A1".into() },
            [4.0, 5.0, 6.0],
        );

        let mut child = SceneNode::new(
            "child",
            Semantic::Station {
                station_kind: "rotated-station".into(),
            },
            [1.0, 2.0, 3.0],
        );
        child.rotation_z_deg = 90.0;
        child.children.push(grandchild);

        let sibling = SceneNode::new("sibling", Semantic::Deck, [-1.0, -2.0, -3.0]);
        let mut root = SceneNode::new("root", Semantic::Room, [10.0, 20.0, 30.0]);
        root.children = vec![child, sibling];
        root
    }

    #[test]
    fn walk_visits_each_node_once_in_depth_first_order() {
        let root = traversal_tree();
        let mut visited = Vec::new();

        root.walk(&mut |node, _| visited.push(node.id.clone()));

        assert_eq!(visited, ["root", "child", "grandchild", "sibling"]);
    }

    #[test]
    fn walk_accumulates_translations_without_applying_rotation() {
        let root = traversal_tree();
        let mut visited = Vec::new();

        root.walk(&mut |node, origin| visited.push((node.id.clone(), origin)));

        assert_eq!(
            visited,
            [
                ("root".into(), [10.0, 20.0, 30.0]),
                ("child".into(), [11.0, 22.0, 33.0]),
                ("grandchild".into(), [15.0, 27.0, 39.0]),
                ("sibling".into(), [9.0, 18.0, 27.0]),
            ]
        );
    }
}
