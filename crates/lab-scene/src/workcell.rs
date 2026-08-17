//! The workcell room scene: stations in a row, each with what geometry it
//! has. The liquid handler nests its full deck; instruments without a
//! real asset render as labeled, nominally-dimensioned boxes, which is the
//! asset registry's last-resort tier working as intended.

use lab_compiler::backend::hamilton::star::profile::StarTargetProfile;

use crate::assets::AssetCatalog;
use crate::scene::{Geometry, SCENE_FORMAT, Scene, SceneError, SceneNode, Semantic};
use crate::star::star_deck_scene;

/// Nominal gap between station origins along the room's x axis.
const STATION_PITCH_MM: f64 = 1600.0;
/// Nominal bench height stations stand at.
const BENCH_TOP_MM: f64 = 900.0;

/// One station to place: its name, its kind string, and its STAR profile
/// when it is a liquid handler.
pub struct StationScene {
    pub name: String,
    pub kind: String,
    pub star_profile: Option<StarTargetProfile>,
}

/// Nominal instrument body extents by station kind. The STAR renders as a
/// plinth under its deck, so the deck's carriers and labware stay visible
/// above it.
fn station_extent(kind: &str) -> [f64; 3] {
    match kind {
        "hamilton.star" => [1400.0, 700.0, 60.0],
        "inheco.odtc" => [200.0, 320.0, 260.0],
        "byonoy.absorbance96" => [160.0, 240.0, 120.0],
        _ => [400.0, 400.0, 400.0],
    }
}

/// The registry fallback chain, stated once: an asset when the facility
/// has one, the dimensioned box otherwise.
pub(crate) fn geometry_for(
    assets: Option<&AssetCatalog>,
    key: &str,
    fallback: [f64; 3],
) -> Geometry {
    match assets {
        Some(catalog) => catalog.resolve(key, fallback),
        None => Geometry::Box {
            x: fallback[0],
            y: fallback[1],
            z: fallback[2],
        },
    }
}

/// Kit-room slab thickness.
const SLAB_MM: f64 = 80.0;

/// The room shell an idealized facility renders: an environment asset
/// when the facility names one, a kit floor and three walls otherwise
/// (the front stays open so a default camera sees in).
fn room_shell(room: &lab_runfmt::facility::Room, assets: Option<&AssetCatalog>) -> Vec<SceneNode> {
    let (width, depth, height) = (room.width_mm, room.depth_mm, room.height_mm);
    if let Some(key) = &room.environment {
        return vec![
            SceneNode::new("room:environment", Semantic::Room, [0.0, 0.0, 0.0])
                .with_geometry(geometry_for(assets, key, [width, depth, height])),
        ];
    }
    let slab = |id: &str, translation: [f64; 3], extent: [f64; 3]| {
        SceneNode::new(id, Semantic::Room, translation).with_geometry(Geometry::Box {
            x: extent[0],
            y: extent[1],
            z: extent[2],
        })
    };
    vec![
        slab("room:floor", [0.0, 0.0, -SLAB_MM], [width, depth, SLAB_MM]),
        slab(
            "room:wall-back",
            [0.0, depth, 0.0],
            [width, SLAB_MM, height],
        ),
        slab(
            "room:wall-left",
            [-SLAB_MM, 0.0, 0.0],
            [SLAB_MM, depth, height],
        ),
        slab(
            "room:wall-right",
            [width, 0.0, 0.0],
            [SLAB_MM, depth, height],
        ),
    ]
}

/// Builds the room scene for a set of stations. A facility supplies the
/// layout: per-station floor positions and rotations, and the room shell.
/// Without one, stations line up in declaration order on a bare floor.
pub fn workcell_scene(
    name: &str,
    stations: Vec<StationScene>,
    assets: Option<&AssetCatalog>,
    facility: Option<&lab_runfmt::facility::Facility>,
) -> Result<Scene, SceneError> {
    let mut room = SceneNode::new("room", Semantic::Room, [0.0, 0.0, 0.0]);
    if let Some(shell) = facility.and_then(|facility| facility.room.as_ref()) {
        room.children.extend(room_shell(shell, assets));
    }
    for (index, station) in stations.into_iter().enumerate() {
        let placed = facility.and_then(|facility| facility.station(&station.name));
        let position = placed
            .and_then(|declaration| declaration.position_mm)
            .unwrap_or([index as f64 * STATION_PITCH_MM, 0.0]);
        let mut node = SceneNode::new(
            station.name.clone(),
            Semantic::Station {
                station_kind: station.kind.clone(),
            },
            [position[0], position[1], BENCH_TOP_MM],
        )
        .with_geometry(geometry_for(
            assets,
            &station.kind,
            station_extent(&station.kind),
        ));
        node.rotation_z_deg = placed
            .and_then(|declaration| declaration.rotation_deg)
            .unwrap_or(0.0);
        // A real asset carries its own body detail; the procedural
        // assembly dresses the box tier so a bare facility still reads
        // as a lab.
        if !matches!(node.geometry, Some(Geometry::Mesh { .. })) {
            let extent = station_extent(&station.kind);
            node.children.extend(crate::instruments::assembly(
                &station.name,
                &station.kind,
                [extent[0], extent[1]],
            ));
        }
        if let Some(profile) = &station.star_profile {
            // The deck scene is in the machine's own frame, which shares
            // the station node's origin.
            node.children.push(star_deck_scene(profile, assets)?);
        }
        room.children.push(node);
    }
    Ok(Scene {
        format: SCENE_FORMAT.to_string(),
        name: name.to_string(),
        root: room,
    })
}

/// A single bench is a room with one station.
pub fn star_bench_scene(
    name: &str,
    profile: &StarTargetProfile,
    assets: Option<&AssetCatalog>,
) -> Result<Scene, SceneError> {
    workcell_scene(
        name,
        vec![StationScene {
            name: "star".to_string(),
            kind: "hamilton.star".to_string(),
            star_profile: Some(profile.clone()),
        }],
        assets,
        None,
    )
}
