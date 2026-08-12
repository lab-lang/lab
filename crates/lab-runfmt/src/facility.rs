//! A facility describes a lab as it stands: the stations on its benches,
//! the storage that holds its stock, its consumables, and how labware
//! travels between stations.
//!
//! One facility serves every package that runs in that lab, and one package
//! can be simulated against several candidate facilities — the relationship
//! is many-to-many, so the description lives in its own file under
//! `facilities/` and a manifest carries at most a pointer to a default.
//! Station addresses stay runtime input, exactly as they are for `lab run`.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The directory a package's facility files live under.
pub const FACILITY_DIR: &str = "facilities";

#[derive(Debug, thiserror::Error)]
pub enum FacilityError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not a valid facility description: {source}")]
    Parse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("facility '{facility}' declares station '{station}' twice")]
    DuplicateStation { facility: String, station: String },
    #[error("facility '{facility}' declares storage '{storage}' twice")]
    DuplicateStorage { facility: String, storage: String },
    #[error(
        "facility '{facility}' transport is '{found}'; this runtime supports 'human' transport only"
    )]
    UnsupportedTransport { facility: String, found: String },
    #[error(
        "facility '{facility}' walks between stations in {seconds} s; travel time must be positive"
    )]
    NonPositiveWalk { facility: String, seconds: f64 },
    #[error(
        "the plan needs station '{station}' of kind '{kind}', which facility '{facility}' does not have{hint}"
    )]
    MissingStation {
        facility: String,
        station: String,
        kind: String,
        hint: String,
    },
    #[error(
        "facility '{facility}' places station '{station}' at ({x}, {y}) mm, outside its {width} x {depth} mm room"
    )]
    StationOutsideRoom {
        facility: String,
        station: String,
        x: f64,
        y: f64,
        width: f64,
        depth: f64,
    },
    #[error(
        "facility '{facility}' declares a {width} x {depth} mm room; both extents must be positive"
    )]
    EmptyRoom {
        facility: String,
        width: f64,
        depth: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Facility {
    pub facility: FacilityMetadata,
    /// The room the stations stand in; absent for a bare bench.
    #[serde(default)]
    pub room: Option<Room>,
    #[serde(default, rename = "station")]
    pub stations: Vec<FacilityStation>,
    #[serde(default, rename = "storage")]
    pub storage: Vec<StorageUnit>,
    #[serde(default, rename = "consumable")]
    pub consumables: Vec<Consumable>,
    #[serde(default)]
    pub transport: FacilityTransport,
}

/// The room an idealized facility is laid out in: a floor plan for the
/// scene and the bounds station positions are checked against.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Room {
    pub width_mm: f64,
    pub depth_mm: f64,
    #[serde(default = "default_room_height")]
    pub height_mm: f64,
    /// An asset key for a modeled or scanned environment; without one the
    /// scene renders a kit room (floor and walls) at these dimensions.
    #[serde(default)]
    pub environment: Option<String>,
}

fn default_room_height() -> f64 {
    3000.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FacilityMetadata {
    pub name: String,
}

/// One instrument the facility has, in the same vocabulary workcell
/// profiles use for stations.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FacilityStation {
    pub name: String,
    /// The station kind string, e.g. `hamilton.star` or `inheco.odtc`.
    pub kind: String,
    /// The target profile this station's bench is described by, when one
    /// exists under `targets/`.
    #[serde(default)]
    pub profile: Option<String>,
    /// Where the station answers on this bench; runtime input, never
    /// compiled into artifacts.
    #[serde(default)]
    pub address: Option<String>,
    /// Floor position of the station's origin, in room millimeters.
    /// Stations without one are laid out in declaration order.
    #[serde(default)]
    pub position_mm: Option<[f64; 2]>,
    /// Counterclockwise rotation about the station's origin.
    #[serde(default)]
    pub rotation_deg: Option<f64>,
}

/// One storage location and the stock it holds. Stock is inventory state,
/// named by the symbolic identities source declares.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageUnit {
    pub name: String,
    /// `fridge`, `freezer`, or `shelf`.
    pub kind: String,
    #[serde(default)]
    pub temperature_c: Option<f64>,
    #[serde(default)]
    pub materials: BTreeSet<String>,
    #[serde(default)]
    pub artifacts: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Consumable {
    /// The labware catalog id, e.g. `tip_rack_300ul`.
    pub labware: String,
    pub count: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FacilityTransport {
    /// How labware moves between stations. `human` is the only supported
    /// mode; an arm is a future station kind, not a transport string.
    pub between: String,
    /// How long one human handoff takes in this facility, door to door:
    /// seal, carry, seat, confirm.
    pub walk_seconds: f64,
}

impl Default for FacilityTransport {
    fn default() -> Self {
        Self {
            between: "human".to_string(),
            walk_seconds: 90.0,
        }
    }
}

impl Facility {
    pub fn parse(path_label: &str, text: &str) -> Result<Self, FacilityError> {
        let facility: Facility = toml::from_str(text).map_err(|source| FacilityError::Parse {
            path: path_label.to_string(),
            source,
        })?;
        facility.validate()?;
        Ok(facility)
    }

    fn validate(&self) -> Result<(), FacilityError> {
        if let Some(room) = &self.room
            && (room.width_mm <= 0.0 || room.depth_mm <= 0.0)
        {
            return Err(FacilityError::EmptyRoom {
                facility: self.facility.name.clone(),
                width: room.width_mm,
                depth: room.depth_mm,
            });
        }
        let mut station_names = BTreeSet::new();
        for station in &self.stations {
            if !station_names.insert(station.name.as_str()) {
                return Err(FacilityError::DuplicateStation {
                    facility: self.facility.name.clone(),
                    station: station.name.clone(),
                });
            }
            if let (Some(room), Some([x, y])) = (&self.room, &station.position_mm)
                && (!(0.0..=room.width_mm).contains(x) || !(0.0..=room.depth_mm).contains(y))
            {
                return Err(FacilityError::StationOutsideRoom {
                    facility: self.facility.name.clone(),
                    station: station.name.clone(),
                    x: *x,
                    y: *y,
                    width: room.width_mm,
                    depth: room.depth_mm,
                });
            }
        }
        let mut storage_names = BTreeSet::new();
        for storage in &self.storage {
            if !storage_names.insert(storage.name.as_str()) {
                return Err(FacilityError::DuplicateStorage {
                    facility: self.facility.name.clone(),
                    storage: storage.name.clone(),
                });
            }
        }
        if self.transport.between != "human" {
            return Err(FacilityError::UnsupportedTransport {
                facility: self.facility.name.clone(),
                found: self.transport.between.clone(),
            });
        }
        if self.transport.walk_seconds <= 0.0 {
            return Err(FacilityError::NonPositiveWalk {
                facility: self.facility.name.clone(),
                seconds: self.transport.walk_seconds,
            });
        }
        Ok(())
    }

    pub fn station(&self, name: &str) -> Option<&FacilityStation> {
        self.stations.iter().find(|station| station.name == name)
    }

    /// Every material stocked anywhere in the facility.
    pub fn stocked_materials(&self) -> BTreeSet<String> {
        self.storage
            .iter()
            .flat_map(|unit| unit.materials.iter().cloned())
            .collect()
    }

    /// Every artifact held anywhere in the facility.
    pub fn stocked_artifacts(&self) -> BTreeSet<String> {
        self.storage
            .iter()
            .flat_map(|unit| unit.artifacts.iter().cloned())
            .collect()
    }

    /// Checks that every station a plan needs exists here, by name and
    /// kind. A near-miss names the kind mismatch instead of a bare absence.
    pub fn check_stations(&self, required: &[crate::WorkcellStation]) -> Result<(), FacilityError> {
        for needed in required {
            match self.station(&needed.name) {
                Some(station) if station.kind == needed.kind => {}
                Some(station) => {
                    return Err(FacilityError::MissingStation {
                        facility: self.facility.name.clone(),
                        station: needed.name.clone(),
                        kind: needed.kind.clone(),
                        hint: format!("; its station '{}' is a '{}'", station.name, station.kind),
                    });
                }
                None => {
                    return Err(FacilityError::MissingStation {
                        facility: self.facility.name.clone(),
                        station: needed.name.clone(),
                        kind: needed.kind.clone(),
                        hint: String::new(),
                    });
                }
            }
        }
        Ok(())
    }
}

/// Loads and validates one facility file.
pub fn load_facility(path: &Path) -> Result<Facility, FacilityError> {
    let text = std::fs::read_to_string(path).map_err(|source| FacilityError::Io {
        path: path.display().to_string(),
        source,
    })?;
    Facility::parse(&path.display().to_string(), &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN_BENCH: &str = r#"
[facility]
name = "main-bench"

[[station]]
name = "star-1"
kind = "hamilton.star"
profile = "hamilton-star"

[[station]]
name = "odtc-1"
kind = "inheco.odtc"
address = "169.254.10.40:8080"

[[storage]]
name = "fridge-a"
kind = "fridge"
temperature_c = 4.0
materials = ["assembly_mix", "competent_cells"]

[[consumable]]
labware = "tip_rack_300ul"
count = 12

[transport]
between = "human"
walk_seconds = 45.0
"#;

    #[test]
    fn a_facility_parses_and_answers_stock_and_station_questions() {
        let facility = Facility::parse("main-bench.toml", MAIN_BENCH).unwrap();
        assert_eq!(facility.facility.name, "main-bench");
        assert_eq!(facility.station("odtc-1").unwrap().kind, "inheco.odtc");
        assert!(facility.stocked_materials().contains("assembly_mix"));
        assert_eq!(facility.transport.walk_seconds, 45.0);

        let plan_stations = vec![
            crate::WorkcellStation {
                name: "star-1".to_string(),
                kind: "hamilton.star".to_string(),
                program_dir: "stations/star-1".to_string(),
            },
            crate::WorkcellStation {
                name: "odtc-1".to_string(),
                kind: "inheco.odtc".to_string(),
                program_dir: "stations/odtc-1".to_string(),
            },
        ];
        facility.check_stations(&plan_stations).unwrap();

        let elsewhere = vec![crate::WorkcellStation {
            name: "reader-1".to_string(),
            kind: "byonoy.absorbance96".to_string(),
            program_dir: "stations/reader-1".to_string(),
        }];
        let error = facility.check_stations(&elsewhere).unwrap_err();
        assert!(
            error.to_string().contains("reader-1"),
            "the missing station is named: {error}"
        );
    }

    #[test]
    fn a_kind_mismatch_is_named_rather_than_reported_as_absence() {
        let facility = Facility::parse("main-bench.toml", MAIN_BENCH).unwrap();
        let mismatched = vec![crate::WorkcellStation {
            name: "odtc-1".to_string(),
            kind: "byonoy.absorbance96".to_string(),
            program_dir: "stations/odtc-1".to_string(),
        }];
        let error = facility.check_stations(&mismatched).unwrap_err();
        assert!(
            error.to_string().contains("is a 'inheco.odtc'"),
            "the near-miss names the actual kind: {error}"
        );
    }

    #[test]
    fn validation_rejects_duplicates_and_unknown_transport() {
        let duplicated =
            format!("{MAIN_BENCH}\n[[station]]\nname = \"star-1\"\nkind = \"hamilton.star\"\n");
        assert!(matches!(
            Facility::parse("f.toml", &duplicated),
            Err(FacilityError::DuplicateStation { .. })
        ));

        let arm = MAIN_BENCH.replace("between = \"human\"", "between = \"arm\"");
        assert!(matches!(
            Facility::parse("f.toml", &arm),
            Err(FacilityError::UnsupportedTransport { .. })
        ));
    }

    #[test]
    fn a_room_bounds_the_stations_placed_in_it() {
        let placed = format!("{MAIN_BENCH}\n[room]\nwidth_mm = 8000.0\ndepth_mm = 6000.0\n");
        let facility = Facility::parse("f.toml", &placed).expect("unplaced stations are fine");
        assert_eq!(facility.room.as_ref().unwrap().height_mm, 3000.0);

        let inside = placed.replace(
            "name = \"star-1\"\nkind = \"hamilton.star\"",
            "name = \"star-1\"\nkind = \"hamilton.star\"\nposition_mm = [1200.0, 2400.0]\nrotation_deg = 90.0",
        );
        let facility = Facility::parse("f.toml", &inside).unwrap();
        let star = facility.station("star-1").unwrap();
        assert_eq!(star.position_mm, Some([1200.0, 2400.0]));
        assert_eq!(star.rotation_deg, Some(90.0));

        let outside = placed.replace(
            "name = \"star-1\"\nkind = \"hamilton.star\"",
            "name = \"star-1\"\nkind = \"hamilton.star\"\nposition_mm = [9000.0, 2400.0]",
        );
        assert!(matches!(
            Facility::parse("f.toml", &outside),
            Err(FacilityError::StationOutsideRoom { .. })
        ));
    }

    #[test]
    fn a_misspelled_key_must_not_silently_vanish() {
        let misspelled = MAIN_BENCH.replace("materials =", "material =");
        assert!(matches!(
            Facility::parse("f.toml", &misspelled),
            Err(FacilityError::Parse { .. })
        ));
    }
}
