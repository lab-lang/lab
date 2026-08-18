//! Procedurally modeled instruments: the middle tier of the asset
//! registry, between a vendor mesh and the bare box.
//!
//! Each assembly is built from the same primitives every renderer draws,
//! so a facility with no asset files still reads as a lab: the STAR has
//! its towers, gantry, and glass hood; the cycler has its lid and status
//! light; every station stands on a bench. Parts are cosmetic children of
//! the station node — the station's own geometry stays the simple body the
//! seat and animation math measure against.

use crate::scene::{Geometry, SceneNode, Semantic};

fn part(id: String, material: &str, translation: [f64; 3], extent: [f64; 3]) -> SceneNode {
    SceneNode::new(
        id,
        Semantic::Part {
            material: material.to_string(),
        },
        translation,
    )
    .with_geometry(Geometry::Box {
        x: extent[0],
        y: extent[1],
        z: extent[2],
    })
}

/// A bench under a station: worktop and four legs, sized to the footprint
/// with an overhang. The station origin stays at the bench top.
fn bench(station: &str, footprint: [f64; 2]) -> Vec<SceneNode> {
    let width = footprint[0].max(500.0) + 200.0;
    let depth = footprint[1].max(400.0) + 150.0;
    let x0 = (footprint[0] - width) / 2.0;
    let y0 = (footprint[1] - depth) / 2.0;
    let mut parts = vec![part(
        format!("{station}:bench:top"),
        "panel",
        [x0, y0, -40.0],
        [width, depth, 40.0],
    )];
    let leg = 60.0;
    for (index, (leg_x, leg_y)) in [
        (x0 + 20.0, y0 + 20.0),
        (x0 + width - leg - 20.0, y0 + 20.0),
        (x0 + 20.0, y0 + depth - leg - 20.0),
        (x0 + width - leg - 20.0, y0 + depth - leg - 20.0),
    ]
    .into_iter()
    .enumerate()
    {
        parts.push(part(
            format!("{station}:bench:leg-{}", index + 1),
            "frame",
            [leg_x, leg_y, -860.0],
            [leg, leg, 820.0],
        ));
    }
    parts
}

/// The Hamilton STAR body around its deck: side towers, the gantry beam
/// the pipetting head rides, a back panel, and the front glass.
fn star_assembly(station: &str) -> Vec<SceneNode> {
    let mut parts = bench(station, [1400.0, 700.0]);
    let tower = |id: &str, x: f64| {
        part(
            format!("{station}:{id}"),
            "frame",
            [x, 0.0, 60.0],
            [90.0, 700.0, 560.0],
        )
    };
    parts.push(tower("tower-left", -90.0));
    parts.push(tower("tower-right", 1400.0));
    // The gantry beam spans the towers over the deck's working area.
    parts.push(part(
        format!("{station}:gantry"),
        "frame",
        [-90.0, 240.0, 620.0],
        [1580.0, 220.0, 110.0],
    ));
    parts.push(part(
        format!("{station}:back-panel"),
        "panel",
        [-90.0, 690.0, 60.0],
        [1580.0, 60.0, 670.0],
    ));
    parts.push(part(
        format!("{station}:front-glass"),
        "glass",
        [0.0, -25.0, 80.0],
        [1400.0, 12.0, 520.0],
    ));
    parts.push(part(
        format!("{station}:status-light"),
        "accent",
        [-90.0, -20.0, 640.0],
        [1580.0, 20.0, 24.0],
    ));
    parts
}

/// The on-deck thermocycler: lid seam, vents, and a status light on the
/// body the plate seats on.
fn odtc_assembly(station: &str) -> Vec<SceneNode> {
    let mut parts = bench(station, [200.0, 320.0]);
    parts.push(part(
        format!("{station}:base"),
        "frame",
        [-20.0, -20.0, 0.0],
        [240.0, 360.0, 24.0],
    ));
    parts.push(part(
        format!("{station}:lid-seam"),
        "frame",
        [-4.0, -4.0, 180.0],
        [208.0, 328.0, 14.0],
    ));
    parts.push(part(
        format!("{station}:vents"),
        "frame",
        [-12.0, 40.0, 60.0],
        [12.0, 240.0, 90.0],
    ));
    parts.push(part(
        format!("{station}:status-light"),
        "accent",
        [20.0, -6.0, 220.0],
        [160.0, 6.0, 12.0],
    ));
    parts
}

/// The plate reader: a slim body with its tray slot and status light.
fn reader_assembly(station: &str) -> Vec<SceneNode> {
    let mut parts = bench(station, [160.0, 240.0]);
    parts.push(part(
        format!("{station}:tray-slot"),
        "frame",
        [20.0, -6.0, 30.0],
        [120.0, 6.0, 20.0],
    ));
    parts.push(part(
        format!("{station}:status-light"),
        "accent",
        [130.0, -6.0, 90.0],
        [16.0, 6.0, 10.0],
    ));
    parts
}

/// The cosmetic assembly for a station kind. Every kind at least stands
/// on a bench.
pub(crate) fn assembly(station: &str, kind: &str, footprint: [f64; 2]) -> Vec<SceneNode> {
    match kind {
        "hamilton.star" => star_assembly(station),
        "inheco.odtc" => odtc_assembly(station),
        "byonoy.absorbance96" => reader_assembly(station),
        _ => bench(station, footprint),
    }
}
