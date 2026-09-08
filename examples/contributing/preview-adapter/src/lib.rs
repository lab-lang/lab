//! A complete file-only adapter: emit canonical pipetting programs for inspection.
//! This format is a review aid and has no hardware executor.

use lab_adapter_api::*;
use std::collections::{BTreeMap, BTreeSet};

pub const DRIVER: &str = "example.pipetting-preview";
pub const FORMAT: &str = "example.pipetting-preview.v1";
pub const IMPLEMENTATION: &str = "https://example.org/implementation#PipettingPreviewV1";
const FEATURES: &[ProgramFeature] = &[
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

pub fn registration() -> Result<AdapterRegistration, AdapterProfileContractError> {
    Ok(AdapterRegistration::new(
        AdapterDescriptor {
            id: DRIVER.into(),
            display_name: "Pipetting review example".into(),
            manufacturer: None,
            features: BTreeSet::new(),
            procedure_implementations: vec![ProcedureImplementationDescriptor {
                id: ProcedureImplementationId::new(IMPLEMENTATION).expect("absolute IRI"),
                contract: ProcedureContractId::new(PIPETTING_PROGRAM_V1).expect("absolute IRI"),
                capability_kinds: [
                    METERED_LIQUID_TRANSFER,
                    IN_WELL_MIXING,
                    TEMPERATURE_CONTROLLED_STAGING,
                    LIQUID_LEVEL_AWARE_ASPIRATION,
                    VESSEL_RELATIVE_LIQUID_ACCESS,
                    AIR_GAP_HANDLING,
                    POST_DISPENSE_BLOWOUT,
                    TOUCH_TIP,
                ]
                .into_iter()
                .map(|iri| CapabilityKind::new(iri).expect("absolute IRI"))
                .collect(),
                control_modes: BTreeSet::from([ControlMode::ReviewedFile]),
                accepted_run_formats: BTreeSet::new(),
                emitted_run_formats: BTreeSet::from([FORMAT.into()]),
                program_features: FEATURES.iter().cloned().collect(),
                services: AdapterServices {
                    planning: true,
                    lowering: true,
                    simulation: false,
                    runtime: false,
                },
            }],
            profile_schema: serde_json::json!({"type":"object", "additionalProperties":false}),
            default_profile: validate_profile("default", "")?,
        },
        validate_profile,
        feasible,
        lower,
    ))
}

fn validate_profile(
    name: &str,
    contents: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    if !contents.trim().is_empty() {
        return Err(AdapterProfileContractError::Invalid {
            driver: DRIVER.into(),
            message: "this adapter has no profile settings".into(),
        });
    }
    canonical_adapter_profile(DRIVER, name, &BTreeMap::<String, String>::new())
}

fn feasible(
    _: &ValidatedAdapterProfile,
    implementation: &ProcedureImplementationDescriptor,
    task: &PlanningProcedureTask,
    contracts: &ProcedureContractRegistry,
) -> Result<(), String> {
    let program = task
        .program
        .as_ref()
        .ok_or("a canonical program is required")?;
    if program.contract != implementation.contract {
        return Err("expected pipetting".into());
    }
    let validated = program.validate(contracts).map_err(|e| e.to_string())?;
    if !validated
        .features()
        .is_subset(&implementation.program_features)
    {
        return Err("unsupported pipetting feature".into());
    }
    Ok(())
}

fn lower(
    _: &ValidatedAdapterProfile,
    plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
    _: &ProcedureContractRegistry,
) -> Result<AdapterInvocationLowering, AdapterLoweringError> {
    let error = |message: String| AdapterLoweringError::Lowering {
        driver: DRIVER.into(),
        message,
    };
    let mut artifacts = ArtifactBundle::new();
    let mut documents = Vec::new();
    for (index, task) in plan
        .allocated
        .methods
        .iter()
        .flat_map(|method| &method.tasks)
        .filter(|task| invocation.tasks.contains(&task.id))
        .enumerate()
    {
        let path = format!("task-{:03}.pipetting.json", index + 1);
        let contents = serde_json::to_string_pretty(&serde_json::json!({
            "format": FORMAT, "task": task.id, "program": task.program,
            "materials": task.materials, "inputs": task.inputs, "outputs": task.outputs,
        }))
        .map_err(|e| error(e.to_string()))?;
        artifacts
            .insert_text(&path, "application/json", contents)
            .map_err(|e| error(e.to_string()))?;
        documents.push(AdapterInvocationDocument {
            path,
            format: FORMAT.into(),
            requirements: task
                .requirements
                .iter()
                .filter(|r| invocation.requirements.contains(&r.id))
                .map(|r| r.id.clone())
                .collect(),
        });
    }
    Ok(AdapterInvocationLowering {
        artifacts,
        documents,
    })
}
