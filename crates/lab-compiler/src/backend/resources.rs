//! Deterministic source-rack and plate-well allocation primitives.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::backend::AdapterConstraintError;

use crate::backend::error::PlanningError;
use crate::backend::profile::Plates;

/// A well on one of the plates a stage may hold several of. `plate` indexes
/// the stage's declared slot list, so adding a slot to adapter configuration raises
/// the build's capacity without changing any address already assigned.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Well {
    pub plate: usize,
    pub well: String,
}

pub(in crate::backend) fn assign_source_wells(
    backend: &'static str,
    stage: &'static str,
    keys: BTreeSet<String>,
    capacity: usize,
) -> Result<BTreeMap<String, String>, PlanningError> {
    if keys.len() > capacity {
        return Err(AdapterConstraintError::CapacityExceeded {
            adapter: backend.into(),
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
pub(in crate::backend) struct PlateAllocator<'a> {
    backend: &'static str,
    stage: &'static str,
    resource: &'static str,
    plates: &'a Plates,
    wells: Vec<String>,
    cursor: usize,
}

impl<'a> PlateAllocator<'a> {
    pub(in crate::backend) fn new(
        backend: &'static str,
        stage: &'static str,
        resource: &'static str,
        plates: &'a Plates,
    ) -> Self {
        Self {
            backend,
            stage,
            resource,
            wells: plate_wells(plates.capacity),
            plates,
            cursor: 0,
        }
    }

    pub(in crate::backend) fn take(&mut self, count: usize) -> Result<Vec<Well>, PlanningError> {
        (0..count).map(|_| self.next_well()).collect()
    }

    fn next_well(&mut self) -> Result<Well, PlanningError> {
        let capacity = self.plates.total_capacity();
        if self.cursor >= capacity {
            return Err(AdapterConstraintError::CapacityExceeded {
                adapter: self.backend.into(),
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
        Ok(Well { plate, well })
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
pub(in crate::backend) fn plate_wells(capacity: usize) -> Vec<String> {
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

fn source_rack_wells(capacity: usize) -> Vec<String> {
    plate_wells(capacity)
}
