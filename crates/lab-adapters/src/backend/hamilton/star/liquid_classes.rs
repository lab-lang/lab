//! Versioned, data-defined Hamilton STAR liquid classes.
//!
//! A liquid class is operational knowledge, not a biological-operation
//! switch.  The class library is parsed and validated independently of the
//! planner, selection is a pure query over physical applicability, and every
//! selected definition receives a content digest that can be frozen into the
//! reviewed plan.
#![doc = include_str!("README.md")]

use std::collections::BTreeSet;
use std::fmt::{Display, Formatter, Write};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// The only library schema understood by this adapter implementation.
pub const LIQUID_CLASS_LIBRARY_SCHEMA: &str = "lab.hamilton-star-liquid-classes.v1";

const DEFAULT_LIBRARY: &str = include_str!("liquid_classes.v1.toml");

/// An authored liquid-class library document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LiquidClassLibraryDocument {
    pub schema_version: String,
    pub id: String,
    pub version: String,
    pub classes: Vec<LiquidClassDefinition>,
}

/// Exact library version selected by an Asset profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LiquidClassLibraryReference {
    pub id: String,
    pub version: String,
}

/// The package-carried registry available to one exact STAR Asset profile.
/// Selection is explicit and exact; registration order is never policy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LiquidClassLibraryRegistry {
    pub selected_library: LiquidClassLibraryReference,
    pub libraries: Vec<LiquidClassLibraryDocument>,
}

impl Default for LiquidClassLibraryRegistry {
    fn default() -> Self {
        let document: LiquidClassLibraryDocument = toml::from_str(DEFAULT_LIBRARY)
            .expect("the built-in Hamilton liquid-class document is valid TOML");
        Self {
            selected_library: LiquidClassLibraryReference {
                id: document.id.clone(),
                version: document.version.clone(),
            },
            libraries: vec![document],
        }
    }
}

/// One versioned Hamilton liquid class. The SHA-256 is deliberately not an
/// authored field: it is calculated from the normalized contents below.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LiquidClassDefinition {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub priority: i32,
    pub applicability: LiquidClassApplicability,
    pub correction: VolumeCorrection,
    pub speeds: PipettingSpeeds,
    pub lld: LiquidLevelDetection,
    pub margins: PipettingMargins,
    pub calibration: CalibrationProvenance,
}

/// Physical conditions under which a class may be selected. Each selector
/// accepts exact values and the explicit `*` fallback. Selector vocabulary is
/// intentionally open so a method pack can introduce a liquid or technique
/// without changing this Rust module.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LiquidClassApplicability {
    pub liquids: Vec<String>,
    pub techniques: Vec<String>,
    /// STAR catalog identifier for the placed tip rack, which fixes the tip
    /// type used by the run.
    pub tips: Vec<String>,
    pub source_labware: Vec<String>,
    pub destination_labware: Vec<String>,
    pub min_volume_ul: f64,
    pub max_volume_ul: f64,
}

/// A point on a target-volume to commanded-piston-volume correction curve.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CorrectionPoint {
    pub target_ul: f64,
    pub commanded_ul: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VolumeCorrection {
    pub points: Vec<CorrectionPoint>,
}

/// Liquid-specific channel speeds in microlitres per second.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PipettingSpeeds {
    pub aspirate_ul_s: f64,
    pub dispense_ul_s: f64,
    pub aspirate_mix_ul_s: f64,
    pub dispense_mix_ul_s: f64,
}

/// How this class obtains liquid level information.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiquidClassLldMode {
    /// Never enable LLD for this class.
    Off,
    /// Use the exact off/gamma policy validated by the selected Asset profile.
    Profile,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LiquidLevelDetection {
    pub mode: LiquidClassLldMode,
    pub gamma_sensitivity: u32,
    pub pressure_sensitivity: u32,
}

/// Vertical safety margins, in millimetres. These replace planner-wide magic
/// numbers and are therefore part of the class content digest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PipettingMargins {
    pub aspiration_immersion_mm: f64,
    pub bottom_standoff_mm: f64,
    pub dispense_clearance_mm: f64,
    pub lld_search_clearance_mm: f64,
}

/// Traceability for the measurements or vendor table behind a class.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CalibrationProvenance {
    pub source: String,
    pub source_version: String,
    pub instrument: String,
    pub performed_by: String,
    pub observed_at: String,
    pub notes: String,
}

/// Stable identity frozen into execution evidence.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LiquidClassIdentity {
    pub id: String,
    pub version: String,
    pub content_sha256: String,
}

/// Stable identity of the complete selected library, independently of the
/// profile file that registered it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LiquidClassLibraryIdentity {
    pub id: String,
    pub version: String,
    pub content_sha256: String,
}

/// The complete class snapshot attached to a reviewed plan. The identity pins
/// the normalized settings and the settings make the evidence inspectable
/// without finding the source library.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LiquidClassEvidence {
    pub schema_version: String,
    pub identity: LiquidClassIdentity,
    pub priority: i32,
    pub applicability: LiquidClassApplicability,
    pub correction: VolumeCorrection,
    pub speeds: PipettingSpeeds,
    pub lld: LiquidLevelDetection,
    pub margins: PipettingMargins,
    pub calibration: CalibrationProvenance,
}

/// One validated class and its calculated stable identity.
#[derive(Clone, Debug, PartialEq)]
pub struct LiquidClass {
    definition: LiquidClassDefinition,
    identity: LiquidClassIdentity,
}

impl LiquidClass {
    pub fn definition(&self) -> &LiquidClassDefinition {
        &self.definition
    }

    pub fn identity(&self) -> &LiquidClassIdentity {
        &self.identity
    }

    pub fn evidence(&self) -> LiquidClassEvidence {
        LiquidClassEvidence {
            schema_version: LIQUID_CLASS_LIBRARY_SCHEMA.to_owned(),
            identity: self.identity.clone(),
            priority: self.definition.priority,
            applicability: self.definition.applicability.clone(),
            correction: self.definition.correction.clone(),
            speeds: self.definition.speeds.clone(),
            lld: self.definition.lld.clone(),
            margins: self.definition.margins.clone(),
            calibration: self.definition.calibration.clone(),
        }
    }

    /// Interpolates the normalized correction table. Selection guarantees the
    /// requested volume lies inside both the class range and the table range.
    pub fn corrected_volume(&self, target_ul: f64) -> f64 {
        let points = &self.definition.correction.points;
        let pair = points
            .windows(2)
            .find(|pair| target_ul <= pair[1].target_ul)
            .unwrap_or_else(|| &points[points.len() - 2..]);
        let first = pair[0];
        let second = pair[1];
        if second.target_ul == first.target_ul {
            return first.commanded_ul;
        }
        first.commanded_ul
            + (target_ul - first.target_ul) * (second.commanded_ul - first.commanded_ul)
                / (second.target_ul - first.target_ul)
    }
}

/// A validated, deterministically ordered liquid-class library.
#[derive(Clone, Debug, PartialEq)]
pub struct LiquidClassLibrary {
    identity: LiquidClassLibraryIdentity,
    classes: Vec<LiquidClass>,
}

/// The complete physical selection query. No biological operation name is
/// involved in liquid-class selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidClassQuery<'a> {
    pub liquid: &'a str,
    pub technique: &'a str,
    pub tip: &'a str,
    pub source_labware: &'a str,
    pub destination_labware: &'a str,
    pub volume_ul: f64,
}

impl LiquidClassLibrary {
    /// Parses, normalizes, validates, and digests an authored TOML library.
    pub fn parse_toml(text: &str) -> Result<Self, LiquidClassError> {
        let document: LiquidClassLibraryDocument =
            toml::from_str(text).map_err(|error| LiquidClassError::Parse(error.to_string()))?;
        Self::from_document(document)
    }

    /// Loads the classes shipped with this adapter. They live in a data file
    /// so adding a calibrated class does not add a Rust dispatch arm.
    pub fn embedded_v1() -> Result<Self, LiquidClassError> {
        Self::parse_toml(DEFAULT_LIBRARY)
    }

    pub fn from_document(
        mut document: LiquidClassLibraryDocument,
    ) -> Result<Self, LiquidClassError> {
        if document.schema_version != LIQUID_CLASS_LIBRARY_SCHEMA {
            return Err(LiquidClassError::SchemaVersion {
                found: document.schema_version,
            });
        }
        validate_library_reference(
            &LiquidClassLibraryReference {
                id: document.id.clone(),
                version: document.version.clone(),
            },
            "liquid-class library",
        )?;
        if document.classes.is_empty() {
            return Err(LiquidClassError::Invalid(
                "the liquid-class library contains no classes".to_owned(),
            ));
        }

        let mut identities = BTreeSet::new();
        let mut classes = Vec::with_capacity(document.classes.len());
        for definition in &mut document.classes {
            normalize_definition(definition);
            validate_definition(definition)?;
            if !identities.insert((definition.id.clone(), definition.version.clone())) {
                return Err(LiquidClassError::Invalid(format!(
                    "liquid class '{}@{}' is declared more than once",
                    definition.id, definition.version
                )));
            }
            let canonical = serde_json::to_vec(definition).map_err(|error| {
                LiquidClassError::Invalid(format!(
                    "liquid class '{}@{}' cannot be canonicalized: {error}",
                    definition.id, definition.version
                ))
            })?;
            let identity = LiquidClassIdentity {
                id: definition.id.clone(),
                version: definition.version.clone(),
                content_sha256: hex_sha256(&canonical),
            };
            classes.push(LiquidClass {
                definition: definition.clone(),
                identity,
            });
        }
        classes.sort_by(|left, right| {
            left.identity.id.cmp(&right.identity.id).then_with(|| {
                version_key(&right.identity.version).cmp(&version_key(&left.identity.version))
            })
        });
        document.classes = classes
            .iter()
            .map(|class| class.definition.clone())
            .collect();
        let canonical = serde_json::to_vec(&document)
            .expect("validated liquid-class documents serialize infallibly");
        Ok(Self {
            identity: LiquidClassLibraryIdentity {
                id: document.id,
                version: document.version,
                content_sha256: hex_sha256(&canonical),
            },
            classes,
        })
    }

    pub fn identity(&self) -> &LiquidClassLibraryIdentity {
        &self.identity
    }

    pub fn classes(&self) -> &[LiquidClass] {
        &self.classes
    }

    /// Selects by descending explicit priority and physical specificity. Newer
    /// versions of one stable class supersede older versions. Equally ranked
    /// distinct classes are rejected instead of making a safety-relevant
    /// choice from file order.
    pub fn select(&self, query: LiquidClassQuery<'_>) -> Result<&LiquidClass, LiquidClassError> {
        if !query.volume_ul.is_finite() || query.volume_ul < 0.0 {
            return Err(LiquidClassError::Invalid(format!(
                "liquid-class query volume {} uL is not finite and non-negative",
                query.volume_ul
            )));
        }
        let mut matches = self
            .classes
            .iter()
            .filter_map(|class| {
                applicability_score(&class.definition.applicability, query)
                    .map(|specificity| (class, specificity))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|(left, left_specificity), (right, right_specificity)| {
            right
                .definition
                .priority
                .cmp(&left.definition.priority)
                .then_with(|| right_specificity.cmp(left_specificity))
                .then_with(|| left.identity.id.cmp(&right.identity.id))
                .then_with(|| {
                    version_key(&right.identity.version).cmp(&version_key(&left.identity.version))
                })
        });
        let Some((selected, selected_specificity)) = matches.first().copied() else {
            return Err(LiquidClassError::NoMatch(Box::new(LiquidClassQueryOwned {
                liquid: query.liquid.to_owned(),
                technique: query.technique.to_owned(),
                tip: query.tip.to_owned(),
                source_labware: query.source_labware.to_owned(),
                destination_labware: query.destination_labware.to_owned(),
                volume_ul: query.volume_ul,
            })));
        };
        let ambiguous = matches
            .iter()
            .skip(1)
            .take_while(|(class, specificity)| {
                class.definition.priority == selected.definition.priority
                    && *specificity == selected_specificity
            })
            .filter(|(class, _)| class.identity.id != selected.identity.id)
            .map(|(class, _)| format!("{}@{}", class.identity.id, class.identity.version))
            .collect::<Vec<_>>();
        if ambiguous.is_empty() {
            Ok(selected)
        } else {
            let mut candidates = vec![format!(
                "{}@{}",
                selected.identity.id, selected.identity.version
            )];
            candidates.extend(ambiguous);
            Err(LiquidClassError::Ambiguous {
                candidates: candidates.join(", "),
            })
        }
    }

    /// JSON Schema for editors and external class-library tooling. Runtime
    /// loading still performs the stricter numeric and uniqueness checks in
    /// [`Self::from_document`].
    pub fn json_schema() -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(LiquidClassLibraryDocument))
            .expect("the derived liquid-class schema serializes")
    }
}

impl LiquidClassLibraryRegistry {
    /// Validates the complete registry and resolves its exact selection.
    /// Malformed unselected registrations also fail so package composition
    /// cannot hide unusable contributed data until a later profile edit.
    pub fn resolve_selected(&self) -> Result<LiquidClassLibrary, LiquidClassError> {
        if self.libraries.is_empty() {
            return Err(LiquidClassError::Invalid(
                "the liquid-class registry contains no libraries".to_owned(),
            ));
        }
        validate_library_reference(&self.selected_library, "selected liquid-class library")?;
        let mut identities = BTreeSet::new();
        let mut selected = None;
        for document in &self.libraries {
            let reference = LiquidClassLibraryReference {
                id: document.id.clone(),
                version: document.version.clone(),
            };
            validate_library_reference(&reference, "registered liquid-class library")?;
            if !identities.insert((reference.id.clone(), reference.version.clone())) {
                return Err(LiquidClassError::DuplicateLibrary {
                    id: reference.id,
                    version: reference.version,
                });
            }
            let library = LiquidClassLibrary::from_document(document.clone())?;
            if reference == self.selected_library {
                selected = Some(library);
            }
        }
        selected.ok_or_else(|| LiquidClassError::UnknownSelectedLibrary {
            id: self.selected_library.id.clone(),
            version: self.selected_library.version.clone(),
        })
    }
}

/// A narrow interchange record for values copied or exported from the
/// Hamilton VENUS Liquid Class Editor. This is deliberately not presented as
/// a parser for VENUS's proprietary databases: the record makes every value
/// and its provenance explicit before it enters Lab's versioned library.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VenusLiquidClassRecord {
    pub id: String,
    pub version: String,
    pub venus_name: String,
    pub venus_revision: String,
    pub liquid: String,
    pub technique: String,
    pub tip: String,
    pub source_labware: String,
    pub destination_labware: String,
    pub min_volume_ul: f64,
    pub max_volume_ul: f64,
    pub correction_points: Vec<CorrectionPoint>,
    pub aspirate_ul_s: f64,
    pub dispense_ul_s: f64,
    pub aspirate_mix_ul_s: f64,
    pub dispense_mix_ul_s: f64,
    pub lld_mode: LiquidClassLldMode,
    pub gamma_sensitivity: u32,
    pub pressure_sensitivity: u32,
    pub aspiration_immersion_mm: f64,
    pub bottom_standoff_mm: f64,
    pub dispense_clearance_mm: f64,
    pub lld_search_clearance_mm: f64,
    pub instrument: String,
    pub exported_by: String,
    pub exported_at: String,
    pub notes: String,
}

/// Parses Lab's explicit VENUS interchange JSON and produces an ordinary
/// data-defined class. It goes through the same validation and digest path
/// once placed in a [`LiquidClassLibraryDocument`].
pub fn import_venus_record_json(text: &str) -> Result<LiquidClassDefinition, LiquidClassError> {
    let record: VenusLiquidClassRecord = serde_json::from_str(text)
        .map_err(|error| LiquidClassError::VenusImport(error.to_string()))?;
    Ok(import_venus_record(record))
}

pub fn import_venus_record(record: VenusLiquidClassRecord) -> LiquidClassDefinition {
    LiquidClassDefinition {
        id: record.id,
        version: record.version,
        priority: 0,
        applicability: LiquidClassApplicability {
            liquids: vec![record.liquid],
            techniques: vec![record.technique],
            tips: vec![record.tip],
            source_labware: vec![record.source_labware],
            destination_labware: vec![record.destination_labware],
            min_volume_ul: record.min_volume_ul,
            max_volume_ul: record.max_volume_ul,
        },
        correction: VolumeCorrection {
            points: record.correction_points,
        },
        speeds: PipettingSpeeds {
            aspirate_ul_s: record.aspirate_ul_s,
            dispense_ul_s: record.dispense_ul_s,
            aspirate_mix_ul_s: record.aspirate_mix_ul_s,
            dispense_mix_ul_s: record.dispense_mix_ul_s,
        },
        lld: LiquidLevelDetection {
            mode: record.lld_mode,
            gamma_sensitivity: record.gamma_sensitivity,
            pressure_sensitivity: record.pressure_sensitivity,
        },
        margins: PipettingMargins {
            aspiration_immersion_mm: record.aspiration_immersion_mm,
            bottom_standoff_mm: record.bottom_standoff_mm,
            dispense_clearance_mm: record.dispense_clearance_mm,
            lld_search_clearance_mm: record.lld_search_clearance_mm,
        },
        calibration: CalibrationProvenance {
            source: format!("Hamilton VENUS Liquid Class Editor: {}", record.venus_name),
            source_version: record.venus_revision,
            instrument: record.instrument,
            performed_by: record.exported_by,
            observed_at: record.exported_at,
            notes: record.notes,
        },
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum LiquidClassError {
    #[error("failed to parse Hamilton liquid-class TOML: {0}")]
    Parse(String),
    #[error(
        "Hamilton liquid-class library declares schema '{found}', expected '{LIQUID_CLASS_LIBRARY_SCHEMA}'"
    )]
    SchemaVersion { found: String },
    #[error("invalid Hamilton liquid-class library: {0}")]
    Invalid(String),
    #[error("no Hamilton liquid class applies to {0}")]
    NoMatch(Box<LiquidClassQueryOwned>),
    #[error(
        "Hamilton liquid-class selection is ambiguous among equally applicable classes: {candidates}; set an explicit priority or narrow applicability"
    )]
    Ambiguous { candidates: String },
    #[error("liquid-class library '{id}@{version}' is registered more than once")]
    DuplicateLibrary { id: String, version: String },
    #[error("selected liquid-class library '{id}@{version}' is not registered in this profile")]
    UnknownSelectedLibrary { id: String, version: String },
    #[error("failed to parse VENUS liquid-class interchange JSON: {0}")]
    VenusImport(String),
}

fn validate_library_reference(
    reference: &LiquidClassLibraryReference,
    label: &str,
) -> Result<(), LiquidClassError> {
    if !valid_stable_id(&reference.id) {
        return Err(LiquidClassError::Invalid(format!(
            "{label} id must be a non-empty stable ASCII identifier"
        )));
    }
    if version_key(&reference.version).is_none() {
        return Err(LiquidClassError::Invalid(format!(
            "{label} '{}' version must be numeric MAJOR.MINOR.PATCH",
            reference.id
        )));
    }
    Ok(())
}

/// Owned query details retained by a failed selection without making every
/// liquid-class result carry a large error value on its stack frame.
#[derive(Debug, PartialEq)]
pub struct LiquidClassQueryOwned {
    pub liquid: String,
    pub technique: String,
    pub tip: String,
    pub source_labware: String,
    pub destination_labware: String,
    pub volume_ul: f64,
}

impl Display for LiquidClassQueryOwned {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "liquid '{}', technique '{}', tip '{}', source labware '{}', destination labware '{}', and volume {} uL",
            self.liquid,
            self.technique,
            self.tip,
            self.source_labware,
            self.destination_labware,
            self.volume_ul,
        )
    }
}

fn normalize_definition(definition: &mut LiquidClassDefinition) {
    for values in [
        &mut definition.applicability.liquids,
        &mut definition.applicability.techniques,
        &mut definition.applicability.tips,
        &mut definition.applicability.source_labware,
        &mut definition.applicability.destination_labware,
    ] {
        values.sort();
        values.dedup();
    }
    definition
        .correction
        .points
        .sort_by(|left, right| left.target_ul.total_cmp(&right.target_ul));
}

fn validate_definition(definition: &LiquidClassDefinition) -> Result<(), LiquidClassError> {
    let label = format!("{}@{}", definition.id, definition.version);
    if !valid_stable_id(&definition.id) {
        return invalid(&label, "id must be a non-empty stable ASCII identifier");
    }
    if version_key(&definition.version).is_none() {
        return invalid(&label, "version must be numeric MAJOR.MINOR.PATCH");
    }
    for (name, values) in [
        ("liquids", &definition.applicability.liquids),
        ("techniques", &definition.applicability.techniques),
        ("tips", &definition.applicability.tips),
        ("source_labware", &definition.applicability.source_labware),
        (
            "destination_labware",
            &definition.applicability.destination_labware,
        ),
    ] {
        if values.is_empty() || values.iter().any(String::is_empty) {
            return invalid(&label, &format!("applicability.{name} must contain values"));
        }
    }
    let range = &definition.applicability;
    if !finite_non_negative(range.min_volume_ul)
        || !range.max_volume_ul.is_finite()
        || range.max_volume_ul <= range.min_volume_ul
    {
        return invalid(
            &label,
            "applicability volume bounds must be finite, non-negative, and increasing",
        );
    }
    let points = &definition.correction.points;
    if points.len() < 2 {
        return invalid(&label, "correction requires at least two points");
    }
    if points.iter().any(|point| {
        !finite_non_negative(point.target_ul)
            || !finite_non_negative(point.commanded_ul)
            || point.commanded_ul > 1250.0
    }) || points
        .windows(2)
        .any(|pair| pair[0].target_ul >= pair[1].target_ul)
    {
        return invalid(
            &label,
            "correction points must be finite, non-negative, within the 1250 uL firmware limit, and strictly increasing by target",
        );
    }
    if points[0].target_ul > range.min_volume_ul
        || points
            .last()
            .expect("two correction points exist")
            .target_ul
            < range.max_volume_ul
    {
        return invalid(
            &label,
            "correction points must cover the complete applicability volume range",
        );
    }
    for (name, speed) in [
        ("aspirate_ul_s", definition.speeds.aspirate_ul_s),
        ("dispense_ul_s", definition.speeds.dispense_ul_s),
        ("aspirate_mix_ul_s", definition.speeds.aspirate_mix_ul_s),
        ("dispense_mix_ul_s", definition.speeds.dispense_mix_ul_s),
    ] {
        if !speed.is_finite() || !(0.4..=500.0).contains(&speed) {
            return invalid(
                &label,
                &format!("speeds.{name} must fit the STAR firmware range 0.4..=500 uL/s"),
            );
        }
    }
    if !(1..=4).contains(&definition.lld.gamma_sensitivity)
        || !(1..=4).contains(&definition.lld.pressure_sensitivity)
    {
        return invalid(
            &label,
            "LLD sensitivity must be in the firmware range 1..=4",
        );
    }
    for (name, value) in [
        (
            "aspiration_immersion_mm",
            definition.margins.aspiration_immersion_mm,
        ),
        ("bottom_standoff_mm", definition.margins.bottom_standoff_mm),
        (
            "dispense_clearance_mm",
            definition.margins.dispense_clearance_mm,
        ),
        (
            "lld_search_clearance_mm",
            definition.margins.lld_search_clearance_mm,
        ),
    ] {
        if !finite_non_negative(value) {
            return invalid(
                &label,
                &format!("margins.{name} must be finite and non-negative"),
            );
        }
    }
    for (name, value) in [
        ("source", &definition.calibration.source),
        ("source_version", &definition.calibration.source_version),
        ("instrument", &definition.calibration.instrument),
        ("performed_by", &definition.calibration.performed_by),
        ("observed_at", &definition.calibration.observed_at),
    ] {
        if value.trim().is_empty() {
            return invalid(&label, &format!("calibration.{name} must not be empty"));
        }
    }
    Ok(())
}

fn valid_stable_id(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | '/')
        })
}

fn invalid<T>(label: &str, message: &str) -> Result<T, LiquidClassError> {
    Err(LiquidClassError::Invalid(format!(
        "liquid class '{label}' {message}"
    )))
}

fn finite_non_negative(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut output, "{byte:02x}").expect("writing into a String cannot fail");
    }
    output
}

fn version_key(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.split('.');
    let key = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(key)
}

fn selector_score(values: &[String], value: &str) -> Option<u8> {
    if values.iter().any(|candidate| candidate == value) {
        Some(1)
    } else if values.iter().any(|candidate| candidate == "*") {
        Some(0)
    } else {
        None
    }
}

fn applicability_score(
    applicability: &LiquidClassApplicability,
    query: LiquidClassQuery<'_>,
) -> Option<u8> {
    if query.volume_ul < applicability.min_volume_ul
        || query.volume_ul > applicability.max_volume_ul
    {
        return None;
    }
    Some(
        selector_score(&applicability.liquids, query.liquid)?
            + selector_score(&applicability.techniques, query.technique)?
            + selector_score(&applicability.tips, query.tip)?
            + selector_score(&applicability.source_labware, query.source_labware)?
            + selector_score(
                &applicability.destination_labware,
                query.destination_labware,
            )?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom_class_toml(id: &str, version: &str, commanded: f64) -> String {
        format!(
            r#"
schema_version = "{LIQUID_CLASS_LIBRARY_SCHEMA}"
id = "{id}.library"
version = "{version}"

[[classes]]
id = "{id}"
version = "{version}"
priority = 7

[classes.applicability]
liquids = ["glycerol"]
techniques = ["tracked_surface"]
tips = ["tip_rack_50ul_filter"]
source_labware = ["sample_tubes_24"]
destination_labware = ["pcr_plate_96"]
min_volume_ul = 1.0
max_volume_ul = 20.0

[classes.correction]
points = [
  {{ target_ul = 1.0, commanded_ul = 1.5 }},
  {{ target_ul = 20.0, commanded_ul = {commanded} }},
]

[classes.speeds]
aspirate_ul_s = 20.0
dispense_ul_s = 30.0
aspirate_mix_ul_s = 40.0
dispense_mix_ul_s = 40.0

[classes.lld]
mode = "profile"
gamma_sensitivity = 2
pressure_sensitivity = 3

[classes.margins]
aspiration_immersion_mm = 1.5
bottom_standoff_mm = 0.8
dispense_clearance_mm = 3.0
lld_search_clearance_mm = 6.0

[classes.calibration]
source = "local gravimetric calibration"
source_version = "run-42"
instrument = "STAR-123"
performed_by = "operator@example.org"
observed_at = "2026-09-05"
notes = "Imported independently of the compiler."
"#
        )
    }

    #[test]
    fn a_new_class_is_added_and_selected_as_data_only() {
        let library = LiquidClassLibrary::parse_toml(&custom_class_toml(
            "org.example.glycerol",
            "2.1.0",
            25.0,
        ))
        .expect("the contributed class is valid data");
        let selected = library
            .select(LiquidClassQuery {
                liquid: "glycerol",
                technique: "tracked_surface",
                tip: "tip_rack_50ul_filter",
                source_labware: "sample_tubes_24",
                destination_labware: "pcr_plate_96",
                volume_ul: 20.0,
            })
            .expect("the data-defined class matches");

        assert_eq!(selected.identity().id, "org.example.glycerol");
        assert_eq!(selected.identity().version, "2.1.0");
        assert_eq!(selected.identity().content_sha256.len(), 64);
        assert_eq!(selected.corrected_volume(20.0), 25.0);
    }

    #[test]
    fn a_profile_registry_selects_one_exact_contributed_library() {
        let document: LiquidClassLibraryDocument =
            toml::from_str(&custom_class_toml("org.example.glycerol", "2.1.0", 25.0)).unwrap();
        let mut document = document;
        document.id = "org.example.hamilton.methods".to_owned();
        document.version = "4.3.0".to_owned();
        let registry = LiquidClassLibraryRegistry {
            selected_library: LiquidClassLibraryReference {
                id: "org.example.hamilton.methods".to_owned(),
                version: "4.3.0".to_owned(),
            },
            libraries: vec![document],
        };

        let selected = registry
            .resolve_selected()
            .expect("the exact registration resolves");
        assert_eq!(selected.identity().id, "org.example.hamilton.methods");
        assert_eq!(selected.identity().version, "4.3.0");
        assert_eq!(selected.identity().content_sha256.len(), 64);
        assert_eq!(selected.classes()[0].identity().id, "org.example.glycerol");
    }

    #[test]
    fn every_profile_registry_entry_is_validated_before_selection() {
        let mut registry = LiquidClassLibraryRegistry::default();
        let mut broken = registry.libraries[0].clone();
        broken.id = "org.example.broken".to_owned();
        broken.schema_version = "lab.hamilton-star-liquid-classes.v99".to_owned();
        registry.libraries.push(broken);

        assert!(matches!(
            registry.resolve_selected(),
            Err(LiquidClassError::SchemaVersion { .. })
        ));
    }

    #[test]
    fn a_profile_registry_selection_must_name_a_registered_version() {
        let mut registry = LiquidClassLibraryRegistry::default();
        registry.selected_library.version = "2.0.0".to_owned();

        assert!(matches!(
            registry.resolve_selected(),
            Err(LiquidClassError::UnknownSelectedLibrary { version, .. }) if version == "2.0.0"
        ));
    }

    #[test]
    fn incompatible_library_schema_is_rejected() {
        let text = custom_class_toml("org.example.future", "1.0.0", 25.0).replace(
            LIQUID_CLASS_LIBRARY_SCHEMA,
            "lab.hamilton-star-liquid-classes.v99",
        );
        assert!(matches!(
            LiquidClassLibrary::parse_toml(&text),
            Err(LiquidClassError::SchemaVersion { .. })
        ));
    }

    #[test]
    fn normalized_content_has_a_stable_digest() {
        let ordered = custom_class_toml("org.example.stable", "1.0.0", 24.0);
        let reordered = ordered.replace(
            "liquids = [\"glycerol\"]",
            "liquids = [\"glycerol\", \"*\"]",
        );
        let restored = reordered.replace(
            "liquids = [\"glycerol\", \"*\"]",
            "liquids = [\"*\", \"glycerol\"]",
        );
        let first = LiquidClassLibrary::parse_toml(&reordered).unwrap();
        let second = LiquidClassLibrary::parse_toml(&restored).unwrap();
        assert_eq!(
            first.classes()[0].identity().content_sha256,
            second.classes()[0].identity().content_sha256,
            "selector order is normalized before hashing"
        );
        assert_eq!(
            first.identity().content_sha256,
            second.identity().content_sha256,
            "the complete library digest uses the same normalized content"
        );
    }

    #[test]
    fn exact_applicability_beats_a_wildcard_deterministically() {
        let mut document: LiquidClassLibraryDocument =
            toml::from_str(&custom_class_toml("org.example.specific", "1.0.0", 24.0)).unwrap();
        let mut fallback: LiquidClassLibraryDocument =
            toml::from_str(&custom_class_toml("org.example.fallback", "1.0.0", 22.0)).unwrap();
        fallback.classes[0].applicability.liquids = vec!["*".to_owned()];
        document.classes.extend(fallback.classes);
        let library =
            LiquidClassLibrary::from_document(document).expect("both overlapping classes validate");
        let selected = library
            .select(LiquidClassQuery {
                liquid: "glycerol",
                technique: "tracked_surface",
                tip: "tip_rack_50ul_filter",
                source_labware: "sample_tubes_24",
                destination_labware: "pcr_plate_96",
                volume_ul: 10.0,
            })
            .unwrap();
        assert_eq!(selected.identity().id, "org.example.specific");
    }

    #[test]
    fn the_bundled_reference_library_fails_closed_for_unlisted_liquids() {
        let library = LiquidClassLibrary::embedded_v1().unwrap();
        let error = library
            .select(LiquidClassQuery {
                liquid: "serum",
                technique: "surface",
                tip: "tip_rack_50ul_filter",
                source_labware: "sample_tubes_24",
                destination_labware: "pcr_plate_96",
                volume_ul: 10.0,
            })
            .expect_err("unqualified water reference data must not match an arbitrary liquid");
        assert!(matches!(error, LiquidClassError::NoMatch(_)));
    }

    #[test]
    fn equally_applicable_distinct_classes_fail_closed() {
        let mut document: LiquidClassLibraryDocument =
            toml::from_str(&custom_class_toml("org.example.a", "1.0.0", 24.0)).unwrap();
        let second: LiquidClassLibraryDocument =
            toml::from_str(&custom_class_toml("org.example.b", "1.0.0", 25.0)).unwrap();
        document.classes.extend(second.classes);
        let library = LiquidClassLibrary::from_document(document).expect("both classes validate");
        let error = library
            .select(LiquidClassQuery {
                liquid: "glycerol",
                technique: "tracked_surface",
                tip: "tip_rack_50ul_filter",
                source_labware: "sample_tubes_24",
                destination_labware: "pcr_plate_96",
                volume_ul: 10.0,
            })
            .expect_err("equally ranked independent classes are unsafe to choose by file order");
        assert!(matches!(error, LiquidClassError::Ambiguous { .. }));
    }

    #[test]
    fn venus_interchange_becomes_an_ordinary_validated_class() {
        let json = r#"{
          "id":"org.example.venus.serum",
          "version":"1.0.0",
          "venus_name":"Serum_50ul_Surface",
          "venus_revision":"VENUS 6.2 rev 17",
          "liquid":"serum",
          "technique":"tracked_surface",
          "tip":"tip_rack_50ul_filter",
          "source_labware":"sample_tubes_24",
          "destination_labware":"pcr_plate_96",
          "min_volume_ul":1.0,
          "max_volume_ul":20.0,
          "correction_points":[
            {"target_ul":1.0,"commanded_ul":1.4},
            {"target_ul":20.0,"commanded_ul":22.1}
          ],
          "aspirate_ul_s":10.0,
          "dispense_ul_s":15.0,
          "aspirate_mix_ul_s":20.0,
          "dispense_mix_ul_s":20.0,
          "lld_mode":"profile",
          "gamma_sensitivity":2,
          "pressure_sensitivity":2,
          "aspiration_immersion_mm":2.0,
          "bottom_standoff_mm":0.5,
          "dispense_clearance_mm":2.0,
          "lld_search_clearance_mm":5.0,
          "instrument":"STAR-123",
          "exported_by":"operator@example.org",
          "exported_at":"2026-09-05",
          "notes":"Values transcribed from the VENUS editor."
        }"#;
        let definition = import_venus_record_json(json).expect("the interchange record parses");
        let library = LiquidClassLibrary::from_document(LiquidClassLibraryDocument {
            schema_version: LIQUID_CLASS_LIBRARY_SCHEMA.to_owned(),
            id: "org.example.venus-imports".to_owned(),
            version: "1.0.0".to_owned(),
            classes: vec![definition],
        })
        .expect("the imported class passes the ordinary schema and semantic validation");
        assert_eq!(
            library.classes()[0].identity().id,
            "org.example.venus.serum"
        );
        assert_eq!(
            library.classes()[0].definition().calibration.source,
            "Hamilton VENUS Liquid Class Editor: Serum_50ul_Surface"
        );
    }

    #[test]
    fn schema_exposes_every_extension_surface() {
        let schema = LiquidClassLibrary::json_schema();
        let text = serde_json::to_string(&schema).unwrap();
        for field in [
            "applicability",
            "correction",
            "speeds",
            "lld",
            "margins",
            "calibration",
        ] {
            assert!(text.contains(field), "schema omitted {field}");
        }
    }
}
