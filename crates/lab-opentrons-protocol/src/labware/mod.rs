//! Labware definitions (labware schema 2): the standard definitions this
//! crate embeds, and the typed view validation reads from.
//!
//! A definition is stored verbatim as parsed JSON so the emitted protocol
//! embeds exactly what Opentrons publishes, alongside a typed view holding the
//! fields validation needs: identity, tip-rack-ness, and per-well capacity.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;
use thiserror::Error;

/// Standard definitions bundled into this crate, vendored from the Opentrons
/// shared-data tree. See `definitions/README.md` for source and license.
const EMBEDDED_DEFINITIONS: [&str; 9] = [
    include_str!("definitions/opentrons_flex_96_tiprack_50ul_v1.json"),
    include_str!("definitions/opentrons_flex_96_tiprack_200ul_v1.json"),
    include_str!("definitions/opentrons_flex_96_tiprack_1000ul_v1.json"),
    include_str!("definitions/opentrons_flex_96_filtertiprack_50ul_v1.json"),
    include_str!("definitions/opentrons_flex_96_filtertiprack_200ul_v1.json"),
    include_str!("definitions/opentrons_flex_96_filtertiprack_1000ul_v1.json"),
    include_str!("definitions/nest_96_wellplate_100ul_pcr_full_skirt_v3.json"),
    include_str!("definitions/opentrons_24_aluminumblock_nest_1.5ml_snapcap_v2.json"),
    include_str!("definitions/opentrons_15_tuberack_falcon_15ml_conical_v2.json"),
];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LabwareDefinitionError {
    #[error("labware definition is not valid JSON: {0}")]
    Json(String),
    #[error(
        "labware definition '{load_name}' declares labware schema {found}, but this crate authors labware schema 2"
    )]
    UnsupportedSchema { load_name: String, found: u64 },
}

/// One labware-schema-2 definition: the verbatim JSON document plus the typed
/// view validation reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabwareDefinition {
    raw: serde_json::Value,
    view: LabwareView,
}

/// The subset of a definition the builder validates against. Unknown fields
/// remain in the verbatim document; this view deliberately ignores them.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct LabwareView {
    #[serde(rename = "schemaVersion")]
    schema_version: u64,
    namespace: String,
    version: u64,
    parameters: LabwareParameters,
    wells: BTreeMap<String, WellView>,
}

// Well coordinates are finite by construction in published definitions, so
// bitwise f64 equality is the equality the view needs.
impl Eq for LabwareView {}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct LabwareParameters {
    #[serde(rename = "loadName")]
    load_name: String,
    #[serde(rename = "isTiprack")]
    is_tiprack: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct WellView {
    #[serde(rename = "totalLiquidVolume")]
    total_liquid_volume: f64,
}

impl LabwareDefinition {
    /// Parse a labware-schema-2 definition from its published JSON.
    pub fn parse(json: &str) -> Result<Self, LabwareDefinitionError> {
        let raw: serde_json::Value = serde_json::from_str(json)
            .map_err(|error| LabwareDefinitionError::Json(error.to_string()))?;
        let view: LabwareView = serde_json::from_value(raw.clone())
            .map_err(|error| LabwareDefinitionError::Json(error.to_string()))?;
        if view.schema_version != 2 {
            return Err(LabwareDefinitionError::UnsupportedSchema {
                load_name: view.parameters.load_name,
                found: view.schema_version,
            });
        }
        Ok(Self { raw, view })
    }

    pub fn load_name(&self) -> &str {
        &self.view.parameters.load_name
    }

    pub fn namespace(&self) -> &str {
        &self.view.namespace
    }

    pub fn version(&self) -> u64 {
        self.view.version
    }

    pub fn is_tip_rack(&self) -> bool {
        self.view.parameters.is_tiprack
    }

    /// The `labwareDefinitions` key every Opentrons tool derives for a
    /// definition: `{namespace}/{loadName}/{version}`.
    pub fn definition_key(&self) -> String {
        format!(
            "{}/{}/{}",
            self.view.namespace, self.view.parameters.load_name, self.view.version
        )
    }

    pub fn has_well(&self, well: &str) -> bool {
        self.view.wells.contains_key(well)
    }

    pub fn well_count(&self) -> usize {
        self.view.wells.len()
    }

    /// Capacity of one well in µL. `None` when the well does not exist.
    pub fn well_volume_ul(&self, well: &str) -> Option<f64> {
        self.view
            .wells
            .get(well)
            .map(|well| well.total_liquid_volume)
    }

    /// The verbatim definition document, embedded as-is into protocols.
    pub fn raw(&self) -> &serde_json::Value {
        &self.raw
    }
}

fn registry() -> &'static BTreeMap<&'static str, LabwareDefinition> {
    static REGISTRY: OnceLock<BTreeMap<&'static str, LabwareDefinition>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        EMBEDDED_DEFINITIONS
            .into_iter()
            .map(|json| {
                let definition = LabwareDefinition::parse(json)
                    .expect("embedded labware definitions are vendored verbatim from Opentrons");
                let load_name: &'static str =
                    Box::leak(definition.load_name().to_owned().into_boxed_str());
                (load_name, definition)
            })
            .collect()
    })
}

/// Look up an embedded standard definition by load name.
pub fn standard_definition(load_name: &str) -> Option<&'static LabwareDefinition> {
    registry().get(load_name)
}

/// Load names of every embedded standard definition, for error messages.
pub fn standard_load_names() -> Vec<&'static str> {
    registry().keys().copied().collect()
}

#[cfg(test)]
mod tests {
    use crate::labware::*;

    #[test]
    fn every_embedded_definition_parses_with_its_identity_intact() {
        for load_name in standard_load_names() {
            let definition = standard_definition(load_name).unwrap();
            assert_eq!(definition.load_name(), load_name);
            assert_eq!(definition.namespace(), "opentrons");
            assert!(definition.version() >= 1);
            assert!(definition.has_well("A1"), "{load_name} must have well A1");
        }
    }

    #[test]
    fn tip_racks_and_plates_are_told_apart() {
        assert!(
            standard_definition("opentrons_flex_96_tiprack_50ul")
                .unwrap()
                .is_tip_rack()
        );
        assert!(
            !standard_definition("nest_96_wellplate_100ul_pcr_full_skirt")
                .unwrap()
                .is_tip_rack()
        );
    }

    #[test]
    fn well_capacity_comes_from_the_definition() {
        let plate = standard_definition("nest_96_wellplate_100ul_pcr_full_skirt").unwrap();
        assert_eq!(plate.well_volume_ul("A1"), Some(100.0));
        assert_eq!(plate.well_volume_ul("H13"), None);
        assert_eq!(plate.well_count(), 96);
        assert_eq!(
            plate.definition_key(),
            "opentrons/nest_96_wellplate_100ul_pcr_full_skirt/3"
        );
    }

    #[test]
    fn rejects_a_definition_from_another_labware_schema() {
        let error = LabwareDefinition::parse(
            r#"{"schemaVersion": 3, "namespace": "opentrons", "version": 1,
                "parameters": {"loadName": "future_plate", "isTiprack": false}, "wells": {}}"#,
        )
        .expect_err("labware schema 3 is not the schema this crate authors");
        assert_eq!(
            error,
            LabwareDefinitionError::UnsupportedSchema {
                load_name: "future_plate".into(),
                found: 3
            }
        );
    }
}
