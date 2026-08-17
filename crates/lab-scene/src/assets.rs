//! The asset registry: how a scene node gets a real mesh.
//!
//! Resolution is a three-tier fallback that never blocks a render: a file
//! in the facility's assets directory wins, the procedural primitive is
//! next, and a labeled dimensioned box is the floor every consumer can
//! draw. Keys are the identities the scene already speaks: station kind
//! strings (`hamilton.star`), labware catalog ids (`pcr_plate_96`),
//! carrier catalog ids, and `room`.
//!
//! Assets are authored in millimeters (`metersPerUnit = 0.001` in USD
//! layers), origin at the node anchor, +X right, +Y back, Z up. See
//! `docs/integrations/photoreal-assets.md` for the preparation recipe.

use std::path::{Path, PathBuf};

use crate::scene::{Geometry, Scene, SceneNode};

/// Extensions each consumer family can load, in preference order.
const GLTF_EXTENSIONS: [&str; 2] = ["glb", "gltf"];
const USD_EXTENSIONS: [&str; 3] = ["usd", "usdc", "usda"];

/// A facility's assets directory, queried by key.
pub struct AssetCatalog {
    directory: PathBuf,
}

impl AssetCatalog {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    fn find(&self, key: &str, extensions: &[&str]) -> Option<String> {
        extensions.iter().find_map(|extension| {
            let path = self.directory.join(format!("{key}.{extension}"));
            path.is_file().then(|| path.display().to_string())
        })
    }

    /// The geometry for a key: a mesh when any asset file exists, the
    /// fallback box otherwise. Recorded paths point at the source files;
    /// [`bundle_assets`] rewrites them relative to a scene bundle.
    pub fn resolve(&self, key: &str, fallback: [f64; 3]) -> Geometry {
        let gltf = self.find(key, &GLTF_EXTENSIONS);
        let usd = self.find(key, &USD_EXTENSIONS);
        if gltf.is_some() || usd.is_some() {
            Geometry::Mesh {
                gltf,
                usd,
                fallback,
            }
        } else {
            Geometry::Box {
                x: fallback[0],
                y: fallback[1],
                z: fallback[2],
            }
        }
    }
}

/// Copies every referenced asset into `<out_dir>/assets/` and rewrites the
/// scene's paths to be relative to the scene file, so a scene bundle is
/// self-contained for the web server and for USD tools alike. Returns the
/// copied file names.
pub fn bundle_assets(scene: &mut Scene, out_dir: &Path) -> std::io::Result<Vec<String>> {
    let assets_dir = out_dir.join("assets");
    let mut copied = Vec::new();
    bundle_node(&mut scene.root, &assets_dir, &mut copied)?;
    Ok(copied)
}

fn bundle_node(
    node: &mut SceneNode,
    assets_dir: &Path,
    copied: &mut Vec<String>,
) -> std::io::Result<()> {
    if let Some(Geometry::Mesh { gltf, usd, .. }) = &mut node.geometry {
        for slot in [gltf, usd] {
            if let Some(source) = slot.as_ref() {
                let source_path = PathBuf::from(source);
                let Some(file_name) = source_path.file_name().map(|name| name.to_owned()) else {
                    continue;
                };
                std::fs::create_dir_all(assets_dir)?;
                std::fs::copy(&source_path, assets_dir.join(&file_name))?;
                let file_name = file_name.to_string_lossy().into_owned();
                *slot = Some(format!("assets/{file_name}"));
                if !copied.contains(&file_name) {
                    copied.push(file_name);
                }
            }
        }
    }
    for child in &mut node.children {
        bundle_node(child, assets_dir, copied)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{SCENE_FORMAT, Semantic};

    #[test]
    fn resolution_prefers_assets_and_falls_back_to_the_box() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("inheco.odtc.glb"), b"stub").unwrap();
        std::fs::write(directory.path().join("inheco.odtc.usda"), b"#usda 1.0").unwrap();
        let catalog = AssetCatalog::new(directory.path());

        let resolved = catalog.resolve("inheco.odtc", [200.0, 320.0, 260.0]);
        let Geometry::Mesh {
            gltf,
            usd,
            fallback,
        } = resolved
        else {
            panic!("an existing asset resolves to a mesh");
        };
        assert!(gltf.unwrap().ends_with("inheco.odtc.glb"));
        assert!(usd.unwrap().ends_with("inheco.odtc.usda"));
        assert_eq!(fallback, [200.0, 320.0, 260.0]);

        assert_eq!(
            catalog.resolve("hamilton.star", [1.0, 2.0, 3.0]),
            Geometry::Box {
                x: 1.0,
                y: 2.0,
                z: 3.0
            },
            "a missing asset is the dimensioned box, never an error"
        );
    }

    #[test]
    fn bundling_copies_assets_beside_the_scene_and_relativizes_paths() {
        let source = tempfile::tempdir().unwrap();
        std::fs::write(source.path().join("room.glb"), b"stub").unwrap();
        let catalog = AssetCatalog::new(source.path());

        let mut scene = Scene {
            format: SCENE_FORMAT.to_string(),
            name: "test".to_string(),
            root: SceneNode::new("room", Semantic::Room, [0.0, 0.0, 0.0])
                .with_geometry(catalog.resolve("room", [1.0, 1.0, 1.0])),
        };
        let out = tempfile::tempdir().unwrap();
        let copied = bundle_assets(&mut scene, out.path()).unwrap();
        assert_eq!(copied, ["room.glb"]);
        assert!(out.path().join("assets/room.glb").is_file());
        let Some(Geometry::Mesh { gltf, .. }) = &scene.root.geometry else {
            panic!("the mesh survives bundling");
        };
        assert_eq!(gltf.as_deref(), Some("assets/room.glb"));
    }
}
