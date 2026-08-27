//! The boundary between portable Lab packages and SBOLInventory facility graphs.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use sbol_inventory::{InventoryDocument, InventoryValidationReport};
use sbol3::{Iri, RdfFormat, ReadError, Resource};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// A fully validated inventory document with its package-level facility selection frozen.
#[derive(Clone, Debug)]
pub struct InventorySnapshot {
    document: InventoryDocument,
    source_path: PathBuf,
    source_sha256: String,
    facility: Iri,
}

/// Active material lots in one selected facility, indexed by the exact SBOL Component they realize.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialLotCatalog {
    facility: Iri,
    by_component: BTreeMap<Iri, Vec<Iri>>,
}

impl MaterialLotCatalog {
    pub fn facility(&self) -> &Iri {
        &self.facility
    }

    /// Returns active lot IRIs in deterministic order for one exact Component IRI.
    pub fn candidates(&self, component: &Iri) -> &[Iri] {
        self.by_component
            .get(component)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn components(&self) -> impl Iterator<Item = (&Iri, &[Iri])> {
        self.by_component
            .iter()
            .map(|(component, lots)| (component, lots.as_slice()))
    }
}

impl InventorySnapshot {
    /// Loads one package-relative inventory document and applies Lab's exact facility-selection rules.
    pub fn load(
        package_root: impl AsRef<Path>,
        document_path: impl AsRef<Path>,
        facility: Option<&str>,
    ) -> Result<Self, InventoryLoadError> {
        let package_root = package_root.as_ref();
        let document_path = document_path.as_ref();
        validate_document_path(document_path)?;

        let canonical_root = fs::canonicalize(package_root).map_err(|source| {
            InventoryLoadError::CanonicalizePackageRoot {
                path: package_root.to_path_buf(),
                source,
            }
        })?;
        let joined_path = canonical_root.join(document_path);
        let source_path =
            fs::canonicalize(&joined_path).map_err(|source| InventoryLoadError::Read {
                path: joined_path.clone(),
                source,
            })?;
        if !source_path.starts_with(&canonical_root) {
            return Err(InventoryLoadError::DocumentOutsidePackage {
                document: document_path.to_path_buf(),
            });
        }

        let format = RdfFormat::from_path(&source_path).ok_or_else(|| {
            InventoryLoadError::UnsupportedFormat {
                path: document_path.to_path_buf(),
            }
        })?;
        let bytes = fs::read(&source_path).map_err(|source| InventoryLoadError::Read {
            path: source_path.clone(),
            source,
        })?;
        let source_sha256 = sha256_hex(&bytes);
        let input = String::from_utf8(bytes).map_err(|source| InventoryLoadError::Utf8 {
            path: source_path.clone(),
            source,
        })?;
        let document = InventoryDocument::read(&input, format).map_err(|source| {
            InventoryLoadError::Parse {
                path: source_path.clone(),
                source,
            }
        })?;
        if let Err(report) = document.check() {
            return Err(InventoryLoadError::InvalidProfile { report });
        }

        let available = facility_iris(&document)?;
        let facility = select_facility(facility, &available)?;
        Ok(Self {
            document,
            source_path,
            source_sha256,
            facility,
        })
    }

    pub fn document(&self) -> &InventoryDocument {
        &self.document
    }

    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }

    pub fn facility(&self) -> &Iri {
        &self.facility
    }

    /// Reconstitutes the query-safe profile view. Construction already proved this succeeds.
    pub fn validated(&self) -> sbol_inventory::ValidatedInventory<'_> {
        self.document
            .check()
            .expect("an InventorySnapshot contains a validated immutable document")
    }

    /// Indexes only active lots governed by the selected facility.
    ///
    /// Availability is never inferred from names, display IDs, identity prefixes, or a lot's location.
    pub fn active_material_lots(&self) -> Result<MaterialLotCatalog, MaterialLotCatalogError> {
        let facility = Resource::Iri(self.facility.clone());
        let mut by_component = BTreeMap::<Iri, Vec<Iri>>::new();
        for lot in self
            .document
            .material_lots()
            .filter(|lot| lot.facility_id() == Some(&facility) && lot.is_active() == Some(true))
        {
            let lot_identity = lot.identity().as_iri().cloned().ok_or_else(|| {
                MaterialLotCatalogError::NonIriMaterialLot {
                    identity: lot.identity().clone(),
                }
            })?;
            let built = lot
                .built_id()
                .expect("validated MaterialLots have exactly one sbol:built reference")
                .as_iri()
                .cloned()
                .ok_or_else(|| MaterialLotCatalogError::NonIriBuilt {
                    material_lot: lot.identity().clone(),
                })?;
            by_component.entry(built).or_default().push(lot_identity);
        }
        for lots in by_component.values_mut() {
            lots.sort();
        }
        Ok(MaterialLotCatalog {
            facility: self.facility.clone(),
            by_component,
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MaterialLotCatalogError {
    #[error("validated MaterialLot `{identity}` does not have an IRI identity")]
    NonIriMaterialLot { identity: Resource },
    #[error("validated MaterialLot `{material_lot}` has a non-IRI sbol:built reference")]
    NonIriBuilt { material_lot: Resource },
}

#[derive(Debug, Error)]
pub enum InventoryLoadError {
    #[error("inventory document path must be a non-empty package-relative path without '..': {0}")]
    InvalidDocumentPath(PathBuf),
    #[error("failed to resolve package root `{path}`")]
    CanonicalizePackageRoot {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read inventory document `{path}`")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("inventory document `{document}` resolves outside its package")]
    DocumentOutsidePackage { document: PathBuf },
    #[error("unsupported inventory RDF format for `{path}`; use .ttl, .rdf, .jsonld, or .nt")]
    UnsupportedFormat { path: PathBuf },
    #[error("inventory document `{path}` is not UTF-8")]
    Utf8 {
        path: PathBuf,
        #[source]
        source: std::string::FromUtf8Error,
    },
    #[error("failed to parse inventory document `{path}`")]
    Parse {
        path: PathBuf,
        #[source]
        source: ReadError,
    },
    #[error("inventory document does not conform to SBOLInventory Profile 0.2: {report}")]
    InvalidProfile {
        #[source]
        report: InventoryValidationReport,
    },
    #[error("facility selector `{facility}` is not an absolute IRI: {message}")]
    InvalidFacilityIri { facility: String, message: String },
    #[error("inventory document contains no fac:Facility; add one or select another document")]
    NoFacilities,
    #[error(
        "inventory document contains several facilities ({facilities}); set inventory.facility"
    )]
    MultipleFacilities { facilities: String },
    #[error(
        "facility `{facility}` is not a fac:Facility in the inventory document; available facilities: {available}"
    )]
    FacilityNotFound { facility: String, available: String },
    #[error("validated facility `{identity}` does not have an IRI identity")]
    NonIriFacility { identity: Resource },
}

fn validate_document_path(path: &Path) -> Result<(), InventoryLoadError> {
    let invalid = path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });
    if invalid {
        Err(InventoryLoadError::InvalidDocumentPath(path.to_path_buf()))
    } else {
        Ok(())
    }
}

fn facility_iris(document: &InventoryDocument) -> Result<Vec<Iri>, InventoryLoadError> {
    let mut facilities = document
        .facilities()
        .map(|facility| {
            facility.identity().as_iri().cloned().ok_or_else(|| {
                InventoryLoadError::NonIriFacility {
                    identity: facility.identity().clone(),
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    facilities.sort();
    Ok(facilities)
}

fn select_facility(requested: Option<&str>, available: &[Iri]) -> Result<Iri, InventoryLoadError> {
    if let Some(requested) = requested {
        let selected = Iri::new(requested.to_owned()).map_err(|error| {
            InventoryLoadError::InvalidFacilityIri {
                facility: requested.to_owned(),
                message: error.to_string(),
            }
        })?;
        if available.contains(&selected) {
            return Ok(selected);
        }
        return Err(InventoryLoadError::FacilityNotFound {
            facility: requested.to_owned(),
            available: render_facilities(available),
        });
    }

    match available {
        [] => Err(InventoryLoadError::NoFacilities),
        [facility] => Ok(facility.clone()),
        facilities => Err(InventoryLoadError::MultipleFacilities {
            facilities: render_facilities(facilities),
        }),
    }
}

fn render_facilities(facilities: &[Iri]) -> String {
    if facilities.is_empty() {
        "none".to_owned()
    } else {
        facilities
            .iter()
            .map(Iri::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::TempDir;

    use super::*;

    const MINIMAL: &str = r#"@prefix cap: <https://draggon.org/ns/capability#> .
@prefix ex: <https://example.org/sbolinventory/> .
@prefix fac: <https://draggon.org/ns/facility#> .
@prefix sbol: <http://sbols.org/v3#> .

ex:facility a sbol:TopLevel, fac:Facility ; sbol:displayId "facility" ;
    sbol:hasNamespace <https://example.org/sbolinventory> ; sbol:name "Example facility" .
ex:room a sbol:TopLevel, fac:Zone ; sbol:displayId "room" ;
    sbol:hasNamespace <https://example.org/sbolinventory> ; fac:facility ex:facility ;
    fac:zoneKind fac:Room ; fac:isActive true .
ex:cycler a sbol:TopLevel, fac:Asset ; sbol:displayId "cycler" ;
    sbol:hasNamespace <https://example.org/sbolinventory> ; fac:facility ex:facility ;
    fac:assetKind fac:Instrument ; fac:locatedIn ex:room ; fac:isActive true ;
    fac:capability <https://example.org/sbolinventory/cycler/thermal_cycling> .
<https://example.org/sbolinventory/cycler/thermal_cycling>
    a sbol:Identified, fac:CapabilityOffering ; sbol:displayId "thermal_cycling" ;
    fac:capabilityKind cap:ThermalCycling ; fac:qualification fac:Plannable ;
    fac:controlMode fac:ReviewedFileControl ; fac:isActive true .
"#;

    fn write_inventory(root: &Path, contents: &str) {
        fs::create_dir(root.join("inventory")).unwrap();
        fs::write(root.join("inventory/catalog.ttl"), contents).unwrap();
    }

    #[test]
    fn loads_validates_hashes_and_selects_the_only_facility() {
        let package = TempDir::new().unwrap();
        write_inventory(package.path(), MINIMAL);

        let snapshot =
            InventorySnapshot::load(package.path(), "inventory/catalog.ttl", None).unwrap();

        assert_eq!(
            snapshot.facility().as_str(),
            "https://example.org/sbolinventory/facility"
        );
        assert_eq!(snapshot.source_sha256(), sha256_hex(MINIMAL.as_bytes()));
        assert_eq!(snapshot.validated().facilities().count(), 1);
        assert!(snapshot.source_path().is_absolute());
    }

    #[test]
    fn explicit_facility_selection_is_exact() {
        let package = TempDir::new().unwrap();
        let several = format!(
            "{MINIMAL}\nex:second a sbol:TopLevel, fac:Facility ; sbol:displayId \"second\" ; sbol:hasNamespace <https://example.org/sbolinventory> .\n"
        );
        write_inventory(package.path(), &several);

        let omitted =
            InventorySnapshot::load(package.path(), "inventory/catalog.ttl", None).unwrap_err();
        assert!(matches!(
            omitted,
            InventoryLoadError::MultipleFacilities { .. }
        ));

        let selected = InventorySnapshot::load(
            package.path(),
            "inventory/catalog.ttl",
            Some("https://example.org/sbolinventory/second"),
        )
        .unwrap();
        assert_eq!(
            selected.facility().as_str(),
            "https://example.org/sbolinventory/second"
        );

        let missing = InventorySnapshot::load(
            package.path(),
            "inventory/catalog.ttl",
            Some("https://example.org/sbolinventory/missing"),
        )
        .unwrap_err();
        assert!(matches!(
            missing,
            InventoryLoadError::FacilityNotFound { .. }
        ));
    }

    #[test]
    fn rejects_invalid_profiles_and_non_portable_paths() {
        let package = TempDir::new().unwrap();
        write_inventory(
            package.path(),
            &MINIMAL.replace("fac:isActive true", "fac:isActive \"yes\""),
        );

        let invalid =
            InventorySnapshot::load(package.path(), "inventory/catalog.ttl", None).unwrap_err();
        assert!(matches!(invalid, InventoryLoadError::InvalidProfile { .. }));

        let escaping = InventorySnapshot::load(package.path(), "../catalog.ttl", None).unwrap_err();
        assert!(matches!(
            escaping,
            InventoryLoadError::InvalidDocumentPath(_)
        ));
    }

    #[test]
    fn indexes_active_material_lots_by_exact_component_within_the_selected_facility() {
        let package = TempDir::new().unwrap();
        let contents = format!(
            r#"{MINIMAL}
@prefix inv: <https://draggon.org/ns/inventory#> .

ex:design a sbol:Component ; sbol:displayId "design" ;
    sbol:hasNamespace <https://example.org/sbolinventory> ;
    sbol:type <https://identifiers.org/SBO:0000251> .
ex:lot_b a sbol:Implementation ; sbol:displayId "lot_b" ;
    sbol:hasNamespace <https://example.org/sbolinventory> ; sbol:built ex:design ;
    fac:materialKind inv:DnaSample ; fac:facility ex:facility ; fac:isActive true .
ex:lot_a a sbol:Implementation ; sbol:displayId "lot_a" ;
    sbol:hasNamespace <https://example.org/sbolinventory> ; sbol:built ex:design ;
    fac:materialKind inv:DnaSample ; fac:facility ex:facility ; fac:isActive true .
ex:retired_lot a sbol:Implementation ; sbol:displayId "retired_lot" ;
    sbol:hasNamespace <https://example.org/sbolinventory> ; sbol:built ex:design ;
    fac:materialKind inv:DnaSample ; fac:facility ex:facility ; fac:isActive false .
"#
        );
        write_inventory(package.path(), &contents);

        let snapshot =
            InventorySnapshot::load(package.path(), "inventory/catalog.ttl", None).unwrap();
        let catalog = snapshot.active_material_lots().unwrap();
        let design = Iri::new("https://example.org/sbolinventory/design".to_owned()).unwrap();
        let candidates = catalog
            .candidates(&design)
            .iter()
            .map(Iri::as_str)
            .collect::<Vec<_>>();

        assert_eq!(catalog.facility(), snapshot.facility());
        assert_eq!(
            candidates,
            [
                "https://example.org/sbolinventory/lot_a",
                "https://example.org/sbolinventory/lot_b",
            ]
        );
        assert!(
            catalog
                .components()
                .all(|(component, _)| component == &design)
        );
        let display_name = Iri::new("https://example.org/design".to_owned()).unwrap();
        assert!(catalog.candidates(&display_name).is_empty());
    }
}
