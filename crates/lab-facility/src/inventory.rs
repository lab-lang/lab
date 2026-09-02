//! Facility-owned MaterialLot evidence and its allocation cross-validation.

use std::collections::{BTreeMap, BTreeSet};

use lab_capability::AbsoluteIri;
use lab_compiler::allocation::{AllocatedProgram, AllocatedProgramValidationError};
use lab_compiler::method::LocalId;
use lab_compiler::planning::{SelectedMaterialBinding, SelectedMaterialSource};
use lab_language::{CheckedDeclaration, CheckedModule};
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

    /// Revalidate serialized material evidence before planning or allocation consumes it.
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

/// Cross-validate one intrinsically valid allocation against its exact facility evidence.
pub fn validate_allocated_material_inventory(
    allocated: &AllocatedProgram,
    material_inventory: &MaterialLotInventory,
) -> Result<(), AllocatedMaterialInventoryValidationError> {
    material_inventory.validate()?;
    if material_inventory.source_sha256() != allocated.inventory_sha256
        || material_inventory.facility() != allocated.facility
    {
        return Err(AllocatedMaterialInventoryValidationError::EvidenceMismatch);
    }
    allocated.validate()?;
    for material in allocated
        .methods
        .iter()
        .flat_map(|method| &method.tasks)
        .flat_map(|task| &task.materials)
    {
        if !material_binding_matches_inventory(material, material_inventory) {
            return Err(
                AllocatedMaterialInventoryValidationError::MaterialBindingMismatch {
                    input: material.input.clone(),
                },
            );
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum AllocatedMaterialInventoryValidationError {
    #[error("material inventory evidence is invalid: {0}")]
    InvalidMaterialInventory(#[from] MaterialLotInventoryValidationError),
    #[error(
        "material inventory evidence does not match the allocation inventory digest and facility"
    )]
    EvidenceMismatch,
    #[error("allocated program is intrinsically invalid: {0}")]
    InvalidAllocatedProgram(#[from] AllocatedProgramValidationError),
    #[error("allocated material input `{input}` does not match the retained material inventory")]
    MaterialBindingMismatch { input: LocalId },
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

fn material_binding_matches_inventory(
    binding: &SelectedMaterialBinding,
    inventory: &MaterialLotInventory,
) -> bool {
    let SelectedMaterialSource::MaterialLot {
        component,
        material_lot,
    } = &binding.source
    else {
        return true;
    };
    let Some(MaterialLotCandidates::Identified {
        component: expected_component,
        material_lots,
    }) = inventory.candidates(&binding.symbol)
    else {
        return false;
    };
    let expected_alternatives = material_lots
        .iter()
        .filter_map(|candidate| (candidate != material_lot).then_some(candidate.as_str()))
        .collect::<BTreeSet<_>>();
    component == expected_component
        && material_lots.contains(material_lot)
        && binding
            .interchangeable_alternatives
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            == expected_alternatives
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Debug, Error)]
pub enum MaterialLotInventoryError {
    #[error(
        "inventory lookup key `{symbol}` refers to both SBOL Components `{first}` and `{second}`"
    )]
    ConflictingDesignIdentity {
        symbol: String,
        first: String,
        second: String,
    },
    #[error(transparent)]
    InvalidInventory(#[from] MaterialLotInventoryValidationError),
}

/// Bind checked declaration identities to every active candidate lot in one facility snapshot.
///
/// Candidate preservation is intentional. Selecting among several equally usable lots is
/// facility allocation policy, so the chosen lot and its alternatives remain reviewable.
pub fn build_material_lot_inventory(
    modules: &[&CheckedModule],
    source_sha256: impl Into<String>,
    facility: impl Into<String>,
    lots_by_component: &BTreeMap<String, Vec<String>>,
) -> Result<MaterialLotInventory, MaterialLotInventoryError> {
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
    let inventory = MaterialLotInventory::new(source_sha256, facility, materials, artifacts);
    inventory.validate()?;
    Ok(inventory)
}

fn insert_declaration(
    entries: &mut BTreeMap<String, MaterialLotCandidates>,
    lookup_key: &str,
    identity: Option<&str>,
    lots_by_component: &BTreeMap<String, Vec<String>>,
) -> Result<(), MaterialLotInventoryError> {
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
) -> Result<(), MaterialLotInventoryError> {
    match (first, second) {
        (
            MaterialLotCandidates::Identified {
                component: first, ..
            },
            MaterialLotCandidates::Identified {
                component: second, ..
            },
        ) if first != second => Err(MaterialLotInventoryError::ConflictingDesignIdentity {
            symbol: symbol.to_owned(),
            first: first.clone(),
            second: second.clone(),
        }),
        (MaterialLotCandidates::Unidentified, MaterialLotCandidates::Identified { .. })
        | (MaterialLotCandidates::Identified { .. }, MaterialLotCandidates::Unidentified) => {
            Err(MaterialLotInventoryError::ConflictingDesignIdentity {
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
    use std::collections::BTreeSet;

    use lab_capability::{CapabilityKind, ControlMode, MethodId, OperationId, QualificationLevel};
    use lab_compiler::allocation::{
        AllocatedMethod, AllocatedProcedureTask, AllocatedRequirementBinding,
    };
    use lab_compiler::method::IntentOperationId;
    use lab_language::compile_module;

    use super::*;

    fn lots() -> BTreeMap<String, Vec<String>> {
        BTreeMap::from([(
            "https://example.org/inventory/input".to_owned(),
            vec!["https://example.org/inventory/input_lot".to_owned()],
        )])
    }

    fn evidence(
        materials: BTreeMap<String, MaterialLotCandidates>,
        artifacts: BTreeMap<String, MaterialLotCandidates>,
    ) -> MaterialLotInventory {
        MaterialLotInventory::new(
            "b".repeat(64),
            "https://example.org/facility",
            materials,
            artifacts,
        )
    }

    fn allocated_program(materials: Vec<SelectedMaterialBinding>) -> AllocatedProgram {
        AllocatedProgram {
            problem_sha256: "a".repeat(64),
            inventory_sha256: "b".repeat(64),
            facility: "https://example.org/facility".to_owned(),
            methods: vec![AllocatedMethod {
                choice: LocalId::new("choice").unwrap(),
                source_operation: IntentOperationId::new("example.operation").unwrap(),
                method: MethodId::new("https://example.org/method").unwrap(),
                after: Vec::new(),
                inputs: Vec::new(),
                outputs: Vec::new(),
                yields: Vec::new(),
                tasks: vec![AllocatedProcedureTask {
                    id: LocalId::new("choice::task").unwrap(),
                    operation: OperationId::new("https://example.org/operation").unwrap(),
                    program: None,
                    inputs: Vec::new(),
                    outputs: Vec::new(),
                    parameters: Vec::new(),
                    materials,
                    requirements: vec![AllocatedRequirementBinding {
                        id: LocalId::new("choice::requirement").unwrap(),
                        capability_kind: CapabilityKind::new("https://example.org/capability")
                            .unwrap(),
                        minimum_qualification: QualificationLevel::Executable,
                        accepted_control_modes: BTreeSet::from([ControlMode::Manual]),
                        offering: "https://example.org/offering".to_owned(),
                        asset: "https://example.org/asset".to_owned(),
                        observed_qualification: QualificationLevel::Executable.to_string(),
                        control_mode: ControlMode::Manual.to_string(),
                        parameters: Vec::new(),
                        procedure_implementation: None,
                        adapter: None,
                    }],
                }],
            }],
        }
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

        evidence(materials, artifacts).validate().unwrap();
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
            evidence(
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
            evidence(materials, artifacts).validate(),
            Err(MaterialLotInventoryValidationError::ConflictingSymbol { .. })
        ));
    }

    #[test]
    fn allocated_materials_match_exact_candidates_and_alternatives() {
        let selected = "https://example.org/lot/a".to_owned();
        let alternative = "https://example.org/lot/b".to_owned();
        let inventory = evidence(
            BTreeMap::from([(
                "sample".to_owned(),
                MaterialLotCandidates::Identified {
                    component: "https://example.org/component/sample".to_owned(),
                    material_lots: vec![selected.clone(), alternative.clone()],
                },
            )]),
            BTreeMap::new(),
        );
        let binding = SelectedMaterialBinding {
            input: LocalId::new("choice::material").unwrap(),
            symbol: "sample".to_owned(),
            source: SelectedMaterialSource::MaterialLot {
                component: "https://example.org/component/sample".to_owned(),
                material_lot: selected,
            },
            interchangeable_alternatives: vec![alternative],
        };
        let mut allocated = allocated_program(vec![binding]);
        validate_allocated_material_inventory(&allocated, &inventory).unwrap();

        allocated.methods[0].tasks[0].materials[0]
            .interchangeable_alternatives
            .clear();
        allocated.validate().unwrap();
        assert!(matches!(
            validate_allocated_material_inventory(&allocated, &inventory),
            Err(AllocatedMaterialInventoryValidationError::MaterialBindingMismatch { .. })
        ));
    }

    #[test]
    fn allocated_evidence_must_match_inventory_digest_and_facility() {
        let allocated = allocated_program(Vec::new());
        let wrong_inventory = MaterialLotInventory::new(
            "c".repeat(64),
            "https://example.org/facility",
            BTreeMap::new(),
            BTreeMap::new(),
        );
        assert_eq!(
            validate_allocated_material_inventory(&allocated, &wrong_inventory),
            Err(AllocatedMaterialInventoryValidationError::EvidenceMismatch)
        );
        validate_allocated_material_inventory(
            &allocated,
            &evidence(BTreeMap::new(), BTreeMap::new()),
        )
        .unwrap();
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
        let inventory = build_material_lot_inventory(
            &[&checked],
            "a".repeat(64),
            "https://example.org/inventory/facility",
            &lots(),
        )
        .unwrap();

        assert_eq!(
            inventory.facility(),
            "https://example.org/inventory/facility"
        );
        assert!(inventory.materials().contains_key("SKU-1"));
        assert!(inventory.materials().contains_key("source"));
        let expected = MaterialLotCandidates::Identified {
            component: "https://example.org/inventory/input".to_owned(),
            material_lots: vec!["https://example.org/inventory/input_lot".to_owned()],
        };
        assert_eq!(inventory.materials()["SKU-1"], expected);
        assert_eq!(inventory.materials()["source"], expected);
        assert_eq!(
            inventory.artifacts()["product"],
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
        let error = build_material_lot_inventory(
            &[&checked],
            "a".repeat(64),
            "https://example.org/inventory/facility",
            &lots(),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            MaterialLotInventoryError::ConflictingDesignIdentity { symbol, .. }
                if symbol == "same-sku"
        ));
    }
}
