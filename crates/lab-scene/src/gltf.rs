//! glTF 2.0 export: a derived projection of the scene for any standard
//! 3D viewer. Two unit meshes (a box and a cylinder) are instanced with
//! per-node scale, so the file stays small no matter how many wells a
//! deck has.
//!
//! The lab frame is Z-up millimeters; glTF is Y-up meters. The root node
//! carries the rotation and scale that map one to the other, so every
//! other transform in the file is the scene's own.

use base64::Engine as _;
use serde_json::{Value, json};

use crate::scene::{Geometry, Scene, SceneNode, Semantic};

/// Renders the scene as a self-contained `.gltf` JSON document with its
/// buffer embedded as a data URI.
pub fn render_gltf(scene: &Scene) -> String {
    let buffer = MeshBuffer::build();
    let mut nodes: Vec<Value> = Vec::new();

    // Root: Z-up mm -> Y-up m. A -90 degree rotation about X, then a
    // uniform 0.001 scale.
    let root_index = 0usize;
    nodes.push(json!({
        "name": scene.name,
        "rotation": [-std::f64::consts::FRAC_1_SQRT_2, 0.0, 0.0, std::f64::consts::FRAC_1_SQRT_2],
        "scale": [0.001, 0.001, 0.001],
        "children": Vec::<u32>::new(),
    }));
    let top = emit_node(&scene.root, &mut nodes);
    nodes[root_index]["children"] = json!([top]);

    let document = json!({
        "asset": { "version": "2.0", "generator": "lab-scene" },
        "scene": 0,
        "scenes": [ { "name": scene.name, "nodes": [root_index] } ],
        "nodes": nodes,
        "meshes": [
            unit_mesh("unit-box", 0, 1, 2),
            unit_mesh("unit-cylinder", 3, 4, 5),
        ],
        "materials": materials(),
        "accessors": buffer.accessors,
        "bufferViews": buffer.views,
        "buffers": [ {
            "byteLength": buffer.bytes.len(),
            "uri": format!(
                "data:application/octet-stream;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&buffer.bytes)
            ),
        } ],
    });
    serde_json::to_string_pretty(&document).expect("the glTF document serializes")
}

/// Emits one scene node (and its geometry as a scaled child), returning
/// its index.
fn emit_node(node: &SceneNode, nodes: &mut Vec<Value>) -> usize {
    let index = nodes.len();
    nodes.push(json!({
        "name": node.id,
        "translation": [node.translation[0], node.translation[1], node.translation[2]],
    }));
    let mut children: Vec<usize> = Vec::new();
    if let Some(geometry) = &node.geometry {
        let geometry_index = nodes.len();
        let (mesh, scale) = match geometry {
            Geometry::Box { x, y, z } => (0, [*x, *y, *z]),
            Geometry::Cylinder { diameter, height } => (1, [*diameter, *diameter, *height]),
        };
        nodes.push(json!({
            "name": format!("{}#geometry", node.id),
            "mesh": mesh,
            "scale": scale,
            "extras": { "material": material_index(&node.semantic, &node.id) },
        }));
        children.push(geometry_index);
    }
    for child in &node.children {
        children.push(emit_node(child, nodes));
    }
    if !children.is_empty() {
        nodes[index]["children"] = json!(children);
    }
    index
}

fn unit_mesh(name: &str, position: usize, normal: usize, indices: usize) -> Value {
    json!({
        "name": name,
        "primitives": [ {
            "attributes": { "POSITION": position, "NORMAL": normal },
            "indices": indices,
            "material": 0,
        } ],
    })
}

fn materials() -> Value {
    json!([
        { "name": "deck",    "pbrMetallicRoughness": { "baseColorFactor": [0.55, 0.56, 0.58, 1.0], "roughnessFactor": 0.9 }, "doubleSided": true },
        { "name": "carrier", "pbrMetallicRoughness": { "baseColorFactor": [0.33, 0.34, 0.38, 1.0], "roughnessFactor": 0.8 }, "doubleSided": true },
        { "name": "plate",   "pbrMetallicRoughness": { "baseColorFactor": [0.92, 0.92, 0.94, 1.0], "roughnessFactor": 0.5 }, "doubleSided": true },
        { "name": "tips",    "pbrMetallicRoughness": { "baseColorFactor": [0.90, 0.60, 0.20, 1.0], "roughnessFactor": 0.6 }, "doubleSided": true },
        { "name": "well",    "pbrMetallicRoughness": { "baseColorFactor": [0.30, 0.55, 0.90, 0.45], "roughnessFactor": 0.3 }, "alphaMode": "BLEND", "doubleSided": true },
        { "name": "station", "pbrMetallicRoughness": { "baseColorFactor": [0.62, 0.64, 0.68, 1.0], "roughnessFactor": 0.9 }, "doubleSided": true }
    ])
}

/// The material a semantic renders with. Recorded in node extras; the
/// viewer applies it (glTF binds materials to primitives, and the two
/// shared unit meshes cannot carry per-instance materials themselves).
fn material_index(semantic: &Semantic, id: &str) -> usize {
    match semantic {
        Semantic::Deck => 0,
        Semantic::Carrier { .. } => 1,
        Semantic::Labware { catalog } if catalog.contains("tip") => 3,
        Semantic::Labware { .. } => 2,
        Semantic::Well { .. } if id.contains("tip") => 3,
        Semantic::Well { .. } => 4,
        Semantic::Room | Semantic::Station { .. } | Semantic::Site { .. } => 5,
    }
}

/// The shared vertex data: a unit box (minimum corner at the origin) and
/// a unit cylinder (diameter 1, height 1, standing on the origin).
struct MeshBuffer {
    bytes: Vec<u8>,
    views: Value,
    accessors: Value,
}

impl MeshBuffer {
    fn build() -> Self {
        let (box_positions, box_normals, box_indices) = unit_box();
        let (cyl_positions, cyl_normals, cyl_indices) = unit_cylinder(24);

        let mut bytes: Vec<u8> = Vec::new();
        let mut views: Vec<Value> = Vec::new();
        let mut accessors: Vec<Value> = Vec::new();

        let push_f32 = |data: &[[f32; 3]],
                        bytes: &mut Vec<u8>,
                        views: &mut Vec<Value>,
                        accessors: &mut Vec<Value>| {
            let offset = bytes.len();
            for vertex in data {
                for component in vertex {
                    bytes.extend_from_slice(&component.to_le_bytes());
                }
            }
            let (min, max) = bounds(data);
            views.push(json!({ "buffer": 0, "byteOffset": offset, "byteLength": data.len() * 12, "target": 34962 }));
            accessors.push(json!({
                "bufferView": views.len() - 1, "componentType": 5126, "count": data.len(),
                "type": "VEC3", "min": min, "max": max,
            }));
        };
        let push_u16 = |data: &[u16],
                        bytes: &mut Vec<u8>,
                        views: &mut Vec<Value>,
                        accessors: &mut Vec<Value>| {
            // Index views must be 4-byte alignable after f32 views; u16 is
            // 2-aligned, which the f32 runs already guarantee.
            let offset = bytes.len();
            for index in data {
                bytes.extend_from_slice(&index.to_le_bytes());
            }
            if !bytes.len().is_multiple_of(4) {
                bytes.extend_from_slice(&[0, 0]);
            }
            views.push(json!({ "buffer": 0, "byteOffset": offset, "byteLength": data.len() * 2, "target": 34963 }));
            accessors.push(json!({
                "bufferView": views.len() - 1, "componentType": 5123, "count": data.len(),
                "type": "SCALAR",
            }));
        };

        push_f32(&box_positions, &mut bytes, &mut views, &mut accessors);
        push_f32(&box_normals, &mut bytes, &mut views, &mut accessors);
        push_u16(&box_indices, &mut bytes, &mut views, &mut accessors);
        push_f32(&cyl_positions, &mut bytes, &mut views, &mut accessors);
        push_f32(&cyl_normals, &mut bytes, &mut views, &mut accessors);
        push_u16(&cyl_indices, &mut bytes, &mut views, &mut accessors);

        Self {
            bytes,
            views: Value::Array(views),
            accessors: Value::Array(accessors),
        }
    }
}

fn bounds(data: &[[f32; 3]]) -> (Vec<f32>, Vec<f32>) {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for vertex in data {
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex[axis]);
            max[axis] = max[axis].max(vertex[axis]);
        }
    }
    (min.to_vec(), max.to_vec())
}

/// 24 vertices (per-face normals), 36 indices, min corner at the origin.
#[allow(clippy::type_complexity)]
fn unit_box() -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u16>) {
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        // -Z
        (
            [0.0, 0.0, -1.0],
            [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
        ),
        // +Z
        (
            [0.0, 0.0, 1.0],
            [
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [1.0, 1.0, 1.0],
                [0.0, 1.0, 1.0],
            ],
        ),
        // -Y
        (
            [0.0, -1.0, 0.0],
            [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
            ],
        ),
        // +Y
        (
            [0.0, 1.0, 0.0],
            [
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
                [1.0, 1.0, 1.0],
                [0.0, 1.0, 1.0],
            ],
        ),
        // -X
        (
            [-1.0, 0.0, 0.0],
            [
                [0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 1.0, 1.0],
                [0.0, 0.0, 1.0],
            ],
        ),
        // +X
        (
            [1.0, 0.0, 0.0],
            [
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [1.0, 1.0, 1.0],
                [1.0, 0.0, 1.0],
            ],
        ),
    ];
    let mut positions = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (normal, corners) in faces {
        let base = positions.len() as u16;
        positions.extend_from_slice(&corners);
        normals.extend([normal; 4]);
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (positions, normals, indices)
}

/// A cylinder of diameter and height 1 standing on the origin: smooth
/// side normals, flat caps.
#[allow(clippy::type_complexity)]
fn unit_cylinder(segments: usize) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u16>) {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();

    // Side: paired bottom/top vertices around the rim.
    for segment in 0..=segments {
        let angle = std::f32::consts::TAU * segment as f32 / segments as f32;
        let (sin, cos) = angle.sin_cos();
        positions.push([0.5 * cos, 0.5 * sin, 0.0]);
        positions.push([0.5 * cos, 0.5 * sin, 1.0]);
        normals.push([cos, sin, 0.0]);
        normals.push([cos, sin, 0.0]);
    }
    for segment in 0..segments {
        let a = (segment * 2) as u16;
        indices.extend_from_slice(&[a, a + 2, a + 1, a + 1, a + 2, a + 3]);
    }

    // Caps: a center vertex and a rim per cap.
    for (z, normal_z) in [(0.0f32, -1.0f32), (1.0, 1.0)] {
        let center = positions.len() as u16;
        positions.push([0.0, 0.0, z]);
        normals.push([0.0, 0.0, normal_z]);
        let rim_start = positions.len() as u16;
        for segment in 0..segments {
            let angle = std::f32::consts::TAU * segment as f32 / segments as f32;
            let (sin, cos) = angle.sin_cos();
            positions.push([0.5 * cos, 0.5 * sin, z]);
            normals.push([0.0, 0.0, normal_z]);
        }
        for segment in 0..segments {
            let a = rim_start + segment as u16;
            let b = rim_start + ((segment + 1) % segments) as u16;
            if normal_z > 0.0 {
                indices.extend_from_slice(&[center, a, b]);
            } else {
                indices.extend_from_slice(&[center, b, a]);
            }
        }
    }
    (positions, normals, indices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{SCENE_FORMAT, SceneNode, Semantic};

    #[test]
    fn the_gltf_document_is_structurally_sound() {
        let scene = Scene {
            format: SCENE_FORMAT.to_string(),
            name: "test".to_string(),
            root: SceneNode::new("room", Semantic::Room, [0.0, 0.0, 0.0]).with_geometry(
                Geometry::Box {
                    x: 100.0,
                    y: 100.0,
                    z: 10.0,
                },
            ),
        };
        let text = render_gltf(&scene);
        let document: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(document["asset"]["version"], "2.0");
        assert_eq!(document["meshes"].as_array().unwrap().len(), 2);
        let accessor_count = document["accessors"].as_array().unwrap().len();
        assert_eq!(accessor_count, 6, "positions, normals, indices per mesh");
        // Every node child index is in range.
        let nodes = document["nodes"].as_array().unwrap();
        for node in nodes {
            if let Some(children) = node["children"].as_array() {
                for child in children {
                    assert!((child.as_u64().unwrap() as usize) < nodes.len());
                }
            }
        }
        // The buffer decodes and matches its declared length.
        let uri = document["buffers"][0]["uri"].as_str().unwrap();
        let encoded = uri.split(',').nth(1).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        assert_eq!(
            bytes.len() as u64,
            document["buffers"][0]["byteLength"].as_u64().unwrap()
        );
    }
}
