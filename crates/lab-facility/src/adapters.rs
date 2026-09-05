//! Exact operational bindings between configured adapters and SBOLInventory offerings.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lab_capability::{
    CapabilityKind, ControlMode, OperationId, ProcedureContractId, ProcedureImplementationId,
};
use lab_compiler::procedure::ProgramFeature;
use lab_inventory::{FacilityAssetError, FacilityScalarValue, InventorySnapshot};
use sbol_inventory::vocabulary::Qualification;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use lab_adapters::{
    AdapterDescriptor, AdapterServices, ProcedureImplementationDescriptor, ValidatedAdapterProfile,
    adapter_catalog,
};

pub const ADAPTER_BINDINGS_SCHEMA_VERSION: &str = "lab.adapter-bindings.v4";

#[derive(Clone, Debug)]
pub struct AdapterBindingRequest {
    pub asset: String,
    pub driver: String,
    pub profile_path: PathBuf,
    pub profile: ValidatedAdapterProfile,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AdapterBindingSnapshot {
    pub schema_version: String,
    pub inventory_sha256: String,
    pub facility: String,
    pub bindings: Vec<ResolvedAdapterBinding>,
}

impl AdapterBindingSnapshot {
    pub fn resolve(
        inventory: &InventorySnapshot,
        requests: Vec<AdapterBindingRequest>,
    ) -> Result<Self, AdapterBindingError> {
        let catalog = adapter_catalog().map_err(|error| AdapterBindingError::Catalog {
            message: error.to_string(),
        })?;
        let mut bindings = Vec::new();
        let mut seen = BTreeSet::new();
        for request in requests {
            if request.profile.driver != request.driver {
                return Err(AdapterBindingError::ProfileDriverMismatch {
                    asset: request.asset,
                    binding_driver: request.driver,
                    profile_driver: request.profile.driver,
                });
            }
            if !seen.insert((request.asset.clone(), request.driver.clone())) {
                return Err(AdapterBindingError::DuplicateBinding {
                    asset: request.asset,
                    driver: request.driver,
                });
            }
            let descriptor = catalog
                .adapters
                .iter()
                .find(|adapter| adapter.id == request.driver)
                .ok_or_else(|| AdapterBindingError::UnknownDriver {
                    asset: request.asset.clone(),
                    driver: request.driver.clone(),
                })?;
            let asset = inventory.facility_asset(&request.asset)?;
            let mut offerings = asset
                .offerings
                .iter()
                .filter(|offering| {
                    adapter_supports_capability(
                        descriptor,
                        offering.capability_kind.as_str(),
                        offering.control_mode.iri(),
                    )
                })
                .map(|offering| BoundCapabilityOffering {
                    offering: offering.identity.as_str().to_owned(),
                    capability_kind: offering.capability_kind.as_str().to_owned(),
                    qualification: offering.qualification.iri().to_owned(),
                    control_mode: offering.control_mode.iri().to_owned(),
                    parameters: offering
                        .parameters
                        .iter()
                        .map(|parameter| BoundCapabilityParameter {
                            parameter: parameter.identity.as_str().to_owned(),
                            property_kind: parameter.property_kind.as_str().to_owned(),
                            value: match &parameter.value {
                                FacilityScalarValue::Text(value) => {
                                    BoundCapabilityParameterValue::Text {
                                        value: value.clone(),
                                    }
                                }
                                FacilityScalarValue::Integer(value) => {
                                    BoundCapabilityParameterValue::Integer {
                                        value: value.clone(),
                                    }
                                }
                                FacilityScalarValue::Real(value) => {
                                    BoundCapabilityParameterValue::Real {
                                        value: value.clone(),
                                    }
                                }
                                FacilityScalarValue::Boolean(value) => {
                                    BoundCapabilityParameterValue::Boolean { value: *value }
                                }
                                FacilityScalarValue::Iri(value) => {
                                    BoundCapabilityParameterValue::Iri {
                                        value: value.as_str().to_owned(),
                                    }
                                }
                            },
                            unit: parameter.unit.as_ref().map(|unit| unit.as_str().to_owned()),
                        })
                        .collect(),
                    effectively_active: offering.effectively_active,
                    planning_eligible: offering.effectively_active
                        && supports_service(
                            descriptor,
                            offering.capability_kind.as_str(),
                            offering.control_mode.iri(),
                            |services| services.planning,
                        )
                        && offering.qualification >= Qualification::Plannable,
                    simulation_eligible: offering.effectively_active
                        && supports_service(
                            descriptor,
                            offering.capability_kind.as_str(),
                            offering.control_mode.iri(),
                            |services| services.simulation,
                        )
                        && offering.qualification >= Qualification::Simulatable,
                    execution_eligible: offering.effectively_active
                        && supports_service(
                            descriptor,
                            offering.capability_kind.as_str(),
                            offering.control_mode.iri(),
                            |services| services.runtime,
                        )
                        && offering.qualification >= Qualification::Executable,
                })
                .collect::<Vec<_>>();
            offerings.sort_by(|left, right| left.offering.cmp(&right.offering));
            if offerings.is_empty() {
                return Err(AdapterBindingError::NoCompatibleOffering {
                    asset: request.asset,
                    driver: request.driver,
                    adapter_capabilities: render_set(&descriptor.capabilities),
                    adapter_control_modes: render_set(&descriptor.control_modes),
                });
            }
            bindings.push(ResolvedAdapterBinding {
                asset: asset.identity.as_str().to_owned(),
                driver: request.driver,
                profile_path: request.profile_path,
                profile_sha256: request.profile.sha256,
                features: descriptor.features.clone(),
                accepted_run_formats: descriptor.accepted_run_formats.clone(),
                emitted_run_formats: descriptor.emitted_run_formats.clone(),
                services: descriptor.services.clone(),
                procedure_implementations: descriptor
                    .procedure_implementations
                    .iter()
                    .map(BoundProcedureImplementation::from)
                    .collect(),
                offerings,
            });
        }
        bindings
            .sort_by(|left, right| (&left.asset, &left.driver).cmp(&(&right.asset, &right.driver)));
        Ok(Self {
            schema_version: ADAPTER_BINDINGS_SCHEMA_VERSION.to_owned(),
            inventory_sha256: inventory.source_sha256().to_owned(),
            facility: inventory.facility().as_str().to_owned(),
            bindings,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedAdapterBinding {
    pub asset: String,
    pub driver: String,
    pub profile_path: PathBuf,
    pub profile_sha256: String,
    pub features: BTreeSet<String>,
    pub accepted_run_formats: BTreeSet<String>,
    pub emitted_run_formats: BTreeSet<String>,
    pub services: AdapterServices,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub procedure_implementations: Vec<BoundProcedureImplementation>,
    pub offerings: Vec<BoundCapabilityOffering>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundProcedureImplementation {
    pub id: ProcedureImplementationId,
    pub contract: ProcedureContractId,
    pub operations: BTreeSet<OperationId>,
    pub capability_kinds: BTreeSet<CapabilityKind>,
    pub control_modes: BTreeSet<ControlMode>,
    pub accepted_run_formats: BTreeSet<String>,
    pub emitted_run_formats: BTreeSet<String>,
    pub program_features: BTreeMap<OperationId, BTreeSet<ProgramFeature>>,
    pub services: AdapterServices,
}

impl From<&ProcedureImplementationDescriptor> for BoundProcedureImplementation {
    fn from(value: &ProcedureImplementationDescriptor) -> Self {
        Self {
            id: value.id.clone(),
            contract: value.contract.clone(),
            operations: value.operations.clone(),
            capability_kinds: value.capability_kinds.clone(),
            control_modes: value.control_modes.clone(),
            accepted_run_formats: value.accepted_run_formats.clone(),
            emitted_run_formats: value.emitted_run_formats.clone(),
            program_features: value.program_features.clone(),
            services: value.services.clone(),
        }
    }
}

fn adapter_supports_capability(
    descriptor: &AdapterDescriptor,
    capability_kind: &str,
    control_mode: &str,
) -> bool {
    legacy_supports(descriptor, capability_kind, control_mode)
        || descriptor
            .procedure_implementations
            .iter()
            .any(|implementation| {
                implementation_supports(implementation, capability_kind, control_mode)
            })
}

fn supports_service(
    descriptor: &AdapterDescriptor,
    capability_kind: &str,
    control_mode: &str,
    service: impl Fn(&AdapterServices) -> bool,
) -> bool {
    (legacy_supports(descriptor, capability_kind, control_mode) && service(&descriptor.services))
        || descriptor
            .procedure_implementations
            .iter()
            .any(|implementation| {
                implementation_supports(implementation, capability_kind, control_mode)
                    && service(&implementation.services)
            })
}

fn legacy_supports(
    descriptor: &AdapterDescriptor,
    capability_kind: &str,
    control_mode: &str,
) -> bool {
    descriptor
        .capabilities
        .iter()
        .any(|kind| kind.as_str() == capability_kind)
        && descriptor
            .control_modes
            .iter()
            .any(|mode| mode.iri() == control_mode)
}

fn implementation_supports(
    implementation: &ProcedureImplementationDescriptor,
    capability_kind: &str,
    control_mode: &str,
) -> bool {
    implementation
        .capability_kinds
        .iter()
        .any(|kind| kind.as_str() == capability_kind)
        && implementation
            .control_modes
            .iter()
            .any(|mode| mode.iri() == control_mode)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundCapabilityOffering {
    pub offering: String,
    pub capability_kind: String,
    pub qualification: String,
    pub control_mode: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<BoundCapabilityParameter>,
    pub effectively_active: bool,
    pub planning_eligible: bool,
    pub simulation_eligible: bool,
    pub execution_eligible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundCapabilityParameter {
    pub parameter: String,
    pub property_kind: String,
    #[serde(flatten)]
    pub value: BoundCapabilityParameterValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "value_type", rename_all = "snake_case")]
pub enum BoundCapabilityParameterValue {
    Text { value: String },
    Integer { value: String },
    Real { value: String },
    Boolean { value: bool },
    Iri { value: String },
}

#[derive(Debug, Error)]
pub enum AdapterBindingError {
    #[error("failed to load the compiler adapter catalog: {message}")]
    Catalog { message: String },
    #[error(transparent)]
    Asset(#[from] FacilityAssetError),
    #[error("asset `{asset}` binds unknown adapter driver `{driver}`")]
    UnknownDriver { asset: String, driver: String },
    #[error(
        "asset `{asset}` binds driver `{binding_driver}`, but its validated profile is for `{profile_driver}`"
    )]
    ProfileDriverMismatch {
        asset: String,
        binding_driver: String,
        profile_driver: String,
    },
    #[error("asset `{asset}` binds adapter `{driver}` more than once")]
    DuplicateBinding { asset: String, driver: String },
    #[error(
        "asset `{asset}` has no offering supported by adapter `{driver}`; adapter capabilities: {adapter_capabilities}; adapter control modes: {adapter_control_modes}"
    )]
    NoCompatibleOffering {
        asset: String,
        driver: String,
        adapter_capabilities: String,
        adapter_control_modes: String,
    },
}

fn render_set<T>(values: &BTreeSet<T>) -> String
where
    T: std::fmt::Display,
{
    if values.is_empty() {
        "none".to_owned()
    } else {
        values
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use lab_adapters::validate_adapter_profile;

    use super::*;

    const INVENTORY: &str = r#"@prefix cap: <https://sbol.io/ns/capability#> .
@prefix ex: <https://example.org/facility/> .
@prefix fac: <https://sbol.io/ns/facility#> .
@prefix sbol: <http://sbols.org/v3#> .

ex:facility a sbol:TopLevel, fac:Facility ; sbol:displayId "facility" ;
    sbol:hasNamespace <https://example.org/facility> .
ex:room a sbol:TopLevel, fac:Zone ; sbol:displayId "room" ;
    sbol:hasNamespace <https://example.org/facility> ; fac:facility ex:facility ;
    fac:zoneKind fac:Room ; fac:isActive true .
ex:star a sbol:TopLevel, fac:Asset ; sbol:displayId "star" ;
    sbol:hasNamespace <https://example.org/facility> ; fac:facility ex:facility ;
    fac:assetKind fac:Instrument ; fac:locatedIn ex:room ; fac:isActive true ;
    fac:capability <https://example.org/facility/star/liquid_handling> .
<https://example.org/facility/star/liquid_handling>
    a sbol:Identified, fac:CapabilityOffering ; sbol:displayId "liquid_handling" ;
    fac:capabilityKind cap:LiquidHandling ; fac:qualification fac:Plannable ;
    fac:controlMode fac:ReviewedFileControl ; fac:isActive true ;
    fac:parameter <https://example.org/facility/star/liquid_handling/plate_wells> .
<https://example.org/facility/star/liquid_handling/plate_wells>
    a sbol:Identified, fac:PropertyValue ; sbol:displayId "plate_wells" ;
    fac:propertyKind cap:SupportedPlateWells ; fac:integerValue 96 .
"#;

    fn inventory(contents: &str) -> (TempDir, InventorySnapshot) {
        let directory = TempDir::new().unwrap();
        fs::write(directory.path().join("inventory.ttl"), contents).unwrap();
        let snapshot = InventorySnapshot::load(directory.path(), "inventory.ttl", None).unwrap();
        (directory, snapshot)
    }

    fn request(driver: &str) -> AdapterBindingRequest {
        AdapterBindingRequest {
            asset: "https://example.org/facility/star".to_owned(),
            driver: driver.to_owned(),
            profile_path: PathBuf::from("adapters/star.toml"),
            profile: validate_adapter_profile(driver, "star", "").unwrap(),
        }
    }

    #[test]
    fn freezes_exact_asset_offering_and_profile_bindings_without_promoting_qualification() {
        let (_directory, inventory) = inventory(INVENTORY);

        let snapshot =
            AdapterBindingSnapshot::resolve(&inventory, vec![request("hamilton.star")]).unwrap();

        assert_eq!(snapshot.schema_version, ADAPTER_BINDINGS_SCHEMA_VERSION);
        assert_eq!(snapshot.facility, "https://example.org/facility/facility");
        assert_eq!(snapshot.bindings.len(), 1);
        let binding = &snapshot.bindings[0];
        assert_eq!(binding.asset, "https://example.org/facility/star");
        assert_eq!(binding.driver, "hamilton.star");
        assert_eq!(binding.profile_sha256.len(), 64);
        assert_eq!(binding.procedure_implementations.len(), 1);
        assert_eq!(
            binding.procedure_implementations[0].contract.as_str(),
            lab_compiler::procedure::vocabulary::PIPETTING_PROGRAM_V1
        );
        assert_eq!(binding.offerings.len(), 1);
        let offering = &binding.offerings[0];
        assert_eq!(
            offering.offering,
            "https://example.org/facility/star/liquid_handling"
        );
        assert_eq!(offering.parameters.len(), 1);
        assert_eq!(
            offering.parameters[0].property_kind,
            "https://sbol.io/ns/capability#SupportedPlateWells"
        );
        assert_eq!(
            offering.parameters[0].value,
            BoundCapabilityParameterValue::Integer {
                value: "96".to_owned()
            }
        );
        assert!(offering.planning_eligible);
        assert!(!offering.simulation_eligible);
        assert!(!offering.execution_eligible);

        let json = serde_json::to_string(&snapshot).unwrap();
        assert_eq!(
            serde_json::from_str::<AdapterBindingSnapshot>(&json).unwrap(),
            snapshot
        );
    }

    #[test]
    fn rejects_a_driver_without_an_exact_capability_and_control_mode_match() {
        let (_directory, inventory) = inventory(INVENTORY);

        let error =
            AdapterBindingSnapshot::resolve(&inventory, vec![request("inheco.odtc")]).unwrap_err();

        assert!(matches!(
            error,
            AdapterBindingError::NoCompatibleOffering { .. }
        ));
    }
}
