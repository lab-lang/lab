//! The boundary between portable Lab packages and SBOLInventory facility graphs.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use sbol_inventory::vocabulary::{ControlMode, Qualification};
use sbol_inventory::{
    CandidateQuery, InventoryDocument, InventoryValidationReport, ScalarValueRef,
};
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

/// Owned planning facts for one exact Asset governed by the selected facility.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FacilityAsset {
    pub identity: Iri,
    pub located_in: Option<Iri>,
    pub part_of: Option<Iri>,
    pub position: Option<String>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub offerings: Vec<FacilityCapabilityOffering>,
}

/// One exact installed capability offering owned by a [`FacilityAsset`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FacilityCapabilityOffering {
    pub identity: Iri,
    pub capability_kind: Iri,
    pub qualification: Qualification,
    pub control_mode: ControlMode,
    pub parameters: Vec<FacilityCapabilityParameter>,
    /// True only when both the offering and its complete Asset/Zone containment chain are active.
    pub effectively_active: bool,
}

/// One exact typed parameter owned by a capability offering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FacilityCapabilityParameter {
    pub identity: Iri,
    pub property_kind: Iri,
    pub value: FacilityScalarValue,
    pub unit: Option<Iri>,
}

/// The five scalar value forms allowed by SBOLInventory Profile 0.2.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FacilityScalarValue {
    Text(String),
    Integer(String),
    Real(String),
    Boolean(bool),
    Iri(Iri),
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

    /// Resolves an exact Asset IRI and owns the profile facts facility planning may inspect.
    pub fn facility_asset(&self, asset: &str) -> Result<FacilityAsset, FacilityAssetError> {
        let identity =
            Iri::new(asset.to_owned()).map_err(|error| FacilityAssetError::InvalidAssetIri {
                asset: asset.to_owned(),
                message: error.to_string(),
            })?;
        let resource = Resource::Iri(identity.clone());
        let view =
            self.document
                .asset(&resource)
                .ok_or_else(|| FacilityAssetError::AssetNotFound {
                    asset: identity.clone(),
                })?;
        let selected_facility = Resource::Iri(self.facility.clone());
        if view.facility_id() != Some(&selected_facility) {
            return Err(FacilityAssetError::WrongFacility {
                asset: identity,
                selected: self.facility.clone(),
                actual: view
                    .facility_id()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "none".to_owned()),
            });
        }

        let validated = self.validated();
        let mut offerings = Vec::new();
        for offering in view.capabilities() {
            let offering_identity = required_iri(
                offering.identity(),
                &resource,
                FacilityAssetReference::CapabilityOffering,
            )?;
            let capability_kind = offering
                .kind()
                .expect("validated offerings have exactly one capability kind")
                .clone();
            let qualification = offering
                .qualification()
                .expect("validated offerings have a known qualification");
            let control_mode = offering
                .control_mode()
                .expect("validated offerings have a known control mode");
            let mut parameters = Vec::new();
            for parameter in offering.parameters() {
                let parameter_identity =
                    parameter.identity().as_iri().cloned().ok_or_else(|| {
                        FacilityAssetError::NonIriCapabilityParameter {
                            offering: offering.identity().clone(),
                            parameter: parameter.identity().clone(),
                        }
                    })?;
                let value = match parameter
                    .value()
                    .expect("validated PropertyValues have exactly one typed value")
                {
                    ScalarValueRef::Text(value) => FacilityScalarValue::Text(value.to_owned()),
                    ScalarValueRef::Integer(value) => {
                        FacilityScalarValue::Integer(value.to_owned())
                    }
                    ScalarValueRef::Real(value) => FacilityScalarValue::Real(value.to_owned()),
                    ScalarValueRef::Boolean(value) => FacilityScalarValue::Boolean(value),
                    ScalarValueRef::Iri(value) => FacilityScalarValue::Iri(value.clone()),
                };
                parameters.push(FacilityCapabilityParameter {
                    identity: parameter_identity,
                    property_kind: parameter
                        .kind()
                        .expect("validated PropertyValues have one property kind")
                        .clone(),
                    value,
                    unit: parameter.unit().cloned(),
                });
            }
            parameters.sort_by(|left, right| left.identity.cmp(&right.identity));
            let query = CandidateQuery::new(capability_kind.clone(), Qualification::Discovered)
                .within_facility(self.facility.clone());
            let effectively_active =
                validated
                    .find_qualified_assets(&query)
                    .iter()
                    .any(|candidate| {
                        candidate.asset().identity() == &resource
                            && candidate.offering().identity() == offering.identity()
                    });
            offerings.push(FacilityCapabilityOffering {
                identity: offering_identity,
                capability_kind,
                qualification,
                control_mode,
                parameters,
                effectively_active,
            });
        }
        offerings.sort_by(|left, right| left.identity.cmp(&right.identity));

        Ok(FacilityAsset {
            identity,
            located_in: optional_iri(
                view.located_in_id(),
                &resource,
                FacilityAssetReference::Location,
            )?,
            part_of: optional_iri(
                view.part_of_id(),
                &resource,
                FacilityAssetReference::ParentAsset,
            )?,
            position: view.position().map(str::to_owned),
            manufacturer: view.manufacturer().map(str::to_owned),
            model: view.model().map(str::to_owned),
            offerings,
        })
    }

    /// Owns every Asset governed by the selected facility in deterministic IRI order.
    pub fn facility_assets(&self) -> Result<Vec<FacilityAsset>, FacilityAssetError> {
        let selected = Resource::Iri(self.facility.clone());
        let mut identities = self
            .document
            .assets()
            .filter(|asset| asset.facility_id() == Some(&selected))
            .map(|asset| {
                asset.identity().as_iri().cloned().ok_or_else(|| {
                    FacilityAssetError::NonIriAssetIdentity {
                        identity: asset.identity().clone(),
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        identities.sort();
        identities
            .iter()
            .map(|identity| self.facility_asset(identity.as_str()))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FacilityAssetReference {
    CapabilityOffering,
    Location,
    ParentAsset,
}

impl std::fmt::Display for FacilityAssetReference {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::CapabilityOffering => "capability offering",
            Self::Location => "location",
            Self::ParentAsset => "parent Asset",
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FacilityAssetError {
    #[error("adapter asset `{asset}` is not an absolute IRI: {message}")]
    InvalidAssetIri { asset: String, message: String },
    #[error("adapter asset `{asset}` is not a fac:Asset in the inventory document")]
    AssetNotFound { asset: Iri },
    #[error("validated fac:Asset `{identity}` does not have an IRI identity")]
    NonIriAssetIdentity { identity: Resource },
    #[error(
        "adapter asset `{asset}` belongs to facility `{actual}`, not selected facility `{selected}`"
    )]
    WrongFacility {
        asset: Iri,
        selected: Iri,
        actual: String,
    },
    #[error("adapter asset `{asset}` has a non-IRI {reference} `{value}`")]
    NonIriReference {
        asset: Resource,
        reference: FacilityAssetReference,
        value: Resource,
    },
    #[error("capability offering `{offering}` owns non-IRI parameter `{parameter}`")]
    NonIriCapabilityParameter {
        offering: Resource,
        parameter: Resource,
    },
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

fn optional_iri(
    value: Option<&Resource>,
    asset: &Resource,
    reference: FacilityAssetReference,
) -> Result<Option<Iri>, FacilityAssetError> {
    value
        .map(|value| required_iri(value, asset, reference))
        .transpose()
}

fn required_iri(
    value: &Resource,
    asset: &Resource,
    reference: FacilityAssetReference,
) -> Result<Iri, FacilityAssetError> {
    value
        .as_iri()
        .cloned()
        .ok_or_else(|| FacilityAssetError::NonIriReference {
            asset: asset.clone(),
            reference,
            value: value.clone(),
        })
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
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

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
    fac:controlMode fac:ReviewedFileControl ; fac:isActive true ;
    fac:parameter <https://example.org/sbolinventory/cycler/thermal_cycling/temperature> .
<https://example.org/sbolinventory/cycler/thermal_cycling/temperature>
    a sbol:Identified, fac:PropertyValue ; sbol:displayId "temperature" ;
    fac:propertyKind cap:Temperature ; fac:realValue "37.0"^^xsd:double ;
    fac:unit <http://qudt.org/vocab/unit/DEG_C> .
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

    #[test]
    fn resolves_exact_asset_and_capability_offering_facts() {
        let package = TempDir::new().unwrap();
        write_inventory(package.path(), MINIMAL);
        let snapshot =
            InventorySnapshot::load(package.path(), "inventory/catalog.ttl", None).unwrap();

        let asset = snapshot
            .facility_asset("https://example.org/sbolinventory/cycler")
            .unwrap();
        let assets = snapshot.facility_assets().unwrap();

        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0], asset);
        assert_eq!(
            asset.identity.as_str(),
            "https://example.org/sbolinventory/cycler"
        );
        assert_eq!(
            asset.located_in.as_ref().map(Iri::as_str),
            Some("https://example.org/sbolinventory/room")
        );
        assert_eq!(asset.offerings.len(), 1);
        let offering = &asset.offerings[0];
        assert_eq!(
            offering.identity.as_str(),
            "https://example.org/sbolinventory/cycler/thermal_cycling"
        );
        assert_eq!(
            offering.capability_kind.as_str(),
            "https://draggon.org/ns/capability#ThermalCycling"
        );
        assert_eq!(offering.qualification, Qualification::Plannable);
        assert_eq!(offering.control_mode, ControlMode::ReviewedFile);
        assert_eq!(offering.parameters.len(), 1);
        assert_eq!(
            offering.parameters[0].identity.as_str(),
            "https://example.org/sbolinventory/cycler/thermal_cycling/temperature"
        );
        assert_eq!(
            offering.parameters[0].property_kind.as_str(),
            "https://draggon.org/ns/capability#Temperature"
        );
        assert_eq!(
            offering.parameters[0].value,
            FacilityScalarValue::Real("37.0".to_owned())
        );
        assert_eq!(
            offering.parameters[0].unit.as_ref().map(Iri::as_str),
            Some("http://qudt.org/vocab/unit/DEG_C")
        );
        assert!(offering.effectively_active);
    }

    #[test]
    fn exact_asset_resolution_rejects_missing_and_cross_facility_assets() {
        let package = TempDir::new().unwrap();
        let several = format!(
            "{MINIMAL}\nex:second a sbol:TopLevel, fac:Facility ; sbol:displayId \"second\" ; sbol:hasNamespace <https://example.org/sbolinventory> .\n"
        );
        write_inventory(package.path(), &several);

        let first = InventorySnapshot::load(
            package.path(),
            "inventory/catalog.ttl",
            Some("https://example.org/sbolinventory/facility"),
        )
        .unwrap();
        assert!(matches!(
            first.facility_asset("not-an-iri"),
            Err(FacilityAssetError::InvalidAssetIri { .. })
        ));
        assert!(matches!(
            first.facility_asset("https://example.org/sbolinventory/missing"),
            Err(FacilityAssetError::AssetNotFound { .. })
        ));

        let second = InventorySnapshot::load(
            package.path(),
            "inventory/catalog.ttl",
            Some("https://example.org/sbolinventory/second"),
        )
        .unwrap();
        assert!(matches!(
            second.facility_asset("https://example.org/sbolinventory/cycler"),
            Err(FacilityAssetError::WrongFacility { .. })
        ));
    }
}
