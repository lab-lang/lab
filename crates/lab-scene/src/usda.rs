//! USD export: a plain-text `.usda` layer for Isaac Sim, Omniverse, and
//! Unreal. Same scene, same millimeters; USD carries the unit and up-axis
//! declaration itself, so no root transform is needed.
//!
//! Every geometry prim binds a `UsdPreviewSurface` material by semantic —
//! the portable interchange surface Blender, Omniverse, and usdview all
//! honor. Referenced assets carry their own richer material networks
//! untouched. The animated variant (see `animate`) adds time samples over
//! the same emission.

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::scene::{Geometry, Scene, SceneNode, Semantic};

/// A pipetting head's motion: positions per frame event, and the moments
/// it appears and hides.
#[derive(Clone, Debug, Default)]
pub(crate) struct HeadTrack {
    pub positions: Vec<(f64, [f64; 3])>,
    pub visibility: Vec<(f64, bool)>,
}

/// What one emission run carries beyond the scene itself.
#[derive(Default)]
pub(crate) struct EmitContext {
    /// Emit the material scope and bind geometry prims to it.
    pub materials: bool,
    /// Animated stage: `endTimeCode` in simulated seconds.
    pub end_time_code: Option<f64>,
    /// Labware translate tracks, keyed by node id, in parent-local mm.
    pub labware_tracks: BTreeMap<String, Vec<(f64, [f64; 3])>>,
    /// Pipetting-head tracks, keyed by the station node id they ride under.
    pub head_tracks: BTreeMap<String, HeadTrack>,
}

/// Renders the scene as a static `.usda` text layer: one `Xform` per node,
/// with `Cube`/`Cylinder` geometry prims scaled to the stated extents and
/// asset references composed in.
pub fn render_usda(scene: &Scene) -> String {
    render_usda_with(
        scene,
        &EmitContext {
            materials: true,
            ..EmitContext::default()
        },
    )
}

pub(crate) fn render_usda_with(scene: &Scene, context: &EmitContext) -> String {
    let mut text = String::new();
    let root = prim_name(&scene.root.id);
    let _ = writeln!(text, "#usda 1.0\n(");
    let _ = writeln!(text, "    defaultPrim = \"{root}\"");
    let _ = writeln!(text, "    metersPerUnit = 0.001");
    let _ = writeln!(text, "    upAxis = \"Z\"");
    if let Some(end) = context.end_time_code {
        let _ = writeln!(text, "    startTimeCode = 0");
        let _ = writeln!(text, "    endTimeCode = {end}");
        let _ = writeln!(text, "    timeCodesPerSecond = 1");
        let _ = writeln!(text, "    framesPerSecond = 1");
    }
    let _ = writeln!(text, ")\n");
    let mut emitter = Emitter {
        context,
        root: root.clone(),
        text,
    };
    emitter.node(&scene.root, 0);
    emitter.text
}

/// USD prim names are identifiers; scene ids carry `/`, `:`, `#`.
pub(crate) fn prim_name(id: &str) -> String {
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

/// The material name a semantic renders with, mirroring the glTF table.
fn material_name(semantic: &Semantic, id: &str) -> &'static str {
    match semantic {
        Semantic::Deck => "lab_deck",
        Semantic::Carrier { .. } => "lab_carrier",
        Semantic::Labware { catalog } if catalog.contains("tip") => "lab_tips",
        Semantic::Labware { .. } => "lab_plate",
        Semantic::Well { .. } if id.contains("tip") => "lab_tips",
        Semantic::Well { .. } => "lab_well",
        Semantic::Room => "lab_room",
        Semantic::Station { .. } | Semantic::Site { .. } => "lab_station",
    }
}

/// `(name, diffuse rgb, roughness, metallic, opacity)` per material.
const MATERIALS: [(&str, [f64; 3], f64, f64, f64); 7] = [
    ("lab_deck", [0.55, 0.56, 0.58], 0.9, 0.4, 1.0),
    ("lab_carrier", [0.33, 0.34, 0.38], 0.55, 0.8, 1.0),
    ("lab_plate", [0.92, 0.92, 0.94], 0.5, 0.0, 1.0),
    ("lab_tips", [0.90, 0.60, 0.20], 0.6, 0.0, 1.0),
    ("lab_well", [0.30, 0.55, 0.90], 0.3, 0.0, 0.45),
    ("lab_station", [0.62, 0.64, 0.68], 0.85, 0.3, 1.0),
    ("lab_room", [0.82, 0.82, 0.80], 0.95, 0.0, 1.0),
];

struct Emitter<'context> {
    context: &'context EmitContext,
    root: String,
    text: String,
}

impl Emitter<'_> {
    fn indent(&mut self, depth: usize) {
        for _ in 0..depth {
            self.text.push_str("    ");
        }
    }

    fn line(&mut self, depth: usize, line: &str) {
        self.indent(depth);
        let _ = writeln!(self.text, "{line}");
    }

    fn materials_scope(&mut self, depth: usize) {
        self.line(depth, "def Scope \"Materials\"");
        self.line(depth, "{");
        for (name, diffuse, roughness, metallic, opacity) in MATERIALS {
            let root = self.root.clone();
            self.line(depth + 1, &format!("def Material \"{name}\""));
            self.line(depth + 1, "{");
            self.line(
                depth + 2,
                &format!(
                    "token outputs:surface.connect = </{root}/Materials/{name}/surface.outputs:surface>"
                ),
            );
            self.line(depth + 2, "def Shader \"surface\"");
            self.line(depth + 2, "{");
            self.line(depth + 3, "uniform token info:id = \"UsdPreviewSurface\"");
            self.line(
                depth + 3,
                &format!(
                    "color3f inputs:diffuseColor = ({}, {}, {})",
                    diffuse[0], diffuse[1], diffuse[2]
                ),
            );
            self.line(depth + 3, &format!("float inputs:roughness = {roughness}"));
            self.line(depth + 3, &format!("float inputs:metallic = {metallic}"));
            self.line(depth + 3, &format!("float inputs:opacity = {opacity}"));
            self.line(depth + 3, "token outputs:surface");
            self.line(depth + 2, "}");
            self.line(depth + 1, "}");
        }
        self.line(depth, "}");
    }

    fn binding(&mut self, depth: usize, semantic: &Semantic, id: &str) {
        if self.context.materials {
            let name = material_name(semantic, id);
            let root = self.root.clone();
            self.line(
                depth,
                &format!("rel material:binding = </{root}/Materials/{name}>"),
            );
        }
    }

    fn time_samples(&mut self, depth: usize, samples: &[(f64, [f64; 3])]) {
        self.line(depth, "double3 xformOp:translate.timeSamples = {");
        for (t, value) in samples {
            self.line(
                depth + 1,
                &format!("{t}: ({}, {}, {}),", value[0], value[1], value[2]),
            );
        }
        self.line(depth, "}");
    }

    /// A USD Cube spans [-size/2, size/2]; scale a unit cube and seat its
    /// minimum corner on the node origin.
    fn fallback_box(&mut self, depth: usize, extent: &[f64; 3], semantic: &Semantic, id: &str) {
        let (x, y, z) = (extent[0], extent[1], extent[2]);
        self.line(depth + 1, "def Cube \"geometry\"");
        self.line(depth + 1, "{");
        self.binding(depth + 2, semantic, id);
        self.line(depth + 2, "double size = 1");
        self.line(
            depth + 2,
            &format!(
                "double3 xformOp:translate = ({}, {}, {})",
                x / 2.0,
                y / 2.0,
                z / 2.0
            ),
        );
        self.line(
            depth + 2,
            &format!("float3 xformOp:scale = ({x}, {y}, {z})"),
        );
        self.line(
            depth + 2,
            "uniform token[] xformOpOrder = [\"xformOp:translate\", \"xformOp:scale\"]",
        );
        self.line(depth + 1, "}");
    }

    fn head_prim(&mut self, depth: usize, track: &HeadTrack) {
        let semantic = crate::animate::head_semantic();
        self.line(depth + 1, "def Xform \"pipetting_head\"");
        self.line(depth + 1, "{");
        if !track.positions.is_empty() {
            self.time_samples(depth + 2, &track.positions);
            self.line(
                depth + 2,
                "uniform token[] xformOpOrder = [\"xformOp:translate\"]",
            );
        }
        if !track.visibility.is_empty() {
            self.line(depth + 2, "token visibility.timeSamples = {");
            for (t, visible) in &track.visibility {
                let value = if *visible { "inherited" } else { "invisible" };
                self.line(depth + 3, &format!("{t}: \"{value}\","));
            }
            self.line(depth + 2, "}");
        }
        // The carriage block and the tip below it, centered on the head.
        self.line(depth + 2, "def Cube \"carriage\"");
        self.line(depth + 2, "{");
        self.binding(depth + 3, &semantic, "pipetting_head");
        self.line(depth + 3, "double size = 1");
        self.line(depth + 3, "double3 xformOp:translate = (0, 0, 55)");
        self.line(depth + 3, "float3 xformOp:scale = (60, 60, 110)");
        self.line(
            depth + 3,
            "uniform token[] xformOpOrder = [\"xformOp:translate\", \"xformOp:scale\"]",
        );
        self.line(depth + 2, "}");
        self.line(depth + 2, "def Cylinder \"tip\"");
        self.line(depth + 2, "{");
        self.binding(depth + 3, &semantic, "pipetting_head");
        self.line(depth + 3, "uniform token axis = \"Z\"");
        self.line(depth + 3, "double height = 70");
        self.line(depth + 3, "double radius = 2.5");
        self.line(depth + 3, "double3 xformOp:translate = (0, 0, -35)");
        self.line(
            depth + 3,
            "uniform token[] xformOpOrder = [\"xformOp:translate\"]",
        );
        self.line(depth + 2, "}");
        self.line(depth + 1, "}");
    }

    fn node(&mut self, node: &SceneNode, depth: usize) {
        let name = prim_name(&node.id);
        self.line(depth, &format!("def Xform \"{name}\" ("));
        self.line(
            depth,
            &format!("    customData = {{ string labId = \"{}\" }}", node.id),
        );
        self.line(depth, ")");
        self.line(depth, "{");

        if let Some(samples) = self.context.labware_tracks.get(&node.id) {
            let samples = samples.clone();
            self.time_samples(depth + 1, &samples);
        } else {
            self.line(
                depth + 1,
                &format!(
                    "double3 xformOp:translate = ({}, {}, {})",
                    node.translation[0], node.translation[1], node.translation[2]
                ),
            );
        }
        if node.rotation_z_deg != 0.0 {
            self.line(
                depth + 1,
                &format!("double xformOp:rotateZ = {}", node.rotation_z_deg),
            );
            self.line(
                depth + 1,
                "uniform token[] xformOpOrder = [\"xformOp:translate\", \"xformOp:rotateZ\"]",
            );
        } else {
            self.line(
                depth + 1,
                "uniform token[] xformOpOrder = [\"xformOp:translate\"]",
            );
        }

        if depth == 0 && self.context.materials {
            self.materials_scope(depth + 1);
        }

        if let Some(geometry) = &node.geometry {
            match geometry {
                Geometry::Box { x, y, z } => {
                    self.fallback_box(depth, &[*x, *y, *z], &node.semantic, &node.id);
                }
                Geometry::Cylinder { diameter, height } => {
                    let (diameter, height) = (*diameter, *height);
                    self.line(depth + 1, "def Cylinder \"geometry\"");
                    self.line(depth + 1, "{");
                    self.binding(depth + 2, &node.semantic, &node.id);
                    self.line(depth + 2, "uniform token axis = \"Z\"");
                    self.line(depth + 2, &format!("double height = {height}"));
                    self.line(depth + 2, &format!("double radius = {}", diameter / 2.0));
                    self.line(
                        depth + 2,
                        &format!("double3 xformOp:translate = (0, 0, {})", height / 2.0),
                    );
                    self.line(
                        depth + 2,
                        "uniform token[] xformOpOrder = [\"xformOp:translate\"]",
                    );
                    self.line(depth + 1, "}");
                }
                Geometry::Mesh { usd, fallback, .. } => match usd {
                    // The asset layer carries its own geometry and
                    // materials; referencing it composes it here.
                    Some(path) => {
                        self.line(depth + 1, "def Xform \"geometry\" (");
                        self.line(depth + 1, &format!("    prepend references = @{path}@"));
                        self.line(depth + 1, ")");
                        self.line(depth + 1, "{");
                        self.line(depth + 1, "}");
                    }
                    // No USD flavor of this asset: the fallback box,
                    // exactly as an un-assetted node renders.
                    None => {
                        self.fallback_box(depth, fallback, &node.semantic, &node.id);
                    }
                },
            }
        }

        if let Some(track) = self.context.head_tracks.get(&node.id) {
            let track = track.clone();
            self.head_prim(depth, &track);
        }

        for child in &node.children {
            self.node(child, depth + 1);
        }
        self.line(depth, "}");
    }
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
        assert!(
            text.contains("def Material \"lab_well\"")
                && text.contains("rel material:binding = </room/Materials/lab_well>"),
            "geometry binds a preview-surface material"
        );
        assert!(
            !text.contains("endTimeCode"),
            "a static layer carries no timeline"
        );
    }
}
