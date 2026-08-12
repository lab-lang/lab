//! Scene graphs for Lab benches and workcells: the geometry the compiler
//! plans against, arranged as a tree a renderer can draw.
//!
//! The scene is the semantic source of truth (`lab.scene.v0`); glTF and
//! USD files are derived projections of it. A viewer binds simulation
//! trace events to scene nodes by id — labware nodes carry the same
//! resource names the plans and traces use — and computes nothing itself.
//!
//! Geometry comes in two grades and the scene is honest about which is
//! which: positions are exact, taken from the same catalog the planner
//! uses; extents (footprints, heights) are nominal visualization
//! dimensions from a side table, because the planning catalog carries
//! anchor points, not bounding boxes.

pub mod animate;
pub mod assets;
pub mod dims;
pub mod gltf;
pub(crate) mod instruments;
pub mod scene;
pub mod star;
pub mod usda;
pub mod workcell;

pub use scene::{Geometry, Scene, SceneError, SceneNode, Semantic};
