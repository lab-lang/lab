//! Complete registration and callbacks for inheco.odtc.

use lab_capability::ControlMode;
use lab_compiler::procedure::{ProcedureContractRegistry, ProgramFeature};

use crate::{AdapterInvocation, AdapterInvocationPlan};
use lab_runfmt::THERMOCYCLE_RUN_FORMAT;
use lab_runtime::reviewed_documents::load_thermocycle_run;

use crate::runtime::{
    AdapterRuntimeRegistrationExt, RuntimeDocumentRegistration, odtc_live_factory,
    simulation_executor,
};

use lab_adapter_api::{
    AdapterInvocationLowering, AdapterLoweringError, AdapterProfileContractError,
    AdapterRegistration, AdapterServices, ValidatedAdapterProfile,
};

use crate::backend::adapters::{
    EmptyAdapterProfile, declared_program_feasible, descriptor, implementation_id, schema_value,
    thermal_implementation, validate_empty_for,
};

const ODTC_THERMAL_IMPLEMENTATION: &str =
    "https://www.lab-compiler.org/ns/adapter-implementation#InhecoOdtcThermalV1";

const ODTC_THERMAL_FEATURES: &[ProgramFeature] = &[
    ProgramFeature::ThermalStageRepeat,
    ProgramFeature::ThermalHeatedLid,
    ProgramFeature::ThermalControlledRamp,
    ProgramFeature::ThermalFinalHold,
];

pub fn registration() -> Result<AdapterRegistration, AdapterProfileContractError> {
    let descriptor = descriptor(
        "inheco.odtc",
        "Inheco ODTC",
        Some("Inheco"),
        ["network-session", "thermal-profile"],
        vec![thermal_implementation(
            ODTC_THERMAL_IMPLEMENTATION,
            ODTC_THERMAL_FEATURES,
            [ControlMode::Sila2],
            [THERMOCYCLE_RUN_FORMAT],
            [THERMOCYCLE_RUN_FORMAT],
            AdapterServices {
                planning: true,
                lowering: true,
                simulation: true,
                runtime: true,
            },
            true,
        )?],
        schema_value::<EmptyAdapterProfile>()?,
        validate_odtc_profile("inheco.odtc", "")?,
    )?;
    Ok(AdapterRegistration::new(
        descriptor,
        validate_odtc_profile,
        declared_program_feasible,
        lower_odtc_invocation,
    )
    .with_runtime_document(
        RuntimeDocumentRegistration::new(
            implementation_id(ODTC_THERMAL_IMPLEMENTATION),
            THERMOCYCLE_RUN_FORMAT,
            load_thermocycle_run,
        )
        .with_simulation(simulation_executor)
        .with_live_executor(odtc_live_factory),
    ))
}

fn validate_odtc_profile(
    name: &str,
    contents: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    validate_empty_for("inheco.odtc", name, contents)
}

fn lower_odtc_invocation(
    profile: &ValidatedAdapterProfile,
    plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
    contracts: &ProcedureContractRegistry,
) -> Result<AdapterInvocationLowering, AdapterLoweringError> {
    crate::backend::inheco::odtc::lower_invocation(plan, invocation, contracts).map_err(|message| {
        AdapterLoweringError::Lowering {
            driver: profile.driver.clone(),
            message,
        }
    })
}
