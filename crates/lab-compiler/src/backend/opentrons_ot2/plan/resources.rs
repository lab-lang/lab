//! Deterministic OT-2 source-rack and plate-well allocation primitives.

use std::collections::{BTreeMap, BTreeSet};

use crate::backend::TargetConstraintError;
use crate::backend::opentrons_ot2::profile::Plates;

use super::execution::Ot2Well;
use super::trace::{AssemblyTrace, StrainTrace};
use super::{Ot2PlanningError, TARGET};

pub(super) fn assembly_source_keys(
    traces: &[AssemblyTrace],
    context: &pliron::context::Context,
) -> BTreeSet<String> {
    let mut keys = BTreeSet::from([
        "reagent:nuclease_free_water".into(),
        "reagent:T4_DNA_ligase".into(),
        "reagent:T4_DNA_ligase_buffer".into(),
    ]);
    for trace in traces {
        keys.insert(format!("dna:{}", trace.backbone(context)));
        keys.extend(
            trace
                .components(context)
                .iter()
                .map(|component| format!("dna:{component}")),
        );
        keys.insert(format!("enzyme:{}", trace.restriction_enzyme(context)));
    }
    keys
}

pub(super) fn transformation_source_keys(
    traces: &[StrainTrace],
    context: &pliron::context::Context,
) -> BTreeSet<String> {
    let mut keys = BTreeSet::from(["reagent:recovery_medium".into()]);
    keys.extend(
        traces
            .iter()
            .map(|trace| format!("cells:{}", trace.host(context))),
    );
    keys
}

pub(super) fn assign_source_wells(
    stage: &'static str,
    keys: BTreeSet<String>,
    capacity: usize,
) -> Result<BTreeMap<String, String>, Ot2PlanningError> {
    if keys.len() > capacity {
        return Err(TargetConstraintError::CapacityExceeded {
            target: TARGET.into(),
            operation: stage.into(),
            subject: "automation_batch".into(),
            resource: "source_rack".into(),
            required: keys.len() as u64,
            capacity: capacity as u64,
            unit: "wells".into(),
        }
        .into());
    }
    Ok(keys
        .into_iter()
        .zip(source_rack_wells(capacity))
        .collect::<BTreeMap<_, _>>())
}

/// Hands out wells across every plate a stage declares, filling each in turn.
pub(super) struct PlateAllocator<'a> {
    stage: &'static str,
    resource: &'static str,
    plates: &'a Plates,
    wells: Vec<String>,
    cursor: usize,
}

impl<'a> PlateAllocator<'a> {
    pub(super) fn new(stage: &'static str, resource: &'static str, plates: &'a Plates) -> Self {
        Self {
            stage,
            resource,
            wells: plate_wells(plates.capacity),
            plates,
            cursor: 0,
        }
    }

    pub(super) fn take(&mut self, count: usize) -> Result<Vec<Ot2Well>, Ot2PlanningError> {
        (0..count).map(|_| self.next_well()).collect()
    }

    fn next_well(&mut self) -> Result<Ot2Well, Ot2PlanningError> {
        let capacity = self.plates.total_capacity();
        if self.cursor >= capacity {
            return Err(TargetConstraintError::CapacityExceeded {
                target: TARGET.into(),
                operation: self.stage.into(),
                subject: "automation_batch".into(),
                resource: self.resource.into(),
                required: (self.cursor + 1) as u64,
                capacity: capacity as u64,
                unit: "wells".into(),
            }
            .into());
        }
        let plate = self.cursor / self.plates.capacity;
        let well = self.wells[self.cursor % self.plates.capacity].clone();
        self.cursor += 1;
        Ok(Ot2Well { plate, well })
    }
}

/// Well counts and their SBS row/column geometry. Capacity alone does not
/// determine a layout, so only formats this backend knows how to address are
/// accepted; an unfamiliar count is a planning error rather than a guess.
const PLATE_GEOMETRIES: [(usize, usize, usize); 5] = [
    (15, 3, 5),
    (24, 4, 6),
    (48, 6, 8),
    (96, 8, 12),
    (384, 16, 24),
];

/// Column-major well names for a plate of the given capacity.
pub(super) fn plate_wells(capacity: usize) -> Vec<String> {
    let Some((_, rows, columns)) = PLATE_GEOMETRIES
        .into_iter()
        .find(|(wells, _, _)| *wells == capacity)
    else {
        return Vec::new();
    };
    (1..=columns)
        .flat_map(|column| {
            (0..rows).map(move |row| format!("{}{column}", char::from(b'A' + row as u8)))
        })
        .collect()
}

pub(super) fn require_known_geometry(
    resource: &'static str,
    capacity: usize,
) -> Result<(), Ot2PlanningError> {
    if plate_wells(capacity).is_empty() {
        return Err(Ot2PlanningError::InvalidProtocol(format!(
            "target profile gives {resource} {capacity} wells, which is not a labware format this backend can address"
        )));
    }
    Ok(())
}

fn source_rack_wells(capacity: usize) -> Vec<String> {
    plate_wells(capacity)
}
