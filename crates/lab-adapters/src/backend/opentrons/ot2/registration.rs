//! Complete registration and callbacks for opentrons.ot2.

use lab_capability::ControlMode;
use lab_compiler::planning::PlanningProcedureTask;
use lab_compiler::procedure::vocabulary::{
    AIR_GAP_HANDLING, IN_WELL_MIXING, LIQUID_LEVEL_AWARE_ASPIRATION, METERED_LIQUID_TRANSFER,
    POST_DISPENSE_BLOWOUT, TEMPERATURE_CONTROLLED_STAGING, TOUCH_TIP,
    VESSEL_RELATIVE_LIQUID_ACCESS,
};
use lab_compiler::procedure::{ProcedureContractRegistry, ProgramFeature};

use crate::backend::opentrons::ot2::Ot2AdapterProfile;
use crate::{AdapterInvocation, AdapterInvocationPlan};
use lab_runfmt::OPENTRONS_PYTHON_PROTOCOL_FORMAT;
use lab_runtime::reviewed_documents::load_opentrons_python_protocol;

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

const OT2_PIPETTING_IMPLEMENTATION: &str =
    "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsOt2PipettingV1";

const OT2_THERMAL_IMPLEMENTATION: &str =
    "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsOt2ThermalV1";

const OPENTRONS_THERMAL_FEATURES: &[ProgramFeature] = &[
    ProgramFeature::ThermalStageRepeat,
    ProgramFeature::ThermalHeatedLid,
    ProgramFeature::ThermalFinalHold,
    ProgramFeature::ThermalMultiSample,
];

pub fn registration() -> Result<AdapterRegistration, AdapterProfileContractError> {
    let descriptor = descriptor(
        "opentrons.ot2",
        "Opentrons OT-2",
        Some("Opentrons"),
        ["on-deck-modules", "python-protocol-api", "single-channel"],
        vec![
            pipetting_implementation(
                OT2_PIPETTING_IMPLEMENTATION,
                &[
                    ProgramFeature::Transfer,
                    ProgramFeature::Distribute,
                    ProgramFeature::Mix,
                    ProgramFeature::MultiPositionVessel,
                    ProgramFeature::VesselVolumeLimits,
                    ProgramFeature::VesselTemperatureControl,
                    ProgramFeature::FluidPathIsolatedDestinations,
                    ProgramFeature::FluidPathSharedSourceNoReentry,
                    ProgramFeature::FluidPathGroup,
                    ProgramFeature::AspirateLiquid,
                    ProgramFeature::AspirateTrackedSurface,
                    ProgramFeature::AspirateVesselBottom,
                    ProgramFeature::DispenseLiquid,
                    ProgramFeature::DispenseAboveLiquid,
                    ProgramFeature::DispenseVesselBottom,
                    ProgramFeature::DispenseVesselTop,
                    ProgramFeature::DispenseMaterialSurface,
                    ProgramFeature::AirGap,
                    ProgramFeature::PostDispenseBlowout,
                    ProgramFeature::TouchTip,
                ],
                [
                    METERED_LIQUID_TRANSFER,
                    IN_WELL_MIXING,
                    TEMPERATURE_CONTROLLED_STAGING,
                    LIQUID_LEVEL_AWARE_ASPIRATION,
                    VESSEL_RELATIVE_LIQUID_ACCESS,
                    AIR_GAP_HANDLING,
                    POST_DISPENSE_BLOWOUT,
                    TOUCH_TIP,
                ],
                [ControlMode::ReviewedFile],
                [],
                [OPENTRONS_PYTHON_PROTOCOL_FORMAT],
                AdapterServices {
                    planning: true,
                    lowering: true,
                    simulation: false,
                    runtime: false,
                },
            )?,
            thermal_implementation(
                OT2_THERMAL_IMPLEMENTATION,
                OPENTRONS_THERMAL_FEATURES,
                [ControlMode::ReviewedFile],
                [],
                [OPENTRONS_PYTHON_PROTOCOL_FORMAT],
                AdapterServices {
                    planning: true,
                    lowering: true,
                    simulation: false,
                    runtime: false,
                },
                false,
            )?,
        ],
        schema_value::<Ot2AdapterProfile>()?,
        validate_ot2_profile("opentrons.ot2", "")?,
    )?;
    Ok(AdapterRegistration::new(
        descriptor,
        validate_ot2_profile,
        ot2_program_feasible,
        lower_ot2_invocation,
    )
    .with_runtime_document(RuntimeDocumentRegistration::new(
        implementation_id(OT2_PIPETTING_IMPLEMENTATION),
        OPENTRONS_PYTHON_PROTOCOL_FORMAT,
        load_opentrons_python_protocol,
    ))
    .with_runtime_document(RuntimeDocumentRegistration::new(
        implementation_id(OT2_THERMAL_IMPLEMENTATION),
        OPENTRONS_PYTHON_PROTOCOL_FORMAT,
        load_opentrons_python_protocol,
    )))
}

fn validate_ot2_profile(
    name: &str,
    contents: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    let driver = "opentrons.ot2";
    let profile =
        Ot2AdapterProfile::parse(name, contents).map_err(|error| invalid(driver, error))?;
    canonical_adapter_profile(driver, name, &profile)
}

fn ot2_program_feasible(
    profile: &ValidatedAdapterProfile,
    implementation: &ProcedureImplementationDescriptor,
    task: &PlanningProcedureTask,
    contracts: &ProcedureContractRegistry,
) -> Result<(), String> {
    declared_program_feasible(profile, implementation, task, contracts)?;
    let parsed = Ot2AdapterProfile::parse(&profile.name, &profile.canonical_toml)
        .map_err(|error| error.to_string())?;
    let allocated = feasibility_task(task, implementation)?;
    crate::backend::opentrons::ot2::check_task_feasibility(&parsed, &allocated, contracts)
}

fn lower_ot2_invocation(
    profile: &ValidatedAdapterProfile,
    plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
    contracts: &ProcedureContractRegistry,
) -> Result<AdapterInvocationLowering, AdapterLoweringError> {
    let parsed = parse_profile_for_lowering(profile, |name, contents| {
        Ot2AdapterProfile::parse(name, contents).map_err(|error| error.to_string())
    })?;
    crate::backend::opentrons::ot2::lower_invocation(&parsed, plan, invocation, contracts).map_err(
        |message| AdapterLoweringError::Lowering {
            driver: profile.driver.clone(),
            message,
        },
    )
}
