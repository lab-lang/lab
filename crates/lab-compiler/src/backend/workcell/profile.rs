//! Deserializable shape of a workcell target profile: the stations a bench
//! composes and the transport between them.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::backend::workcell::BACKEND;

/// A workcell: one liquid handler, the instruments beside it, and a human
/// carrying labware between them. Station machine configuration is not
/// repeated here — a robot station names an existing single-machine target
/// profile, and instrument stations carry only bench properties.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkcellProfile {
    #[serde(default)]
    pub target: TargetMetadata,
    #[serde(rename = "station")]
    pub stations: Vec<StationDecl>,
    #[serde(default)]
    pub transport: Transport,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetMetadata {
    /// The workcell this profile describes, supplied by the loader from the
    /// profile's filename.
    #[serde(skip_deserializing, default)]
    pub name: String,
    #[serde(default = "default_backend")]
    pub backend: String,
}

impl Default for TargetMetadata {
    fn default() -> Self {
        Self {
            name: String::new(),
            backend: default_backend(),
        }
    }
}

fn default_backend() -> String {
    BACKEND.to_string()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StationDecl {
    pub name: String,
    pub kind: StationKind,
    /// For a robot station: the single-machine target profile it runs,
    /// named the way targets are (`targets/<profile>.toml`).
    #[serde(default)]
    pub profile: Option<String>,
    /// For a networked instrument: where it answers on this bench. A bench
    /// property only — compiled artifacts never depend on it.
    #[serde(default)]
    pub address: Option<String>,
}

/// The station kinds this toolchain can plan for. The kind fixes the
/// station's capabilities; assignment is deterministic over kinds rather
/// than negotiated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StationKind {
    #[serde(rename = "hamilton.star")]
    HamiltonStar,
    #[serde(rename = "inheco.odtc")]
    InhecoOdtc,
    #[serde(rename = "byonoy.absorbance96")]
    ByonoyAbsorbance96,
}

impl StationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HamiltonStar => "hamilton.star",
            Self::InhecoOdtc => "inheco.odtc",
            Self::ByonoyAbsorbance96 => "byonoy.absorbance96",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transport {
    #[serde(default = "default_transport")]
    pub between: String,
}

impl Default for Transport {
    fn default() -> Self {
        Self {
            between: default_transport(),
        }
    }
}

fn default_transport() -> String {
    "human".to_string()
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WorkcellProfileError {
    #[error("failed to parse workcell profile: {0}")]
    Toml(String),
    #[error("target profile declares backend '{found}', but this backend is '{expected}'")]
    WrongBackend {
        expected: &'static str,
        found: String,
    },
    #[error("a workcell declares at least one station")]
    NoStations,
    #[error("station name '{name}' is declared twice; station names identify handoff endpoints")]
    DuplicateStation { name: String },
    #[error(
        "a workcell needs exactly one liquid-handler station (kind 'hamilton.star'), found {found}"
    )]
    LiquidHandlerCount { found: usize },
    #[error(
        "station '{name}' is the liquid handler but names no profile; add profile = \"<target>\" pointing at its single-machine target"
    )]
    MissingStationProfile { name: String },
    #[error("a workcell declares at most one '{kind}' station, found {found}")]
    DuplicateInstrument { kind: &'static str, found: usize },
    #[error(
        "transport between stations is '{found}'; a human carrying labware is the only transport this toolchain plans for"
    )]
    UnsupportedTransport { found: String },
}

impl WorkcellProfile {
    /// Parses and validates a profile. The name comes from the loader — a
    /// profile is selected as `targets/<name>.toml`, so the file cannot
    /// disagree with its own name.
    pub fn parse(name: &str, text: &str) -> Result<Self, WorkcellProfileError> {
        let mut profile: Self =
            toml::from_str(text).map_err(|error| WorkcellProfileError::Toml(error.to_string()))?;
        profile.target.name = name.to_owned();
        profile.validate()?;
        Ok(profile)
    }

    fn validate(&self) -> Result<(), WorkcellProfileError> {
        if self.target.backend != BACKEND {
            return Err(WorkcellProfileError::WrongBackend {
                expected: BACKEND,
                found: self.target.backend.clone(),
            });
        }
        if self.stations.is_empty() {
            return Err(WorkcellProfileError::NoStations);
        }
        let mut seen = std::collections::BTreeSet::new();
        for station in &self.stations {
            if !seen.insert(station.name.as_str()) {
                return Err(WorkcellProfileError::DuplicateStation {
                    name: station.name.clone(),
                });
            }
        }
        let handlers: Vec<&StationDecl> = self
            .stations
            .iter()
            .filter(|station| station.kind == StationKind::HamiltonStar)
            .collect();
        if handlers.len() != 1 {
            return Err(WorkcellProfileError::LiquidHandlerCount {
                found: handlers.len(),
            });
        }
        if handlers[0].profile.is_none() {
            return Err(WorkcellProfileError::MissingStationProfile {
                name: handlers[0].name.clone(),
            });
        }
        for kind in [StationKind::InhecoOdtc, StationKind::ByonoyAbsorbance96] {
            let count = self
                .stations
                .iter()
                .filter(|station| station.kind == kind)
                .count();
            if count > 1 {
                return Err(WorkcellProfileError::DuplicateInstrument {
                    kind: kind.as_str(),
                    found: count,
                });
            }
        }
        if self.transport.between != "human" {
            return Err(WorkcellProfileError::UnsupportedTransport {
                found: self.transport.between.clone(),
            });
        }
        Ok(())
    }

    /// The single liquid-handler station.
    pub fn liquid_handler(&self) -> &StationDecl {
        self.stations
            .iter()
            .find(|station| station.kind == StationKind::HamiltonStar)
            .expect("validation guaranteed exactly one liquid handler")
    }

    /// The thermocycler station, when the workcell has one.
    pub fn thermocycler(&self) -> Option<&StationDecl> {
        self.stations
            .iter()
            .find(|station| station.kind == StationKind::InhecoOdtc)
    }

    /// The plate-reader station, when the workcell has one.
    pub fn reader(&self) -> Option<&StationDecl> {
        self.stations
            .iter()
            .find(|station| station.kind == StationKind::ByonoyAbsorbance96)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"
[target]
backend = "workcell"

[[station]]
name = "star-1"
kind = "hamilton.star"
profile = "hamilton-star"

[[station]]
name = "odtc-1"
kind = "inheco.odtc"
address = "192.168.1.40:8080"

[[station]]
name = "reader-1"
kind = "byonoy.absorbance96"

[transport]
between = "human"
"#;

    #[test]
    fn a_full_workcell_parses_and_names_its_stations() {
        let profile = WorkcellProfile::parse("bench-cell", FULL).expect("the profile is valid");
        assert_eq!(profile.target.name, "bench-cell");
        assert_eq!(profile.liquid_handler().name, "star-1");
        assert_eq!(
            profile.liquid_handler().profile.as_deref(),
            Some("hamilton-star")
        );
        assert_eq!(
            profile.thermocycler().map(|s| s.name.as_str()),
            Some("odtc-1")
        );
        assert_eq!(profile.reader().map(|s| s.name.as_str()), Some("reader-1"));
    }

    #[test]
    fn rejects_a_profile_written_for_another_backend() {
        let error = WorkcellProfile::parse("cell", "[target]\nbackend = \"hamilton.star\"\n")
            .expect_err("this backend compiles only its own profiles");
        assert!(error.to_string().contains(BACKEND), "{error}");
    }

    #[test]
    fn rejects_a_workcell_without_exactly_one_liquid_handler() {
        let error = WorkcellProfile::parse(
            "cell",
            "[[station]]\nname = \"odtc-1\"\nkind = \"inheco.odtc\"\n",
        )
        .expect_err("an instrument alone is not a workcell");
        assert_eq!(error, WorkcellProfileError::LiquidHandlerCount { found: 0 });
    }

    #[test]
    fn rejects_a_liquid_handler_without_a_profile_reference() {
        let error = WorkcellProfile::parse(
            "cell",
            "[[station]]\nname = \"star-1\"\nkind = \"hamilton.star\"\n",
        )
        .expect_err("the liquid handler must name its machine profile");
        assert_eq!(
            error,
            WorkcellProfileError::MissingStationProfile {
                name: "star-1".into()
            }
        );
    }

    #[test]
    fn rejects_duplicate_station_names_and_duplicate_instruments() {
        let twice = r#"
[[station]]
name = "star-1"
kind = "hamilton.star"
profile = "hamilton-star"

[[station]]
name = "star-1"
kind = "inheco.odtc"
"#;
        assert_eq!(
            WorkcellProfile::parse("cell", twice).expect_err("names collide"),
            WorkcellProfileError::DuplicateStation {
                name: "star-1".into()
            }
        );
    }

    #[test]
    fn rejects_transport_this_toolchain_cannot_plan() {
        let armed = r#"
[[station]]
name = "star-1"
kind = "hamilton.star"
profile = "hamilton-star"

[transport]
between = "arm"
"#;
        let error = WorkcellProfile::parse("cell", armed).expect_err("arms are not planned yet");
        assert_eq!(
            error,
            WorkcellProfileError::UnsupportedTransport {
                found: "arm".into()
            }
        );
    }
}
