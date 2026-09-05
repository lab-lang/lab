//! Requirement-scoped lowering from exact facility allocations to reviewed STAR run documents.

use std::collections::{BTreeMap, BTreeSet};

use lab_compiler::allocation::{AllocatedProcedureTask, AllocatedRequirementBinding};
use lab_compiler::method::LocalId;
use lab_compiler::procedure::vocabulary::PIPETTING_PROGRAM_V1;
use lab_compiler::procedure::{
    AspirationStrategy, DispenseStrategy, FluidPathPolicy, MixTechnique, PipettingProgramV1,
    PipettingStep, ProcedureContractRegistry, ProcedureLocalId, TransferTechnique, VesselRole,
};
use lab_runfmt::STAR_RUN_FORMAT;
use serde::Serialize;

use crate::backend::adapters::{AdapterInvocationDocument, AdapterInvocationLowering};
use crate::backend::document::{Column, Doc, DocMeta, bold, code, text};
use crate::backend::hamilton::star::BACKEND;
use crate::backend::hamilton::star::emit::render_run;
use crate::backend::hamilton::star::liquid_classes::{
    LiquidClassEvidence, LiquidClassLibraryIdentity,
};
use crate::backend::hamilton::star::plan::{
    DeckIndex, FluidPathOperation, LiquidState, PLATE_DEAD_VOLUME_UL, RunBuilder, SourceFill,
    StarExecutionPlan, StarRunPlan, StarWell, TUBE_DEAD_VOLUME_UL, TipClass, TipFeeder, Transfer,
    execution_plan, seeded_liquids, source_fill, tip_usage,
};
use crate::backend::hamilton::star::profile::StarAdapterProfile;
use crate::backend::invocation::{ProcedureTaskView, exact_invocation_tasks};
use crate::backend::procedure::{canonical_pipetting_program, volume_microlitres};
use crate::backend::resources::plate_wells;
use crate::backend::typst;
use crate::{AdapterInvocation, AdapterInvocationPlan, ArtifactBundle, GeneratedArtifact};
use lab_compiler::planning::{
    PlanningProcedureParameter, PlanningTaskInput, PlanningTaskOutput, SelectedCapabilityParameter,
    SelectedMaterialBinding, SelectedMaterialSource,
};

const TASK_PLAN_SCHEMA: &str = "lab.hamilton-star-task.v3";

#[derive(Serialize)]
struct StarTaskPlan {
    schema_version: String,
    facility: String,
    asset: String,
    adapter: String,
    adapter_profile: String,
    adapter_profile_sha256: String,
    requirements: Vec<RequirementReview>,
    task: TaskReview,
    execution: StarTaskExecution,
    liquid_class_library: LiquidClassLibraryIdentity,
    liquid_classes: Vec<LiquidClassEvidence>,
    source_fills: Vec<SourceFill>,
    tip_usage: std::collections::BTreeMap<String, usize>,
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
enum StarTaskExecution {
    PipettingProgram {
        title: String,
        program: PipettingProgramV1,
        locations: BTreeMap<ProcedureLocalId, Vec<StarWell>>,
        sources: Vec<CanonicalSourceReview>,
    },
}

#[derive(Clone, Serialize)]
struct CanonicalSourceReview {
    vessel: ProcedureLocalId,
    material: Option<ProcedureLocalId>,
    binding: Option<SelectedMaterialBinding>,
    wells: Vec<StarWell>,
}

/// Lower only the Procedure tasks and requirements allocated to this exact STAR invocation.
pub(in crate::backend) fn lower_invocation(
    profile: &StarAdapterProfile,
    invocation_plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
    contracts: &ProcedureContractRegistry,
) -> Result<AdapterInvocationLowering, String> {
    let tasks = exact_invocation_tasks("STAR", invocation_plan, invocation)?;
    let mut artifacts = ArtifactBundle::new();
    let mut documents = Vec::new();

    for (ordinal, member) in tasks.into_iter().enumerate() {
        let (slug, execution, device_plan) =
            plan_task(profile, member.task, &member.requirements, contracts)?;
        let run = device_plan
            .runs
            .first()
            .ok_or_else(|| format!("STAR Procedure task '{}' produced no run", member.task.id))?;
        if device_plan.runs.len() != 1 {
            return Err(format!(
                "STAR Procedure task '{}' produced {} runs instead of one exact run",
                member.task.id,
                device_plan.runs.len()
            ));
        }
        let run_contents = render_run(&device_plan, run).map_err(|error| error.to_string())?;
        let directory = format!("tasks/{:03}-{slug}", ordinal + 1);
        let run_path = format!("{directory}/automation_run.json");
        let plan = StarTaskPlan {
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
            execution,
            liquid_class_library: device_plan.liquid_class_library.clone(),
            liquid_classes: device_plan.liquid_classes.clone(),
            source_fills: device_plan.source_fills.clone(),
            tip_usage: device_plan.tip_usage.clone(),
        };
        artifacts
            .insert_text(&run_path, "application/json", run_contents)
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
            path: run_path,
            format: STAR_RUN_FORMAT.to_owned(),
        });
    }

    Ok(AdapterInvocationLowering {
        artifacts,
        documents,
    })
}

fn plan_task(
    profile: &StarAdapterProfile,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
    contracts: &ProcedureContractRegistry,
) -> Result<
    (
        &'static str,
        StarTaskExecution,
        crate::backend::hamilton::star::plan::StarExecutionPlan,
    ),
    String,
> {
    if task
        .program
        .as_ref()
        .is_some_and(|program| program.contract.as_str() == PIPETTING_PROGRAM_V1)
    {
        return plan_pipetting_program(profile, task, requirements, contracts);
    }
    Err(format!(
        "STAR Procedure task '{}' does not carry the canonical pipetting contract",
        task.id
    ))
}

fn plan_pipetting_program(
    profile: &StarAdapterProfile,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
    contracts: &ProcedureContractRegistry,
) -> Result<(&'static str, StarTaskExecution, StarExecutionPlan), String> {
    let validated = canonical_pipetting_program("STAR", task, requirements, contracts)?;
    let program = validated.as_program();
    let source_wells = plate_wells(profile.resources.sources.capacity);
    let work_wells = plate_wells(profile.resources.work.capacity);
    let mut source_cursor = 0usize;
    let mut work_cursor = 0usize;
    let mut input_locations = BTreeMap::<u32, Vec<StarWell>>::new();
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
            .map_err(|_| format!("STAR task '{}' vessel position count overflows", task.id))?;
        if input_locations.contains_key(&input) {
            return Err(format!(
                "STAR Procedure task '{}' maps input {input} to more than one canonical vessel",
                task.id
            ));
        }
        let physical = take_star_locations(
            task,
            "reaction-plate input wells",
            &work_wells,
            &mut work_cursor,
            count,
            "work",
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
            .map_err(|_| format!("STAR task '{}' vessel position count overflows", task.id))?;
        let physical = match &vessel.role {
            VesselRole::MaterialSource { material } => {
                let binding = task
                    .materials
                    .iter()
                    .find(|binding| binding.input.as_str() == material.as_str())
                    .ok_or_else(|| {
                        format!(
                            "STAR Procedure task '{}' canonical material '{}' has no exact allocation",
                            task.id, material
                        )
                    })?
                    .clone();
                let physical = take_star_locations(
                    task,
                    "source-rack wells",
                    &source_wells,
                    &mut source_cursor,
                    count,
                    "sources",
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
                        "STAR Procedure task '{}' InputOutput vessel '{}' has no equally sized ProcedureInput {input}",
                        task.id, vessel.id
                    )
                })?,
            VesselRole::ProcedureInput { .. } => unreachable!(),
            VesselRole::Product { .. }
            | VesselRole::MaterialProduct { .. }
            | VesselRole::Intermediate => take_star_locations(
                task,
                "reaction-plate work wells",
                &work_wells,
                &mut work_cursor,
                count,
                "work",
            )?,
        };
        locations.insert(vessel.id.clone(), physical);
    }
    validate_star_canonical_steps(task, program)?;
    let device_plan = plan_star_canonical_program(profile, task, program, &locations, &sources)?;
    Ok((
        "pipetting-program",
        StarTaskExecution::PipettingProgram {
            title: operation_title(task),
            program: program.clone(),
            locations,
            sources,
        },
        device_plan,
    ))
}

fn take_star_locations(
    task: &AllocatedProcedureTask,
    resource: &str,
    wells: &[String],
    cursor: &mut usize,
    count: usize,
    physical_resource: &str,
) -> Result<Vec<StarWell>, String> {
    let end = cursor.checked_add(count).ok_or_else(|| {
        format!(
            "STAR Procedure task '{}' well allocation overflows",
            task.id
        )
    })?;
    if end > wells.len() {
        return Err(ProcedureTaskView::new("STAR", task).capacity_error(
            resource,
            end,
            wells.len(),
        ));
    }
    let result = wells[*cursor..end]
        .iter()
        .map(|well| StarWell::new(physical_resource, well))
        .collect();
    *cursor = end;
    Ok(result)
}

fn validate_star_canonical_steps(
    task: &AllocatedProcedureTask,
    program: &PipettingProgramV1,
) -> Result<(), String> {
    let mut closed_groups = BTreeSet::new();
    let mut open_group: Option<&ProcedureLocalId> = None;
    for step in &program.steps {
        let group = step_group(step);
        if group != open_group {
            if let Some(previous) = open_group {
                closed_groups.insert(previous.clone());
            }
            if group.is_some_and(|group| closed_groups.contains(group)) {
                return Err(format!(
                    "STAR Procedure task '{}' fluid-path group is not contiguous",
                    task.id
                ));
            }
            open_group = group;
        }
        match step {
            PipettingStep::Transfer { technique, .. }
            | PipettingStep::Distribute { technique, .. } => {
                validate_star_transfer_technique(task, technique)?;
            }
            PipettingStep::Mix { technique, .. } => {
                validate_star_mix_technique(task, technique)?;
            }
            PipettingStep::Barrier { .. } => {
                return Err(format!(
                    "STAR Procedure task '{}' contains a Barrier step, which this implementation does not claim",
                    task.id
                ));
            }
        }
    }
    Ok(())
}

fn validate_star_transfer_technique(
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
            "STAR Procedure task '{}' requests a transfer technique outside this implementation's declared canonical features",
            task.id
        ));
    }
    Ok(())
}

fn validate_star_mix_technique(
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
            "STAR Procedure task '{}' requests a mix technique outside this implementation's declared canonical features",
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

fn plan_star_canonical_program(
    profile: &StarAdapterProfile,
    task: &AllocatedProcedureTask,
    program: &PipettingProgramV1,
    locations: &BTreeMap<ProcedureLocalId, Vec<StarWell>>,
    sources: &[CanonicalSourceReview],
) -> Result<StarExecutionPlan, String> {
    profile.validate().map_err(|error| error.to_string())?;
    let deck = DeckIndex::build(profile).map_err(|error| error.to_string())?;
    let mut discovery = LiquidState::new();
    execute_star_canonical_steps(profile, task, program, locations, &deck, &mut discovery)?;

    let mut source_fills = Vec::new();
    for source in sources {
        let vessel = program
            .vessels
            .iter()
            .find(|vessel| vessel.id == source.vessel)
            .expect("source review comes from one canonical vessel");
        let stated = vessel
            .initial_volume_each
            .as_ref()
            .map(|volume| volume_microlitres("STAR", task, "initial", volume))
            .transpose()?;
        for (position, location) in source.wells.iter().enumerate() {
            let key = source
                .binding
                .as_ref()
                .map(|binding| binding.symbol.clone())
                .unwrap_or_else(|| format!("input-{}", source.vessel));
            let dead = if location.resource == "sources" {
                TUBE_DEAD_VOLUME_UL
            } else {
                PLATE_DEAD_VOLUME_UL
            };
            let mut fill = source_fill(
                &deck,
                &discovery,
                format!("{key}[{position}]"),
                location.clone(),
                dead,
            )
            .map_err(|error| error.to_string())?;
            if let Some(stated) = stated {
                fill.load_ul = fill.load_ul.max(stated + dead);
            }
            source_fills.push(fill);
        }
    }
    let mut liquids = seeded_liquids(&source_fills);
    let (operations, feeders, liquid_classes) =
        execute_star_canonical_steps(profile, task, program, locations, &deck, &mut liquids)?;
    let classes = profile
        .liquid_classes
        .resolve_selected()
        .map_err(|error| error.to_string())?;
    Ok(execution_plan(
        profile,
        source_fills,
        tip_usage(feeders),
        classes.identity().clone(),
        liquid_classes,
        StarRunPlan {
            id: "canonical_pipetting_program".to_owned(),
            title: operation_title(task),
            operations,
            manual_after: Vec::new(),
        },
    ))
}

fn execute_star_canonical_steps(
    profile: &StarAdapterProfile,
    task: &AllocatedProcedureTask,
    program: &PipettingProgramV1,
    locations: &BTreeMap<ProcedureLocalId, Vec<StarWell>>,
    deck: &DeckIndex,
    liquids: &mut LiquidState,
) -> Result<
    (
        Vec<crate::backend::hamilton::star::plan::StarOperation>,
        Vec<TipFeeder>,
        Vec<LiquidClassEvidence>,
    ),
    String,
> {
    let classes = profile
        .liquid_classes
        .resolve_selected()
        .map_err(|error| error.to_string())?;
    let mut builder = RunBuilder::new(
        deck,
        liquids,
        Some(TipFeeder::new(
            "small_tips",
            deck,
            profile.resources.small_tips.slots.len(),
            profile.resources.small_tips.capacity,
        )),
        Some(TipFeeder::new(
            "large_tips",
            deck,
            profile.resources.large_tips.slots.len(),
            profile.resources.large_tips.capacity,
        )),
        &classes,
        profile.run.lld,
    );
    let physical = |at: &lab_compiler::procedure::Location| -> Result<StarWell, String> {
        locations
            .get(&at.vessel)
            .and_then(|positions| positions.get(at.position as usize))
            .cloned()
            .ok_or_else(|| {
                format!(
                    "STAR canonical location '{}[{}]' was not allocated",
                    at.vessel, at.position
                )
            })
    };
    let mut cursor = 0usize;
    while cursor < program.steps.len() {
        if let Some(group) = step_group(&program.steps[cursor]) {
            let end = program.steps[cursor + 1..]
                .iter()
                .position(|step| step_group(step) != Some(group))
                .map_or(program.steps.len(), |offset| cursor + 1 + offset);
            let mut path = Vec::new();
            let mut maximum = 0.0_f64;
            for step in &program.steps[cursor..end] {
                match step {
                    PipettingStep::Transfer {
                        source,
                        destination,
                        volume,
                        ..
                    } => {
                        let volume = volume_microlitres("STAR", task, "transfer", volume)?;
                        maximum = maximum.max(volume);
                        path.push(FluidPathOperation::Transfer(Transfer::new(
                            physical(source)?,
                            physical(destination)?,
                            volume,
                        )));
                    }
                    PipettingStep::Mix {
                        targets,
                        cycles,
                        volume,
                        ..
                    } => {
                        let volume = volume_microlitres("STAR", task, "mix", volume)?;
                        maximum = maximum.max(volume);
                        for target in targets {
                            path.push(FluidPathOperation::Mix {
                                well: physical(target)?,
                                cycles: *cycles,
                                volume_ul: volume,
                            });
                        }
                    }
                    PipettingStep::Distribute { .. } => {
                        return Err(format!(
                            "STAR Procedure task '{}' groups a Distribute step into a continuous fluid path; express grouped paths as their ordered Transfer steps",
                            task.id
                        ));
                    }
                    PipettingStep::Barrier { .. } => unreachable!(),
                }
            }
            builder
                .fluid_path(star_tip_class(maximum), &path)
                .map_err(|error| error.to_string())?;
            cursor = end;
            continue;
        }
        match &program.steps[cursor] {
            PipettingStep::Transfer {
                source,
                destination,
                volume,
                ..
            } => {
                let volume = volume_microlitres("STAR", task, "transfer", volume)?;
                builder
                    .distribute(
                        star_tip_class(volume),
                        &[Transfer::new(
                            physical(source)?,
                            physical(destination)?,
                            volume,
                        )],
                    )
                    .map_err(|error| error.to_string())?;
            }
            PipettingStep::Distribute {
                source,
                destinations,
                volume_each,
                fluid_path,
                ..
            } => {
                let volume = volume_microlitres("STAR", task, "distribution", volume_each)?;
                let transfers = destinations
                    .iter()
                    .map(|destination| {
                        Ok(
                            Transfer::new(physical(source)?, physical(destination)?, volume)
                                .with_technique("distribution"),
                        )
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                if matches!(fluid_path, FluidPathPolicy::IsolatedDestinations) {
                    for transfer in transfers {
                        builder
                            .distribute(star_tip_class(volume), &[transfer])
                            .map_err(|error| error.to_string())?;
                    }
                } else {
                    builder
                        .distribute(star_tip_class(volume), &transfers)
                        .map_err(|error| error.to_string())?;
                }
            }
            PipettingStep::Mix {
                targets,
                cycles,
                volume,
                ..
            } => {
                let volume = volume_microlitres("STAR", task, "mix", volume)?;
                let wells = targets
                    .iter()
                    .map(&physical)
                    .collect::<Result<Vec<_>, _>>()?;
                builder
                    .mix_wells(star_tip_class(volume), &wells, (*cycles, volume))
                    .map_err(|error| error.to_string())?;
            }
            PipettingStep::Barrier { .. } => unreachable!(),
        }
        cursor += 1;
    }
    Ok(builder.finish())
}

fn star_tip_class(volume_ul: f64) -> TipClass {
    if volume_ul <= 40.0 {
        TipClass::Small
    } else {
        TipClass::Large
    }
}

/// Pure candidate-time check using the same structural dispatcher and profile limits as lowering.
pub(in crate::backend) fn check_task_feasibility(
    profile: &StarAdapterProfile,
    task: &AllocatedProcedureTask,
    contracts: &ProcedureContractRegistry,
) -> Result<(), String> {
    let requirements = task.requirements.iter().collect::<Vec<_>>();
    plan_task(profile, task, &requirements, contracts).map(|_| ())
}

fn render_manual(plan: &StarTaskPlan) -> Doc {
    let title = match &plan.execution {
        StarTaskExecution::PipettingProgram { title, .. } => title.clone(),
    };
    let mut doc = Doc::new(DocMeta::new(
        title,
        "Operator instructions for one facility-allocated Procedure task",
        &plan.adapter_profile,
        "Hamilton STAR/STARlet",
    ));
    doc.notice([
        bold("Allocation boundary. "),
        text("This reviewed STAR run atomically implements requirements "),
        code(join_requirements(&plan.requirements, |requirement| {
            requirement.id.as_str()
        })),
        text(" on the exact Asset below. Adjacent Procedure work remains in separate plan nodes."),
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
    doc.heading(1, [text("Source loading")]);
    doc.table(
        [
            Column::left("Source"),
            Column::left("STAR location"),
            Column::right("Load volume"),
        ],
        plan.source_fills.iter().map(|fill| {
            vec![
                vec![code(&fill.key)],
                vec![code(format!(
                    "{} {}",
                    fill.location.resource, fill.location.well
                ))],
                vec![text(format!("{:.1} µL", fill.load_ul))],
            ]
        }),
    );
    doc.heading(1, [text("Liquid classes")]);
    doc.para([
        text("Selected library "),
        code(&plan.liquid_class_library.id),
        text(" version "),
        code(&plan.liquid_class_library.version),
        text(" with content SHA-256 "),
        code(&plan.liquid_class_library.content_sha256),
        text("."),
    ]);
    doc.table(
        [
            Column::left("Stable class"),
            Column::left("Version"),
            Column::left("Content SHA-256"),
            Column::left("Calibration source"),
        ],
        plan.liquid_classes.iter().map(|class| {
            vec![
                vec![code(&class.identity.id)],
                vec![code(&class.identity.version)],
                vec![code(&class.identity.content_sha256)],
                vec![text(format!(
                    "{} ({})",
                    class.calibration.source, class.calibration.source_version
                ))],
            ]
        }),
    );
    doc.heading(1, [text("Run this task")]);
    doc.para([
        text("Review "),
        code("automation_run.json"),
        text(" and this exact task manifest together, stage only the listed sources, then execute the run through the STAR runtime."),
    ]);
    match &plan.execution {
        StarTaskExecution::PipettingProgram {
            program,
            locations,
            sources,
            ..
        } => {
            doc.para_text(format!(
                "Execute the {} canonical liquid operations in manifest order. The Procedure operation IRI is descriptive metadata and does not select a recipe-specific lowering path.",
                program.steps.len()
            ));
            if !sources.is_empty() {
                doc.table(
                    [
                        Column::left("Logical vessel"),
                        Column::left("Physical source"),
                        Column::left("STAR wells"),
                    ],
                    sources.iter().map(|source| {
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
                                    .map(|well| format!("{} {}", well.resource, well.well))
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            )],
                        ]
                    }),
                );
            }
            doc.para_text(format!(
                "The reviewed manifest pins {} logical vessels to {} exact physical positions.",
                program.vessels.len(),
                locations.values().map(Vec::len).sum::<usize>()
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use lab_capability::{
        AbsoluteIri, ControlMode, MethodId, OperationId, PropertyKind, PropertyValue,
        QualificationLevel, ScalarValue,
    };
    use lab_compiler::allocation::{AllocatedMethod, AllocatedProgram, InvocationAdapter};
    use lab_compiler::method::{IntentOperationId, PortType, ProcedureValue};
    use lab_compiler::planning::{
        PlanningProcedureParameter, PlanningTaskOutput, SelectedCapabilityParameter,
        SelectedMaterialBinding, SelectedMaterialSource,
    };
    use lab_compiler::procedure::{
        FluidPathPolicy, Location, MaterialInput, MaterialOutput, MixTechnique,
        PipettingConstraints, PipettingProgramV1, PipettingStep, ProcedureLocalId,
        ProcedureProgram, TransferTechnique, Vessel, VesselRole, Volume,
        builtin_procedure_contracts,
    };

    use super::*;
    use crate::backend::hamilton::star::liquid_classes::{
        LiquidClassLibraryReference, LiquidClassLibraryRegistry,
    };
    use crate::{
        ADAPTER_INVOCATIONS_SCHEMA_VERSION, AdapterInvocation, AdapterInvocationPlan,
        adapter_invocation_id, builtin_adapter_registry,
    };

    fn local(value: &str) -> LocalId {
        LocalId::new(value).unwrap()
    }

    fn procedure_local(value: &str) -> ProcedureLocalId {
        ProcedureLocalId::new(value).unwrap()
    }

    fn text_parameter(task: &LocalId, name: &str, value: &str) -> PlanningProcedureParameter {
        PlanningProcedureParameter {
            id: local(&format!("{task}::parameter::{name}")),
            property_kind: PropertyKind::new(format!("https://example.org/property/{name}"))
                .unwrap(),
            value: ProcedureValue::Scalar {
                value: PropertyValue::unitless(ScalarValue::Text(value.to_owned())),
            },
        }
    }

    #[test]
    fn public_registry_lowering_uses_the_profile_selected_liquid_class_library() {
        let mut contributed = LiquidClassLibraryRegistry::default()
            .libraries
            .into_iter()
            .next()
            .unwrap();
        contributed.id = "org.example.lab.star-liquid-classes".to_owned();
        contributed.version = "3.2.1".to_owned();
        contributed.classes.truncate(1);
        let contributed_class = &mut contributed.classes[0];
        contributed_class.id = "org.example.lab.star.aqueous-surface".to_owned();
        contributed_class.version = "7.4.0".to_owned();
        contributed_class.speeds.aspirate_ul_s = 17.0;
        contributed_class.calibration.source =
            "Example Lab gravimetric calibration campaign".to_owned();
        contributed_class.calibration.source_version = "campaign-2026-09".to_owned();

        let authored_profile = StarAdapterProfile {
            liquid_classes: LiquidClassLibraryRegistry {
                selected_library: LiquidClassLibraryReference {
                    id: contributed.id.clone(),
                    version: contributed.version.clone(),
                },
                libraries: vec![contributed],
            },
            ..StarAdapterProfile::default()
        };
        let selected_library = authored_profile
            .liquid_classes
            .resolve_selected()
            .expect("the contributed library validates");
        let expected_library = selected_library.identity().clone();
        let expected_class = selected_library.classes()[0].identity().clone();

        let registry = builtin_adapter_registry().unwrap();
        let profile_toml = toml::to_string_pretty(&authored_profile).unwrap();
        let profile = registry
            .validate_profile(
                "hamilton.star",
                "contributed-package-profile",
                &profile_toml,
            )
            .expect("the public adapter registry validates the contributed profile");
        let descriptor = registry.descriptors().descriptor("hamilton.star").unwrap();
        let implementation = descriptor
            .procedure_implementations
            .iter()
            .find(|implementation| implementation.services.lowering)
            .unwrap();
        let adapter = InvocationAdapter {
            driver: descriptor.id.clone(),
            profile_path: PathBuf::from("adapters/contributed-package-profile.toml"),
            profile_sha256: profile.sha256.clone(),
            features: descriptor.features.clone(),
            accepted_run_formats: implementation.accepted_run_formats.clone(),
            emitted_run_formats: implementation.emitted_run_formats.clone(),
        };

        let task_id = local("assemble::setup");
        let material_id = local("assemble::setup::material::water");
        let output_id = local("assemble::setup::product");
        let source_vessel = procedure_local("source-water");
        let product_vessel = procedure_local("reaction-plate");
        let destination = Location {
            vessel: product_vessel.clone(),
            position: 0,
        };
        let pipetting = PipettingProgramV1::new(
            vec![MaterialInput {
                id: procedure_local(material_id.as_str()),
            }],
            vec![MaterialOutput {
                id: procedure_local(output_id.as_str()),
            }],
            vec![
                Vessel {
                    id: source_vessel.clone(),
                    role: VesselRole::MaterialSource {
                        material: procedure_local(material_id.as_str()),
                    },
                    positions: 1,
                    initial_volume_each: None,
                    working_capacity_each: None,
                    dead_volume_each: None,
                    temperature: None,
                },
                Vessel {
                    id: product_vessel,
                    role: VesselRole::Product {
                        output: procedure_local(output_id.as_str()),
                    },
                    positions: 1,
                    initial_volume_each: None,
                    working_capacity_each: None,
                    dead_volume_each: None,
                    temperature: None,
                },
            ],
            vec![
                PipettingStep::Distribute {
                    id: procedure_local("add-water"),
                    source: Location {
                        vessel: source_vessel,
                        position: 0,
                    },
                    destinations: vec![destination.clone()],
                    volume_each: Volume::parse_microlitres("20").unwrap(),
                    fluid_path: FluidPathPolicy::SharedSourceNoReentry,
                    fluid_path_group: None,
                    technique: TransferTechnique::default(),
                },
                PipettingStep::Mix {
                    id: procedure_local("mix-reaction"),
                    targets: vec![destination],
                    cycles: 3,
                    volume: Volume::parse_microlitres("10").unwrap(),
                    fluid_path: FluidPathPolicy::IsolatedDestinations,
                    fluid_path_group: None,
                    technique: MixTechnique::default(),
                },
            ],
            PipettingConstraints::default(),
        )
        .validate()
        .expect("the test program is a valid canonical pipetting program");
        let formula = pipetting.capability_formula();
        let program = ProcedureProgram::from_pipetting(&pipetting);
        let asset = "https://example.org/facility/hamilton-star";
        let requirements = formula
            .all_of
            .into_iter()
            .map(|clause| {
                let role = clause.role.to_string();
                AllocatedRequirementBinding {
                    id: local(&format!("{task_id}::requirement::{role}")),
                    capability_kind: clause.capability_kind,
                    minimum_qualification: QualificationLevel::Executable,
                    accepted_control_modes: BTreeSet::from([ControlMode::ReviewedFile]),
                    offering: format!("https://example.org/offering/{role}"),
                    asset: asset.to_owned(),
                    observed_qualification: QualificationLevel::Executable.to_string(),
                    control_mode: ControlMode::ReviewedFile.to_string(),
                    parameters: clause
                        .constraints
                        .into_iter()
                        .enumerate()
                        .map(|(index, constraint)| SelectedCapabilityParameter {
                            property_kind: constraint.property_kind,
                            relation: constraint.relation,
                            required: constraint.required.clone(),
                            offering_parameter: format!(
                                "https://example.org/offering/{role}/parameter/{index}"
                            ),
                            observed: constraint.required,
                        })
                        .collect(),
                    procedure_implementation: Some(implementation.id.clone()),
                    adapter: Some(adapter.clone()),
                }
            })
            .collect::<Vec<_>>();
        let requirement_ids = requirements
            .iter()
            .map(|requirement| requirement.id.clone())
            .collect::<Vec<_>>();
        let operation = "https://example.org/operation/contributed-assembly";
        let allocated = AllocatedProgram {
            problem_sha256: "a".repeat(64),
            inventory_sha256: "b".repeat(64),
            facility: "https://example.org/facility".to_owned(),
            methods: vec![AllocatedMethod {
                choice: local("assemble"),
                source_operation: IntentOperationId::new(operation).unwrap(),
                source_intent: crate::test_source_intent(operation),
                method: MethodId::new("https://example.org/method/contributed-assembly").unwrap(),
                after: Vec::new(),
                inputs: Vec::new(),
                outputs: Vec::new(),
                yields: Vec::new(),
                tasks: vec![AllocatedProcedureTask {
                    id: task_id.clone(),
                    operation: OperationId::new(operation).unwrap(),
                    program: Some(program),
                    inputs: Vec::new(),
                    outputs: vec![PlanningTaskOutput {
                        name: output_id,
                        port_type: PortType::Material {
                            state: AbsoluteIri::new("https://example.org/material/assembly")
                                .unwrap(),
                        },
                    }],
                    parameters: vec![
                        text_parameter(&task_id, "artifact", "custom assembly"),
                        text_parameter(&task_id, "setup_strategy", "basic_v1"),
                    ],
                    materials: vec![SelectedMaterialBinding {
                        input: material_id,
                        symbol: "water".to_owned(),
                        source: SelectedMaterialSource::MaterialLot {
                            component: "https://example.org/component/water".to_owned(),
                            material_lot: "https://example.org/material-lot/water-001".to_owned(),
                        },
                        interchangeable_alternatives: Vec::new(),
                    }],
                    requirements,
                }],
            }],
        };
        let invocation = AdapterInvocation {
            id: adapter_invocation_id(asset, &adapter),
            asset: asset.to_owned(),
            adapter,
            tasks: vec![task_id],
            requirements: requirement_ids,
        };
        let plan = AdapterInvocationPlan {
            schema_version: ADAPTER_INVOCATIONS_SCHEMA_VERSION.to_owned(),
            allocated,
            allocated_lair_sha256: "c".repeat(64),
            invocations: vec![invocation.clone()],
        };

        let lowered = registry
            .lower_invocation(&profile, &plan, &invocation, builtin_procedure_contracts())
            .expect("the public registry lowers the exact contributed profile and invocation");
        let manifest = lowered
            .artifacts
            .get("tasks/001-pipetting-program/invocation_manifest.json")
            .unwrap()
            .text_contents()
            .unwrap();
        let manifest: serde_json::Value = serde_json::from_str(manifest).unwrap();
        assert_eq!(
            manifest["liquid_class_library"],
            serde_json::to_value(&expected_library).unwrap()
        );
        assert_eq!(
            manifest["liquid_classes"][0]["identity"],
            serde_json::to_value(&expected_class).unwrap()
        );
        assert_eq!(
            manifest["liquid_classes"][0]["speeds"]["aspirate_ul_s"],
            serde_json::json!(17.0)
        );
        assert_eq!(
            manifest["liquid_classes"][0]["calibration"]["source"],
            "Example Lab gravimetric calibration campaign"
        );

        let run = lowered
            .artifacts
            .get("tasks/001-pipetting-program/automation_run.json")
            .unwrap()
            .text_contents()
            .unwrap();
        assert!(run.contains(&expected_class.id));
        assert!(run.contains(&expected_class.version));
        assert!(run.contains(&expected_class.content_sha256));
    }
}
