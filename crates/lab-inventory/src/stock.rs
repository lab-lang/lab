//! Optional Lab stock annotations on SBOLInventory MaterialLots.
//! These are explicit lot facts, never inferred from names or locations.

use crate::{InventorySnapshot, MaterialLotCatalogError};
use sbol3::{Iri, Term};
use std::collections::BTreeMap;

pub const ALIQUOT_VOLUME_UL: &str = "https://www.lab-compiler.org/ns/inventory#aliquotVolumeUl";
pub const ALIQUOT_COUNT: &str = "https://www.lab-compiler.org/ns/inventory#aliquotCount";
pub const DEAD_VOLUME_UL: &str = "https://www.lab-compiler.org/ns/inventory#deadVolumeUl";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StockAliquots {
    pub volume_each_ul: String,
    pub dead_volume_each_ul: String,
    pub count: u32,
}

impl InventorySnapshot {
    pub fn stock_aliquots(&self) -> Result<BTreeMap<Iri, StockAliquots>, MaterialLotCatalogError> {
        let catalog = self.active_material_lots()?;
        let mut stocks = BTreeMap::new();
        for (_, lots) in catalog.components() {
            for lot in lots {
                let object = &self.document().as_sbol_document().objects()
                    [&sbol3::Resource::Iri(lot.clone())];
                let volume = object.values(ALIQUOT_VOLUME_UL);
                let count = object.values(ALIQUOT_COUNT);
                let dead = object.values(DEAD_VOLUME_UL);
                if volume.is_empty() && count.is_empty() && dead.is_empty() {
                    continue;
                }
                let invalid = |message: &str| MaterialLotCatalogError::InvalidStock {
                    lot: lot.to_string(),
                    message: message.into(),
                };
                let literal = |values: &[Term]| -> Result<String, MaterialLotCatalogError> {
                    match values {
                        [Term::Literal(value)] => Ok(value.value().to_owned()),
                        _ => Err(invalid("stock properties require exactly one literal")),
                    }
                };
                let volume_each_ul = literal(volume)?;
                let count = literal(count)?.parse::<u32>().map_err(|_| {
                    invalid("aliquotCount must be a nonnegative integer within u32")
                })?;
                let dead_volume_each_ul = if dead.is_empty() {
                    "0".into()
                } else {
                    literal(dead)?
                };
                stocks.insert(
                    lot.clone(),
                    StockAliquots {
                        volume_each_ul,
                        dead_volume_each_ul,
                        count,
                    },
                );
            }
        }
        Ok(stocks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stock(properties: &str) -> Result<BTreeMap<Iri, StockAliquots>, MaterialLotCatalogError> {
        let directory = tempfile::tempdir().unwrap();
        let contents = format!(
            r#"
@prefix fac: <https://sbol.io/ns/facility#> .
@prefix inv: <https://sbol.io/ns/inventory#> .
@prefix lab: <https://www.lab-compiler.org/ns/inventory#> .
@prefix sbol: <http://sbols.org/v3#> .
@prefix ex: <https://example.org/stock/> .
ex:facility a sbol:TopLevel, fac:Facility ; sbol:displayId "facility" ; sbol:hasNamespace <https://example.org/stock> .
ex:storage a sbol:TopLevel, fac:Zone ; sbol:displayId "storage" ; sbol:hasNamespace <https://example.org/stock> ; fac:facility ex:facility ; fac:zoneKind fac:StorageZone ; fac:isActive true .
ex:material a sbol:Component ; sbol:displayId "material" ; sbol:hasNamespace <https://example.org/stock> ; sbol:type <https://identifiers.org/SBO:0000251> .
ex:lot a sbol:Implementation ; sbol:displayId "lot" ; sbol:hasNamespace <https://example.org/stock> ; sbol:built ex:material ; fac:materialKind inv:ProcuredMaterial ; fac:locatedIn ex:storage ; fac:isActive true {properties} .
"#
        );
        std::fs::write(directory.path().join("inventory.ttl"), contents).unwrap();
        InventorySnapshot::load(directory.path(), "inventory.ttl", None)
            .unwrap()
            .stock_aliquots()
    }

    #[test]
    fn stock_annotations_are_optional_but_count_is_finite() {
        assert!(stock("").unwrap().is_empty());
        let lots = stock("; lab:aliquotVolumeUl \"1000\" ; lab:aliquotCount 0").unwrap();
        assert_eq!(
            lots.values().next().unwrap(),
            &StockAliquots {
                volume_each_ul: "1000".into(),
                dead_volume_each_ul: "0".into(),
                count: 0,
            }
        );
    }

    #[test]
    fn stock_annotations_must_be_complete_unambiguous_and_nonnegative() {
        for properties in [
            "; lab:aliquotVolumeUl \"1000\"",
            "; lab:aliquotCount 2",
            "; lab:aliquotVolumeUl \"1000\", \"500\" ; lab:aliquotCount 2",
            "; lab:aliquotVolumeUl \"1000\" ; lab:aliquotCount -1",
            "; lab:aliquotVolumeUl \"1000\" ; lab:aliquotCount 1.5",
            "; lab:aliquotVolumeUl ex:volume ; lab:aliquotCount 2",
        ] {
            assert!(
                matches!(
                    stock(properties),
                    Err(MaterialLotCatalogError::InvalidStock { .. })
                ),
                "{properties}"
            );
        }
    }
}
