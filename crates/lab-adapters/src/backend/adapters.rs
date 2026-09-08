//! Adapter discovery and operational-profile validation.
//!
//! An adapter is a Lab implementation, never a facility Asset. The manifest binds an adapter ID
//! to an exact SBOLInventory Asset IRI; this registry states which semantic capability offerings
//! and control modes that implementation can use. Product features stay separate from semantic
//! capability kinds so neither manufacturer nor model can silently select a driver.

use std::collections::BTreeSet;

use lab_capability::{CapabilityKind, ControlMode, ProcedureContractId, ProcedureImplementationId};
use lab_compiler::allocation::{AllocatedProcedureTask, AllocatedRequirementBinding};
use lab_compiler::planning::{
    PlanningMaterialSource, PlanningProcedureTask, SelectedCapabilityParameter,
    SelectedMaterialBinding, SelectedMaterialSource,
};
use lab_compiler::procedure::vocabulary::{
    CONTROLLED_TEMPERATURE_RAMP, HEATED_LID_TEMPERATURE_CONTROL, PIPETTING_PROGRAM_V1,
    PROGRAMMED_BLOCK_TEMPERATURE_CONTROL, THERMAL_PROGRAM_V1,
};
use lab_compiler::procedure::{ProcedureContractRegistry, ProgramFeature};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::runtime::validate_runtime_registry;

pub use lab_adapter_api::{
    ADAPTER_CATALOG_FORMAT, ADAPTER_PROFILE_SCHEMA_VERSION, AdapterCatalog, AdapterDescriptor,
    AdapterInvocationDocument, AdapterInvocationLowering, AdapterLoweringError,
    AdapterProfileContractError, AdapterRegistration, AdapterRegistry, AdapterServices,
    InvocationLowerer, ProcedureImplementationDescriptor, ProfileValidator, ProgramFeasibility,
    ValidatedAdapterProfile,
};

/// The adapters linked into the default Lab application.
///
/// Each entry owns its descriptor, profile parsing, feasibility, and lowering.
/// `lab-project` composes this bundled registry with application extensions.
pub fn builtin_adapter_registry() -> Result<AdapterRegistry, AdapterProfileContractError> {
    let registry = AdapterRegistry::new([
        crate::backend::opentrons::ot2::registration()?,
        crate::backend::opentrons::flex::registration()?,
        crate::backend::hamilton::star::registration()?,
        crate::backend::inheco::odtc::registration()?,
        crate::backend::byonoy::registration()?,
        crate::backend::simulator::registration()?,
    ])?;
    validate_runtime_registry(&registry).map_err(AdapterProfileContractError::Contract)?;
    Ok(registry)
}

/// Describes every concrete adapter in the default Lab application.
pub fn adapter_catalog() -> Result<AdapterCatalog, AdapterProfileContractError> {
    Ok(builtin_adapter_registry()?.catalog())
}

pub(in crate::backend) fn descriptor<const F: usize>(
    id: &'static str,
    display_name: &'static str,
    manufacturer: Option<&'static str>,
    features: [&'static str; F],
    procedure_implementations: Vec<ProcedureImplementationDescriptor>,
    profile_schema: Value,
    default_profile: ValidatedAdapterProfile,
) -> Result<AdapterDescriptor, AdapterProfileContractError> {
    Ok(AdapterDescriptor {
        id: id.to_owned(),
        display_name: display_name.to_owned(),
        manufacturer: manufacturer.map(str::to_owned),
        features: strings(features),
        procedure_implementations,
        profile_schema,
        default_profile,
    })
}

#[allow(clippy::too_many_arguments)]
pub(in crate::backend) fn pipetting_implementation<
    const C: usize,
    const M: usize,
    const A: usize,
    const E: usize,
>(
    id: &'static str,
    program_features: &'static [ProgramFeature],
    capability_kinds: [&'static str; C],
    control_modes: [ControlMode; M],
    accepted_run_formats: [&'static str; A],
    emitted_run_formats: [&'static str; E],
    services: AdapterServices,
) -> Result<ProcedureImplementationDescriptor, AdapterProfileContractError> {
    Ok(ProcedureImplementationDescriptor {
        id: ProcedureImplementationId::new(id).map_err(|error| {
            AdapterProfileContractError::Contract(format!(
                "Procedure implementation '{id}' has an invalid identity: {error}"
            ))
        })?,
        contract: ProcedureContractId::new(PIPETTING_PROGRAM_V1)
            .expect("built-in Procedure contract is an absolute IRI"),
        capability_kinds: capability_kinds
            .into_iter()
            .map(|kind| {
                CapabilityKind::new(kind).map_err(|error| {
                    AdapterProfileContractError::Contract(format!(
                        "Procedure implementation '{id}' declares an invalid capability: {error}"
                    ))
                })
            })
            .collect::<Result<_, _>>()?,
        control_modes: control_modes.into_iter().collect(),
        accepted_run_formats: strings(accepted_run_formats),
        emitted_run_formats: strings(emitted_run_formats),
        program_features: program_features.iter().cloned().collect(),
        services,
    })
}

#[allow(clippy::too_many_arguments)]
pub(in crate::backend) fn thermal_implementation<const M: usize, const A: usize, const E: usize>(
    id: &'static str,
    program_features: &'static [ProgramFeature],
    control_modes: [ControlMode; M],
    accepted_run_formats: [&'static str; A],
    emitted_run_formats: [&'static str; E],
    services: AdapterServices,
    controlled_ramp: bool,
) -> Result<ProcedureImplementationDescriptor, AdapterProfileContractError> {
    let mut capability_kinds = [
        PROGRAMMED_BLOCK_TEMPERATURE_CONTROL,
        HEATED_LID_TEMPERATURE_CONTROL,
    ]
    .into_iter()
    .map(|kind| {
        CapabilityKind::new(kind).map_err(|error| {
            AdapterProfileContractError::Contract(format!(
                "Procedure implementation '{id}' declares an invalid capability: {error}"
            ))
        })
    })
    .collect::<Result<BTreeSet<_>, _>>()?;
    if controlled_ramp {
        capability_kinds.insert(
            CapabilityKind::new(CONTROLLED_TEMPERATURE_RAMP)
                .expect("built-in capability is an absolute IRI"),
        );
    }
    Ok(ProcedureImplementationDescriptor {
        id: ProcedureImplementationId::new(id).map_err(|error| {
            AdapterProfileContractError::Contract(format!(
                "Procedure implementation '{id}' has an invalid identity: {error}"
            ))
        })?,
        contract: ProcedureContractId::new(THERMAL_PROGRAM_V1)
            .expect("built-in Procedure contract is an absolute IRI"),
        capability_kinds,
        control_modes: control_modes.into_iter().collect(),
        accepted_run_formats: strings(accepted_run_formats),
        emitted_run_formats: strings(emitted_run_formats),
        program_features: program_features.iter().cloned().collect(),
        services,
    })
}

fn strings<const N: usize>(values: [&'static str; N]) -> BTreeSet<String> {
    values.into_iter().map(str::to_owned).collect()
}

pub(in crate::backend) fn implementation_id(value: &'static str) -> ProcedureImplementationId {
    ProcedureImplementationId::new(value)
        .expect("built-in Procedure implementation identity is an absolute IRI")
}

pub(in crate::backend) fn validate_empty_for(
    driver: &str,
    name: &str,
    contents: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    let _: EmptyAdapterProfile = toml::from_str(contents)?;
    Ok(empty_profile(driver, name))
}

pub(in crate::backend) fn declared_program_feasible(
    _profile: &ValidatedAdapterProfile,
    implementation: &ProcedureImplementationDescriptor,
    task: &PlanningProcedureTask,
    contracts: &ProcedureContractRegistry,
) -> Result<(), String> {
    let program = task.program.as_ref().ok_or_else(|| {
        format!(
            "Procedure task '{}' has no program for implementation '{}'",
            task.id, implementation.id
        )
    })?;
    let validated = program
        .validate(contracts)
        .map_err(|error| error.to_string())?;
    if validated.contract() != &implementation.contract {
        return Err(format!(
            "program contract '{}' does not match implementation contract '{}'",
            validated.contract(),
            implementation.contract
        ));
    }
    let missing = validated
        .features()
        .difference(&implementation.program_features)
        .map(ProgramFeature::to_string)
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "unsupported program features {}",
            missing.join(", ")
        ));
    }
    Ok(())
}

/// Build the immutable portion of an allocation so an adapter can run the same structural
/// projection during candidate selection that it will run before lowering. Placeholder inventory
/// identities never reach an artifact; the projection reads only program structure, typed task
/// values, and material roles at this point.
pub(in crate::backend) fn feasibility_task(
    task: &PlanningProcedureTask,
    implementation: &ProcedureImplementationDescriptor,
) -> Result<AllocatedProcedureTask, String> {
    let materials = task
        .materials
        .iter()
        .map(|material| SelectedMaterialBinding {
            input: material.id.clone(),
            symbol: material.symbol.clone(),
            source: match &material.source {
                PlanningMaterialSource::Inventory => SelectedMaterialSource::MaterialLot {
                    component: format!("urn:lab:planning-component:{}", material.id),
                    material_lot: format!("urn:lab:planning-lot:{}", material.id),
                },
                PlanningMaterialSource::ChoiceOutput { choice } => {
                    SelectedMaterialSource::ChoiceOutput {
                        choice: choice.clone(),
                    }
                }
            },
            interchangeable_alternatives: Vec::new(),
        })
        .collect();
    let requirements = task
        .requirements
        .iter()
        .map(|requirement| {
            let control_mode = requirement
                .accepted_control_modes
                .iter()
                .next()
                .ok_or_else(|| {
                    format!(
                        "Procedure task '{}' requirement '{}' accepts no control mode",
                        task.id, requirement.id
                    )
                })?
                .iri()
                .to_owned();
            Ok(AllocatedRequirementBinding {
                id: requirement.id.clone(),
                capability_kind: requirement.capability_kind.clone(),
                minimum_qualification: requirement.minimum_qualification,
                accepted_control_modes: requirement.accepted_control_modes.clone(),
                offering: format!("urn:lab:planning-offering:{}", requirement.id),
                asset: "urn:lab:planning-asset".to_owned(),
                observed_qualification: requirement.minimum_qualification.iri().to_owned(),
                control_mode,
                parameters: requirement
                    .constraints
                    .iter()
                    .map(|constraint| SelectedCapabilityParameter {
                        property_kind: constraint.property_kind.clone(),
                        relation: constraint.relation,
                        required: constraint.required.clone(),
                        offering_parameter: format!(
                            "urn:lab:planning-parameter:{}",
                            constraint.property_kind
                        ),
                        observed: constraint.required.clone(),
                    })
                    .collect(),
                procedure_implementation: Some(implementation.id.clone()),
                adapter: None,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(AllocatedProcedureTask {
        id: task.id.clone(),
        operation: task.operation.clone(),
        program: task.program.clone(),
        inputs: task.inputs.clone(),
        outputs: task.outputs.clone(),
        parameters: task.parameters.clone(),
        materials,
        requirements,
    })
}

pub(in crate::backend) fn parse_profile_for_lowering<T>(
    profile: &ValidatedAdapterProfile,
    parse: impl FnOnce(&str, &str) -> Result<T, String>,
) -> Result<T, AdapterLoweringError> {
    parse(&profile.name, &profile.canonical_toml).map_err(|message| {
        AdapterLoweringError::InvalidProfile {
            driver: profile.driver.clone(),
            message,
        }
    })
}

/// Returns the canonical empty or reference profile for one adapter.
pub fn default_adapter_profile(
    driver: &str,
    name: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    validate_adapter_profile(driver, name, "")
}

/// Parses a profile with the schema selected by the explicit adapter ID.
///
/// The profile cannot select another driver. In particular, an omitted or misleading
/// manufacturer/model value never changes which parser runs.
pub fn validate_adapter_profile(
    driver: &str,
    name: &str,
    contents: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    builtin_adapter_registry()?.validate_profile(driver, name, contents)
}

pub(in crate::backend) fn schema_value<T: JsonSchema>() -> Result<Value, AdapterProfileContractError>
{
    let mut schema = serde_json::to_value(schema_for!(T))
        .map_err(|error| AdapterProfileContractError::Contract(error.to_string()))?;
    sanitize_schema_defaults(&mut schema);
    Ok(schema)
}

fn sanitize_schema_defaults(value: &mut Value) {
    let definitions = value.get("$defs").cloned().unwrap_or(Value::Null);
    sanitize_schema_node(value, &definitions);
}

fn sanitize_schema_node(value: &mut Value, definitions: &Value) {
    if let Some(object) = value.as_object_mut() {
        let property_names = closed_object_properties(object, definitions);
        if let (Some(property_names), Some(default)) = (
            property_names,
            object.get_mut("default").and_then(Value::as_object_mut),
        ) {
            default.retain(|name, _| property_names.contains(name));
        }
        for child in object.values_mut() {
            sanitize_schema_node(child, definitions);
        }
    } else if let Some(array) = value.as_array_mut() {
        for child in array {
            sanitize_schema_node(child, definitions);
        }
    }
}

fn closed_object_properties(
    object: &serde_json::Map<String, Value>,
    definitions: &Value,
) -> Option<BTreeSet<String>> {
    let closed_object = if object.get("additionalProperties") == Some(&Value::Bool(false)) {
        Some(object)
    } else {
        object
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|reference| reference.strip_prefix("#/$defs/"))
            .and_then(|name| definitions.get(name))
            .and_then(Value::as_object)
            .filter(|definition| {
                definition.get("additionalProperties") == Some(&Value::Bool(false))
            })
    }?;
    closed_object
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| properties.keys().cloned().collect())
}

pub(in crate::backend) fn canonical_adapter_profile<T: Serialize>(
    driver: &str,
    name: &str,
    profile: &T,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    lab_adapter_api::canonical_adapter_profile(driver, name, profile)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(in crate::backend) struct EmptyAdapterProfile {}

fn empty_profile(driver: &str, name: &str) -> ValidatedAdapterProfile {
    canonical_adapter_profile(driver, name, &EmptyAdapterProfile::default())
        .expect("the empty adapter profile has a canonical representation")
}

pub(in crate::backend) fn invalid(
    driver: &str,
    error: impl std::fmt::Display,
) -> AdapterProfileContractError {
    AdapterProfileContractError::Invalid {
        driver: driver.to_owned(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod feature_scope_tests {
    use super::*;

    fn implementation(driver: &str, suffix: &str) -> ProcedureImplementationDescriptor {
        adapter_catalog()
            .expect("the built-in catalog is valid")
            .adapters
            .into_iter()
            .find(|adapter| adapter.id == driver)
            .expect("adapter is present")
            .procedure_implementations
            .into_iter()
            .find(|implementation| implementation.id.as_str().ends_with(suffix))
            .expect("implementation is present")
    }

    #[test]
    fn procedure_support_is_described_by_contract_and_features() {
        let ot2 = implementation("opentrons.ot2", "OpentronsOt2PipettingV1");
        assert_eq!(ot2.contract.as_str(), PIPETTING_PROGRAM_V1);
        assert!(ot2.program_features.contains(&ProgramFeature::AirGap));
        assert!(
            ot2.program_features
                .contains(&ProgramFeature::DispenseMaterialSurface)
        );
        assert!(
            ot2.program_features
                .contains(&ProgramFeature::AspirateTrackedSurface)
        );
    }

    #[test]
    fn every_procedure_implementation_names_a_contract_and_features() {
        for adapter in adapter_catalog()
            .expect("the built-in catalog is valid")
            .adapters
        {
            for implementation in adapter.procedure_implementations {
                assert!(!implementation.contract.as_str().is_empty());
                assert!(!implementation.program_features.is_empty());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::AdapterRuntimeRegistryExt;
    use anyhow::Result;
    use lab_adapter_api::AdapterProgramFeasibility;
    use lab_capability::{OperationId, QualificationLevel};
    use lab_compiler::method::{LocalId, PortType};
    use lab_compiler::planning::{
        PlanningCapabilityRequirement, PlanningProcedureTask, PlanningTaskInput,
        PlanningTaskOutput, PlanningValueSource,
    };
    use lab_compiler::procedure::{
        Duration, ProcedureLocalId, ProcedureProgram, Temperature, ThermalLoad, ThermalProgramV1,
        ThermalStage, ThermalStep, Volume,
    };
    use lab_runtime::events::EventSink;
    use lab_runtime::execution::{
        DocumentExecutor, LoadedReviewedDocument, ReviewedDocumentLoadRequest,
    };
    use sbol_inventory::vocabulary::ABSORBANCE_MEASUREMENT;
    use serde_json::json;

    const EXAMPLE_REVIEWED_FORMAT: &str = "example.reviewed.v1";

    fn contracts() -> &'static ProcedureContractRegistry {
        lab_compiler::procedure::builtin_procedure_contracts()
    }

    struct ExampleExecutor;

    impl DocumentExecutor for ExampleExecutor {
        fn execute(
            &mut self,
            _document: &LoadedReviewedDocument,
            _events: &mut dyn EventSink,
        ) -> Result<()> {
            Ok(())
        }
    }

    struct ExampleLiveFactory;

    impl crate::LiveExecutorFactory for ExampleLiveFactory {
        fn build(
            &mut self,
            _request: crate::LiveExecutorFactoryRequest<'_>,
        ) -> Result<crate::BuiltLiveExecutor> {
            Ok(crate::BuiltLiveExecutor {
                executor: Box::new(ExampleExecutor),
                consumed_endpoint: false,
            })
        }
    }

    fn load_example_document(
        request: ReviewedDocumentLoadRequest<'_>,
    ) -> Result<LoadedReviewedDocument> {
        LoadedReviewedDocument::new(
            request.format,
            "Example reviewed document",
            request.bytes.to_vec(),
        )
    }

    fn example_simulation_executor() -> Box<dyn DocumentExecutor> {
        Box::new(ExampleExecutor)
    }

    fn example_live_factory() -> Box<dyn crate::LiveExecutorFactory> {
        Box::new(ExampleLiveFactory)
    }

    fn thermal_program(sample_count: u32) -> ProcedureProgram {
        let id = |value: &str| ProcedureLocalId::new(value).unwrap();
        let program = ThermalProgramV1 {
            load: ThermalLoad {
                input: 0,
                outputs: vec![id("amplified")],
                sample_count,
                volume_each: Volume::parse_microlitres("20").unwrap(),
            },
            lid_temperature: Some(Temperature::parse_degrees_celsius("105").unwrap()),
            stages: vec![ThermalStage {
                id: id("pcr"),
                repeats: 30,
                steps: vec![ThermalStep {
                    id: id("anneal"),
                    temperature: Temperature::parse_degrees_celsius("60").unwrap(),
                    hold: Duration::parse_seconds("30").unwrap(),
                    ramp_rate: None,
                }],
            }],
            final_hold: Some(Temperature::parse_degrees_celsius("4").unwrap()),
        }
        .validate()
        .unwrap();
        ProcedureProgram::from_thermal(&program)
    }

    fn thermal_task(sample_count: u32) -> PlanningProcedureTask {
        let program = thermal_program(sample_count);
        let validated = program.validate(contracts()).unwrap();
        let formula = validated.capability_formula();
        let id = LocalId::new("thermal-task").unwrap();
        let state = lab_capability::AbsoluteIri::new("urn:lab:test:sample").unwrap();
        PlanningProcedureTask {
            id: id.clone(),
            operation: OperationId::new("urn:lab:test:thermal").unwrap(),
            program: Some(program),
            binding_scope: formula.binding_scope,
            inputs: vec![PlanningTaskInput {
                source: PlanningValueSource::ChoiceInput {
                    input: LocalId::new("sample").unwrap(),
                },
                port_type: PortType::Material {
                    state: state.clone(),
                },
            }],
            outputs: vec![PlanningTaskOutput {
                name: LocalId::new("amplified").unwrap(),
                port_type: PortType::Material { state },
            }],
            parameters: Vec::new(),
            materials: Vec::new(),
            requirements: formula
                .all_of
                .into_iter()
                .map(|clause| PlanningCapabilityRequirement {
                    id: LocalId::new(format!("{id}::requirement::{}", clause.role)).unwrap(),
                    capability_kind: clause.capability_kind,
                    minimum_qualification: QualificationLevel::Plannable,
                    accepted_control_modes: [lab_capability::ControlMode::ReviewedFile]
                        .into_iter()
                        .collect(),
                    constraints: clause.constraints,
                })
                .collect(),
        }
    }

    fn validate_example_profile(
        name: &str,
        contents: &str,
    ) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
        if !contents.trim().is_empty() {
            return Err(AdapterProfileContractError::Invalid {
                driver: "example.instrument".to_owned(),
                message: "this test profile is empty".to_owned(),
            });
        }
        Ok(empty_profile("example.instrument", name))
    }

    fn example_registration() -> AdapterRegistration {
        let mut descriptor = builtin_adapter_registry()
            .unwrap()
            .registrations()
            .find(|registration| registration.descriptor.id == "byonoy.absorbance96")
            .unwrap()
            .descriptor
            .clone();
        descriptor.id = "example.instrument".to_owned();
        descriptor.display_name = "Example instrument".to_owned();
        descriptor.default_profile = validate_example_profile("default", "").unwrap();
        AdapterRegistration::new(
            descriptor,
            validate_example_profile,
            declared_program_feasible,
            unsupported_invocation,
        )
    }

    fn runtime_example_registration() -> AdapterRegistration {
        let mut registration = example_registration();
        let implementation =
            ProcedureImplementationId::new("https://example.org/implementation/instrument-v1")
                .unwrap();
        registration.descriptor.procedure_implementations =
            vec![ProcedureImplementationDescriptor {
                id: implementation.clone(),
                contract: ProcedureContractId::new(
                    "https://example.org/procedure-contract/instrument-v1",
                )
                .unwrap(),
                capability_kinds: BTreeSet::from([
                    CapabilityKind::new(ABSORBANCE_MEASUREMENT).unwrap()
                ]),
                control_modes: BTreeSet::from([ControlMode::Api]),
                accepted_run_formats: BTreeSet::from([EXAMPLE_REVIEWED_FORMAT.to_owned()]),
                emitted_run_formats: BTreeSet::from([EXAMPLE_REVIEWED_FORMAT.to_owned()]),
                program_features: BTreeSet::new(),
                services: AdapterServices {
                    planning: true,
                    lowering: true,
                    simulation: true,
                    runtime: true,
                },
            }];
        registration.with_runtime_document(
            RuntimeDocumentRegistration::new(
                implementation,
                EXAMPLE_REVIEWED_FORMAT,
                load_example_document,
            )
            .with_simulation(example_simulation_executor)
            .with_live_executor(example_live_factory),
        )
    }

    #[test]
    fn an_application_extends_the_builtin_registry_with_one_registration() {
        let builtin = builtin_adapter_registry().unwrap();
        let extended = builtin.with_registration(example_registration()).unwrap();

        assert!(
            extended
                .descriptors()
                .descriptor("example.instrument")
                .is_some()
        );
        assert_eq!(
            extended
                .validate_profile("example.instrument", "bench-profile", "")
                .unwrap()
                .driver,
            "example.instrument"
        );
    }

    #[test]
    fn one_third_party_registration_composes_loading_simulation_and_live_execution() {
        let registry = builtin_adapter_registry()
            .unwrap()
            .with_registration(runtime_example_registration())
            .unwrap();

        let loaded = registry
            .reviewed_document_loaders()
            .unwrap()
            .load(
                "example.instrument",
                "https://example.org/implementation/instrument-v1",
                EXAMPLE_REVIEWED_FORMAT,
                ABSORBANCE_MEASUREMENT,
                b"reviewed bytes",
                std::path::Path::new("example.reviewed"),
            )
            .unwrap();
        assert_eq!(loaded.format(), EXAMPLE_REVIEWED_FORMAT);
        assert_eq!(loaded.payload::<Vec<u8>>().unwrap(), b"reviewed bytes");

        let runtime = registry
            .registration("example.instrument")
            .unwrap()
            .runtime_documents()
            .next()
            .unwrap();
        assert!(runtime.simulation_factory().is_some());
        let mut live = runtime.live_executor_factory().unwrap()();
        let built = live
            .build(crate::LiveExecutorFactoryRequest {
                execution_directory: std::path::Path::new("."),
                asset: "urn:lab:test:instrument",
                profile_path: "adapter.toml",
                endpoint: None,
            })
            .unwrap();
        assert!(!built.consumed_endpoint);
    }

    #[test]
    fn extending_a_registry_rejects_duplicate_adapter_ids() {
        let builtin = builtin_adapter_registry().unwrap();
        let duplicate = builtin.registrations().next().unwrap().clone();
        let error = builtin.with_registration(duplicate).unwrap_err();
        assert!(error.to_string().contains("registered more than once"));
    }

    #[test]
    fn one_thermal_contract_runs_on_two_thermocyclers_without_an_operation_allowlist() {
        let registry = builtin_adapter_registry().unwrap();
        let contracts = lab_compiler::procedure::builtin_procedure_contracts();
        let task = thermal_task(1);
        for driver in ["opentrons.ot2", "opentrons.flex"] {
            let descriptor = registry.descriptors().descriptor(driver).unwrap();
            let implementation = descriptor
                .procedure_implementations
                .iter()
                .find(|implementation| implementation.contract.as_str() == THERMAL_PROGRAM_V1)
                .unwrap();
            registry
                .check_program(
                    driver,
                    &implementation.id,
                    &descriptor.default_profile,
                    &task,
                    contracts,
                )
                .unwrap();
        }
    }

    #[test]
    fn exact_thermocycler_profile_limits_fail_before_lowering() {
        let registry = builtin_adapter_registry().unwrap();
        let contracts = lab_compiler::procedure::builtin_procedure_contracts();
        let descriptor = registry.descriptors().descriptor("opentrons.ot2").unwrap();
        let implementation = descriptor
            .procedure_implementations
            .iter()
            .find(|implementation| implementation.contract.as_str() == THERMAL_PROGRAM_V1)
            .unwrap();
        let error = registry
            .check_program(
                "opentrons.ot2",
                &implementation.id,
                &descriptor.default_profile,
                &thermal_task(97),
                contracts,
            )
            .unwrap_err();
        assert!(
            error.contains("exact adapter profile provides 96"),
            "{error}"
        );
    }

    #[test]
    fn registry_separates_semantic_capabilities_from_features() {
        let catalog = adapter_catalog().unwrap();

        assert_eq!(catalog.format, ADAPTER_CATALOG_FORMAT);
        assert_eq!(catalog.adapters.len(), 6);
        let star = catalog
            .adapters
            .iter()
            .find(|adapter| adapter.id == "hamilton.star")
            .unwrap();
        assert!(star.features.contains("eight-channel"));
        assert_eq!(star.procedure_implementations.len(), 1);
        let star_pipetting = &star.procedure_implementations[0];
        assert_eq!(star_pipetting.contract.as_str(), PIPETTING_PROGRAM_V1);
        assert!(star_pipetting.control_modes.contains(&ControlMode::Api));
        assert!(
            star_pipetting
                .accepted_run_formats
                .contains(STAR_RUN_FORMAT)
        );
        assert!(star_pipetting.services.lowering);
        assert!(star_pipetting.services.runtime);
        assert!(
            star_pipetting
                .program_features
                .contains(&ProgramFeature::Transfer)
        );
        assert_eq!(
            star_pipetting.capability_kinds,
            [
                METERED_LIQUID_TRANSFER,
                IN_WELL_MIXING,
                LIQUID_LEVEL_AWARE_ASPIRATION,
            ]
            .into_iter()
            .map(|kind| CapabilityKind::new(kind).unwrap())
            .collect()
        );

        let ot2 = catalog
            .adapters
            .iter()
            .find(|adapter| adapter.id == "opentrons.ot2")
            .unwrap();
        let ot2_thermal = ot2
            .procedure_implementations
            .iter()
            .find(|implementation| implementation.contract.as_str() == THERMAL_PROGRAM_V1)
            .expect("OT-2 implements the canonical thermal contract");
        assert!(ot2_thermal.services.lowering);
        assert!(!ot2_thermal.services.runtime);
        assert!(
            ot2_thermal
                .program_features
                .contains(&ProgramFeature::ThermalStageRepeat)
        );
        assert_eq!(
            ot2_thermal.capability_kinds,
            [
                PROGRAMMED_BLOCK_TEMPERATURE_CONTROL,
                HEATED_LID_TEMPERATURE_CONTROL,
            ]
            .into_iter()
            .map(|kind| CapabilityKind::new(kind).unwrap())
            .collect()
        );

        let flex = catalog
            .adapters
            .iter()
            .find(|adapter| adapter.id == "opentrons.flex")
            .unwrap();
        assert!(flex.procedure_implementations.iter().all(|implementation| {
            implementation.services.lowering
                && !implementation.services.runtime
                && implementation
                    .emitted_run_formats
                    .contains(OPENTRONS_PROTOCOL_DESIGNER_FORMAT)
        }));

        let odtc = catalog
            .adapters
            .iter()
            .find(|adapter| adapter.id == "inheco.odtc")
            .unwrap();
        let odtc_thermal = &odtc.procedure_implementations[0];
        assert!(odtc_thermal.services.lowering);
        assert!(odtc_thermal.services.runtime);
        assert_eq!(odtc_thermal.contract.as_str(), THERMAL_PROGRAM_V1);
        assert!(
            odtc_thermal
                .capability_kinds
                .contains(&CapabilityKind::new(CONTROLLED_TEMPERATURE_RAMP).unwrap())
        );
        assert!(
            odtc_thermal
                .emitted_run_formats
                .contains(THERMOCYCLE_RUN_FORMAT)
        );

        let simulator = catalog
            .adapters
            .iter()
            .find(|adapter| adapter.id == "lab.simulator")
            .unwrap();
        assert!(
            simulator
                .procedure_implementations
                .iter()
                .all(|implementation| {
                    implementation.services.simulation
                        && implementation.services.lowering
                        && !implementation.services.runtime
                        && implementation
                            .accepted_run_formats
                            .contains(SIMULATION_RUN_FORMAT)
                })
        );
    }

    #[test]
    fn explicit_driver_selects_the_profile_schema() {
        let wrong = validate_adapter_profile(
            "hamilton.star",
            "star-1",
            "[target]\nbackend = \"opentrons.ot2\"\n",
        )
        .unwrap_err()
        .to_string();
        assert!(wrong.contains("hamilton.star"), "{wrong}");
        assert!(wrong.contains("target"), "{wrong}");

        let flex = validate_adapter_profile("opentrons.flex", "flex-1", "").unwrap();
        assert_eq!(flex.driver, "opentrons.flex");
        assert!(flex.canonical_json.get("target").is_none());
        assert!(!flex.canonical_toml.contains("[target]"));
        let flex_descriptor = adapter_catalog()
            .unwrap()
            .adapters
            .into_iter()
            .find(|adapter| adapter.id == "opentrons.flex")
            .unwrap();
        assert!(
            flex_descriptor.profile_schema["properties"]
                .get("target")
                .is_none()
        );

        let profile = validate_adapter_profile("inheco.odtc", "cycler-1", "").unwrap();
        assert_eq!(profile.driver, "inheco.odtc");
        assert_eq!(profile.canonical_json, json!({}));
        assert_eq!(profile.sha256.len(), 64);
    }

    #[test]
    fn empty_profiles_reject_unknown_operational_configuration() {
        let error = validate_adapter_profile(
            "inheco.odtc",
            "cycler-1",
            "endpoint = \"192.0.2.10:8080\"\n",
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("unknown field `endpoint`"), "{error}");
    }
}

#[cfg(test)]
use crate::backend::byonoy::unsupported_invocation;
#[cfg(test)]
use crate::runtime::{AdapterRuntimeRegistrationExt, RuntimeDocumentRegistration};
#[cfg(test)]
use lab_compiler::procedure::vocabulary::{
    IN_WELL_MIXING, LIQUID_LEVEL_AWARE_ASPIRATION, METERED_LIQUID_TRANSFER,
};
#[cfg(test)]
use lab_runfmt::{
    OPENTRONS_PROTOCOL_DESIGNER_FORMAT, SIMULATION_RUN_FORMAT, STAR_RUN_FORMAT,
    THERMOCYCLE_RUN_FORMAT,
};
