//! Trace-driven USD animation: the same events the web player interprets,
//! written as time samples so Omniverse, Blender, and usdview play the
//! actual run.
//!
//! One timecode is one simulated second. Labware translates hold their
//! home until each confirmed handoff and arrive at the destination's top
//! two seconds later; a synthesized pipetting head per liquid handler
//! follows the frames' deck coordinates and is visible only while its
//! program runs. Door and thermal state are not animated yet: they are
//! material changes, and the referenced-asset material story owns them.

use std::collections::BTreeMap;

use lab_runfmt::{RunEvent, SimTraceDocument};

use crate::scene::{Geometry, Scene, SceneError, SceneNode, Semantic};
use crate::usda::{EmitContext, HeadTrack, render_usda_with};

/// Seconds a carried plate takes to arrive after its handoff confirms.
const HANDOFF_TRAVEL_SECONDS: f64 = 2.0;
/// The pipetting head's working height over the deck, in millimeters.
const HEAD_HEIGHT_MM: f64 = 260.0;
/// Clearance between a seated plate and the station body under it.
const SEAT_CLEARANCE_MM: f64 = 5.0;

struct NodeFacts {
    origin: [f64; 3],
    local: [f64; 3],
    rotated_ancestry: bool,
    body_height: f64,
}

/// Translation-accumulated facts per node id, with a flag for any rotation
/// in the ancestor chain (excluding the node's own rotation).
fn collect_facts(scene: &Scene) -> BTreeMap<String, NodeFacts> {
    let mut facts = BTreeMap::new();
    fn visit(
        node: &SceneNode,
        origin: [f64; 3],
        rotated_ancestry: bool,
        facts: &mut BTreeMap<String, NodeFacts>,
    ) {
        let here = [
            origin[0] + node.translation[0],
            origin[1] + node.translation[1],
            origin[2] + node.translation[2],
        ];
        let body_height = match &node.geometry {
            Some(Geometry::Box { z, .. }) => *z,
            Some(Geometry::Cylinder { height, .. }) => *height,
            Some(Geometry::Mesh { fallback, .. }) => fallback[2],
            None => 0.0,
        };
        facts.insert(
            node.id.clone(),
            NodeFacts {
                origin: here,
                local: node.translation,
                rotated_ancestry,
                body_height,
            },
        );
        let rotated_below = rotated_ancestry || node.rotation_z_deg != 0.0;
        for child in &node.children {
            visit(child, here, rotated_below, facts);
        }
    }
    visit(&scene.root, [0.0, 0.0, 0.0], false, &mut facts);
    facts
}

/// Renders the animated `.usda` layer for a scene and the trace of one
/// simulated run over it.
pub fn render_usda_animated(scene: &Scene, trace: &SimTraceDocument) -> Result<String, SceneError> {
    let facts = collect_facts(scene);
    let mut labware_tracks: BTreeMap<String, Vec<(f64, [f64; 3])>> = BTreeMap::new();
    let mut head_tracks: BTreeMap<String, HeadTrack> = BTreeMap::new();
    // The station whose program most recently emitted a frame, per head.
    let mut heads_visible: BTreeMap<String, bool> = BTreeMap::new();

    for timed in &trace.events {
        match &timed.event {
            RunEvent::LabwareMoved { labware, to, .. } => {
                let Some(moving) = facts.get(labware) else {
                    continue;
                };
                let Some(station) = facts.get(to) else {
                    continue;
                };
                if moving.rotated_ancestry {
                    return Err(SceneError::Resolve {
                        context: labware.clone(),
                        message: format!(
                            "labware '{labware}' sits under a rotated ancestor; animated moves under rotation are not supported yet — drop rotation_deg from that station"
                        ),
                    });
                }
                let track = labware_tracks
                    .entry(labware.clone())
                    .or_insert_with(|| vec![(0.0, moving.local)]);
                // Hold wherever the plate was, then travel to the seat.
                let held = track.last().expect("tracks start seeded").1;
                track.push((timed.t, held));
                let parent_origin = [
                    moving.origin[0] - moving.local[0],
                    moving.origin[1] - moving.local[1],
                    moving.origin[2] - moving.local[2],
                ];
                let seat_world = [
                    station.origin[0],
                    station.origin[1],
                    station.origin[2] + station.body_height + SEAT_CLEARANCE_MM,
                ];
                track.push((
                    timed.t + HANDOFF_TRAVEL_SECONDS,
                    [
                        seat_world[0] - parent_origin[0],
                        seat_world[1] - parent_origin[1],
                        seat_world[2] - parent_origin[2],
                    ],
                ));
            }
            RunEvent::Frame {
                station,
                x_mm: Some(x),
                y_mm: Some(y),
                ..
            } => {
                let track = head_tracks.entry(station.clone()).or_default();
                if !heads_visible.get(station).copied().unwrap_or(false) {
                    track.visibility.push((timed.t, true));
                    heads_visible.insert(station.clone(), true);
                }
                track.positions.push((timed.t, [*x, *y, HEAD_HEIGHT_MM]));
            }
            RunEvent::NodeCompleted { .. } => {
                for (station, visible) in heads_visible.iter_mut() {
                    if *visible {
                        if let Some(track) = head_tracks.get_mut(station) {
                            track.visibility.push((timed.t, false));
                        }
                        *visible = false;
                    }
                }
            }
            _ => {}
        }
    }

    let context = EmitContext {
        materials: true,
        end_time_code: Some(trace.summary.total_seconds),
        labware_tracks,
        head_tracks,
    };
    Ok(render_usda_with(scene, &context))
}

/// The semantic a head prim's parts render with, shared with the static
/// exporter's material table.
pub(crate) fn head_semantic() -> Semantic {
    Semantic::Station {
        station_kind: "pipetting-head".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lab_runfmt::{ProgramExtent, SimSummary, TimedEvent};

    use crate::scene::{Geometry, SCENE_FORMAT};

    fn timed(t: f64, event: RunEvent) -> TimedEvent {
        TimedEvent { t, event }
    }

    fn test_scene() -> Scene {
        let plate = SceneNode::new(
            "reaction_plate",
            Semantic::Labware {
                catalog: "pcr_plate_96".to_string(),
            },
            [100.0, 50.0, 0.0],
        )
        .with_geometry(Geometry::Box {
            x: 127.76,
            y: 85.48,
            z: 16.1,
        });
        let mut star = SceneNode::new(
            "star-1",
            Semantic::Station {
                station_kind: "hamilton.star".to_string(),
            },
            [0.0, 0.0, 900.0],
        )
        .with_geometry(Geometry::Box {
            x: 1400.0,
            y: 700.0,
            z: 60.0,
        });
        star.children.push(plate);
        let odtc = SceneNode::new(
            "odtc-1",
            Semantic::Station {
                station_kind: "inheco.odtc".to_string(),
            },
            [2000.0, 0.0, 900.0],
        )
        .with_geometry(Geometry::Box {
            x: 200.0,
            y: 320.0,
            z: 260.0,
        });
        let mut room = SceneNode::new("room", Semantic::Room, [0.0, 0.0, 0.0]);
        room.children.push(star);
        room.children.push(odtc);
        Scene {
            format: SCENE_FORMAT.to_string(),
            name: "test".to_string(),
            root: room,
        }
    }

    fn test_trace() -> SimTraceDocument {
        SimTraceDocument {
            format: lab_runfmt::SIM_TRACE_FORMAT.to_string(),
            plan: "plan.workcell.json".to_string(),
            durations: "default-v0".to_string(),
            events: vec![
                timed(
                    0.0,
                    RunEvent::ProgramStarted {
                        station: "star-1".to_string(),
                        title: "assembly".to_string(),
                        extent: ProgramExtent::Frames { frames: 2 },
                    },
                ),
                timed(
                    1.0,
                    RunEvent::Frame {
                        station: "star-1".to_string(),
                        index: 1,
                        description: "pick up tips".to_string(),
                        x_mm: Some(117.9),
                        y_mm: Some(241.8),
                    },
                ),
                timed(
                    9.0,
                    RunEvent::Frame {
                        station: "star-1".to_string(),
                        index: 2,
                        description: "aspirate".to_string(),
                        x_mm: Some(300.0),
                        y_mm: Some(180.0),
                    },
                ),
                timed(
                    20.0,
                    RunEvent::NodeCompleted {
                        id: "assembly_run".to_string(),
                    },
                ),
                timed(
                    120.0,
                    RunEvent::LabwareMoved {
                        labware: "reaction_plate".to_string(),
                        from: "star-1".to_string(),
                        to: "odtc-1".to_string(),
                    },
                ),
            ],
            summary: SimSummary {
                total_seconds: 500.0,
                ..SimSummary::default()
            },
        }
    }

    #[test]
    fn the_animated_layer_carries_time_samples_and_a_head() {
        let text = render_usda_animated(&test_scene(), &test_trace()).unwrap();
        assert!(text.contains("endTimeCode = 500"), "{text}");
        assert!(text.contains("timeCodesPerSecond = 1"));
        assert!(
            text.contains("xformOp:translate.timeSamples"),
            "labware animates"
        );
        // The plate holds home (0..120), then arrives on the ODTC top:
        // odtc origin (2000, 0, 900) + body 260 + clearance 5, expressed in
        // the plate's parent frame (star-1 at (0, 0, 900)).
        assert!(
            text.contains("122: (2000, 0, 265)"),
            "the seat sample lands on the station top:\n{}",
            text.lines()
                .filter(|line| line.contains("timeSamples") || line.contains("122:"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert!(text.contains("def Xform \"pipetting_head\""));
        assert!(
            text.contains("token visibility.timeSamples"),
            "the head hides between programs"
        );
        assert!(
            text.contains("1: (117.9, 241.8, 260)"),
            "head samples follow frame coordinates"
        );
    }

    #[test]
    fn animated_labware_under_a_rotated_station_is_refused() {
        let mut scene = test_scene();
        scene.root.children[0].rotation_z_deg = 15.0;
        let error = render_usda_animated(&scene, &test_trace())
            .expect_err("rotation under animation is refused");
        assert!(error.to_string().contains("rotated"), "{error}");
    }
}
