//! Compile-time conformance for an adapter that imports every Lab type from one facade.

use std::collections::{BTreeMap, BTreeSet};

use lab_adapter_api::{
    AdapterDescriptor, AdapterInvocation, AdapterInvocationLowering, AdapterInvocationPlan,
    AdapterLoweringError, AdapterProfileContractError, AdapterRegistration, AdapterRegistry,
    AdapterServices, CapabilityKind, ControlMode, PlanningProcedureTask, ProcedureContractId,
    ProcedureContractRegistry, ProcedureImplementationDescriptor, ProcedureImplementationId,
    ProgramFeature, ValidatedAdapterProfile, canonical_adapter_profile,
};

const DRIVER: &str = "example.cycler";

fn validate_profile(
    name: &str,
    contents: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    if !contents.is_empty() {
        return Err(AdapterProfileContractError::Invalid {
            driver: DRIVER.to_owned(),
            message: "the fixture accepts an empty profile".to_owned(),
        });
    }
    canonical_adapter_profile(DRIVER, name, &BTreeMap::<String, String>::new())
}

fn check_program_feasibility(
    _profile: &ValidatedAdapterProfile,
    _implementation: &ProcedureImplementationDescriptor,
    task: &PlanningProcedureTask,
    contracts: &ProcedureContractRegistry,
) -> Result<(), String> {
    let program = task
        .program
        .as_ref()
        .ok_or_else(|| format!("Procedure task '{}' has no canonical program", task.id))?;
    program
        .validate(contracts)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn lower_invocation(
    profile: &ValidatedAdapterProfile,
    _plan: &AdapterInvocationPlan,
    _invocation: &AdapterInvocation,
    _contracts: &ProcedureContractRegistry,
) -> Result<AdapterInvocationLowering, AdapterLoweringError> {
    Err(AdapterLoweringError::UnsupportedInvocation {
        driver: profile.driver.clone(),
    })
}

fn descriptor() -> AdapterDescriptor {
    AdapterDescriptor {
        id: DRIVER.to_owned(),
        display_name: "Example cycler".to_owned(),
        manufacturer: None,
        features: BTreeSet::new(),
        procedure_implementations: vec![ProcedureImplementationDescriptor {
            id: ProcedureImplementationId::new("https://example.org/CyclerThermalV1").unwrap(),
            contract: ProcedureContractId::new(lab_adapter_api::THERMAL_PROGRAM_V1).unwrap(),
            capability_kinds: BTreeSet::from([CapabilityKind::new(
                lab_adapter_api::PROGRAMMED_BLOCK_TEMPERATURE_CONTROL,
            )
            .unwrap()]),
            control_modes: BTreeSet::from([ControlMode::ReviewedFile]),
            accepted_run_formats: BTreeSet::new(),
            emitted_run_formats: BTreeSet::from(["application/json".to_owned()]),
            program_features: BTreeSet::from([ProgramFeature::ThermalStageRepeat]),
            services: AdapterServices {
                planning: true,
                lowering: true,
                simulation: false,
                runtime: false,
            },
        }],
        profile_schema: Default::default(),
        default_profile: validate_profile("default", "").unwrap(),
    }
}

#[test]
fn named_registration_callbacks_need_only_the_adapter_facade() {
    let registry = AdapterRegistry::new([AdapterRegistration::new(
        descriptor(),
        validate_profile,
        check_program_feasibility,
        lower_invocation,
    )])
    .unwrap();
    assert!(registry.descriptors().descriptor(DRIVER).is_some());
}
