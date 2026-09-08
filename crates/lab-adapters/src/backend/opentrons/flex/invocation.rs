//! Requirement-scoped Flex lowering to standalone Protocol Designer JSON documents.

use std::collections::{BTreeMap, BTreeSet};

use lab_compiler::allocation::{AllocatedProcedureTask, AllocatedRequirementBinding};
use lab_compiler::method::LocalId;
use lab_compiler::procedure::ProcedureContractRegistry;
use lab_compiler::procedure::vocabulary::{PIPETTING_PROGRAM_V1, THERMAL_PROGRAM_V1};
use lab_compiler::procedure::{
    AspirationStrategy, DispenseStrategy, FluidPathPolicy, Location, MixTechnique,
    PipettingProgramV1, PipettingStep, ProcedureLocalId, TransferTechnique, VesselRole,
};
use lab_instruments::ThermalProfile;
use lab_runfmt::OPENTRONS_PROTOCOL_DESIGNER_FORMAT;
use opentrons_protocol::schema::Metadata;
use opentrons_protocol::v8::schema::{WellLocation, WellOrigin};
use opentrons_protocol::{
    FlexPipetteName, FlexProtocolBuilder, FlexSlot, LabwareId, PipetteId, PipetteMount,
    ProtocolError, TemperatureModule, Thermocycler, standard_definition,
};
use serde::Serialize;

use crate::backend::adapters::{AdapterInvocationDocument, AdapterInvocationLowering};
use crate::backend::document::{Column, Doc, DocMeta, bold, code, text};
use crate::backend::invocation::{ProcedureTaskView, exact_invocation_tasks};
use crate::backend::opentrons::flex::BACKEND;
use crate::backend::opentrons::flex::profile::{FlexAdapterProfile, Pipette, TipRacks};
use crate::backend::procedure::{
    canonical_pipetting_program, normalized_thermal_program, volume_microlitres,
};
use crate::backend::resources::{PlateCapacity, plate_wells};
use crate::backend::typst;
use crate::{AdapterInvocation, AdapterInvocationPlan, ArtifactBundle, GeneratedArtifact};
use lab_compiler::planning::{
    PlanningProcedureParameter, PlanningTaskInput, PlanningTaskOutput, SelectedCapabilityParameter,
    SelectedMaterialBinding, SelectedMaterialSource,
};

const TASK_PLAN_SCHEMA: &str = "lab.opentrons-flex-task.v1";

#[derive(Serialize)]
struct FlexTaskPlan {
    schema_version: String,
    facility: String,
    asset: String,
    adapter: String,
    adapter_profile: String,
    adapter_profile_sha256: String,
    requirements: Vec<RequirementReview>,
    task: TaskReview,
    deck: FlexAdapterProfile,
    execution: FlexTaskExecution,
}

#[derive(Serialize)]
struct RequirementReview {
    id: LocalId,
    capability_kind: String,
    offering: String,
    observed_qualification: String,
    control_mode: String,
    parameters: Vec<SelectedCapabilityParameter>,
}

fn requirement_reviews(requirements: &[&AllocatedRequirementBinding]) -> Vec<RequirementReview> {
    requirements
        .iter()
        .map(|requirement| RequirementReview {
            id: requirement.id.clone(),
            capability_kind: requirement.capability_kind.to_string(),
            offering: requirement.offering.clone(),
            observed_qualification: requirement.observed_qualification.clone(),
            control_mode: requirement.control_mode.clone(),
            parameters: requirement.parameters.clone(),
        })
        .collect()
}

#[derive(Serialize)]
struct TaskReview {
    id: LocalId,
    operation: String,
    inputs: Vec<PlanningTaskInput>,
    outputs: Vec<PlanningTaskOutput>,
    parameters: Vec<PlanningProcedureParameter>,
    materials: Vec<SelectedMaterialBinding>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum FlexTaskExecution {
    /// Direct interpretation of the open canonical contract. The operation name is review
    /// metadata only; every executable action comes from `program.steps`.
    PipettingProgram {
        title: String,
        program: PipettingProgramV1,
        locations: BTreeMap<ProcedureLocalId, Vec<FlexPhysicalLocation>>,
        staging_temperatures: BTreeMap<String, f64>,
        sources: Vec<CanonicalSourceReview>,
    },
    ThermalProgram {
        artifact: String,
        reaction_wells: Vec<String>,
        volume_each_ul: f64,
        lid_temperature_c: Option<f64>,
        profile: ThermalProfile,
        final_hold_celsius: Option<f64>,
    },
}

#[derive(Clone, Serialize)]
struct FlexPhysicalLocation {
    resource: FlexPhysicalResource,
    well: String,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum FlexPhysicalResource {
    Sources,
    Work,
    Bulk,
}

#[derive(Clone, Serialize)]
struct CanonicalSourceReview {
    vessel: ProcedureLocalId,
    material: ProcedureLocalId,
    binding: SelectedMaterialBinding,
    wells: Vec<String>,
}

/// Lower only the Procedure tasks and requirements allocated to this exact Flex invocation.
pub(in crate::backend) fn lower_invocation(
    profile: &FlexAdapterProfile,
    invocation_plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
    contracts: &ProcedureContractRegistry,
) -> Result<AdapterInvocationLowering, String> {
    let tasks = exact_invocation_tasks("Flex", invocation_plan, invocation)?;
    let mut artifacts = ArtifactBundle::new();
    let mut documents = Vec::new();

    for (ordinal, member) in tasks.into_iter().enumerate() {
        let (slug, execution) = plan_task(profile, member.task, &member.requirements, contracts)?;
        let directory = format!("tasks/{:03}-{slug}", ordinal + 1);
        let plan = FlexTaskPlan {
            schema_version: TASK_PLAN_SCHEMA.to_owned(),
            facility: invocation_plan.allocated.facility.clone(),
            asset: invocation.asset.clone(),
            adapter: BACKEND.to_owned(),
            adapter_profile: profile.name.clone(),
            adapter_profile_sha256: invocation.adapter.profile_sha256.clone(),
            requirements: requirement_reviews(&member.requirements),
            task: TaskReview {
                id: member.task.id.clone(),
                operation: member.task.operation.to_string(),
                inputs: member.task.inputs.clone(),
                outputs: member.task.outputs.clone(),
                parameters: member.task.parameters.clone(),
                materials: member.task.materials.clone(),
            },
            deck: profile.clone(),
            execution,
        };
        let protocol_path = format!("{directory}/automation_protocol.json");
        artifacts
            .insert_text(&protocol_path, "application/json", render_protocol(&plan)?)
            .map_err(|error| error.to_string())?;
        artifacts
            .insert_text(
                format!("{directory}/invocation_manifest.json"),
                "application/json",
                pretty_json(&plan)?,
            )
            .map_err(|error| error.to_string())?;
        artifacts
            .insert_text(
                format!("{directory}/manual_protocol.typ"),
                "text/x-typst",
                typst::render(&render_manual(&plan)),
            )
            .map_err(|error| error.to_string())?;
        artifacts
            .insert(
                GeneratedArtifact::text(
                    format!("{directory}/{}", typst::STYLE_PATH),
                    "text/x-typst",
                    typst::STYLE,
                )
                .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        documents.push(AdapterInvocationDocument {
            requirements: member
                .requirements
                .iter()
                .map(|requirement| requirement.id.clone())
                .collect(),
            path: protocol_path,
            format: OPENTRONS_PROTOCOL_DESIGNER_FORMAT.to_owned(),
        });
    }

    Ok(AdapterInvocationLowering {
        artifacts,
        documents,
    })
}

fn plan_task(
    profile: &FlexAdapterProfile,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
    contracts: &ProcedureContractRegistry,
) -> Result<(&'static str, FlexTaskExecution), String> {
    if task
        .program
        .as_ref()
        .is_some_and(|program| program.contract.as_str() == THERMAL_PROGRAM_V1)
    {
        return Ok((
            "thermal-program",
            plan_cycle(profile, task, requirements, contracts)?,
        ));
    }
    if task
        .program
        .as_ref()
        .is_some_and(|program| program.contract.as_str() == PIPETTING_PROGRAM_V1)
    {
        return Ok((
            "pipetting-program",
            plan_pipetting_program(profile, task, requirements, contracts)?,
        ));
    }
    Err(format!(
        "Flex invocation does not implement the canonical Procedure program shape in task '{}' (descriptive operation '{}')",
        task.id, task.operation
    ))
}

fn plan_pipetting_program(
    profile: &FlexAdapterProfile,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
    contracts: &ProcedureContractRegistry,
) -> Result<FlexTaskExecution, String> {
    let validated = canonical_pipetting_program("Flex", task, requirements, contracts)?;
    let program = validated.as_program();
    let source_wells = plate_wells(profile.resources.sources.capacity);
    let work_wells = plate_wells(profile.resources.work.capacity);
    let bulk_wells = plate_wells(profile.resources.bulk.capacity);
    let mut bulk_cursor = 0usize;
    let mut source_cursor = 0usize;
    let mut work_cursor = 0usize;
    let mut locations = BTreeMap::new();
    let mut sources = Vec::new();

    // Logical vessels may partition an aggregate input. InputOutput owns its
    // positions directly and does not require a separate input declaration.
    for vessel in program.vessels.iter().filter(|vessel| {
        matches!(
            vessel.role,
            VesselRole::ProcedureInput { .. } | VesselRole::InputOutput { .. }
        )
    }) {
        let count = usize::try_from(vessel.positions)
            .map_err(|_| format!("Flex task '{}' vessel position count overflows", task.id))?;
        let physical = take_flex_locations(
            task,
            "thermocycler work wells",
            &work_wells,
            &mut work_cursor,
            count,
            FlexPhysicalResource::Work,
        )?;
        locations.insert(vessel.id.clone(), physical);
    }

    for vessel in &program.vessels {
        if locations.contains_key(&vessel.id) {
            continue;
        }
        let count = usize::try_from(vessel.positions)
            .map_err(|_| format!("Flex task '{}' vessel position count overflows", task.id))?;
        let physical = match &vessel.role {
            VesselRole::MaterialSource { material } => {
                let binding = task
                    .materials
                    .iter()
                    .find(|binding| binding.input.as_str() == material.as_str())
                    .ok_or_else(|| {
                        format!(
                            "Flex Procedure task '{}' canonical material '{}' has no exact allocation",
                            task.id, material
                        )
                    })?
                    .clone();
                let loaded = vessel
                    .initial_volume_each
                    .as_ref()
                    .map(|v| v.value().to_string().parse::<f64>().unwrap())
                    .unwrap_or(0.0);
                let physical = if loaded > f64::from(profile.resources.sources.max_volume_each_ul) {
                    take_flex_locations(
                        task,
                        "bulk source wells",
                        &bulk_wells,
                        &mut bulk_cursor,
                        count,
                        FlexPhysicalResource::Bulk,
                    )?
                } else {
                    take_flex_locations(
                        task,
                        "temperature-module source wells",
                        &source_wells,
                        &mut source_cursor,
                        count,
                        FlexPhysicalResource::Sources,
                    )?
                };
                sources.push(CanonicalSourceReview {
                    vessel: vessel.id.clone(),
                    material: material.clone(),
                    binding,
                    wells: physical
                        .iter()
                        .map(|location| location.well.clone())
                        .collect(),
                });
                physical
            }
            VesselRole::ProcedureInput { .. } | VesselRole::InputOutput { .. } => unreachable!(),
            VesselRole::Product { .. }
            | VesselRole::MaterialProduct { .. }
            | VesselRole::Intermediate => take_flex_locations(
                task,
                "thermocycler work wells",
                &work_wells,
                &mut work_cursor,
                count,
                FlexPhysicalResource::Work,
            )?,
        };
        locations.insert(vessel.id.clone(), physical);
    }

    super::super::staging::working_volumes(program, |vessel| {
        match locations[vessel][0].resource {
            FlexPhysicalResource::Sources => profile.resources.sources.max_volume_each_ul,
            FlexPhysicalResource::Work => profile.resources.work.max_volume_each_ul,
            FlexPhysicalResource::Bulk => profile.resources.bulk.max_volume_each_ul,
        }
    })?;
    let staging_temperatures =
        super::super::staging::temperatures(program, |vessel| {
            match locations[vessel][0].resource {
                FlexPhysicalResource::Sources => "sources",
                FlexPhysicalResource::Work => "work",
                FlexPhysicalResource::Bulk => "bulk",
            }
        })?;
    validate_flex_canonical_steps(profile, task, program)?;
    Ok(FlexTaskExecution::PipettingProgram {
        title: operation_title(task),
        program: program.clone(),
        staging_temperatures,
        locations,
        sources,
    })
}

fn take_flex_locations(
    task: &AllocatedProcedureTask,
    resource: &str,
    wells: &[String],
    cursor: &mut usize,
    count: usize,
    physical_resource: FlexPhysicalResource,
) -> Result<Vec<FlexPhysicalLocation>, String> {
    let end = cursor.checked_add(count).ok_or_else(|| {
        format!(
            "Flex Procedure task '{}' well allocation overflows",
            task.id
        )
    })?;
    if end > wells.len() {
        return Err(ProcedureTaskView::new("Flex", task).capacity_error(
            resource,
            end,
            wells.len(),
        ));
    }
    let result = wells[*cursor..end]
        .iter()
        .map(|well| FlexPhysicalLocation {
            resource: physical_resource,
            well: well.clone(),
        })
        .collect();
    *cursor = end;
    Ok(result)
}

fn validate_flex_canonical_steps(
    profile: &FlexAdapterProfile,
    task: &AllocatedProcedureTask,
    program: &PipettingProgramV1,
) -> Result<(), String> {
    let small_working =
        working_volume_ul(&profile.instruments.small, &profile.resources.small_tips)?;
    let large_working =
        working_volume_ul(&profile.instruments.large, &profile.resources.large_tips)?;
    let mut small_tips = 0usize;
    let mut large_tips = 0usize;
    let mut open_group: Option<&ProcedureLocalId> = None;
    let mut group_maximum = 0.0_f64;
    let mut closed_groups = BTreeSet::new();

    for step in &program.steps {
        let group = step_group(step);
        if group != open_group {
            if let Some(previous) = open_group {
                closed_groups.insert(previous.clone());
                count_flex_group_tip(
                    task,
                    group_maximum,
                    small_working,
                    large_working,
                    &mut small_tips,
                    &mut large_tips,
                )?;
                group_maximum = 0.0;
            }
            if group.is_some_and(|group| closed_groups.contains(group)) {
                return Err(format!(
                    "Flex Procedure task '{}' fluid-path group is not contiguous",
                    task.id
                ));
            }
            open_group = group;
        }

        let (volume, multiplicity) = match step {
            PipettingStep::Transfer {
                volume, technique, ..
            } => {
                validate_flex_transfer_technique(task, technique)?;
                (volume_microlitres("Flex", task, "transfer", volume)?, 1)
            }
            PipettingStep::Distribute {
                destinations,
                volume_each,
                fluid_path,
                technique,
                ..
            } => {
                validate_flex_transfer_technique(task, technique)?;
                if group.is_some()
                    && matches!(fluid_path, FluidPathPolicy::IsolatedDestinations)
                    && destinations.len() > 1
                {
                    return Err(format!(
                        "Flex Procedure task '{}' cannot combine a shared fluid-path group with an isolated multi-destination distribution",
                        task.id
                    ));
                }
                let volume = volume_microlitres("Flex", task, "distribution", volume_each)?;
                let working = flex_working_volume(task, volume, small_working, large_working)?;
                let multiplicity = match fluid_path {
                    FluidPathPolicy::IsolatedDestinations => destinations.len(),
                    FluidPathPolicy::SharedSourceNoReentry => {
                        ((volume * destinations.len() as f64) / working)
                            .ceil()
                            .max(1.0) as usize
                    }
                };
                (volume, multiplicity)
            }
            PipettingStep::Mix {
                targets,
                volume,
                fluid_path,
                technique,
                ..
            } => {
                validate_flex_mix_technique(task, technique)?;
                if group.is_some()
                    && matches!(fluid_path, FluidPathPolicy::IsolatedDestinations)
                    && targets.len() > 1
                {
                    return Err(format!(
                        "Flex Procedure task '{}' cannot combine a shared fluid-path group with isolated multi-target mixing",
                        task.id
                    ));
                }
                (
                    volume_microlitres("Flex", task, "mix", volume)?,
                    targets.len(),
                )
            }
            PipettingStep::Barrier { .. } => {
                return Err(format!(
                    "Flex Procedure task '{}' contains a Barrier step, which this implementation does not claim",
                    task.id
                ));
            }
        };

        if group.is_some() {
            group_maximum = group_maximum.max(volume);
        } else {
            add_flex_tips(
                task,
                volume,
                multiplicity,
                small_working,
                large_working,
                &mut small_tips,
                &mut large_tips,
            )?;
        }
    }
    if open_group.is_some() {
        count_flex_group_tip(
            task,
            group_maximum,
            small_working,
            large_working,
            &mut small_tips,
            &mut large_tips,
        )?;
    }

    let small_capacity = profile.resources.small_tips.total_capacity();
    if small_tips > small_capacity {
        return Err(ProcedureTaskView::new("Flex", task).capacity_error(
            "small canonical-program tips",
            small_tips,
            small_capacity,
        ));
    }
    let large_capacity = profile.resources.large_tips.total_capacity();
    if large_tips > large_capacity {
        return Err(ProcedureTaskView::new("Flex", task).capacity_error(
            "large canonical-program tips",
            large_tips,
            large_capacity,
        ));
    }
    Ok(())
}

fn flex_working_volume(
    task: &AllocatedProcedureTask,
    volume: f64,
    small_working: f64,
    large_working: f64,
) -> Result<f64, String> {
    if volume <= small_working {
        Ok(small_working)
    } else if volume <= large_working {
        Ok(large_working)
    } else {
        Err(format!(
            "Flex Procedure task '{}' operation requires {volume} uL, above the configured {large_working} uL tip capacity",
            task.id
        ))
    }
}

#[allow(clippy::too_many_arguments)]
fn add_flex_tips(
    task: &AllocatedProcedureTask,
    volume: f64,
    count: usize,
    small_working: f64,
    large_working: f64,
    small_tips: &mut usize,
    large_tips: &mut usize,
) -> Result<(), String> {
    let tips = if volume <= small_working {
        small_tips
    } else if volume <= large_working {
        large_tips
    } else {
        return Err(format!(
            "Flex Procedure task '{}' operation requires {volume} uL, above the configured {large_working} uL tip capacity",
            task.id
        ));
    };
    *tips = tips
        .checked_add(count)
        .ok_or_else(|| format!("Flex Procedure task '{}' tip count overflows", task.id))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn count_flex_group_tip(
    task: &AllocatedProcedureTask,
    maximum: f64,
    small_working: f64,
    large_working: f64,
    small_tips: &mut usize,
    large_tips: &mut usize,
) -> Result<(), String> {
    if maximum == 0.0 {
        return Ok(());
    }
    add_flex_tips(
        task,
        maximum,
        1,
        small_working,
        large_working,
        small_tips,
        large_tips,
    )
}
fn validate_flex_transfer_technique(
    task: &AllocatedProcedureTask,
    technique: &TransferTechnique,
) -> Result<(), String> {
    if !matches!(
        technique.aspiration,
        AspirationStrategy::Liquid | AspirationStrategy::TrackedLiquidSurface
    ) || technique.dispense != DispenseStrategy::Liquid
        || technique.air_gap.is_some()
        || technique.blow_out
        || technique.touch_tip
    {
        return Err(format!(
            "Flex Procedure task '{}' requests a transfer technique outside this implementation's declared canonical features",
            task.id
        ));
    }
    Ok(())
}

fn validate_flex_mix_technique(
    task: &AllocatedProcedureTask,
    technique: &MixTechnique,
) -> Result<(), String> {
    if !matches!(
        technique.aspiration,
        AspirationStrategy::Liquid | AspirationStrategy::TrackedLiquidSurface
    ) || technique.dispense != DispenseStrategy::Liquid
        || technique.blow_out
        || technique.touch_tip
    {
        return Err(format!(
            "Flex Procedure task '{}' requests a mix technique outside this implementation's declared canonical features",
            task.id
        ));
    }
    Ok(())
}

fn step_group(step: &PipettingStep) -> Option<&ProcedureLocalId> {
    match step {
        PipettingStep::Transfer {
            fluid_path_group, ..
        }
        | PipettingStep::Distribute {
            fluid_path_group, ..
        }
        | PipettingStep::Mix {
            fluid_path_group, ..
        } => fluid_path_group.as_ref(),
        PipettingStep::Barrier { .. } => None,
    }
}

fn operation_title(task: &AllocatedProcedureTask) -> String {
    task.operation
        .as_str()
        .rsplit(['#', '/', '.'])
        .find(|part| !part.is_empty())
        .unwrap_or("pipetting program")
        .replace(['_', '-'], " ")
}

/// Pure candidate-time check using the same structural dispatcher and profile limits as lowering.
pub(in crate::backend) fn check_task_feasibility(
    profile: &FlexAdapterProfile,
    task: &AllocatedProcedureTask,
    contracts: &ProcedureContractRegistry,
) -> Result<(), String> {
    let requirements = task.requirements.iter().collect::<Vec<_>>();
    plan_task(profile, task, &requirements, contracts).map(|_| ())
}

fn plan_cycle(
    profile: &FlexAdapterProfile,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
    contracts: &ProcedureContractRegistry,
) -> Result<FlexTaskExecution, String> {
    let procedure = normalized_thermal_program("Flex", task, requirements, contracts)?;
    let view = ProcedureTaskView::new("Flex", task);
    let reaction_wells = known_wells(task, "reaction plate", profile.resources.work.capacity)?;
    if procedure.sample_count > reaction_wells.len() {
        return Err(view.capacity_error(
            "reaction plate",
            procedure.sample_count,
            reaction_wells.len(),
        ));
    }
    for (name, value) in procedure
        .profile
        .stages
        .iter()
        .flat_map(|stage| stage.steps.iter())
        .map(|step| ("block temperature", step.celsius))
        .chain(
            procedure
                .lid_temperature_c
                .map(|value| ("lid temperature", value)),
        )
        .chain(
            procedure
                .final_hold_celsius
                .map(|value| ("final hold temperature", value)),
        )
    {
        let maximum = if name == "lid temperature" {
            110.0
        } else {
            99.0
        };
        if value > maximum {
            return Err(format!(
                "Flex Procedure task '{}' {name} is {value} °C, above the adapter's {maximum} °C limit",
                task.id,
            ));
        }
    }
    if !(10.0..=100.0).contains(&procedure.volume_each_ul) {
        return Err(format!(
            "Flex Procedure task '{}' sample volume {} µL is outside the Thermocycler Module's 10–100 µL working range",
            task.id, procedure.volume_each_ul
        ));
    }
    if procedure
        .profile
        .stages
        .iter()
        .any(|stage| stage.steps.iter().any(|step| step.ramp_c_per_s.is_some()))
    {
        return Err(format!(
            "Flex Procedure task '{}' requests explicit ramp control, which this implementation does not claim",
            task.id
        ));
    }

    Ok(FlexTaskExecution::ThermalProgram {
        artifact: procedure.artifact,
        reaction_wells: reaction_wells
            .into_iter()
            .take(procedure.sample_count)
            .collect(),
        volume_each_ul: procedure.volume_each_ul,
        lid_temperature_c: procedure.lid_temperature_c,
        profile: procedure.profile,
        final_hold_celsius: procedure.final_hold_celsius,
    })
}

fn render_protocol(plan: &FlexTaskPlan) -> Result<String, String> {
    match &plan.execution {
        FlexTaskExecution::PipettingProgram { .. } => render_pipetting_program(plan),
        FlexTaskExecution::ThermalProgram { .. } => render_cycle(plan),
    }
}

fn render_pipetting_program(plan: &FlexTaskPlan) -> Result<String, String> {
    let FlexTaskExecution::PipettingProgram {
        title,
        program,
        staging_temperatures,
        locations,
        ..
    } = &plan.execution
    else {
        unreachable!("canonical renderer receives only a PipettingProgramV1 plan")
    };
    let profile = &plan.deck;
    let mut builder = stage_builder(profile, &format!("Lab canonical pipetting: {title}"));
    let temperature = builder
        .load_module::<TemperatureModule>(slot(&profile.resources.sources.slot))
        .map_err(protocol_error)?;
    let sources = builder
        .load_labware_on_module(&profile.resources.sources.labware, temperature)
        .map_err(protocol_error)?;
    let thermocycler = builder
        .load_module::<Thermocycler>(FlexSlot::B1)
        .map_err(protocol_error)?;
    let work = builder
        .load_labware_on_module(&profile.resources.work.labware, thermocycler)
        .map_err(protocol_error)?;
    let bulk = builder
        .load_labware(
            &profile.resources.bulk.labware,
            slot(&profile.resources.bulk.slot),
        )
        .map_err(protocol_error)?;
    let mut small_tips = TipFeeder::load(&mut builder, &profile.resources.small_tips)?;
    let mut large_tips = TipFeeder::load(&mut builder, &profile.resources.large_tips)?;
    let small = load_instrument(&mut builder, &profile.instruments.small)?;
    let large = load_instrument(&mut builder, &profile.instruments.large)?;
    let small_working = small.max_volume.min(small_tips.tip_volume);
    builder.thermocycler_open_lid(thermocycler);
    if let Some(target) = staging_temperatures.get("sources") {
        builder
            .temperature_module_set_target(temperature, *target)
            .map_err(protocol_error)?;
        builder
            .temperature_module_wait_for_temperature(temperature)
            .map_err(protocol_error)?;
    }
    if let Some(target) = staging_temperatures.get("work") {
        builder
            .thermocycler_set_block_temperature(thermocycler, *target, None, None)
            .map_err(protocol_error)?;
        builder
            .thermocycler_wait_for_block_temperature(thermocycler)
            .map_err(protocol_error)?;
    }

    let physical = |at: &Location| -> Result<(LabwareId, String), String> {
        let location = locations
            .get(&at.vessel)
            .and_then(|positions| positions.get(at.position as usize))
            .ok_or_else(|| {
                format!(
                    "Flex canonical location '{}[{}]' was not allocated",
                    at.vessel, at.position
                )
            })?;
        let labware = match location.resource {
            FlexPhysicalResource::Sources => sources,
            FlexPhysicalResource::Work => work,
            FlexPhysicalResource::Bulk => bulk,
        };
        Ok((labware, location.well.clone()))
    };

    let group_uses_large = |group: &ProcedureLocalId| {
        program
            .steps
            .iter()
            .filter(|step| step_group(step) == Some(group))
            .any(|step| rendered_step_volume(step) > small_working)
    };
    let mut held_group: Option<ProcedureLocalId> = None;
    let mut held_tip: Option<bool> = None;
    let mut withdrawn = BTreeMap::<(ProcedureLocalId, u32), f64>::new();
    for step in &program.steps {
        let group = step_group(step).cloned();
        if held_group != group {
            if let Some(held_large) = held_tip.take() {
                let pipette = if held_large { &large } else { &small };
                builder
                    .drop_tip_into_trash(pipette.id)
                    .map_err(protocol_error)?;
            }
            held_group = group.clone();
        }

        let use_large = group.as_ref().map_or_else(
            || rendered_step_volume(step) > small_working,
            &group_uses_large,
        );
        let (tips, pipette) = if use_large {
            (&mut large_tips, &large)
        } else {
            (&mut small_tips, &small)
        };
        if held_tip.is_some_and(|held_large| held_large != use_large) {
            return Err("Flex canonical fluid-path group crosses pipette classes".to_owned());
        }

        match step {
            PipettingStep::Transfer {
                source,
                destination,
                volume,
                technique,
                ..
            } => {
                if held_tip.is_none() {
                    pick_up_flex_tip(&mut builder, tips, pipette)?;
                    held_tip = Some(use_large);
                }
                execute_flex_transfer(
                    &mut builder,
                    pipette,
                    physical(source)?,
                    physical(destination)?,
                    rendered_volume(volume),
                    technique,
                    &profile.techniques,
                    &mut withdrawn,
                    source,
                )?;
                if group.is_none() {
                    builder
                        .drop_tip_into_trash(pipette.id)
                        .map_err(protocol_error)?;
                    held_tip = None;
                }
            }
            PipettingStep::Distribute {
                source,
                destinations,
                volume_each,
                fluid_path,
                technique,
                ..
            } => {
                let volume = rendered_volume(volume_each);
                if group.is_some() {
                    if held_tip.is_none() {
                        pick_up_flex_tip(&mut builder, tips, pipette)?;
                        held_tip = Some(use_large);
                    }
                    execute_flex_distribution(
                        &mut builder,
                        pipette,
                        tips,
                        physical(source)?,
                        destinations,
                        &physical,
                        volume,
                        technique,
                        &profile.techniques,
                        &mut withdrawn,
                        source,
                        false,
                    )?;
                } else if matches!(fluid_path, FluidPathPolicy::IsolatedDestinations) {
                    for destination in destinations {
                        pick_up_flex_tip(&mut builder, tips, pipette)?;
                        execute_flex_transfer(
                            &mut builder,
                            pipette,
                            physical(source)?,
                            physical(destination)?,
                            volume,
                            technique,
                            &profile.techniques,
                            &mut withdrawn,
                            source,
                        )?;
                        builder
                            .drop_tip_into_trash(pipette.id)
                            .map_err(protocol_error)?;
                    }
                } else {
                    execute_flex_distribution(
                        &mut builder,
                        pipette,
                        tips,
                        physical(source)?,
                        destinations,
                        &physical,
                        volume,
                        technique,
                        &profile.techniques,
                        &mut withdrawn,
                        source,
                        true,
                    )?;
                }
            }
            PipettingStep::Mix {
                targets,
                cycles,
                volume,
                technique,
                ..
            } => {
                let volume = rendered_volume(volume);
                for target in targets {
                    if held_tip.is_none() {
                        pick_up_flex_tip(&mut builder, tips, pipette)?;
                        held_tip = Some(use_large);
                    }
                    let (labware, well) = physical(target)?;
                    execute_flex_mix(
                        &mut builder,
                        pipette,
                        labware,
                        &well,
                        *cycles,
                        volume,
                        technique,
                        &profile.techniques,
                        &mut withdrawn,
                        target,
                    )?;
                    if group.is_none() {
                        builder
                            .drop_tip_into_trash(pipette.id)
                            .map_err(protocol_error)?;
                        held_tip = None;
                    }
                }
            }
            PipettingStep::Barrier { reason, .. } => builder.comment(reason),
        }
    }
    if let Some(held_large) = held_tip {
        let pipette = if held_large { &large } else { &small };
        builder
            .drop_tip_into_trash(pipette.id)
            .map_err(protocol_error)?;
    }
    builder.comment("Canonical PipettingProgramV1 complete.");
    render(builder)
}

// Rendering already has the exact task review but volume diagnostics expect the allocated task.
// Conversions were proven during planning, so this narrow helper keeps rendering infallible
// without duplicating decimal parsing logic in the protocol builder.
fn rendered_volume(volume: &lab_compiler::procedure::Volume) -> f64 {
    volume
        .value()
        .to_string()
        .parse()
        .expect("planning accepted this finite canonical volume")
}

fn rendered_step_volume(step: &PipettingStep) -> f64 {
    match step {
        PipettingStep::Transfer { volume, .. } | PipettingStep::Mix { volume, .. } => {
            rendered_volume(volume)
        }
        PipettingStep::Distribute { volume_each, .. } => rendered_volume(volume_each),
        PipettingStep::Barrier { .. } => 0.0,
    }
}

fn pick_up_flex_tip(
    builder: &mut FlexProtocolBuilder,
    tips: &mut TipFeeder,
    pipette: &Instrument,
) -> Result<(), String> {
    let (rack, well) = tips.next();
    builder
        .pick_up_tip(pipette.id, rack, &well)
        .map_err(protocol_error)
}

#[allow(clippy::too_many_arguments)]
fn execute_flex_transfer(
    builder: &mut FlexProtocolBuilder,
    pipette: &Instrument,
    source: (LabwareId, String),
    destination: (LabwareId, String),
    volume: f64,
    technique: &TransferTechnique,
    calibration: &crate::backend::opentrons::flex::profile::FlexTechniqueCalibration,
    withdrawn: &mut BTreeMap<(ProcedureLocalId, u32), f64>,
    logical_source: &Location,
) -> Result<(), String> {
    let withdrawn_before = *withdrawn
        .get(&(logical_source.vessel.clone(), logical_source.position))
        .unwrap_or(&0.0);
    let aspiration = match technique.aspiration {
        AspirationStrategy::Liquid => None,
        AspirationStrategy::TrackedLiquidSurface => Some(WellLocation::with_offset(
            WellOrigin::Bottom,
            0.0,
            0.0,
            calibration.tracked_offset_mm(withdrawn_before),
        )),
        _ => unreachable!("planning rejected an unsupported Flex aspiration strategy"),
    };
    builder
        .aspirate(
            pipette.id,
            source.0,
            &source.1,
            volume,
            pipette.flow_rate,
            aspiration,
        )
        .map_err(protocol_error)?;
    builder
        .dispense(
            pipette.id,
            destination.0,
            &destination.1,
            volume,
            pipette.flow_rate,
            None,
        )
        .map_err(protocol_error)?;
    *withdrawn
        .entry((logical_source.vessel.clone(), logical_source.position))
        .or_default() += volume;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn execute_flex_distribution(
    builder: &mut FlexProtocolBuilder,
    pipette: &Instrument,
    tips: &mut TipFeeder,
    source: (LabwareId, String),
    destinations: &[Location],
    physical: &impl Fn(&Location) -> Result<(LabwareId, String), String>,
    volume: f64,
    technique: &TransferTechnique,
    calibration: &crate::backend::opentrons::flex::profile::FlexTechniqueCalibration,
    withdrawn: &mut BTreeMap<(ProcedureLocalId, u32), f64>,
    logical_source: &Location,
    manage_tips: bool,
) -> Result<(), String> {
    let working = pipette.max_volume.min(tips.tip_volume);
    let per_load = (working / volume).floor().max(1.0) as usize;
    for chunk in destinations.chunks(per_load) {
        if manage_tips {
            pick_up_flex_tip(builder, tips, pipette)?;
        }
        let withdrawn_before = *withdrawn
            .get(&(logical_source.vessel.clone(), logical_source.position))
            .unwrap_or(&0.0);
        let aspiration = match technique.aspiration {
            AspirationStrategy::Liquid => None,
            AspirationStrategy::TrackedLiquidSurface => Some(WellLocation::with_offset(
                WellOrigin::Bottom,
                0.0,
                0.0,
                calibration.tracked_offset_mm(withdrawn_before),
            )),
            _ => unreachable!("planning rejected an unsupported Flex aspiration strategy"),
        };
        let load = volume * chunk.len() as f64;
        builder
            .aspirate(
                pipette.id,
                source.0,
                &source.1,
                load,
                pipette.flow_rate,
                aspiration,
            )
            .map_err(protocol_error)?;
        for destination in chunk {
            let (labware, well) = physical(destination)?;
            builder
                .dispense(pipette.id, labware, &well, volume, pipette.flow_rate, None)
                .map_err(protocol_error)?;
        }
        *withdrawn
            .entry((logical_source.vessel.clone(), logical_source.position))
            .or_default() += load;
        if manage_tips {
            builder
                .drop_tip_into_trash(pipette.id)
                .map_err(protocol_error)?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn execute_flex_mix(
    builder: &mut FlexProtocolBuilder,
    pipette: &Instrument,
    labware: LabwareId,
    well: &str,
    cycles: u32,
    volume: f64,
    technique: &MixTechnique,
    calibration: &crate::backend::opentrons::flex::profile::FlexTechniqueCalibration,
    withdrawn: &mut BTreeMap<(ProcedureLocalId, u32), f64>,
    logical_target: &Location,
) -> Result<(), String> {
    for _ in 0..cycles {
        let withdrawn_before = *withdrawn
            .get(&(logical_target.vessel.clone(), logical_target.position))
            .unwrap_or(&0.0);
        let aspiration = match technique.aspiration {
            AspirationStrategy::Liquid => None,
            AspirationStrategy::TrackedLiquidSurface => Some(WellLocation::with_offset(
                WellOrigin::Bottom,
                0.0,
                0.0,
                calibration.tracked_offset_mm(withdrawn_before),
            )),
            _ => unreachable!("planning rejected an unsupported Flex aspiration strategy"),
        };
        builder
            .aspirate(
                pipette.id,
                labware,
                well,
                volume,
                pipette.flow_rate,
                aspiration,
            )
            .map_err(protocol_error)?;
        builder
            .dispense(pipette.id, labware, well, volume, pipette.flow_rate, None)
            .map_err(protocol_error)?;
    }
    Ok(())
}

fn render_cycle(plan: &FlexTaskPlan) -> Result<String, String> {
    let FlexTaskExecution::ThermalProgram {
        volume_each_ul,
        lid_temperature_c,
        profile: thermal_profile,
        final_hold_celsius,
        ..
    } = &plan.execution
    else {
        unreachable!("cycle renderer receives only a cycle plan")
    };
    let profile = &plan.deck;
    let mut builder = stage_builder(profile, "Lab canonical thermal program");
    let thermocycler = builder
        .load_module::<Thermocycler>(FlexSlot::B1)
        .map_err(protocol_error)?;
    builder
        .load_labware_on_module(&profile.resources.work.labware, thermocycler)
        .map_err(protocol_error)?;
    builder.thermocycler_close_lid(thermocycler);
    if let Some(lid_temperature_c) = lid_temperature_c {
        builder
            .thermocycler_set_lid_temperature(thermocycler, *lid_temperature_c)
            .map_err(protocol_error)?;
        builder
            .thermocycler_wait_for_lid_temperature(thermocycler)
            .map_err(protocol_error)?;
    }
    let steps = thermal_profile
        .stages
        .iter()
        .flat_map(|stage| {
            (0..stage.repeats).flat_map(|_| {
                stage
                    .steps
                    .iter()
                    .map(|step| (step.celsius, step.hold_seconds))
            })
        })
        .collect::<Vec<_>>();
    builder
        .thermocycler_run_profile(thermocycler, &steps, Some(*volume_each_ul))
        .map_err(protocol_error)?;
    if let Some(final_hold_celsius) = final_hold_celsius {
        builder
            .thermocycler_set_block_temperature(thermocycler, *final_hold_celsius, None, None)
            .map_err(protocol_error)?;
    }
    builder.thermocycler_deactivate_lid(thermocycler);
    builder.thermocycler_open_lid(thermocycler);
    builder.comment("Canonical ThermalProgramV1 complete. Remove and label the samples before another task reuses the staging wells.");
    render(builder)
}

fn render_manual(plan: &FlexTaskPlan) -> Doc {
    let title = match &plan.execution {
        FlexTaskExecution::PipettingProgram { title, .. } => title.clone(),
        FlexTaskExecution::ThermalProgram { artifact, .. } => {
            format!("Run canonical thermal program for {artifact}")
        }
    };
    let mut doc = Doc::new(DocMeta::new(
        title,
        "Operator instructions for one facility-allocated Procedure task",
        &plan.adapter_profile,
        "Opentrons Flex",
    ));
    doc.notice([
        bold("Allocation boundary. "),
        text("This Protocol Designer JSON atomically implements requirements "),
        code(join_requirements(&plan.requirements, |requirement| {
            requirement.id.as_str()
        })),
        text(" on the exact Asset below. Adjacent Procedure work remains in separate reviewed plan nodes."),
    ]);
    doc.heading(1, [text("Reviewed allocation")]);
    doc.table(
        [Column::left("Field"), Column::left("Exact value")],
        [
            vec![vec![text("Asset")], vec![code(&plan.asset)]],
            vec![
                vec![text("Capability offerings")],
                vec![code(join_requirements(&plan.requirements, |requirement| {
                    &requirement.offering
                }))],
            ],
            vec![
                vec![text("Capability kinds")],
                vec![code(join_requirements(&plan.requirements, |requirement| {
                    &requirement.capability_kind
                }))],
            ],
            vec![
                vec![text("Procedure task")],
                vec![code(plan.task.id.as_str())],
            ],
            vec![
                vec![text("Adapter profile SHA-256")],
                vec![code(&plan.adapter_profile_sha256)],
            ],
        ],
    );
    doc.heading(1, [text("Run this task")]);
    doc.para_text("Stage every upstream input at the logical-to-physical locations in this task manifest. Each Procedure is a separate run; preceding files may use different well coordinates.");
    doc.para([
        text("Import "),
        code("automation_protocol.json"),
        text(" into the Opentrons App, inspect its generated deck map and commands, and confirm that the staged material identities match this reviewed task manifest."),
    ]);
    match &plan.execution {
        FlexTaskExecution::PipettingProgram {
            program,
            locations,
            sources,
            ..
        } => {
            doc.para_text(format!(
                "Run the {} canonical liquid operations in manifest order. The operation label is descriptive and does not select a lowering path.",
                program.steps.len()
            ));
            if !sources.is_empty() {
                doc.table(
                    [
                        Column::left("Material"),
                        Column::left("Physical source"),
                        Column::left("Allocated wells"),
                    ],
                    sources.iter().map(|source| {
                        vec![
                            vec![code(&source.binding.symbol)],
                            vec![code(material_source(&source.binding.source))],
                            vec![code(source.wells.join(", "))],
                        ]
                    }),
                );
            }
            doc.para_text(format!(
                "The reviewed manifest pins {} logical vessels to {} exact physical vessel mappings.",
                program.vessels.len(),
                locations.values().map(Vec::len).sum::<usize>()
            ));
        }
        FlexTaskExecution::ThermalProgram { reaction_wells, .. } => {
            doc.para_text(format!(
                "Confirm the canonical program samples are in wells {}. Remove and label them after the run.",
                reaction_wells.join(", ")
            ));
        }
    }
    doc
}

fn join_requirements<'a>(
    requirements: &'a [RequirementReview],
    value: impl Fn(&'a RequirementReview) -> &'a str,
) -> String {
    requirements
        .iter()
        .map(value)
        .collect::<Vec<_>>()
        .join(", ")
}

fn stage_builder(profile: &FlexAdapterProfile, protocol_name: &str) -> FlexProtocolBuilder {
    FlexProtocolBuilder::with_trash(
        Metadata {
            protocol_name: Some(protocol_name.to_owned()),
            author: Some("Lab Compiler".to_owned()),
            description: Some(
                "Generated from one exact facility-allocated Procedure task".to_owned(),
            ),
            ..Metadata::default()
        },
        profile.trash_area(),
    )
}

fn slot(name: &str) -> FlexSlot {
    FlexSlot::parse(name).expect("profile validation accepted only Flex slot names")
}

struct Instrument {
    id: PipetteId,
    flow_rate: f64,
    max_volume: f64,
}

fn load_instrument(
    builder: &mut FlexProtocolBuilder,
    pipette: &Pipette,
) -> Result<Instrument, String> {
    let name = FlexPipetteName::parse(&pipette.model)
        .expect("profile validation accepted only Flex pipette models");
    let mount = match pipette.mount.as_str() {
        "left" => PipetteMount::Left,
        "right" => PipetteMount::Right,
        _ => unreachable!("profile validation accepted only left and right mounts"),
    };
    let id = builder.load_pipette(name, mount).map_err(protocol_error)?;
    Ok(Instrument {
        id,
        flow_rate: name.default_flow_rate_ul_s(),
        max_volume: name.max_volume_ul(),
    })
}

fn load_plates(
    builder: &mut FlexProtocolBuilder,
    labware: &str,
    slots: &[String],
) -> Result<Vec<LabwareId>, String> {
    slots
        .iter()
        .map(|name| {
            builder
                .load_labware(labware, slot(name))
                .map_err(protocol_error)
        })
        .collect()
}

struct TipFeeder {
    racks: Vec<LabwareId>,
    wells: Vec<String>,
    tip_volume: f64,
    cursor: usize,
}

impl TipFeeder {
    fn load(builder: &mut FlexProtocolBuilder, racks: &TipRacks) -> Result<Self, String> {
        let loaded = load_plates(builder, &racks.labware, &racks.slots)?;
        let tip_volume = standard_definition(&racks.labware)
            .and_then(|definition| definition.well_volume_ul("A1"))
            .ok_or_else(|| format!("Flex tip rack '{}' has no known A1 volume", racks.labware))?;
        Ok(Self {
            racks: loaded,
            wells: plate_wells(racks.capacity),
            tip_volume,
            cursor: 0,
        })
    }

    fn next(&mut self) -> (LabwareId, String) {
        let rack = self.racks[self.cursor / self.wells.len()];
        let well = self.wells[self.cursor % self.wells.len()].clone();
        self.cursor += 1;
        (rack, well)
    }
}

fn working_volume_ul(pipette: &Pipette, tips: &TipRacks) -> Result<f64, String> {
    let name = FlexPipetteName::parse(&pipette.model)
        .expect("profile validation accepted only Flex pipette models");
    let tip_volume = standard_definition(&tips.labware)
        .and_then(|definition| definition.well_volume_ul("A1"))
        .ok_or_else(|| format!("Flex tip rack '{}' has no known A1 volume", tips.labware))?;
    Ok(name.max_volume_ul().min(tip_volume))
}

fn render(builder: FlexProtocolBuilder) -> Result<String, String> {
    builder
        .build()
        .to_json_pretty()
        .map_err(|error| error.to_string())
}

fn protocol_error(error: ProtocolError) -> String {
    error.to_string()
}

fn known_wells(
    _task: &AllocatedProcedureTask,
    _resource: &str,
    capacity: PlateCapacity,
) -> Result<Vec<String>, String> {
    Ok(plate_wells(capacity))
}

fn pretty_json(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_string_pretty(value)
        .map(|mut text| {
            text.push('\n');
            text
        })
        .map_err(|error| error.to_string())
}

fn material_source(source: &SelectedMaterialSource) -> String {
    match source {
        SelectedMaterialSource::MaterialLot { material_lot, .. } => material_lot.clone(),
        SelectedMaterialSource::ChoiceOutput { choice } => {
            format!("Method choice output {choice}")
        }
    }
}
