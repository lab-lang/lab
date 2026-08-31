//! Deterministic source-rack and plate-well allocation primitives.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
    capacity: PlateCapacity,
) -> Result<BTreeMap<String, String>, PlanningError> {
    if keys.len() > capacity.get() {
        return Err(AdapterConstraintError::CapacityExceeded {
            adapter: backend.into(),
            operation: stage.into(),
            subject: "automation_batch".into(),
            resource: "source_rack".into(),
            required: keys.len() as u64,
            capacity: capacity.get() as u64,
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
        let plate = self.cursor / self.plates.capacity.get();
        let well = self.wells[self.cursor % self.plates.capacity.get()].clone();
        self.cursor += 1;
        Ok(Well { plate, well })
    }
}

/// Well counts and their SBS row/column geometry. Capacity alone does not
/// determine a layout, so only formats a backend knows how to address are
/// accepted; an unfamiliar count is rejected when a profile is parsed.
const PLATE_GEOMETRIES: [(usize, usize, usize); 5] = [
    (15, 3, 5),
    (24, 4, 6),
    (48, 6, 8),
    (96, 8, 12),
    (384, 16, 24),
];

/// Every well count this compiler can address, for diagnostics.
pub fn supported_plate_capacities() -> Vec<usize> {
    PLATE_GEOMETRIES
        .into_iter()
        .map(|(wells, _, _)| wells)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "labware capacity {found} has no addressable SBS geometry in this compiler; supported capacities are {supported}"
)]
pub struct UnknownPlateGeometry {
    pub found: usize,
    supported: String,
}

/// A well count this compiler knows how to address.
///
/// Construction is the only gate: a profile that declares an unfamiliar capacity fails to parse,
/// so no planner can hold a capacity it cannot turn into addresses.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "usize", into = "usize")]
#[schemars(schema_with = "plate_capacity_schema")]
pub struct PlateCapacity(usize);

/// Publishes the exact well counts this compiler can address, so a consumer of the adapter-profile
/// schema rejects the same capacities the compiler does.
fn plate_capacity_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let mut schema = generator.subschema_for::<usize>();
    schema
        .ensure_object()
        .insert("enum".to_owned(), supported_plate_capacities().into());
    schema
}

impl PlateCapacity {
    pub fn new(capacity: usize) -> Result<Self, UnknownPlateGeometry> {
        if PLATE_GEOMETRIES
            .into_iter()
            .any(|(wells, _, _)| wells == capacity)
        {
            Ok(Self(capacity))
        } else {
            Err(UnknownPlateGeometry {
                found: capacity,
                supported: supported_plate_capacities()
                    .into_iter()
                    .map(|wells| wells.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            })
        }
    }

    pub fn get(self) -> usize {
        self.0
    }

    /// Column-major well names for this plate.
    pub fn wells(self) -> Vec<String> {
        let (_, rows, columns) = PLATE_GEOMETRIES
            .into_iter()
            .find(|(wells, _, _)| *wells == self.0)
            .expect("a PlateCapacity is only constructed from a known geometry");
        (1..=columns)
            .flat_map(|column| {
                (0..rows).map(move |row| format!("{}{column}", char::from(b'A' + row as u8)))
            })
            .collect()
    }
}

impl TryFrom<usize> for PlateCapacity {
    type Error = UnknownPlateGeometry;

    fn try_from(capacity: usize) -> Result<Self, Self::Error> {
        Self::new(capacity)
    }
}

impl From<PlateCapacity> for usize {
    fn from(capacity: PlateCapacity) -> Self {
        capacity.0
    }
}

impl std::fmt::Display for PlateCapacity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Column-major well names for a plate of the given capacity.
pub(in crate::backend) fn plate_wells(capacity: PlateCapacity) -> Vec<String> {
    capacity.wells()
}

fn source_rack_wells(capacity: PlateCapacity) -> Vec<String> {
    plate_wells(capacity)
}

#[cfg(test)]
mod tests {
    use super::{PlateCapacity, supported_plate_capacities};

    #[test]
    fn known_geometries_address_every_declared_well() {
        for capacity in supported_plate_capacities() {
            let wells = PlateCapacity::new(capacity).unwrap().wells();
            assert_eq!(wells.len(), capacity);
        }
    }

    #[test]
    fn a_capacity_without_a_geometry_cannot_be_constructed() {
        let error = PlateCapacity::new(20).unwrap_err();
        assert_eq!(error.found, 20);
        assert!(error.to_string().contains("15, 24, 48, 96, 384"));
    }

    #[test]
    fn wells_are_column_major() {
        let wells = PlateCapacity::new(96).unwrap().wells();
        assert_eq!(wells[0], "A1");
        assert_eq!(wells[7], "H1");
        assert_eq!(wells[8], "A2");
    }

    #[test]
    fn an_unfamiliar_capacity_fails_to_deserialize() {
        assert!(serde_json::from_str::<PlateCapacity>("20").is_err());
        assert_eq!(
            serde_json::from_str::<PlateCapacity>("96").unwrap(),
            PlateCapacity::new(96).unwrap()
        );
    }
}
