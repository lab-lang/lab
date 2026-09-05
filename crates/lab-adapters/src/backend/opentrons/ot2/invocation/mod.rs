//! Requirement-scoped lowering from exact facility allocations to standalone OT-2 protocols.

use std::collections::{BTreeMap, BTreeSet};

use lab_compiler::allocation::{AllocatedProcedureTask, AllocatedRequirementBinding};
use lab_compiler::method::LocalId;
use lab_compiler::procedure::ProcedureContractRegistry;
use lab_compiler::procedure::vocabulary::{PIPETTING_PROGRAM_V1, THERMAL_PROGRAM_V1};
use lab_compiler::procedure::{
    FluidPathPolicy, PipettingProgramV1, PipettingStep, ProcedureLocalId, VesselRole,
};
use lab_instruments::ThermalProfile;
use lab_runfmt::OPENTRONS_PYTHON_PROTOCOL_FORMAT;
use serde::Serialize;

use crate::backend::adapters::{AdapterInvocationDocument, AdapterInvocationLowering};
use crate::backend::document::{Column, Doc, DocMeta, bold, code, text};
use crate::backend::invocation::{ProcedureTaskView, exact_invocation_tasks};
use crate::backend::opentrons::ot2::BACKEND;
use crate::backend::opentrons::ot2::profile::Ot2AdapterProfile;
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

const TASK_PLAN_SCHEMA: &str = "lab.opentrons-ot2-task.v1";

const CYCLE_TEMPLATE: &str = include_str!("thermal_cycle.py");
const PIPETTING_TEMPLATE: &str = include_str!("pipetting_program.py");
const API_LEVEL_SENTINEL: &str = "\"2.21\",  # LAB:API_LEVEL";
const PLAN_SENTINEL: &str = "\"{}\"  # LAB:INVOCATION_PLAN";

#[derive(Serialize)]
struct Ot2TaskPlan {
    schema_version: String,
    facility: String,
    asset: String,
    adapter: String,
    adapter_profile: String,
    adapter_profile_sha256: String,
    requirements: Vec<RequirementReview>,
    task: TaskReview,
    deck: Ot2AdapterProfile,
    execution: Ot2TaskExecution,
}

#[derive(Clone, Serialize)]
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

#[derive(Clone, Serialize)]
struct TaskReview {
    id: LocalId,
    operation: String,
    inputs: Vec<PlanningTaskInput>,
    outputs: Vec<PlanningTaskOutput>,
    parameters: Vec<PlanningProcedureParameter>,
    materials: Vec<SelectedMaterialBinding>,
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum Ot2TaskExecution {
    PipettingProgram(Box<CanonicalPipettingExecution>),
    ThermalProgram {
        title: String,
        sample_wells: Vec<String>,
        volume_each_ul: f64,
        lid_temperature_c: Option<f64>,
        profile: ThermalProfile,
        final_hold_celsius: Option<f64>,
    },
}

#[derive(Clone, Serialize)]
pub(super) struct CanonicalPipettingExecution {
    title: String,
    program: PipettingProgramV1,
    locations: BTreeMap<ProcedureLocalId, Vec<Ot2PhysicalLocation>>,
    sources: Vec<CanonicalSourceReview>,
}

#[derive(Clone, Serialize)]
struct Ot2PhysicalLocation {
    resource: Ot2PhysicalResource,
    well: String,
}

#[derive(Clone, Copy, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Ot2PhysicalResource {
    Sources,
    Work,
}

#[derive(Clone, Serialize)]
struct CanonicalSourceReview {
    vessel: ProcedureLocalId,
    material: Option<ProcedureLocalId>,
    binding: Option<SelectedMaterialBinding>,
    wells: Vec<Ot2PhysicalLocation>,
}

/// Lower only the Procedure tasks and requirements allocated to this exact invocation.
pub(in crate::backend) fn lower_invocation(
    profile: &Ot2AdapterProfile,
    invocation_plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
    contracts: &ProcedureContractRegistry,
) -> Result<AdapterInvocationLowering, String> {
    let tasks = exact_invocation_tasks("OT-2", invocation_plan, invocation)?;
    let mut artifacts = ArtifactBundle::new();
    let mut documents = Vec::new();

    for (ordinal, member) in tasks.into_iter().enumerate() {
        let (slug, execution) = plan_task(profile, member.task, &member.requirements, contracts)?;
        let directory = format!("tasks/{:03}-{slug}", ordinal + 1);
        let plan = Ot2TaskPlan {
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
        let protocol_path = format!("{directory}/automation_protocol.py");
        artifacts
            .insert_text(
                &protocol_path,
                "text/x-python",
                render_python_protocol(&plan)?,
            )
            .map_err(|error| error.to_string())?;

        let mut manifest =
            serde_json::to_string_pretty(&plan).map_err(|error| error.to_string())?;
        manifest.push('\n');
        artifacts
            .insert_text(
                format!("{directory}/invocation_manifest.json"),
                "application/json",
                manifest,
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
            format: OPENTRONS_PYTHON_PROTOCOL_FORMAT.to_owned(),
        });
    }

    Ok(AdapterInvocationLowering {
        artifacts,
        documents,
    })
}

pub(super) fn plan_task(
    profile: &Ot2AdapterProfile,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
    contracts: &ProcedureContractRegistry,
) -> Result<(&'static str, Ot2TaskExecution), String> {
    if task
        .program
        .as_ref()
        .is_some_and(|program| program.contract.as_str() == THERMAL_PROGRAM_V1)
    {
        return Ok((
            "thermal-program",
            plan_thermal(profile, task, requirements, contracts)?,
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
        "OT-2 invocation does not implement the canonical Procedure program shape in task '{}' (descriptive operation '{}')",
        task.id, task.operation
    ))
}

fn plan_pipetting_program(
    profile: &Ot2AdapterProfile,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
    contracts: &ProcedureContractRegistry,
) -> Result<Ot2TaskExecution, String> {
    let validated = canonical_pipetting_program("OT-2", task, requirements, contracts)?;
    let program = validated.as_program();
    let source_wells = plate_wells(profile.resources.sources.capacity);
    let work_wells = plate_wells(profile.resources.work.capacity);
    let mut source_cursor = 0usize;
    let mut work_cursor = 0usize;
    let mut input_locations = BTreeMap::<u32, Vec<Ot2PhysicalLocation>>::new();
    let mut locations = BTreeMap::new();
    let mut sources = Vec::new();

    for vessel in program
        .vessels
        .iter()
        .filter(|vessel| matches!(vessel.role, VesselRole::ProcedureInput { .. }))
    {
        let VesselRole::ProcedureInput { input } = vessel.role else {
            unreachable!()
        };
        let count = usize::try_from(vessel.positions)
            .map_err(|_| format!("OT-2 task '{}' vessel position count overflows", task.id))?;
        if input_locations.contains_key(&input) {
            return Err(format!(
                "OT-2 Procedure task '{}' maps input {input} to more than one canonical vessel",
                task.id
            ));
        }
        let physical = take_ot2_locations(
            task,
            "thermocycler input wells",
            &work_wells,
            &mut work_cursor,
            count,
            |well| Ot2PhysicalLocation {
                resource: Ot2PhysicalResource::Work,
                well,
            },
        )?;
        sources.push(CanonicalSourceReview {
            vessel: vessel.id.clone(),
            material: None,
            binding: None,
            wells: physical.clone(),
        });
        input_locations.insert(input, physical.clone());
        locations.insert(vessel.id.clone(), physical);
    }

    for vessel in &program.vessels {
        if locations.contains_key(&vessel.id) {
            continue;
        }
        let count = usize::try_from(vessel.positions)
            .map_err(|_| format!("OT-2 task '{}' vessel position count overflows", task.id))?;
        let physical = match &vessel.role {
            VesselRole::MaterialSource { material } => {
                let binding = material_binding("OT-2", task, material)?.clone();
                let physical = take_ot2_locations(
                    task,
                    "temperature-module source wells",
                    &source_wells,
                    &mut source_cursor,
                    count,
                    |well| Ot2PhysicalLocation {
                        resource: Ot2PhysicalResource::Sources,
                        well,
                    },
                )?;
                sources.push(CanonicalSourceReview {
                    vessel: vessel.id.clone(),
                    material: Some(material.clone()),
                    binding: Some(binding),
                    wells: physical.clone(),
                });
                physical
            }
            VesselRole::InputOutput { input, .. } => input_locations
                .get(input)
                .filter(|locations| locations.len() == count)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "OT-2 Procedure task '{}' InputOutput vessel '{}' has no equally sized ProcedureInput {input}",
                        task.id, vessel.id
                    )
                })?,
            VesselRole::ProcedureInput { .. } => unreachable!(),
            VesselRole::Product { .. }
            | VesselRole::MaterialProduct { .. }
            | VesselRole::Intermediate => take_ot2_locations(
                task,
                "thermocycler work wells",
                &work_wells,
                &mut work_cursor,
                count,
                |well| Ot2PhysicalLocation {
                    resource: Ot2PhysicalResource::Work,
                    well,
                },
            )?,
        };
        locations.insert(vessel.id.clone(), physical);
    }
    validate_ot2_canonical_steps(profile, task, program)?;
    Ok(Ot2TaskExecution::PipettingProgram(Box::new(
        CanonicalPipettingExecution {
            title: operation_title(task),
            program: program.clone(),
            locations,
            sources,
        },
    )))
}

fn material_binding<'a>(
    adapter: &str,
    task: &'a AllocatedProcedureTask,
    material: &ProcedureLocalId,
) -> Result<&'a SelectedMaterialBinding, String> {
    task.materials
        .iter()
        .find(|binding| binding.input.as_str() == material.as_str())
        .ok_or_else(|| {
            format!(
                "{adapter} Procedure task '{}' canonical material '{}' has no exact allocation",
                task.id, material
            )
        })
}

fn take_ot2_locations(
    task: &AllocatedProcedureTask,
    resource: &str,
    wells: &[String],
    cursor: &mut usize,
    count: usize,
    physical: impl Fn(String) -> Ot2PhysicalLocation,
) -> Result<Vec<Ot2PhysicalLocation>, String> {
    let end = cursor.checked_add(count).ok_or_else(|| {
        format!(
            "OT-2 Procedure task '{}' well allocation overflows",
            task.id
        )
    })?;
    if end > wells.len() {
        return Err(ProcedureTaskView::new("OT-2", task).capacity_error(
            resource,
            end,
            wells.len(),
        ));
    }
    let result = wells[*cursor..end].iter().cloned().map(physical).collect();
    *cursor = end;
    Ok(result)
}

fn validate_ot2_canonical_steps(
    profile: &Ot2AdapterProfile,
    task: &AllocatedProcedureTask,
    program: &PipettingProgramV1,
) -> Result<(), String> {
    let small_capacity = 20.0;
    let large_capacity = 200.0;
    let mut small_tips = 0usize;
    let mut large_tips = 0usize;
    let mut open_group: Option<&ProcedureLocalId> = None;
    let mut group_maximum = 0.0_f64;
    let mut closed_groups = BTreeSet::new();
    for step in &program.steps {
        let group = ot2_step_group(step);
        if group != open_group {
            if let Some(previous) = open_group {
                closed_groups.insert(previous.clone());
                count_ot2_group_tip(
                    task,
                    group_maximum,
                    small_capacity,
                    large_capacity,
                    &mut small_tips,
                    &mut large_tips,
                )?;
                group_maximum = 0.0;
            }
            if group.is_some_and(|group| closed_groups.contains(group)) {
                return Err(format!(
                    "OT-2 Procedure task '{}' fluid-path group is not contiguous",
                    task.id
                ));
            }
            open_group = group;
        }
        let (volume, multiplicity) = match step {
            PipettingStep::Transfer {
                volume, technique, ..
            } => (
                volume_microlitres("OT-2", task, "transfer", volume)?
                    + technique
                        .air_gap
                        .as_ref()
                        .map(|volume| volume_microlitres("OT-2", task, "air gap", volume))
                        .transpose()?
                        .unwrap_or(0.0),
                1,
            ),
            PipettingStep::Distribute {
                destinations,
                volume_each,
                fluid_path,
                technique,
                ..
            } => {
                let each = volume_microlitres("OT-2", task, "distribution", volume_each)?;
                let air = technique
                    .air_gap
                    .as_ref()
                    .map(|volume| volume_microlitres("OT-2", task, "air gap", volume))
                    .transpose()?
                    .unwrap_or(0.0);
                if group.is_some()
                    && matches!(fluid_path, FluidPathPolicy::IsolatedDestinations)
                    && destinations.len() > 1
                {
                    return Err(format!(
                        "OT-2 Procedure task '{}' cannot combine a shared fluid-path group with an isolated multi-destination distribution",
                        task.id
                    ));
                }
                let path_volume = each + air;
                let working = if path_volume <= small_capacity {
                    small_capacity
                } else if path_volume <= large_capacity {
                    large_capacity
                } else {
                    return Err(format!(
                        "OT-2 Procedure task '{}' distribution requires {path_volume} uL, above the configured {large_capacity} uL tip capacity",
                        task.id
                    ));
                };
                let tips = match fluid_path {
                    FluidPathPolicy::IsolatedDestinations => destinations.len(),
                    FluidPathPolicy::SharedSourceNoReentry => {
                        ((each * destinations.len() as f64) / (working - air))
                            .ceil()
                            .max(1.0) as usize
                    }
                };
                (path_volume, tips)
            }
            PipettingStep::Mix {
                targets,
                volume,
                fluid_path,
                ..
            } => {
                if group.is_some()
                    && matches!(fluid_path, FluidPathPolicy::IsolatedDestinations)
                    && targets.len() > 1
                {
                    return Err(format!(
                        "OT-2 Procedure task '{}' cannot combine a shared fluid-path group with isolated multi-target mixing",
                        task.id
                    ));
                }
                (
                    volume_microlitres("OT-2", task, "mix", volume)?,
                    targets.len(),
                )
            }
            PipettingStep::Barrier { .. } => (0.0, 0),
        };
        if group.is_some() {
            group_maximum = group_maximum.max(volume);
        } else if volume <= small_capacity {
            small_tips = small_tips
                .checked_add(multiplicity)
                .ok_or_else(|| format!("OT-2 Procedure task '{}' tip count overflows", task.id))?;
        } else if volume <= large_capacity {
            large_tips = large_tips
                .checked_add(multiplicity)
                .ok_or_else(|| format!("OT-2 Procedure task '{}' tip count overflows", task.id))?;
        } else {
            return Err(format!(
                "OT-2 Procedure task '{}' operation requires {volume} uL, above the configured 200 uL tip capacity",
                task.id
            ));
        }
    }
    if open_group.is_some() {
        count_ot2_group_tip(
            task,
            group_maximum,
            small_capacity,
            large_capacity,
            &mut small_tips,
            &mut large_tips,
        )?;
    }
    let small_capacity = profile.resources.small_tips.total_capacity();
    if small_tips > small_capacity {
        return Err(ProcedureTaskView::new("OT-2", task).capacity_error(
            "small tips",
            small_tips,
            small_capacity,
        ));
    }
    let large_capacity = profile.resources.large_tips.total_capacity();
    if large_tips > large_capacity {
        return Err(ProcedureTaskView::new("OT-2", task).capacity_error(
            "large tips",
            large_tips,
            large_capacity,
        ));
    }
    Ok(())
}

fn count_ot2_group_tip(
    task: &AllocatedProcedureTask,
    maximum: f64,
    small_capacity: f64,
    large_capacity: f64,
    small_tips: &mut usize,
    large_tips: &mut usize,
) -> Result<(), String> {
    if maximum == 0.0 {
        return Ok(());
    }
    if maximum <= small_capacity {
        *small_tips = small_tips
            .checked_add(1)
            .ok_or_else(|| format!("OT-2 Procedure task '{}' tip count overflows", task.id))?;
    } else if maximum <= large_capacity {
        *large_tips = large_tips
            .checked_add(1)
            .ok_or_else(|| format!("OT-2 Procedure task '{}' tip count overflows", task.id))?;
    } else {
        return Err(format!(
            "OT-2 Procedure task '{}' fluid path requires {maximum} uL, above the configured {large_capacity} uL tip capacity",
            task.id
        ));
    }
    Ok(())
}

fn ot2_step_group(step: &PipettingStep) -> Option<&ProcedureLocalId> {
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
    profile: &Ot2AdapterProfile,
    task: &AllocatedProcedureTask,
    contracts: &ProcedureContractRegistry,
) -> Result<(), String> {
    let requirements = task.requirements.iter().collect::<Vec<_>>();
    plan_task(profile, task, &requirements, contracts).map(|_| ())
}

fn plan_thermal(
    profile: &Ot2AdapterProfile,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
    contracts: &ProcedureContractRegistry,
) -> Result<Ot2TaskExecution, String> {
    let procedure = normalized_thermal_program("OT-2", task, requirements, contracts)?;
    let view = ProcedureTaskView::new("OT-2", task);
    let sample_wells = known_wells(task, "sample plate", profile.resources.work.capacity)?;
    if procedure.sample_count > sample_wells.len() {
        return Err(view.capacity_error(
            "sample plate",
            procedure.sample_count,
            sample_wells.len(),
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
                "OT-2 Procedure task '{}' {name} is {value} °C, above the adapter's {maximum} °C limit",
                task.id,
            ));
        }
    }
    if !(10.0..=100.0).contains(&procedure.volume_each_ul) {
        return Err(format!(
            "OT-2 Procedure task '{}' sample volume {} µL is outside the Thermocycler Module's 10–100 µL working range",
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
            "OT-2 Procedure task '{}' requests explicit ramp control, which this implementation does not claim",
            task.id
        ));
    }

    Ok(Ot2TaskExecution::ThermalProgram {
        title: procedure.title,
        sample_wells: sample_wells
            .into_iter()
            .take(procedure.sample_count)
            .collect(),
        volume_each_ul: procedure.volume_each_ul,
        lid_temperature_c: procedure.lid_temperature_c,
        profile: procedure.profile,
        final_hold_celsius: procedure.final_hold_celsius,
    })
}

fn render_python_protocol(plan: &Ot2TaskPlan) -> Result<String, String> {
    let template = match &plan.execution {
        Ot2TaskExecution::PipettingProgram(_) => PIPETTING_TEMPLATE,
        Ot2TaskExecution::ThermalProgram { .. } => CYCLE_TEMPLATE,
    };
    render_embedded_python_protocol(template, &plan.deck, plan)
}

fn render_embedded_python_protocol<T: Serialize>(
    template: &str,
    profile: &Ot2AdapterProfile,
    plan: &T,
) -> Result<String, String> {
    let api_level =
        serde_json::to_string(&profile.protocol.api_level).map_err(|error| error.to_string())?;
    let output = replace_once(
        template,
        API_LEVEL_SENTINEL,
        &format!("{api_level},  # LAB:API_LEVEL"),
    )?;
    let plan_json = serde_json::to_string(plan).map_err(|error| error.to_string())?;
    let plan_literal = python_string_expression(&plan_json)?;
    replace_once(
        &output,
        PLAN_SENTINEL,
        &format!("{plan_literal}  # LAB:INVOCATION_PLAN"),
    )
}

fn python_string_expression(value: &str) -> Result<String, String> {
    const MAX_LITERAL_WIDTH: usize = 88;

    let mut chunks = Vec::new();
    let mut current = String::new();
    for character in value.chars() {
        let mut candidate = current.clone();
        candidate.push(character);
        let encoded = serde_json::to_string(&candidate).map_err(|error| error.to_string())?;
        if encoded.len() > MAX_LITERAL_WIDTH && !current.is_empty() {
            chunks.push(current);
            current = character.to_string();
        } else {
            current = candidate;
        }
    }
    chunks.push(current);

    let literals = chunks
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(format!("(\n    {}\n)", literals.join("\n    ")))
}

fn replace_once(source: &str, needle: &str, replacement: &str) -> Result<String, String> {
    match source.matches(needle).count() {
        1 => Ok(source.replacen(needle, replacement, 1)),
        count => Err(format!(
            "OT-2 invocation template expected one {needle:?} marker, found {count}"
        )),
    }
}

fn render_manual(plan: &Ot2TaskPlan) -> Doc {
    let title = match &plan.execution {
        Ot2TaskExecution::PipettingProgram(execution) => execution.title.clone(),
        Ot2TaskExecution::ThermalProgram { title, .. } => title.clone(),
    };
    let mut doc = Doc::new(DocMeta::new(
        title,
        "Operator instructions for one facility-allocated Procedure task",
        &plan.adapter_profile,
        "Opentrons OT-2",
    ));
    doc.notice([
        bold("Allocation boundary. "),
        text("This protocol atomically implements requirements "),
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

    match &plan.execution {
        Ot2TaskExecution::PipettingProgram(execution) => {
            doc.heading(1, [text("Stage canonical vessel bindings")]);
            doc.para_text(format!(
                "Execute all {} canonical PipettingProgramV1 steps in manifest order. The descriptive Procedure operation does not select a lowering path.",
                execution.program.steps.len()
            ));
            if !execution.sources.is_empty() {
                doc.table(
                    [
                        Column::left("Logical vessel"),
                        Column::left("Physical source"),
                        Column::left("Allocated wells"),
                    ],
                    execution.sources.iter().map(|source| {
                        vec![
                            vec![code(source.vessel.as_str())],
                            vec![code(source.binding.as_ref().map_or_else(
                                || "Procedure input".to_owned(),
                                |binding| material_source(&binding.source),
                            ))],
                            vec![code(
                                source
                                    .wells
                                    .iter()
                                    .map(|location| location.well.clone())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            )],
                        ]
                    }),
                );
            }
            doc.para_text(format!(
                "The manifest pins {} logical vessels to {} physical positions.",
                execution.program.vessels.len(),
                execution.locations.values().map(Vec::len).sum::<usize>()
            ));
            doc.para([
                text("Import "),
                code("automation_protocol.py"),
                text(" and review its embedded canonical program and exact logical-to-physical mapping before execution."),
            ]);
        }
        Ot2TaskExecution::ThermalProgram {
            sample_wells,
            volume_each_ul,
            lid_temperature_c,
            profile,
            final_hold_celsius,
            ..
        } => {
            doc.heading(1, [text("Stage canonical thermal-program samples")]);
            doc.para([
                text("Place the samples in thermocycler wells "),
                code(sample_wells.join(", ")),
                text(" of the configured plate."),
            ]);
            doc.heading(1, [text("Review the thermal program")]);
            let mut rows = Vec::new();
            if let Some(lid) = lid_temperature_c {
                rows.push(vec![
                    vec![text("Heated lid")],
                    vec![text(format!("{lid} °C"))],
                    vec![text("throughout program")],
                ]);
            }
            for (stage_index, stage) in profile.stages.iter().enumerate() {
                for (step_index, step) in stage.steps.iter().enumerate() {
                    rows.push(vec![
                        vec![text(format!(
                            "Stage {} step {}",
                            stage_index + 1,
                            step_index + 1
                        ))],
                        vec![text(format!("{} °C", step.celsius))],
                        vec![text(format!("{} s × {}", step.hold_seconds, stage.repeats))],
                    ]);
                }
            }
            if let Some(hold) = final_hold_celsius {
                rows.push(vec![
                    vec![text("Final hold")],
                    vec![text(format!("{hold} °C"))],
                    vec![text("until recovery")],
                ]);
            }
            doc.table(
                [
                    Column::left("Step"),
                    Column::right("Temperature"),
                    Column::right("Duration / repeats"),
                ],
                rows,
            );
            doc.para([
                text("Import "),
                code("automation_protocol.py"),
                text(format!(
                    " and verify a {volume_each_ul} µL block volume before starting."
                )),
            ]);
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

fn material_source(source: &SelectedMaterialSource) -> String {
    match source {
        SelectedMaterialSource::MaterialLot { material_lot, .. } => material_lot.clone(),
        SelectedMaterialSource::ChoiceOutput { choice } => {
            format!("Method choice output {choice}")
        }
    }
}

fn known_wells(
    _task: &AllocatedProcedureTask,
    _resource: &str,
    capacity: PlateCapacity,
) -> Result<Vec<String>, String> {
    Ok(plate_wells(capacity))
}
