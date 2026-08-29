//! Facility-wide allocation of reachable workflow requirements to exact SBOLInventory offerings.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lab_capability::{
    AbsoluteIri, ExactDecimal, ExactInteger, PropertyConstraint, PropertyValue, ScalarValue,
    UnitIri,
};
use lab_inventory::{
    FacilityAsset, FacilityAssetError, FacilityCapabilityOffering, FacilityCapabilityParameter,
    FacilityScalarValue, InventorySnapshot,
};
use lab_language::{CheckedExpression, TypedExpression};
use sbol_inventory::vocabulary::Qualification;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{
    ADAPTER_BINDINGS_SCHEMA_VERSION, AdapterBindingSnapshot,
    CAPABILITY_REQUIREMENT_INSTANCES_SCHEMA_VERSION, CAPABILITY_REQUIREMENTS_SCHEMA_VERSION,
    CapabilityParameterConstraint, CapabilityRequirement, CapabilityRequirementInstances,
    CapabilityRequirements, ParameterRelation, RequirementQualification,
};

pub const FACILITY_ALLOCATION_SCHEMA_VERSION: &str = "lab.facility-allocation.v1";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FacilityAllocation {
    pub schema_version: String,
    pub inventory_sha256: String,
    pub facility: String,
    pub requirements_schema_version: String,
    pub instances_schema_version: String,
    pub allocations: Vec<RequirementAllocation>,
}

impl FacilityAllocation {
    pub fn allocate(
        requirements: &CapabilityRequirements,
        instances: &CapabilityRequirementInstances,
        inventory: &InventorySnapshot,
        adapters: Option<&AdapterBindingSnapshot>,
    ) -> Result<Self, FacilityAllocationError> {
        validate_inputs(requirements, instances, inventory, adapters)?;
        let templates = requirements
            .requirements
            .iter()
            .map(|requirement| (requirement.id.as_str(), requirement))
            .collect::<BTreeMap<_, _>>();
        let assets = inventory.facility_assets()?;
        let mut allocations = Vec::new();
        for instance in &instances.instances {
            let requirement = templates.get(instance.template.as_str()).ok_or_else(|| {
                FacilityAllocationError::MissingRequirementTemplate {
                    instance: instance.id.clone(),
                    template: instance.template.clone(),
                }
            })?;
            allocations.push(allocate_requirement(
                &instance.id,
                requirement,
                &assets,
                adapters,
            )?);
        }
        Ok(Self {
            schema_version: FACILITY_ALLOCATION_SCHEMA_VERSION.to_owned(),
            inventory_sha256: inventory.source_sha256().to_owned(),
            facility: inventory.facility().as_str().to_owned(),
            requirements_schema_version: requirements.schema_version.clone(),
            instances_schema_version: instances.schema_version.clone(),
            allocations,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RequirementAllocation {
    pub requirement_instance: String,
    pub requirement_template: String,
    pub capability_kind: String,
    pub minimum_qualification: String,
    pub accepted_control_modes: BTreeSet<String>,
    pub offering: String,
    pub asset: String,
    pub observed_qualification: String,
    pub control_mode: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<MatchedCapabilityParameter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<AllocatedAdapter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected_candidates: Vec<RejectedCapabilityCandidate>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchedCapabilityParameter {
    pub argument: String,
    pub property_kind: String,
    pub relation: ParameterRelation,
    pub required: TypedExpression,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_unit: Option<String>,
    pub offering_parameter: String,
    #[serde(flatten)]
    pub observed: AllocationScalarValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_unit: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "value_type", rename_all = "snake_case")]
pub enum AllocationScalarValue {
    Text { value: String },
    Integer { value: String },
    Real { value: String },
    Boolean { value: bool },
    Iri { value: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AllocatedAdapter {
    pub driver: String,
    pub profile_path: PathBuf,
    pub profile_sha256: String,
    pub features: BTreeSet<String>,
    pub accepted_run_formats: BTreeSet<String>,
    pub emitted_run_formats: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectedCapabilityCandidate {
    pub offering: String,
    pub asset: String,
    pub observed_qualification: String,
    pub control_mode: String,
    pub reasons: Vec<CandidateRejectionReason>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum CandidateRejectionReason {
    Inactive,
    InsufficientQualification {
        required: String,
        observed: String,
    },
    UnsupportedControlMode {
        accepted: BTreeSet<String>,
        observed: String,
    },
    MissingParameter {
        property_kind: String,
    },
    UnitMismatch {
        property_kind: String,
        required: Option<String>,
        observed: Option<String>,
    },
    ValueMismatch {
        property_kind: String,
        required: String,
        observed: String,
    },
    UnsupportedRequirementValue {
        property_kind: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EligibleCapabilityCandidate {
    pub offering: String,
    pub asset: String,
    pub observed_qualification: String,
    pub control_mode: String,
}

#[derive(Debug, Error)]
pub enum FacilityAllocationError {
    #[error("{artifact} declares schema `{found}`, but allocation expects `{expected}`")]
    WrongSchema {
        artifact: &'static str,
        expected: &'static str,
        found: String,
    },
    #[error(
        "capability instances reference requirements schema `{instances}`, but the supplied catalog declares `{requirements}`"
    )]
    RequirementSchemaMismatch {
        instances: String,
        requirements: String,
    },
    #[error(
        "adapter bindings freeze inventory `{binding_hash}` facility `{binding_facility}`, but allocation uses inventory `{inventory_hash}` facility `{inventory_facility}`"
    )]
    AdapterInventoryMismatch {
        binding_hash: String,
        binding_facility: String,
        inventory_hash: String,
        inventory_facility: String,
    },
    #[error(transparent)]
    Asset(#[from] FacilityAssetError),
    #[error("requirement instance `{instance}` references absent template `{template}`")]
    MissingRequirementTemplate { instance: String, template: String },
    #[error(
        "requirement `{requirement}` has no eligible `{capability_kind}` offering; {candidate_count} candidate(s) were rejected"
    )]
    NoEligibleOffering {
        requirement: String,
        capability_kind: String,
        candidate_count: usize,
        rejections: Vec<RejectedCapabilityCandidate>,
    },
    #[error(
        "requirement `{requirement}` has {candidate_count} equally eligible offerings; allocation policy must select one"
    )]
    AmbiguousOffering {
        requirement: String,
        candidate_count: usize,
        candidates: Vec<EligibleCapabilityCandidate>,
    },
    #[error(
        "offering `{offering}` on asset `{asset}` has several planning adapters ({drivers}); select one explicitly"
    )]
    AmbiguousAdapter {
        offering: String,
        asset: String,
        drivers: String,
    },
}

fn validate_inputs(
    requirements: &CapabilityRequirements,
    instances: &CapabilityRequirementInstances,
    inventory: &InventorySnapshot,
    adapters: Option<&AdapterBindingSnapshot>,
) -> Result<(), FacilityAllocationError> {
    require_schema(
        "capability requirements",
        &requirements.schema_version,
        CAPABILITY_REQUIREMENTS_SCHEMA_VERSION,
    )?;
    require_schema(
        "capability requirement instances",
        &instances.schema_version,
        CAPABILITY_REQUIREMENT_INSTANCES_SCHEMA_VERSION,
    )?;
    if instances.requirements_schema_version != requirements.schema_version {
        return Err(FacilityAllocationError::RequirementSchemaMismatch {
            instances: instances.requirements_schema_version.clone(),
            requirements: requirements.schema_version.clone(),
        });
    }
    if let Some(adapters) = adapters {
        require_schema(
            "adapter bindings",
            &adapters.schema_version,
            ADAPTER_BINDINGS_SCHEMA_VERSION,
        )?;
        if adapters.inventory_sha256 != inventory.source_sha256()
            || adapters.facility != inventory.facility().as_str()
        {
            return Err(FacilityAllocationError::AdapterInventoryMismatch {
                binding_hash: adapters.inventory_sha256.clone(),
                binding_facility: adapters.facility.clone(),
                inventory_hash: inventory.source_sha256().to_owned(),
                inventory_facility: inventory.facility().as_str().to_owned(),
            });
        }
    }
    Ok(())
}

fn require_schema(
    artifact: &'static str,
    found: &str,
    expected: &'static str,
) -> Result<(), FacilityAllocationError> {
    if found == expected {
        Ok(())
    } else {
        Err(FacilityAllocationError::WrongSchema {
            artifact,
            expected,
            found: found.to_owned(),
        })
    }
}

fn allocate_requirement(
    instance: &str,
    requirement: &CapabilityRequirement,
    assets: &[FacilityAsset],
    adapters: Option<&AdapterBindingSnapshot>,
) -> Result<RequirementAllocation, FacilityAllocationError> {
    let minimum = inventory_qualification(requirement.minimum_qualification);
    let accepted_control_modes = requirement
        .accepted_control_modes
        .iter()
        .map(|mode| mode.iri().to_owned())
        .collect::<BTreeSet<_>>();
    let mut eligible = Vec::new();
    let mut rejected = Vec::new();
    for asset in assets {
        for offering in &asset.offerings {
            if offering.capability_kind.as_str() != requirement.capability_kind.as_str() {
                continue;
            }
            let mut reasons = Vec::new();
            if !offering.effectively_active {
                reasons.push(CandidateRejectionReason::Inactive);
            }
            if offering.qualification < minimum {
                reasons.push(CandidateRejectionReason::InsufficientQualification {
                    required: requirement.minimum_qualification.iri().to_owned(),
                    observed: offering.qualification.iri().to_owned(),
                });
            }
            if !accepted_control_modes.contains(offering.control_mode.iri()) {
                reasons.push(CandidateRejectionReason::UnsupportedControlMode {
                    accepted: accepted_control_modes.clone(),
                    observed: offering.control_mode.iri().to_owned(),
                });
            }
            let mut matched = Vec::new();
            for constraint in &requirement.parameter_constraints {
                match match_parameter(constraint, offering) {
                    Ok(parameter) => matched.push(parameter),
                    Err(reason) => reasons.push(reason),
                }
            }
            if reasons.is_empty() {
                eligible.push((asset, offering, matched));
            } else {
                rejected.push(rejected_candidate(asset, offering, reasons));
            }
        }
    }
    eligible.sort_by(|left, right| {
        (&left.0.identity, &left.1.identity).cmp(&(&right.0.identity, &right.1.identity))
    });
    rejected
        .sort_by(|left, right| (&left.asset, &left.offering).cmp(&(&right.asset, &right.offering)));
    if eligible.is_empty() {
        return Err(FacilityAllocationError::NoEligibleOffering {
            requirement: instance.to_owned(),
            capability_kind: requirement.capability_kind.to_string(),
            candidate_count: rejected.len(),
            rejections: rejected,
        });
    }
    if eligible.len() > 1 {
        let candidates = eligible
            .into_iter()
            .map(|(asset, offering, _)| eligible_candidate(asset, offering))
            .collect::<Vec<_>>();
        return Err(FacilityAllocationError::AmbiguousOffering {
            requirement: instance.to_owned(),
            candidate_count: candidates.len(),
            candidates,
        });
    }
    let (asset, offering, parameters) = eligible.pop().expect("one eligible candidate remains");
    let adapter = select_adapter(
        adapters,
        asset.identity.as_str(),
        offering.identity.as_str(),
    )?;
    Ok(RequirementAllocation {
        requirement_instance: instance.to_owned(),
        requirement_template: requirement.id.clone(),
        capability_kind: requirement.capability_kind.to_string(),
        minimum_qualification: requirement.minimum_qualification.iri().to_owned(),
        accepted_control_modes,
        offering: offering.identity.as_str().to_owned(),
        asset: asset.identity.as_str().to_owned(),
        observed_qualification: offering.qualification.iri().to_owned(),
        control_mode: offering.control_mode.iri().to_owned(),
        parameters,
        adapter,
        rejected_candidates: rejected,
    })
}

fn match_parameter(
    constraint: &CapabilityParameterConstraint,
    offering: &FacilityCapabilityOffering,
) -> Result<MatchedCapabilityParameter, CandidateRejectionReason> {
    let Some(parameter) = offering
        .parameters
        .iter()
        .find(|parameter| parameter.property_kind.as_str() == constraint.property_kind.as_str())
    else {
        return Err(CandidateRejectionReason::MissingParameter {
            property_kind: constraint.property_kind.to_string(),
        });
    };
    let observed_unit = parameter.unit.as_ref().map(|unit| unit.as_str().to_owned());
    let required_unit = constraint
        .unit
        .as_ref()
        .map(|unit| unit.as_str().to_owned());
    if required_unit != observed_unit {
        return Err(CandidateRejectionReason::UnitMismatch {
            property_kind: constraint.property_kind.to_string(),
            required: required_unit,
            observed: observed_unit,
        });
    }
    let Some(required_value) = semantic_requirement_value(&constraint.value) else {
        return Err(CandidateRejectionReason::UnsupportedRequirementValue {
            property_kind: constraint.property_kind.to_string(),
        });
    };
    let Some(observed_value) = semantic_observed_value(&parameter.value) else {
        return Err(CandidateRejectionReason::UnsupportedRequirementValue {
            property_kind: constraint.property_kind.to_string(),
        });
    };
    let required = PropertyValue::new(required_value, constraint.unit.clone())
        .expect("checked quantity constraints carry units only on numeric values");
    let observed = PropertyValue::new(
        observed_value,
        parameter
            .unit
            .as_ref()
            .map(|unit| UnitIri::new(unit.as_str()).expect("an sbol3 Iri is an absolute IRI")),
    )
    .expect("validated SBOLInventory properties carry units only on numeric values");
    let semantic = PropertyConstraint {
        property_kind: constraint.property_kind.clone(),
        relation: constraint.relation,
        required,
    };
    match semantic.is_satisfied_by(&observed) {
        Ok(true) => Ok(MatchedCapabilityParameter {
            argument: constraint.argument.clone(),
            property_kind: constraint.property_kind.to_string(),
            relation: constraint.relation,
            required: constraint.value.clone(),
            required_unit: constraint.unit.as_ref().map(ToString::to_string),
            offering_parameter: parameter.identity.as_str().to_owned(),
            observed: allocation_value(parameter),
            observed_unit: parameter.unit.as_ref().map(|unit| unit.as_str().to_owned()),
        }),
        Ok(false) => Err(CandidateRejectionReason::ValueMismatch {
            property_kind: constraint.property_kind.to_string(),
            required: render_requirement_value(&constraint.value),
            observed: render_observed_value(&parameter.value),
        }),
        Err(_) => Err(CandidateRejectionReason::UnsupportedRequirementValue {
            property_kind: constraint.property_kind.to_string(),
        }),
    }
}

fn semantic_requirement_value(required: &TypedExpression) -> Option<ScalarValue> {
    match &required.value {
        CheckedExpression::Integer { value } => ExactInteger::parse(value.to_string())
            .ok()
            .map(ScalarValue::Integer),
        CheckedExpression::Decimal { text }
        | CheckedExpression::Quantity {
            magnitude: text, ..
        } => ExactDecimal::parse(text).ok().map(ScalarValue::Real),
        CheckedExpression::Unary { operator, operand } if operator == "negate" => {
            numeric_requirement(operand).map(|value| ScalarValue::Real(value.negated()))
        }
        CheckedExpression::String { value } => Some(ScalarValue::Text(value.clone())),
        CheckedExpression::Reference { .. }
        | CheckedExpression::List { .. }
        | CheckedExpression::Call { .. }
        | CheckedExpression::Construct { .. }
        | CheckedExpression::Field { .. }
        | CheckedExpression::Unary { .. }
        | CheckedExpression::Binary { .. } => None,
    }
}

fn numeric_requirement(required: &TypedExpression) -> Option<ExactDecimal> {
    match &required.value {
        CheckedExpression::Integer { value } => ExactDecimal::parse(value.to_string()).ok(),
        CheckedExpression::Decimal { text } => ExactDecimal::parse(text).ok(),
        CheckedExpression::Quantity { magnitude, .. } => ExactDecimal::parse(magnitude).ok(),
        CheckedExpression::Unary { operator, operand } if operator == "negate" => {
            numeric_requirement(operand).map(|value| value.negated())
        }
        CheckedExpression::Reference { .. }
        | CheckedExpression::List { .. }
        | CheckedExpression::Call { .. }
        | CheckedExpression::Construct { .. }
        | CheckedExpression::Field { .. }
        | CheckedExpression::Unary { .. }
        | CheckedExpression::Binary { .. }
        | CheckedExpression::String { .. } => None,
    }
}

fn semantic_observed_value(observed: &FacilityScalarValue) -> Option<ScalarValue> {
    match observed {
        FacilityScalarValue::Text(value) => Some(ScalarValue::Text(value.clone())),
        FacilityScalarValue::Integer(value) => {
            ExactInteger::parse(value).ok().map(ScalarValue::Integer)
        }
        FacilityScalarValue::Real(value) => ExactDecimal::parse(value).ok().map(ScalarValue::Real),
        FacilityScalarValue::Boolean(value) => Some(ScalarValue::Boolean(*value)),
        FacilityScalarValue::Iri(value) => {
            AbsoluteIri::new(value.as_str()).ok().map(ScalarValue::Iri)
        }
    }
}

fn allocation_value(parameter: &FacilityCapabilityParameter) -> AllocationScalarValue {
    match &parameter.value {
        FacilityScalarValue::Text(value) => AllocationScalarValue::Text {
            value: value.clone(),
        },
        FacilityScalarValue::Integer(value) => AllocationScalarValue::Integer {
            value: value.clone(),
        },
        FacilityScalarValue::Real(value) => AllocationScalarValue::Real {
            value: value.clone(),
        },
        FacilityScalarValue::Boolean(value) => AllocationScalarValue::Boolean { value: *value },
        FacilityScalarValue::Iri(value) => AllocationScalarValue::Iri {
            value: value.as_str().to_owned(),
        },
    }
}

fn render_requirement_value(value: &TypedExpression) -> String {
    match &value.value {
        CheckedExpression::Integer { value } => value.to_string(),
        CheckedExpression::Decimal { text } => text.clone(),
        CheckedExpression::String { value } => value.clone(),
        CheckedExpression::Quantity { magnitude, .. } => magnitude.clone(),
        CheckedExpression::Unary { operator, operand } if operator == "negate" => {
            format!("-{}", render_requirement_value(operand))
        }
        _ => "dynamic expression".to_owned(),
    }
}

fn render_observed_value(value: &FacilityScalarValue) -> String {
    match value {
        FacilityScalarValue::Text(value)
        | FacilityScalarValue::Integer(value)
        | FacilityScalarValue::Real(value) => value.clone(),
        FacilityScalarValue::Boolean(value) => value.to_string(),
        FacilityScalarValue::Iri(value) => value.as_str().to_owned(),
    }
}

fn select_adapter(
    adapters: Option<&AdapterBindingSnapshot>,
    asset: &str,
    offering: &str,
) -> Result<Option<AllocatedAdapter>, FacilityAllocationError> {
    let Some(adapters) = adapters else {
        return Ok(None);
    };
    let mut candidates = adapters
        .bindings
        .iter()
        .filter(|binding| binding.asset == asset)
        .filter(|binding| {
            binding
                .offerings
                .iter()
                .any(|candidate| candidate.offering == offering && candidate.planning_eligible)
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.driver.cmp(&right.driver));
    if candidates.len() > 1 {
        return Err(FacilityAllocationError::AmbiguousAdapter {
            offering: offering.to_owned(),
            asset: asset.to_owned(),
            drivers: candidates
                .iter()
                .map(|binding| binding.driver.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        });
    }
    Ok(candidates.pop().map(|binding| AllocatedAdapter {
        driver: binding.driver.clone(),
        profile_path: binding.profile_path.clone(),
        profile_sha256: binding.profile_sha256.clone(),
        features: binding.features.clone(),
        accepted_run_formats: binding.accepted_run_formats.clone(),
        emitted_run_formats: binding.emitted_run_formats.clone(),
    }))
}

fn rejected_candidate(
    asset: &FacilityAsset,
    offering: &FacilityCapabilityOffering,
    reasons: Vec<CandidateRejectionReason>,
) -> RejectedCapabilityCandidate {
    RejectedCapabilityCandidate {
        offering: offering.identity.as_str().to_owned(),
        asset: asset.identity.as_str().to_owned(),
        observed_qualification: offering.qualification.iri().to_owned(),
        control_mode: offering.control_mode.iri().to_owned(),
        reasons,
    }
}

fn eligible_candidate(
    asset: &FacilityAsset,
    offering: &FacilityCapabilityOffering,
) -> EligibleCapabilityCandidate {
    EligibleCapabilityCandidate {
        offering: offering.identity.as_str().to_owned(),
        asset: asset.identity.as_str().to_owned(),
        observed_qualification: offering.qualification.iri().to_owned(),
        control_mode: offering.control_mode.iri().to_owned(),
    }
}

fn inventory_qualification(qualification: RequirementQualification) -> Qualification {
    match qualification {
        RequirementQualification::Discovered => Qualification::Discovered,
        RequirementQualification::Described => Qualification::Described,
        RequirementQualification::Plannable => Qualification::Plannable,
        RequirementQualification::Simulatable => Qualification::Simulatable,
        RequirementQualification::Executable => Qualification::Executable,
        RequirementQualification::Qualified => Qualification::Qualified,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use lab_language::compile_module;
    use tempfile::TempDir;

    use crate::backend::validate_adapter_profile;
    use crate::planning::{AdapterBindingRequest, AdapterBindingSnapshot};

    use super::*;

    const INVENTORY: &str = r#"@prefix cap: <https://sbol.io/ns/capability#> .
@prefix ex: <https://example.org/facility/> .
@prefix fac: <https://sbol.io/ns/facility#> .
@prefix sbol: <http://sbols.org/v3#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:facility a sbol:TopLevel, fac:Facility ; sbol:displayId "facility" ;
    sbol:hasNamespace <https://example.org/facility> .
ex:room a sbol:TopLevel, fac:Zone ; sbol:displayId "room" ;
    sbol:hasNamespace <https://example.org/facility> ; fac:facility ex:facility ;
    fac:zoneKind fac:Room ; fac:isActive true .
ex:freezer a sbol:TopLevel, fac:Asset ; sbol:displayId "freezer" ;
    sbol:hasNamespace <https://example.org/facility> ; fac:facility ex:facility ;
    fac:assetKind fac:StorageAsset ; fac:locatedIn ex:room ; fac:isActive true ;
    fac:capability <https://example.org/facility/freezer/cold_storage> .
<https://example.org/facility/freezer/cold_storage>
    a sbol:Identified, fac:CapabilityOffering ; sbol:displayId "cold_storage" ;
    fac:capabilityKind cap:ColdStorage ; fac:qualification fac:Plannable ;
    fac:controlMode fac:ManualControl ; fac:isActive true ;
    fac:parameter <https://example.org/facility/freezer/cold_storage/temperature> .
<https://example.org/facility/freezer/cold_storage/temperature>
    a sbol:Identified, fac:PropertyValue ; sbol:displayId "temperature" ;
    fac:propertyKind cap:Temperature ; fac:realValue "-80.0"^^xsd:double ;
    fac:unit <http://qudt.org/vocab/unit/DEG_C> .
"#;

    fn inventory(contents: &str) -> (TempDir, InventorySnapshot) {
        let directory = TempDir::new().unwrap();
        fs::write(directory.path().join("inventory.ttl"), contents).unwrap();
        let snapshot = InventorySnapshot::load(directory.path(), "inventory.ttl", None).unwrap();
        (directory, snapshot)
    }

    fn requirements() -> (CapabilityRequirements, CapabilityRequirementInstances) {
        storage_requirements("-80")
    }

    fn storage_requirements(
        temperature: &str,
    ) -> (CapabilityRequirements, CapabilityRequirementInstances) {
        let module = compile_module(&format!(
            r#"use std.lab.plasmid

workflow main(plasmid: Material<Plasmid>) -> Material<Plasmid>:
  stored <- store plasmid at {temperature} C
  return stored
"#
        ))
        .unwrap();
        let requirements = CapabilityRequirements::extract(&[&module]).unwrap();
        let instances = requirements
            .instantiate_reachable(&[&module], "standalone", "main")
            .unwrap();
        (requirements, instances)
    }

    fn liquid_requirements() -> (CapabilityRequirements, CapabilityRequirementInstances) {
        let module = compile_module(
            r#"use std.lab.plasmid

workflow main(culture: Material<Culture>) -> Material<Culture>:
  diluted <- dilute culture
  return diluted
"#,
        )
        .unwrap();
        let requirements = CapabilityRequirements::extract(&[&module]).unwrap();
        let instances = requirements
            .instantiate_reachable(&[&module], "standalone", "main")
            .unwrap();
        (requirements, instances)
    }

    #[test]
    fn allocates_an_exact_parameterized_offering_without_requiring_an_adapter() {
        let (_directory, inventory) = inventory(INVENTORY);
        let (requirements, instances) = requirements();

        let allocation =
            FacilityAllocation::allocate(&requirements, &instances, &inventory, None).unwrap();

        assert_eq!(
            allocation.schema_version,
            FACILITY_ALLOCATION_SCHEMA_VERSION
        );
        assert_eq!(allocation.allocations.len(), 1);
        let selected = &allocation.allocations[0];
        assert_eq!(selected.asset, "https://example.org/facility/freezer");
        assert_eq!(
            selected.offering,
            "https://example.org/facility/freezer/cold_storage"
        );
        assert!(selected.adapter.is_none());
        assert_eq!(selected.parameters.len(), 1);
        assert_eq!(
            selected.parameters[0].offering_parameter,
            "https://example.org/facility/freezer/cold_storage/temperature"
        );
        assert_eq!(
            selected.parameters[0].observed,
            AllocationScalarValue::Real {
                value: "-80.0".to_owned()
            }
        );
        let json = serde_json::to_string(&allocation).unwrap();
        assert_eq!(
            serde_json::from_str::<FacilityAllocation>(&json).unwrap(),
            allocation
        );
    }

    #[test]
    fn reports_typed_parameter_mismatch_instead_of_selecting_the_asset() {
        let mismatched = INVENTORY.replace("-80.0", "-20.0");
        let (_directory, inventory) = inventory(&mismatched);
        let (requirements, instances) = requirements();

        let error =
            FacilityAllocation::allocate(&requirements, &instances, &inventory, None).unwrap_err();

        let FacilityAllocationError::NoEligibleOffering { rejections, .. } = error else {
            panic!("expected a no-candidate diagnostic")
        };
        assert!(matches!(
            rejections[0].reasons.as_slice(),
            [CandidateRejectionReason::ValueMismatch { .. }]
        ));
    }

    #[test]
    fn parameter_matching_never_rounds_through_binary_floating_point() {
        let contents = INVENTORY.replace("-80.0", "9007199254740992.0");
        let (_directory, inventory) = inventory(&contents);
        let (requirements, instances) = storage_requirements("9007199254740993");

        let error =
            FacilityAllocation::allocate(&requirements, &instances, &inventory, None).unwrap_err();

        let FacilityAllocationError::NoEligibleOffering { rejections, .. } = error else {
            panic!("expected exact decimals to reject the rounded candidate")
        };
        assert!(matches!(
            rejections[0].reasons.as_slice(),
            [CandidateRejectionReason::ValueMismatch { required, observed, .. }]
                if required == "9007199254740993" && observed == "9007199254740992.0"
        ));
    }

    #[test]
    fn freezes_the_single_explicit_planning_adapter_when_one_is_available() {
        let contents = INVENTORY
            .replace("cap:ColdStorage", "cap:LiquidHandling")
            .replace("fac:ManualControl", "fac:ReviewedFileControl");
        let (_directory, inventory) = inventory(&contents);
        let bindings = AdapterBindingSnapshot::resolve(
            &inventory,
            vec![AdapterBindingRequest {
                asset: "https://example.org/facility/freezer".to_owned(),
                driver: "hamilton.star".to_owned(),
                profile_path: PathBuf::from("adapters/star.toml"),
                profile: validate_adapter_profile("hamilton.star", "star", "").unwrap(),
            }],
        )
        .unwrap();
        let (requirements, instances) = liquid_requirements();

        let allocation =
            FacilityAllocation::allocate(&requirements, &instances, &inventory, Some(&bindings))
                .unwrap();

        let adapter = allocation.allocations[0].adapter.as_ref().unwrap();
        assert_eq!(adapter.driver, "hamilton.star");
        assert_eq!(adapter.profile_path, PathBuf::from("adapters/star.toml"));
        assert_eq!(adapter.profile_sha256.len(), 64);
    }

    #[test]
    fn refuses_to_turn_deterministic_candidate_ordering_into_allocation_policy() {
        let second = format!(
            r#"{INVENTORY}
ex:freezer_b a sbol:TopLevel, fac:Asset ; sbol:displayId "freezer_b" ;
    sbol:hasNamespace <https://example.org/facility> ; fac:facility ex:facility ;
    fac:assetKind fac:StorageAsset ; fac:locatedIn ex:room ; fac:isActive true ;
    fac:capability <https://example.org/facility/freezer_b/cold_storage> .
<https://example.org/facility/freezer_b/cold_storage>
    a sbol:Identified, fac:CapabilityOffering ; sbol:displayId "cold_storage" ;
    fac:capabilityKind cap:ColdStorage ; fac:qualification fac:Plannable ;
    fac:controlMode fac:ManualControl ; fac:isActive true ;
    fac:parameter <https://example.org/facility/freezer_b/cold_storage/temperature> .
<https://example.org/facility/freezer_b/cold_storage/temperature>
    a sbol:Identified, fac:PropertyValue ; sbol:displayId "temperature" ;
    fac:propertyKind cap:Temperature ; fac:realValue "-80.0"^^xsd:double ;
    fac:unit <http://qudt.org/vocab/unit/DEG_C> .
"#,
        );
        let (_directory, inventory) = inventory(&second);
        let (requirements, instances) = requirements();

        let error =
            FacilityAllocation::allocate(&requirements, &instances, &inventory, None).unwrap_err();

        let FacilityAllocationError::AmbiguousOffering { candidates, .. } = error else {
            panic!("expected an ambiguity")
        };
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].asset, "https://example.org/facility/freezer");
        assert_eq!(
            candidates[1].asset,
            "https://example.org/facility/freezer_b"
        );
    }
}
