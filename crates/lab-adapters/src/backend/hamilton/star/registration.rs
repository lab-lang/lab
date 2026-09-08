//! Complete registration and callbacks for hamilton.star.

use lab_capability::ControlMode;
use lab_compiler::planning::PlanningProcedureTask;
use lab_compiler::procedure::vocabulary::{
    IN_WELL_MIXING, LIQUID_LEVEL_AWARE_ASPIRATION, METERED_LIQUID_TRANSFER,
};
use lab_compiler::procedure::{ProcedureContractRegistry, ProgramFeature};

use crate::backend::hamilton::star::StarAdapterProfile;
use crate::{AdapterInvocation, AdapterInvocationPlan};
use lab_runfmt::STAR_RUN_FORMAT;
use lab_runtime::reviewed_documents::load_star_run;

use crate::runtime::{
    AdapterRuntimeRegistrationExt, RuntimeDocumentRegistration, hamilton_star_live_factory,
    simulation_executor,
};

use lab_adapter_api::{
    AdapterInvocationLowering, AdapterLoweringError, AdapterProfileContractError,
    AdapterRegistration, AdapterServices, ProcedureImplementationDescriptor,
    ValidatedAdapterProfile,
};

use crate::backend::adapters::{
    canonical_adapter_profile, declared_program_feasible, descriptor, feasibility_task,
    implementation_id, invalid, parse_profile_for_lowering, pipetting_implementation, schema_value,
};

const STAR_PIPETTING_IMPLEMENTATION: &str =
    "https://www.lab-compiler.org/ns/adapter-implementation#HamiltonStarPipettingV1";

const STAR_PIPETTING_FEATURES: &[ProgramFeature] = &[
    ProgramFeature::VesselVolumeLimits,
    ProgramFeature::MultiPositionVessel,
    ProgramFeature::FluidPathGroup,
    ProgramFeature::Transfer,
    ProgramFeature::Distribute,
    ProgramFeature::Mix,
    ProgramFeature::AspirateLiquid,
    ProgramFeature::AspirateTrackedSurface,
    ProgramFeature::DispenseLiquid,
    ProgramFeature::FluidPathIsolatedDestinations,
    ProgramFeature::FluidPathSharedSourceNoReentry,
];

pub fn registration() -> Result<AdapterRegistration, AdapterProfileContractError> {
    let descriptor = descriptor(
        "hamilton.star",
        "Hamilton STAR/STARlet",
        Some("Hamilton"),
        ["eight-channel", "firmware-frames", "live-usb"],
        vec![pipetting_implementation(
            STAR_PIPETTING_IMPLEMENTATION,
            STAR_PIPETTING_FEATURES,
            [
                METERED_LIQUID_TRANSFER,
                IN_WELL_MIXING,
                LIQUID_LEVEL_AWARE_ASPIRATION,
            ],
            [ControlMode::ReviewedFile, ControlMode::Api],
            [STAR_RUN_FORMAT],
            [STAR_RUN_FORMAT],
            AdapterServices {
                planning: true,
                lowering: true,
                simulation: true,
                runtime: true,
            },
        )?],
        schema_value::<StarAdapterProfile>()?,
        validate_star_profile("hamilton.star", "")?,
    )?;
    Ok(AdapterRegistration::new(
        descriptor,
        validate_star_profile,
        star_program_feasible,
        lower_star_invocation,
    )
    .with_runtime_document(
        RuntimeDocumentRegistration::new(
            implementation_id(STAR_PIPETTING_IMPLEMENTATION),
            STAR_RUN_FORMAT,
            load_star_run,
        )
        .with_simulation(simulation_executor)
        .with_live_executor(hamilton_star_live_factory),
    ))
}

fn validate_star_profile(
    name: &str,
    contents: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    let driver = "hamilton.star";
    let profile =
        StarAdapterProfile::parse(name, contents).map_err(|error| invalid(driver, error))?;
    canonical_adapter_profile(driver, name, &profile)
}

fn star_program_feasible(
    profile: &ValidatedAdapterProfile,
    implementation: &ProcedureImplementationDescriptor,
    task: &PlanningProcedureTask,
    contracts: &ProcedureContractRegistry,
) -> Result<(), String> {
    declared_program_feasible(profile, implementation, task, contracts)?;
    let parsed = StarAdapterProfile::parse(&profile.name, &profile.canonical_toml)
        .map_err(|error| error.to_string())?;
    let allocated = feasibility_task(task, implementation)?;
    crate::backend::hamilton::star::check_task_feasibility(&parsed, &allocated, contracts)
}

fn lower_star_invocation(
    profile: &ValidatedAdapterProfile,
    plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
    contracts: &ProcedureContractRegistry,
) -> Result<AdapterInvocationLowering, AdapterLoweringError> {
    let parsed = parse_profile_for_lowering(profile, |name, contents| {
        StarAdapterProfile::parse(name, contents).map_err(|error| error.to_string())
    })?;
    crate::backend::hamilton::star::lower_invocation(&parsed, plan, invocation, contracts).map_err(
        |message| AdapterLoweringError::Lowering {
            driver: profile.driver.clone(),
            message,
        },
    )
}
