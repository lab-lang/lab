//! The STAR deck scene: the same composition chain the planner uses —
//! deck origin, carrier at rail, site offset, labware layout, well — as a
//! node tree.
//!
//! Positions are exact: every well's local offset is derived from the
//! catalog's own `well_position` composition, never re-derived here.
//! Labware nodes carry the plan's resource names so trace events bind to
//! them directly.

use std::collections::BTreeMap;

use lab_compiler::backend::hamilton::star::catalog::{
    self, CARRIER_ORIGIN_Y, CARRIER_ORIGIN_Z, RAIL_ONE_X, RAIL_PITCH,
};
use lab_compiler::backend::hamilton::star::profile::StarTargetProfile;

use crate::dims;
use crate::scene::{Geometry, SceneError, SceneNode, Semantic};

/// One physical placement the profile claims: the plan's resource name,
/// the `<carrier>/<site>` address, and the labware catalog id.
struct Placement {
    resource: String,
    address: String,
    labware: String,
}

/// Every labware placement a profile makes, under the same resource-name
/// convention the planner's deck index uses (`dna_plate/1`, ...). The two
/// deck-level placements keep their field names.
fn placements(profile: &StarTargetProfile) -> Vec<Placement> {
    let mut out = vec![
        Placement {
            resource: "source_rack".to_string(),
            address: profile.deck.source_rack.site.clone(),
            labware: profile.deck.source_rack.labware.clone(),
        },
        Placement {
            resource: "reaction_plate".to_string(),
            address: profile.deck.reaction_plate.site.clone(),
            labware: profile.deck.reaction_plate.labware.clone(),
        },
    ];
    fn family(out: &mut Vec<Placement>, prefix: &str, labware: &str, slots: &[String]) {
        for (index, slot) in slots.iter().enumerate() {
            out.push(Placement {
                resource: format!("{prefix}/{}", index + 1),
                address: slot.clone(),
                labware: labware.to_string(),
            });
        }
    }
    let stages = &profile.stages;
    family(
        &mut out,
        "assembly_small_tips",
        &stages.assembly.small_tips.labware,
        &stages.assembly.small_tips.slots,
    );
    family(
        &mut out,
        "dna_plate",
        &stages.transformation.dna_plate.labware,
        &stages.transformation.dna_plate.slots,
    );
    family(
        &mut out,
        "transformation_small_tips",
        &stages.transformation.small_tips.labware,
        &stages.transformation.small_tips.slots,
    );
    family(
        &mut out,
        "transformation_large_tips",
        &stages.transformation.large_tips.labware,
        &stages.transformation.large_tips.slots,
    );
    family(
        &mut out,
        "dilution_plate",
        &stages.plating.dilution_plate.labware,
        &stages.plating.dilution_plate.slots,
    );
    family(
        &mut out,
        "agar_plate",
        &stages.plating.agar_plate.labware,
        &stages.plating.agar_plate.slots,
    );
    family(
        &mut out,
        "plating_small_tips",
        &stages.plating.small_tips.labware,
        &stages.plating.small_tips.slots,
    );
    family(
        &mut out,
        "plating_large_tips",
        &stages.plating.large_tips.labware,
        &stages.plating.large_tips.slots,
    );
    out.push(Placement {
        resource: "media_rack".to_string(),
        address: stages.plating.media_rack.slot.clone(),
        labware: stages.plating.media_rack.labware.clone(),
    });
    out
}

/// Builds the deck node for one STAR bench: deck plate, carriers at their
/// rails, sites, labware with every well as a child cylinder.
pub fn star_deck_scene(profile: &StarTargetProfile) -> Result<SceneNode, SceneError> {
    let rails = f64::from(profile.machine.variant.rails());
    let mut deck = SceneNode::new(
        "deck",
        Semantic::Deck,
        [
            RAIL_ONE_X - RAIL_PITCH,
            CARRIER_ORIGIN_Y - 40.0,
            CARRIER_ORIGIN_Z - dims::DECK_THICKNESS_MM,
        ],
    )
    .with_geometry(Geometry::Box {
        x: (rails + 2.0) * RAIL_PITCH,
        y: dims::DECK_DEPTH_MM,
        z: dims::DECK_THICKNESS_MM,
    });
    // The deck node's children are positioned in the lab frame, so undo
    // the deck plate's own offset once instead of at every child.
    let deck_origin = deck.translation;

    // Carrier nodes, keyed by the profile-local carrier name.
    let mut carriers: BTreeMap<String, SceneNode> = BTreeMap::new();
    for (name, placement) in &profile.deck.carriers {
        let definition =
            catalog::carrier(&placement.catalog).ok_or_else(|| SceneError::UnknownCarrier {
                name: name.clone(),
                catalog: placement.catalog.clone(),
            })?;
        let origin = [
            RAIL_ONE_X + f64::from(placement.rail - 1) * RAIL_PITCH,
            CARRIER_ORIGIN_Y,
            CARRIER_ORIGIN_Z,
        ];
        let extent = dims::carrier_extent(definition);
        let node = SceneNode::new(
            name.clone(),
            Semantic::Carrier {
                catalog: placement.catalog.clone(),
            },
            [
                origin[0] - deck_origin[0],
                origin[1] - deck_origin[1],
                origin[2] - deck_origin[2],
            ],
        )
        .with_geometry(Geometry::Box {
            x: extent[0],
            y: extent[1],
            z: dims::CARRIER_HEIGHT_MM,
        });
        carriers.insert(name.clone(), node);
    }

    // Labware onto carrier sites. Two stage aliases can claim one physical
    // site (the source rack serves assembly and transformation), so the
    // first claim renders and the rest are the same object.
    let mut occupied: BTreeMap<(String, usize), String> = BTreeMap::new();
    for placement in placements(profile) {
        let Some((carrier_name, _)) = placement.address.split_once('/') else {
            return Err(SceneError::BadSiteAddress {
                address: placement.address.clone(),
            });
        };
        let resolved = profile
            .resolve_labware(&placement.resource, &placement.address, &placement.labware)
            .map_err(|error| SceneError::Resolve {
                context: placement.resource.clone(),
                message: error.to_string(),
            })?;
        if occupied
            .insert(
                (carrier_name.to_string(), resolved.site),
                placement.resource.clone(),
            )
            .is_some()
        {
            continue;
        }

        let site_offset = resolved.carrier.sites[resolved.site];
        let labware_extent = dims::labware_extent(resolved.labware);
        let mut labware_node = SceneNode::new(
            placement.resource.clone(),
            Semantic::Labware {
                catalog: resolved.labware.id.to_string(),
            },
            [site_offset.x, site_offset.y, site_offset.z],
        )
        .with_geometry(Geometry::Box {
            x: labware_extent[0],
            y: labware_extent[1],
            z: labware_extent[2],
        });

        // Well positions come from the catalog's own composition: the
        // absolute position minus the labware origin is the local offset.
        let labware_origin = [
            RAIL_ONE_X + f64::from(resolved.rail - 1) * RAIL_PITCH + site_offset.x,
            CARRIER_ORIGIN_Y + site_offset.y,
            CARRIER_ORIGIN_Z + site_offset.z,
        ];
        if let Some((rows, columns)) = catalog::grid_for_capacity(resolved.labware.capacity) {
            let diameter = dims::well_diameter(resolved.labware);
            let height = dims::well_height(resolved.labware);
            for column in 0..columns {
                for row in 0..rows {
                    let name = format!("{}{}", (b'A' + row as u8) as char, column + 1);
                    let Some(position) = resolved.well(&name) else {
                        continue;
                    };
                    labware_node.children.push(
                        SceneNode::new(
                            format!("{}:{name}", placement.resource),
                            Semantic::Well { name: name.clone() },
                            [
                                position.x - labware_origin[0],
                                position.y - labware_origin[1],
                                position.z - labware_origin[2],
                            ],
                        )
                        .with_geometry(Geometry::Cylinder { diameter, height }),
                    );
                }
            }
        }

        let site_node = SceneNode::new(
            format!("{}:site-{}", carrier_name, resolved.site + 1),
            Semantic::Site {
                index: resolved.site,
            },
            [0.0, 0.0, 0.0],
        );
        let carrier_node =
            carriers
                .get_mut(carrier_name)
                .ok_or_else(|| SceneError::BadSiteAddress {
                    address: placement.address.clone(),
                })?;
        let mut site_node = site_node;
        site_node.children.push(labware_node);
        carrier_node.children.push(site_node);
    }

    deck.children.extend(carriers.into_values());
    Ok(deck)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Semantic;

    const PROFILE: &str = include_str!("../../../examples/golden-gate/targets/hamilton-star.toml");

    #[test]
    fn the_scene_reproduces_the_catalogs_well_positions_exactly() {
        let profile = StarTargetProfile::parse("hamilton-star", PROFILE).unwrap();
        let deck = star_deck_scene(&profile).unwrap();

        // Find the reaction plate's A1 well by accumulating translations.
        let mut found = None;
        deck.walk(&mut |node, origin| {
            if node.id == "reaction_plate:A1" {
                found = Some(origin);
            }
        });
        let scene_a1 = found.expect("the reaction plate renders its wells");

        let resolved = profile
            .resolve_labware(
                "test",
                &profile.deck.reaction_plate.site,
                &profile.deck.reaction_plate.labware,
            )
            .unwrap();
        let expected = resolved.well("A1").unwrap();
        // The deck node's children live in the lab frame; walk() started
        // from the deck's own translation, so origins are absolute.
        assert!(
            (scene_a1[0] - expected.x).abs() < 1e-9
                && (scene_a1[1] - expected.y).abs() < 1e-9
                && (scene_a1[2] - expected.z).abs() < 1e-9,
            "scene {scene_a1:?} vs catalog ({}, {}, {})",
            expected.x,
            expected.y,
            expected.z
        );
    }

    #[test]
    fn every_placement_renders_once_with_plan_resource_names() {
        let profile = StarTargetProfile::parse("hamilton-star", PROFILE).unwrap();
        let deck = star_deck_scene(&profile).unwrap();
        let mut labware_ids = Vec::new();
        deck.walk(&mut |node, _| {
            if matches!(node.semantic, Semantic::Labware { .. }) {
                labware_ids.push(node.id.clone());
            }
        });
        assert!(
            labware_ids.contains(&"reaction_plate".to_string()),
            "plan resource names are node ids: {labware_ids:?}"
        );
        let mut sorted = labware_ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), labware_ids.len(), "no site renders twice");
    }
}
