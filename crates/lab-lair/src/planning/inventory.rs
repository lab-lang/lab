use std::collections::BTreeMap;

use lab_language::{CheckedDeclaration, CheckedModule};
use thiserror::Error;

use super::model::{BuildInventory, MaterialLotBuildInventory, MaterialLotCandidates};

#[derive(Debug, Error)]
pub enum BuildInventoryError {
    #[error(
        "inventory lookup key `{symbol}` refers to both SBOL Components `{first}` and `{second}`"
    )]
    ConflictingDesignIdentity {
        symbol: String,
        first: String,
        second: String,
    },
}

impl BuildInventory {
    /// Binds checked declaration identities to every active candidate lot in one facility snapshot.
    ///
    /// Candidate preservation is intentional. Selecting among several lots is allocation policy,
    /// so dependency planning reports ambiguity if it actually needs such a declaration.
    pub fn from_material_lots(
        modules: &[&CheckedModule],
        source_sha256: impl Into<String>,
        facility: impl Into<String>,
        lots_by_component: &BTreeMap<String, Vec<String>>,
    ) -> Result<Self, BuildInventoryError> {
        let mut materials = BTreeMap::new();
        let mut artifacts = BTreeMap::new();
        for declaration in modules.iter().flat_map(|module| module.declarations.iter()) {
            match declaration {
                CheckedDeclaration::Catalog {
                    name,
                    sbol_identity,
                    supplier_identity,
                    ..
                } => {
                    insert_declaration(
                        &mut materials,
                        name,
                        sbol_identity.as_deref(),
                        lots_by_component,
                    )?;
                    if supplier_identity != name {
                        insert_declaration(
                            &mut materials,
                            supplier_identity,
                            sbol_identity.as_deref(),
                            lots_by_component,
                        )?;
                    }
                }
                CheckedDeclaration::Artifact {
                    name,
                    sbol_identity,
                    ..
                } => insert_declaration(
                    &mut artifacts,
                    name,
                    sbol_identity.as_deref(),
                    lots_by_component,
                )?,
                _ => {}
            }
        }
        for (symbol, material) in &materials {
            if let Some(artifact) = artifacts.get(symbol) {
                ensure_compatible(symbol, material, artifact)?;
            }
        }
        Ok(Self::MaterialLots(MaterialLotBuildInventory {
            source_sha256: source_sha256.into(),
            facility: facility.into(),
            materials,
            artifacts,
        }))
    }
}

fn insert_declaration(
    entries: &mut BTreeMap<String, MaterialLotCandidates>,
    lookup_key: &str,
    identity: Option<&str>,
    lots_by_component: &BTreeMap<String, Vec<String>>,
) -> Result<(), BuildInventoryError> {
    let candidate = if let Some(identity) = identity {
        let mut material_lots = lots_by_component.get(identity).cloned().unwrap_or_default();
        material_lots.sort();
        material_lots.dedup();
        MaterialLotCandidates::Identified {
            component: identity.to_owned(),
            material_lots,
        }
    } else {
        MaterialLotCandidates::Unidentified
    };

    if let Some(existing) = entries.get(lookup_key) {
        ensure_compatible(lookup_key, existing, &candidate)?;
        return Ok(());
    }
    entries.insert(lookup_key.to_owned(), candidate);
    Ok(())
}

fn ensure_compatible(
    symbol: &str,
    first: &MaterialLotCandidates,
    second: &MaterialLotCandidates,
) -> Result<(), BuildInventoryError> {
    match (first, second) {
        (
            MaterialLotCandidates::Identified {
                component: first, ..
            },
            MaterialLotCandidates::Identified {
                component: second, ..
            },
        ) if first != second => Err(BuildInventoryError::ConflictingDesignIdentity {
            symbol: symbol.to_owned(),
            first: first.clone(),
            second: second.clone(),
        }),
        (MaterialLotCandidates::Unidentified, MaterialLotCandidates::Identified { .. })
        | (MaterialLotCandidates::Identified { .. }, MaterialLotCandidates::Unidentified) => {
            Err(BuildInventoryError::ConflictingDesignIdentity {
                symbol: symbol.to_owned(),
                first: render_identity(first),
                second: render_identity(second),
            })
        }
        _ => Ok(()),
    }
}

fn render_identity(candidates: &MaterialLotCandidates) -> String {
    match candidates {
        MaterialLotCandidates::Unidentified => "<unstated>".to_owned(),
        MaterialLotCandidates::Identified { component, .. } => component.clone(),
    }
}

#[cfg(test)]
mod tests {
    use lab_language::compile_module;

    use super::*;

    fn lots() -> BTreeMap<String, Vec<String>> {
        BTreeMap::from([(
            "https://example.org/inventory/input".to_owned(),
            vec!["https://example.org/inventory/input_lot".to_owned()],
        )])
    }

    #[test]
    fn binds_operational_aliases_to_one_exact_design_iri() {
        let checked = compile_module(
            r#"use std.bio.designs

buy part source:
  sbol_identity = "https://example.org/inventory/input"
  supplier_identity = "SKU-1"

build part product:
  sbol_identity = "https://example.org/inventory/product"
"#,
        )
        .unwrap();
        let inventory = BuildInventory::from_material_lots(
            &[&checked],
            "abc123",
            "https://example.org/inventory/facility",
            &lots(),
        )
        .unwrap();

        let BuildInventory::MaterialLots(inventory) = inventory else {
            panic!("an SBOLInventory document creates semantic inventory");
        };
        assert_eq!(inventory.facility, "https://example.org/inventory/facility");
        assert!(inventory.materials.contains_key("SKU-1"));
        assert!(inventory.materials.contains_key("source"));
        let expected = MaterialLotCandidates::Identified {
            component: "https://example.org/inventory/input".to_owned(),
            material_lots: vec!["https://example.org/inventory/input_lot".to_owned()],
        };
        assert_eq!(inventory.materials["SKU-1"], expected);
        assert_eq!(inventory.materials["source"], expected);
        assert_eq!(
            inventory.artifacts["product"],
            MaterialLotCandidates::Identified {
                component: "https://example.org/inventory/product".to_owned(),
                material_lots: Vec::new(),
            }
        );
    }

    #[test]
    fn rejects_one_operational_key_that_names_different_designs() {
        let checked = compile_module(
            r#"use std.bio.designs

buy part first:
  sbol_identity = "https://example.org/inventory/input"
  supplier_identity = "same-sku"

buy part second:
  sbol_identity = "https://example.org/inventory/product"
  supplier_identity = "same-sku"
"#,
        )
        .unwrap();
        let error = BuildInventory::from_material_lots(
            &[&checked],
            "abc123",
            "https://example.org/inventory/facility",
            &lots(),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            BuildInventoryError::ConflictingDesignIdentity { symbol, .. } if symbol == "same-sku"
        ));
    }
}
