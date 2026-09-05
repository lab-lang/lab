//! Adapter discovery and operational-profile validation.
//!
//! An adapter is a Lab implementation, never a facility Asset. The manifest binds an adapter ID
//! to an exact SBOLInventory Asset IRI; this registry states which semantic capability offerings
//! and control modes that implementation can use. Product features stay separate from semantic
//! capability kinds so neither manufacturer nor model can silently select a driver.

use std::collections::{BTreeMap, BTreeSet};

use lab_capability::{CapabilityKind, ControlMode, ProcedureContractId, ProcedureImplementationId};
use lab_compiler::allocation::{AllocatedProcedureTask, AllocatedRequirementBinding};
use lab_compiler::planning::{
    PlanningMaterialSource, PlanningProcedureTask, SelectedCapabilityParameter,
    SelectedMaterialBinding, SelectedMaterialSource,
};
use lab_compiler::procedure::vocabulary::{
    AIR_GAP_HANDLING, CONTROLLED_TEMPERATURE_RAMP, HEATED_LID_TEMPERATURE_CONTROL, IN_WELL_MIXING,
    LIQUID_LEVEL_AWARE_ASPIRATION, METERED_LIQUID_TRANSFER, PIPETTING_PROGRAM_V1,
    POST_DISPENSE_BLOWOUT, PROGRAMMED_BLOCK_TEMPERATURE_CONTROL, TEMPERATURE_CONTROLLED_STAGING,
    THERMAL_PROGRAM_V1, TOUCH_TIP, VESSEL_RELATIVE_LIQUID_ACCESS,
};
use lab_compiler::procedure::{ProcedureContractRegistry, ProgramFeature};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ArtifactBundle;
use crate::backend::hamilton::star::StarAdapterProfile;
use crate::backend::opentrons::flex::FlexAdapterProfile;
use crate::backend::opentrons::ot2::Ot2AdapterProfile;
use crate::{AdapterInvocation, AdapterInvocationPlan};
use lab_runfmt::{
    OPENTRONS_PROTOCOL_DESIGNER_FORMAT, OPENTRONS_PYTHON_PROTOCOL_FORMAT, SIMULATION_RUN_FORMAT,
    STAR_RUN_FORMAT, SimulationRunDocument, THERMOCYCLE_RUN_FORMAT,
};
use lab_runtime::reviewed_documents::{
    load_opentrons_protocol_designer, load_opentrons_python_protocol, load_simulation_run,
    load_star_run, load_thermocycle_run,
};

use crate::runtime::{
    AdapterRuntimeRegistrationExt, RuntimeDocumentRegistration, hamilton_star_live_factory,
    odtc_live_factory, simulation_executor, validate_runtime_registry,
};

pub use lab_adapter_api::{
    ADAPTER_CATALOG_FORMAT, ADAPTER_PROFILE_SCHEMA_VERSION, AdapterCatalog, AdapterDescriptor,
    AdapterInvocationDocument, AdapterInvocationLowering, AdapterLoweringError,
    AdapterProfileContractError, AdapterRegistration, AdapterRegistry, AdapterServices,
    InvocationLowerer, ProcedureImplementationDescriptor, ProfileValidator, ProgramFeasibility,
    ValidatedAdapterProfile,
};

const OT2_PIPETTING_IMPLEMENTATION: &str =
    "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsOt2PipettingV1";
const OT2_THERMAL_IMPLEMENTATION: &str =
    "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsOt2ThermalV1";
const FLEX_PIPETTING_IMPLEMENTATION: &str =
    "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsFlexPipettingV1";
const FLEX_THERMAL_IMPLEMENTATION: &str =
    "https://www.lab-compiler.org/ns/adapter-implementation#OpentronsFlexThermalV1";
const STAR_PIPETTING_IMPLEMENTATION: &str =
    "https://www.lab-compiler.org/ns/adapter-implementation#HamiltonStarPipettingV1";
const ODTC_THERMAL_IMPLEMENTATION: &str =
    "https://www.lab-compiler.org/ns/adapter-implementation#InhecoOdtcThermalV1";
const SIMULATOR_PIPETTING_IMPLEMENTATION: &str =
    "https://www.lab-compiler.org/ns/adapter-implementation#LabSimulatorPipettingV1";
const SIMULATOR_THERMAL_IMPLEMENTATION: &str =
    "https://www.lab-compiler.org/ns/adapter-implementation#LabSimulatorThermalV1";

/// Thermal features the Opentrons Thermocycler Module templates realize. Neither Opentrons
/// application format expresses a controlled ramp rate.
const OPENTRONS_THERMAL_FEATURES: &[ProgramFeature] = &[
    ProgramFeature::ThermalStageRepeat,
    ProgramFeature::ThermalHeatedLid,
    ProgramFeature::ThermalFinalHold,
    ProgramFeature::ThermalMultiSample,
];

/// Canonical pipetting features the Flex protocol builder realizes.
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

/// Canonical pipetting features the STAR choreographer realizes. It carries its own measured
/// volume-to-height models, so it tracks a liquid surface the Flex builder cannot.
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

/// Thermal features the Inheco ODTC realizes. It controls ramp rate, and it addresses one load at
/// a time rather than an independently addressable sample count.
const ODTC_THERMAL_FEATURES: &[ProgramFeature] = &[
    ProgramFeature::ThermalStageRepeat,
    ProgramFeature::ThermalHeatedLid,
    ProgramFeature::ThermalControlledRamp,
    ProgramFeature::ThermalFinalHold,
];

/// The semantic simulator preserves every canonical value because it emits meaning rather than
/// motion.
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

fn builtin_adapter_descriptors() -> Result<Vec<AdapterDescriptor>, AdapterProfileContractError> {
    let descriptors = vec![
        descriptor(
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
        )?,
        descriptor(
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
        )?,
        descriptor(
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
        )?,
        descriptor(
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
        )?,
        descriptor(
            "byonoy.absorbance96",
            "Byonoy Absorbance 96",
            Some("Byonoy"),
            ["hid", "plate-reader"],
            Vec::new(),
            schema_value::<EmptyAdapterProfile>()?,
            validate_byonoy_profile("byonoy.absorbance96", "")?,
        )?,
        descriptor(
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
        )?,
    ];
    Ok(descriptors)
}

/// The adapters linked into the default Lab application.
///
/// This is the sole composition root. Each entry carries description, profile parsing, pure
/// planning feasibility, and lowering together.
pub fn builtin_adapter_registry() -> Result<AdapterRegistry, AdapterProfileContractError> {
    let mut descriptors = builtin_adapter_descriptors()?
        .into_iter()
        .map(|descriptor| (descriptor.id.clone(), descriptor))
        .collect::<BTreeMap<_, _>>();
    let mut take = |id: &str| {
        descriptors.remove(id).ok_or_else(|| {
            AdapterProfileContractError::Contract(format!(
                "built-in adapter composition is missing descriptor '{id}'"
            ))
        })
    };
    let registry = AdapterRegistry::new([
        AdapterRegistration::new(
            take("opentrons.ot2")?,
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
        )),
        AdapterRegistration::new(
            take("opentrons.flex")?,
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
        )),
        AdapterRegistration::new(
            take("hamilton.star")?,
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
        ),
        AdapterRegistration::new(
            take("inheco.odtc")?,
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
        ),
        AdapterRegistration::new(
            take("byonoy.absorbance96")?,
            validate_byonoy_profile,
            declared_program_feasible,
            unsupported_invocation,
        ),
        AdapterRegistration::new(
            take("lab.simulator")?,
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
        ),
    ])?;
    validate_runtime_registry(&registry).map_err(AdapterProfileContractError::Contract)?;
    debug_assert!(descriptors.is_empty());
    Ok(registry)
}

/// Describes every concrete adapter in the default Lab application.
pub fn adapter_catalog() -> Result<AdapterCatalog, AdapterProfileContractError> {
    Ok(builtin_adapter_registry()?.catalog())
}

fn descriptor<const F: usize>(
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
fn pipetting_implementation<const C: usize, const M: usize, const A: usize, const E: usize>(
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
fn thermal_implementation<const M: usize, const A: usize, const E: usize>(
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

fn implementation_id(value: &'static str) -> ProcedureImplementationId {
    ProcedureImplementationId::new(value)
        .expect("built-in Procedure implementation identity is an absolute IRI")
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

fn validate_flex_profile(
    name: &str,
    contents: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    let driver = "opentrons.flex";
    let profile =
        FlexAdapterProfile::parse(name, contents).map_err(|error| invalid(driver, error))?;
    canonical_adapter_profile(driver, name, &profile)
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

fn validate_empty_for(
    driver: &str,
    name: &str,
    contents: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    let _: EmptyAdapterProfile = toml::from_str(contents)?;
    Ok(empty_profile(driver, name))
}

fn validate_odtc_profile(
    name: &str,
    contents: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    validate_empty_for("inheco.odtc", name, contents)
}

fn validate_byonoy_profile(
    name: &str,
    contents: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    validate_empty_for("byonoy.absorbance96", name, contents)
}

fn validate_simulator_profile(
    name: &str,
    contents: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    validate_empty_for("lab.simulator", name, contents)
}

fn declared_program_feasible(
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

/// Build the immutable portion of an allocation so an adapter can run the same structural
/// projection during candidate selection that it will run before lowering. Placeholder inventory
/// identities never reach an artifact; the projection reads only program structure, typed task
/// values, and material roles at this point.
fn feasibility_task(
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

fn parse_profile_for_lowering<T>(
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

fn lower_simulator_registered_invocation(
    _profile: &ValidatedAdapterProfile,
    plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
    _contracts: &ProcedureContractRegistry,
) -> Result<AdapterInvocationLowering, AdapterLoweringError> {
    lower_simulator_invocation(plan, invocation)
}

fn unsupported_invocation(
    profile: &ValidatedAdapterProfile,
    _plan: &AdapterInvocationPlan,
    _invocation: &AdapterInvocation,
    _contracts: &ProcedureContractRegistry,
) -> Result<AdapterInvocationLowering, AdapterLoweringError> {
    Err(AdapterLoweringError::UnsupportedInvocation {
        driver: profile.driver.clone(),
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

fn schema_value<T: JsonSchema>() -> Result<Value, AdapterProfileContractError> {
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

fn canonical_adapter_profile<T: Serialize>(
    driver: &str,
    name: &str,
    profile: &T,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    lab_adapter_api::canonical_adapter_profile(driver, name, profile)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EmptyAdapterProfile {}

fn empty_profile(driver: &str, name: &str) -> ValidatedAdapterProfile {
    canonical_adapter_profile(driver, name, &EmptyAdapterProfile::default())
        .expect("the empty adapter profile has a canonical representation")
}

fn invalid(driver: &str, error: impl std::fmt::Display) -> AdapterProfileContractError {
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
