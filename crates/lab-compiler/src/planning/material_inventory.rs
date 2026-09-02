//! Exact material-lot evidence retained across facility planning and backend projection.

use std::collections::BTreeMap;

use lab_capability::AbsoluteIri;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Exact candidate lots for the checked declarations in one program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MaterialLotInventory {
    source_sha256: String,
    facility: String,
    materials: BTreeMap<String, MaterialLotCandidates>,
    artifacts: BTreeMap<String, MaterialLotCandidates>,
}

impl MaterialLotInventory {
    /// Construct the durable evidence record from already normalized candidate maps.
    pub fn new(
        source_sha256: impl Into<String>,
        facility: impl Into<String>,
        materials: BTreeMap<String, MaterialLotCandidates>,
        artifacts: BTreeMap<String, MaterialLotCandidates>,
    ) -> Self {
        Self {
            source_sha256: source_sha256.into(),
            facility: facility.into(),
            materials,
            artifacts,
        }
    }

    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }

    pub fn facility(&self) -> &str {
        &self.facility
    }

    pub fn materials(&self) -> &BTreeMap<String, MaterialLotCandidates> {
        &self.materials
    }

    pub fn artifacts(&self) -> &BTreeMap<String, MaterialLotCandidates> {
        &self.artifacts
    }

    /// Resolve one operational symbol using the same material-first rule as facility planning.
    pub fn candidates(&self, symbol: &str) -> Option<&MaterialLotCandidates> {
        self.materials
            .get(symbol)
            .or_else(|| self.artifacts.get(symbol))
    }

    /// Revalidate serialized material evidence before planning or lowering consumes it.
    pub fn validate(&self) -> Result<(), MaterialLotInventoryValidationError> {
        if !is_sha256(&self.source_sha256) {
            return Err(MaterialLotInventoryValidationError::InvalidSourceDigest);
        }
        if AbsoluteIri::new(&self.facility).is_err() {
            return Err(MaterialLotInventoryValidationError::InvalidFacility);
        }
        validate_candidates("material", &self.materials)?;
        validate_candidates("artifact", &self.artifacts)?;
        for (symbol, material) in &self.materials {
            if self
                .artifacts
                .get(symbol)
                .is_some_and(|artifact| artifact != material)
            {
                return Err(MaterialLotInventoryValidationError::ConflictingSymbol {
                    symbol: symbol.clone(),
                });
            }
        }
        Ok(())
    }
}

/// The design identity and active physical lots known for one checked declaration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MaterialLotCandidates {
    Unidentified,
    Identified {
        component: String,
        material_lots: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum MaterialLotInventoryValidationError {
    #[error("material inventory source_sha256 is not a canonical SHA-256 digest")]
    InvalidSourceDigest,
    #[error("material inventory facility is not an absolute IRI")]
    InvalidFacility,
    #[error("material inventory contains invalid {kind} candidates for symbol `{symbol}`")]
    InvalidCandidates { kind: &'static str, symbol: String },
    #[error("material inventory gives conflicting material and artifact evidence for `{symbol}`")]
    ConflictingSymbol { symbol: String },
}

fn validate_candidates(
    kind: &'static str,
    entries: &BTreeMap<String, MaterialLotCandidates>,
) -> Result<(), MaterialLotInventoryValidationError> {
    for (symbol, candidates) in entries {
        let valid = !symbol.is_empty()
            && match candidates {
                MaterialLotCandidates::Unidentified => true,
                MaterialLotCandidates::Identified {
                    component,
                    material_lots,
                } => {
                    AbsoluteIri::new(component).is_ok()
                        && material_lots
                            .iter()
                            .all(|material_lot| AbsoluteIri::new(material_lot).is_ok())
                        && material_lots.windows(2).all(|lots| lots[0] < lots[1])
                }
            };
        if !valid {
            return Err(MaterialLotInventoryValidationError::InvalidCandidates {
                kind,
                symbol: symbol.clone(),
            });
        }
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inventory(
        materials: BTreeMap<String, MaterialLotCandidates>,
        artifacts: BTreeMap<String, MaterialLotCandidates>,
    ) -> MaterialLotInventory {
        MaterialLotInventory::new(
            "a".repeat(64),
            "https://example.org/facility",
            materials,
            artifacts,
        )
    }

    #[test]
    fn validates_canonical_physical_evidence() {
        let candidates = MaterialLotCandidates::Identified {
            component: "https://example.org/component".to_owned(),
            material_lots: vec![
                "https://example.org/lot/a".to_owned(),
                "https://example.org/lot/b".to_owned(),
            ],
        };
        let materials = BTreeMap::from([("sample".to_owned(), candidates.clone())]);
        let artifacts = BTreeMap::from([("sample".to_owned(), candidates)]);

        inventory(materials, artifacts).validate().unwrap();
    }

    #[test]
    fn rejects_noncanonical_or_conflicting_candidate_maps() {
        let unsorted = MaterialLotCandidates::Identified {
            component: "https://example.org/component".to_owned(),
            material_lots: vec![
                "https://example.org/lot/b".to_owned(),
                "https://example.org/lot/a".to_owned(),
            ],
        };
        assert!(matches!(
            inventory(
                BTreeMap::from([("sample".to_owned(), unsorted)]),
                BTreeMap::new()
            )
            .validate(),
            Err(MaterialLotInventoryValidationError::InvalidCandidates { .. })
        ));

        let materials =
            BTreeMap::from([("sample".to_owned(), MaterialLotCandidates::Unidentified)]);
        let artifacts = BTreeMap::from([(
            "sample".to_owned(),
            MaterialLotCandidates::Identified {
                component: "https://example.org/component".to_owned(),
                material_lots: Vec::new(),
            },
        )]);
        assert!(matches!(
            inventory(materials, artifacts).validate(),
            Err(MaterialLotInventoryValidationError::ConflictingSymbol { .. })
        ));
    }
}
