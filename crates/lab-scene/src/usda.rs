//! USD export: a plain-text `.usda` layer for Isaac Sim, Omniverse, and
//! Unreal. Same scene, same millimeters; USD carries the unit and up-axis
//! declaration itself, so no root transform is needed.

use std::fmt::Write;

use crate::scene::{Geometry, Scene, SceneNode};

/// Renders the scene as a `.usda` text layer: one `Xform` per node, with
/// `Cube`/`Cylinder` geometry prims scaled to the stated extents.
pub fn render_usda(scene: &Scene) -> String {
    let mut text = String::new();
    let _ = writeln!(
        text,
        "#usda 1.0\n(\n    defaultPrim = \"{}\"\n    metersPerUnit = 0.001\n    upAxis = \"Z\"\n)\n",
        prim_name(&scene.root.id)
    );
    emit_node(&scene.root, 0, &mut text);
    text
}

/// USD prim names are identifiers; scene ids carry `/`, `:`, `#`.
fn prim_name(id: &str) -> String {
    let mut name: String = id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    if name
        .chars()
        .next()
        .is_none_or(|first| first.is_ascii_digit())
    {
        name.insert(0, '_');
    }
    name
}

fn indent(text: &mut String, depth: usize) {
    for _ in 0..depth {
        text.push_str("    ");
    }
}

/// A USD Cube spans [-size/2, size/2]; scale a unit cube and seat its
/// minimum corner on the node origin.
fn emit_fallback_box(extent: &[f64; 3], depth: usize, text: &mut String) {
    let (x, y, z) = (extent[0], extent[1], extent[2]);
    indent(text, depth + 1);
    let _ = writeln!(text, "def Cube \"geometry\"");
    indent(text, depth + 1);
    let _ = writeln!(text, "{{");
    indent(text, depth + 2);
    let _ = writeln!(text, "double size = 1");
    indent(text, depth + 2);
    let _ = writeln!(
        text,
        "double3 xformOp:translate = ({}, {}, {})",
        x / 2.0,
        y / 2.0,
        z / 2.0
    );
    indent(text, depth + 2);
    let _ = writeln!(text, "float3 xformOp:scale = ({x}, {y}, {z})");
    indent(text, depth + 2);
    let _ = writeln!(
        text,
        "uniform token[] xformOpOrder = [\"xformOp:translate\", \"xformOp:scale\"]"
    );
    indent(text, depth + 1);
    let _ = writeln!(text, "}}");
}

fn emit_node(node: &SceneNode, depth: usize, text: &mut String) {
    indent(text, depth);
    let _ = writeln!(text, "def Xform \"{}\" (", prim_name(&node.id));
    indent(text, depth);
    let _ = writeln!(
        text,
        "    customData = {{ string labId = \"{}\" }}",
        node.id
    );
    indent(text, depth);
    let _ = writeln!(text, ")");
    indent(text, depth);
    let _ = writeln!(text, "{{");
    indent(text, depth + 1);
    let _ = writeln!(
        text,
        "double3 xformOp:translate = ({}, {}, {})",
        node.translation[0], node.translation[1], node.translation[2]
    );
    if node.rotation_z_deg != 0.0 {
        indent(text, depth + 1);
        let _ = writeln!(text, "double xformOp:rotateZ = {}", node.rotation_z_deg);
        indent(text, depth + 1);
        let _ = writeln!(
            text,
            "uniform token[] xformOpOrder = [\"xformOp:translate\", \"xformOp:rotateZ\"]"
        );
    } else {
        indent(text, depth + 1);
        let _ = writeln!(
            text,
            "uniform token[] xformOpOrder = [\"xformOp:translate\"]"
        );
    }

    if let Some(geometry) = &node.geometry {
        match geometry {
            Geometry::Box { x, y, z } => {
                emit_fallback_box(&[*x, *y, *z], depth, text);
            }
            Geometry::Cylinder { diameter, height } => {
                indent(text, depth + 1);
                let _ = writeln!(text, "def Cylinder \"geometry\"");
                indent(text, depth + 1);
                let _ = writeln!(text, "{{");
                indent(text, depth + 2);
                let _ = writeln!(text, "uniform token axis = \"Z\"");
                indent(text, depth + 2);
                let _ = writeln!(text, "double height = {height}");
                indent(text, depth + 2);
                let _ = writeln!(text, "double radius = {}", diameter / 2.0);
                indent(text, depth + 2);
                let _ = writeln!(text, "double3 xformOp:translate = (0, 0, {})", height / 2.0);
                indent(text, depth + 2);
                let _ = writeln!(
                    text,
                    "uniform token[] xformOpOrder = [\"xformOp:translate\"]"
                );
                indent(text, depth + 1);
                let _ = writeln!(text, "}}");
            }
            Geometry::Mesh { usd, fallback, .. } => match usd {
                // The asset layer carries its own geometry and materials;
                // referencing it composes it under this prim.
                Some(path) => {
                    indent(text, depth + 1);
                    let _ = writeln!(text, "def Xform \"geometry\" (");
                    indent(text, depth + 1);
                    let _ = writeln!(text, "    prepend references = @{path}@");
                    indent(text, depth + 1);
                    let _ = writeln!(text, ")");
                    indent(text, depth + 1);
                    let _ = writeln!(text, "{{");
                    indent(text, depth + 1);
                    let _ = writeln!(text, "}}");
                }
                // No USD flavor of this asset: the fallback box, exactly
                // as an un-assetted node renders.
                None => {
                    emit_fallback_box(fallback, depth, text);
                }
            },
        }
    }

    for child in &node.children {
        emit_node(child, depth + 1, text);
    }
    indent(text, depth);
    let _ = writeln!(text, "}}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{SCENE_FORMAT, Semantic};

    #[test]
    fn the_layer_declares_units_and_sanitizes_prim_names() {
        let scene = Scene {
            format: SCENE_FORMAT.to_string(),
            name: "test".to_string(),
            root: SceneNode {
                id: "room".to_string(),
                semantic: Semantic::Room,
                translation: [0.0, 0.0, 0.0],
                rotation_z_deg: 0.0,
                geometry: None,
                children: vec![
                    SceneNode::new(
                        "dna_plate/1:A1",
                        Semantic::Well {
                            name: "A1".to_string(),
                        },
                        [1.0, 2.0, 3.0],
                    )
                    .with_geometry(Geometry::Cylinder {
                        diameter: 6.0,
                        height: 10.0,
                    }),
                ],
            },
        };
        let text = render_usda(&scene);
        assert!(text.starts_with("#usda 1.0"));
        assert!(text.contains("metersPerUnit = 0.001"));
        assert!(text.contains("upAxis = \"Z\""));
        assert!(
            text.contains("def Xform \"dna_plate_1_A1\""),
            "ids sanitize to identifiers:\n{text}"
        );
        assert!(
            text.contains("string labId = \"dna_plate/1:A1\""),
            "the original id survives as customData"
        );
    }
}
