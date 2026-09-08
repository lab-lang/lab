//! Complete registration and callbacks for opentrons.flex.

use lab_capability::ControlMode;
use lab_compiler::planning::PlanningProcedureTask;
use lab_compiler::procedure::vocabulary::{
    IN_WELL_MIXING, LIQUID_LEVEL_AWARE_ASPIRATION, METERED_LIQUID_TRANSFER,
};
use lab_compiler::procedure::{ProcedureContractRegistry, ProgramFeature};

use crate::backend::opentrons::flex::FlexAdapterProfile;
use crate::{AdapterInvocation, AdapterInvocationPlan};
use lab_runfmt::OPENTRONS_PROTOCOL_DESIGNER_FORMAT;
use lab_runtime::reviewed_documents::load_opentrons_protocol_designer;

use crate::runtime::{AdapterRuntimeRegistrationExt, RuntimeDocumentRegistration};

use lab_adapter_api::{
    AdapterInvocationLowering, AdapterLoweringError, AdapterProfileContractError,
    AdapterRegistration, AdapterServices, ProcedureImplementationDescriptor,
    ValidatedAdapterProfile,
};

use crate::backend::adapters::{
    canonical_adapter_profile, declared_program_feasible, descriptor, feasibility_task,
    implementation_id, invalid, parse_profile_for_lowering, pipetting_implementation, schema_value,
    thermal_implementation,
};

const FLEX_PIPETTING_IMPLEMENTATION: &str =
    "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsFlexPipettingV1";

const FLEX_THERMAL_IMPLEMENTATION: &str =
    "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsFlexThermalV1";

const FLEX_PIPETTING_FEATURES: &[ProgramFeature] = &[
    ProgramFeature::VesselVolumeLimits,
    ProgramFeature::MultiPositionVessel,
    ProgramFeature::FluidPathGroup,
    ProgramFeature::AspirateTrackedSurface,
    ProgramFeature::Transfer,
    ProgramFeature::Distribute,
    ProgramFeature::Mix,
    ProgramFeature::AspirateLiquid,
    ProgramFeature::DispenseLiquid,
    ProgramFeature::FluidPathIsolatedDestinations,
    ProgramFeature::FluidPathSharedSourceNoReentry,
];

const OPENTRONS_THERMAL_FEATURES: &[ProgramFeature] = &[
    ProgramFeature::ThermalStageRepeat,
    ProgramFeature::ThermalHeatedLid,
    ProgramFeature::ThermalFinalHold,
    ProgramFeature::ThermalMultiSample,
];

pub fn registration() -> Result<AdapterRegistration, AdapterProfileContractError> {
    let descriptor = descriptor(
        "opentrons.flex",
        "Opentrons Flex",
        Some("Opentrons"),
        ["on-deck-modules", "protocol-designer-json"],
        vec![
            pipetting_implementation(
                FLEX_PIPETTING_IMPLEMENTATION,
                FLEX_PIPETTING_FEATURES,
                [
                    METERED_LIQUID_TRANSFER,
                    IN_WELL_MIXING,
                    LIQUID_LEVEL_AWARE_ASPIRATION,
                ],
                [ControlMode::ReviewedFile],
                [],
                [OPENTRONS_PROTOCOL_DESIGNER_FORMAT],
                AdapterServices {
                    planning: true,
                    lowering: true,
                    simulation: false,
                    runtime: false,
                },
            )?,
            thermal_implementation(
                FLEX_THERMAL_IMPLEMENTATION,
                OPENTRONS_THERMAL_FEATURES,
                [ControlMode::ReviewedFile],
                [],
                [OPENTRONS_PROTOCOL_DESIGNER_FORMAT],
                AdapterServices {
                    planning: true,
                    lowering: true,
                    simulation: false,
                    runtime: false,
                },
                false,
            )?,
        ],
        schema_value::<FlexAdapterProfile>()?,
        validate_flex_profile("opentrons.flex", "")?,
    )?;
    Ok(AdapterRegistration::new(
        descriptor,
        validate_flex_profile,
        flex_program_feasible,
        lower_flex_invocation,
    )
    .with_runtime_document(RuntimeDocumentRegistration::new(
        implementation_id(FLEX_PIPETTING_IMPLEMENTATION),
        OPENTRONS_PROTOCOL_DESIGNER_FORMAT,
        load_opentrons_protocol_designer,
    ))
    .with_runtime_document(RuntimeDocumentRegistration::new(
        implementation_id(FLEX_THERMAL_IMPLEMENTATION),
        OPENTRONS_PROTOCOL_DESIGNER_FORMAT,
        load_opentrons_protocol_designer,
    )))
}

fn validate_flex_profile(
    name: &str,
    contents: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    let driver = "opentrons.flex";
    let profile =
        FlexAdapterProfile::parse(name, contents).map_err(|error| invalid(driver, error))?;
    canonical_adapter_profile(driver, name, &profile)
}

fn flex_program_feasible(
    profile: &ValidatedAdapterProfile,
    implementation: &ProcedureImplementationDescriptor,
    task: &PlanningProcedureTask,
    contracts: &ProcedureContractRegistry,
) -> Result<(), String> {
    declared_program_feasible(profile, implementation, task, contracts)?;
    let parsed = FlexAdapterProfile::parse(&profile.name, &profile.canonical_toml)
        .map_err(|error| error.to_string())?;
    let allocated = feasibility_task(task, implementation)?;
    crate::backend::opentrons::flex::check_task_feasibility(&parsed, &allocated, contracts)
}

fn lower_flex_invocation(
    profile: &ValidatedAdapterProfile,
    plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
    contracts: &ProcedureContractRegistry,
) -> Result<AdapterInvocationLowering, AdapterLoweringError> {
    let parsed = parse_profile_for_lowering(profile, |name, contents| {
        FlexAdapterProfile::parse(name, contents).map_err(|error| error.to_string())
    })?;
    crate::backend::opentrons::flex::lower_invocation(&parsed, plan, invocation, contracts).map_err(
        |message| AdapterLoweringError::Lowering {
            driver: profile.driver.clone(),
            message,
        },
    )
}
