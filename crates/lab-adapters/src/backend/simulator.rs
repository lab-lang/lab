//! Complete registration and callbacks for lab.simulator.

use lab_capability::ControlMode;
use lab_compiler::procedure::vocabulary::{
    AIR_GAP_HANDLING, IN_WELL_MIXING, LIQUID_LEVEL_AWARE_ASPIRATION, METERED_LIQUID_TRANSFER,
    POST_DISPENSE_BLOWOUT, TEMPERATURE_CONTROLLED_STAGING, TOUCH_TIP,
    VESSEL_RELATIVE_LIQUID_ACCESS,
};
use lab_compiler::procedure::{ProcedureContractRegistry, ProgramFeature};

use crate::ArtifactBundle;
use crate::{AdapterInvocation, AdapterInvocationPlan};
use lab_runfmt::{SIMULATION_RUN_FORMAT, SimulationRunDocument};
use lab_runtime::reviewed_documents::load_simulation_run;

use crate::runtime::{
    AdapterRuntimeRegistrationExt, RuntimeDocumentRegistration, simulation_executor,
};

use lab_adapter_api::{
    AdapterInvocationDocument, AdapterInvocationLowering, AdapterLoweringError,
    AdapterProfileContractError, AdapterRegistration, AdapterServices, ValidatedAdapterProfile,
};

use crate::backend::adapters::{
    EmptyAdapterProfile, declared_program_feasible, descriptor, implementation_id,
    pipetting_implementation, schema_value, thermal_implementation, validate_empty_for,
};

const SIMULATOR_PIPETTING_IMPLEMENTATION: &str =
    "https://www.lab-compiler.org/ns/adapter-implementation#LabSimulatorPipettingV1";

const SIMULATOR_THERMAL_IMPLEMENTATION: &str =
    "https://www.lab-compiler.org/ns/adapter-implementation#LabSimulatorThermalV1";

const SIMULATOR_PIPETTING_FEATURES: &[ProgramFeature] = &[
    ProgramFeature::Transfer,
    ProgramFeature::Distribute,
    ProgramFeature::Mix,
    ProgramFeature::Barrier,
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
];

const SIMULATOR_THERMAL_FEATURES: &[ProgramFeature] = &[
    ProgramFeature::ThermalStageRepeat,
    ProgramFeature::ThermalHeatedLid,
    ProgramFeature::ThermalControlledRamp,
    ProgramFeature::ThermalFinalHold,
    ProgramFeature::ThermalMultiSample,
];

pub fn registration() -> Result<AdapterRegistration, AdapterProfileContractError> {
    let descriptor = descriptor(
        "lab.simulator",
        "Lab semantic capability simulator",
        None,
        ["no-hardware", "semantic-simulation"],
        vec![
            pipetting_implementation(
                SIMULATOR_PIPETTING_IMPLEMENTATION,
                SIMULATOR_PIPETTING_FEATURES,
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
                [SIMULATION_RUN_FORMAT],
                [SIMULATION_RUN_FORMAT],
                AdapterServices {
                    planning: true,
                    lowering: true,
                    simulation: true,
                    runtime: false,
                },
            )?,
            thermal_implementation(
                SIMULATOR_THERMAL_IMPLEMENTATION,
                SIMULATOR_THERMAL_FEATURES,
                [ControlMode::ReviewedFile],
                [SIMULATION_RUN_FORMAT],
                [SIMULATION_RUN_FORMAT],
                AdapterServices {
                    planning: true,
                    lowering: true,
                    simulation: true,
                    runtime: false,
                },
                true,
            )?,
        ],
        schema_value::<EmptyAdapterProfile>()?,
        validate_simulator_profile("lab.simulator", "")?,
    )?;
    Ok(AdapterRegistration::new(
        descriptor,
        validate_simulator_profile,
        declared_program_feasible,
        lower_simulator_registered_invocation,
    )
    .with_runtime_document(
        RuntimeDocumentRegistration::new(
            implementation_id(SIMULATOR_PIPETTING_IMPLEMENTATION),
            SIMULATION_RUN_FORMAT,
            load_simulation_run,
        )
        .with_simulation(simulation_executor),
    )
    .with_runtime_document(
        RuntimeDocumentRegistration::new(
            implementation_id(SIMULATOR_THERMAL_IMPLEMENTATION),
            SIMULATION_RUN_FORMAT,
            load_simulation_run,
        )
        .with_simulation(simulation_executor),
    ))
}

fn validate_simulator_profile(
    name: &str,
    contents: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    validate_empty_for("lab.simulator", name, contents)
}

fn lower_simulator_registered_invocation(
    _profile: &ValidatedAdapterProfile,
    plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
    _contracts: &ProcedureContractRegistry,
) -> Result<AdapterInvocationLowering, AdapterLoweringError> {
    lower_simulator_invocation(plan, invocation)
}

fn lower_simulator_invocation(
    invocation_plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
) -> Result<AdapterInvocationLowering, AdapterLoweringError> {
    let requirements = invocation_plan
        .allocated
        .methods
        .iter()
        .flat_map(|method| &method.tasks)
        .flat_map(|task| {
            task.requirements
                .iter()
                .map(move |requirement| (requirement.id.clone(), task))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut artifacts = ArtifactBundle::new();
    let mut documents = Vec::new();
    for (ordinal, requirement_id) in invocation.requirements.iter().enumerate() {
        let task = requirements
            .get(requirement_id)
            .expect("validated invocation requirements belong to exact tasks");
        let requirement = task
            .requirements
            .iter()
            .find(|requirement| &requirement.id == requirement_id)
            .expect("the requirement index preserves the owning requirement");
        let document = SimulationRunDocument {
            format: SIMULATION_RUN_FORMAT.to_owned(),
            id: requirement_id.to_string(),
            title: format!("Simulate {}", task.operation),
            capability_kind: requirement.capability_kind.to_string(),
            assumptions: vec![
                "Semantic simulation only; no physical hardware is contacted.".to_owned(),
                format!("Allocated Asset: {}", invocation.asset),
                format!("Procedure operation: {}", task.operation),
            ],
        };
        let path = format!(
            "requirement-{:03}-{}.simulation.json",
            ordinal + 1,
            short_digest(requirement_id.as_str())
        );
        let mut contents = serde_json::to_string_pretty(&document).map_err(|error| {
            AdapterLoweringError::Lowering {
                driver: invocation.adapter.driver.clone(),
                message: error.to_string(),
            }
        })?;
        contents.push('\n');
        artifacts
            .insert_text(&path, "application/json", contents)
            .map_err(|error| AdapterLoweringError::Lowering {
                driver: invocation.adapter.driver.clone(),
                message: error.to_string(),
            })?;
        documents.push(AdapterInvocationDocument {
            requirements: vec![requirement_id.clone()],
            path,
            format: SIMULATION_RUN_FORMAT.to_owned(),
        });
    }
    Ok(AdapterInvocationLowering {
        artifacts,
        documents,
    })
}

fn short_digest(value: &str) -> String {
    lab_adapter_api::hex_sha256(value.as_bytes())[..8].to_owned()
}
