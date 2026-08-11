//! The workcell room scene: stations in a row, each with what geometry it
//! has. The liquid handler nests its full deck; instruments without a
//! geometry catalog render as labeled, nominally-dimensioned boxes, which
//! is the asset registry's last-resort tier working as intended.

use lab_compiler::backend::hamilton::star::profile::StarTargetProfile;

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

/// Nominal instrument body extents by station kind.
fn station_body(kind: &str) -> Geometry {
    match kind {
        // The STAR renders as a plinth under its deck, so the deck's
        // carriers and labware stay visible above it.
        "hamilton.star" => Geometry::Box {
            x: 1400.0,
            y: 700.0,
            z: 60.0,
        },
        "inheco.odtc" => Geometry::Box {
            x: 200.0,
            y: 320.0,
            z: 260.0,
        },
        "byonoy.absorbance96" => Geometry::Box {
            x: 160.0,
            y: 240.0,
            z: 120.0,
        },
        _ => Geometry::Box {
            x: 400.0,
            y: 400.0,
            z: 400.0,
        },
    }
}

/// Builds the room scene for a set of stations, in declaration order.
pub fn workcell_scene(name: &str, stations: Vec<StationScene>) -> Result<Scene, SceneError> {
    let mut room = SceneNode::new("room", Semantic::Room, [0.0, 0.0, 0.0]);
    for (index, station) in stations.into_iter().enumerate() {
        let mut node = SceneNode::new(
            station.name.clone(),
            Semantic::Station {
                station_kind: station.kind.clone(),
            },
            [index as f64 * STATION_PITCH_MM, 0.0, BENCH_TOP_MM],
        )
        .with_geometry(station_body(&station.kind));
        if let Some(profile) = &station.star_profile {
            // The deck scene is in the machine's own frame, which shares
            // the station node's origin.
            node.children.push(star_deck_scene(profile)?);
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
pub fn star_bench_scene(name: &str, profile: &StarTargetProfile) -> Result<Scene, SceneError> {
    workcell_scene(
        name,
        vec![StationScene {
            name: "star".to_string(),
            kind: "hamilton.star".to_string(),
            star_profile: Some(profile.clone()),
        }],
    )
}
