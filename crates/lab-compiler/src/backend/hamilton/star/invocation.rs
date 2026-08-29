//! Requirement-scoped lowering from exact facility allocations to reviewed STAR run documents.

use std::collections::BTreeSet;

use lab_method::LocalId;
use lab_runfmt::STAR_RUN_FORMAT;
use serde::Serialize;

use crate::backend::adapters::{AdapterInvocationDocument, AdapterInvocationLowering};
use crate::backend::document::{Column, Doc, DocMeta, bold, code, text};
use crate::backend::hamilton::star::BACKEND;
use crate::backend::hamilton::star::emit::render_run;
use crate::backend::hamilton::star::plan::{
    SetupAddition, SourceFill, plan_dilution_invocation, plan_setup_invocation,
};
use crate::backend::hamilton::star::profile::StarAdapterProfile;
use crate::backend::invocation::{ProcedureTaskView, exact_invocation_tasks};
use crate::backend::procedure::{
    SERIAL_DILUTION, SETUP_GOLDEN_GATE, serial_dilution, setup_golden_gate,
};
use crate::backend::resources::{PlateAllocator, Well, assign_source_wells, plate_wells};
use crate::backend::typst;
use crate::planning::{
    AdapterInvocation, AdapterInvocationPlan, AllocatedProcedureTask, AllocatedRequirementBinding,
    PlanningProcedureParameter, PlanningTaskInput, PlanningTaskOutput, PlanningValueSource,
    SelectedCapabilityParameter, SelectedMaterialBinding, SelectedMaterialSource,
};
use crate::{ArtifactBundle, GeneratedArtifact};

const TASK_PLAN_SCHEMA: &str = "lab.hamilton-star-task.v1";

#[derive(Serialize)]
struct StarTaskPlan {
    schema_version: String,
    facility: String,
    asset: String,
    adapter: String,
    adapter_profile: String,
    adapter_profile_sha256: String,
    requirement: RequirementReview,
    task: TaskReview,
    execution: StarTaskExecution,
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
    SetupGoldenGateReaction {
        artifact: String,
        reaction_wells: Vec<String>,
        additions: Vec<MaterialAddition>,
        reaction_volume_ul: u32,
        mix_cycles: u32,
        mix_volume_ul: u32,
    },
    SerialDilution {
        culture_source: PlanningValueSource,
        culture_well: String,
        medium: MaterialPlacement,
        dilution_wells: Vec<Well>,
        medium_volume_ul: u32,
        culture_volume_ul: u32,
        mix_cycles: u32,
        mix_volume_ul: u32,
    },
}

#[derive(Clone, Serialize)]
struct MaterialPlacement {
    role: String,
    input: LocalId,
    symbol: String,
    source: SelectedMaterialSource,
    source_well: String,
}

#[derive(Serialize)]
struct MaterialAddition {
    #[serde(flatten)]
    placement: MaterialPlacement,
    volume_ul: u32,
}

/// Lower only the Procedure tasks and requirements allocated to this exact STAR invocation.
pub(in crate::backend) fn lower_invocation(
    profile: &StarAdapterProfile,
    invocation_plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
) -> Result<AdapterInvocationLowering, String> {
    let tasks = exact_invocation_tasks("STAR", invocation_plan, invocation)?;
    let mut artifacts = ArtifactBundle::new();
    let mut documents = Vec::new();

    for (ordinal, member) in tasks.into_iter().enumerate() {
        let (slug, execution, device_plan) = plan_task(profile, member.task, member.requirement)?;
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
            facility: invocation_plan.facility.clone(),
            asset: invocation.asset.clone(),
            adapter: BACKEND.to_owned(),
            adapter_profile: profile.name.clone(),
            adapter_profile_sha256: invocation.adapter.profile_sha256.clone(),
            requirement: RequirementReview {
                id: member.requirement.id.clone(),
                capability_kind: member.requirement.capability_kind.to_string(),
                offering: member.requirement.offering.clone(),
                observed_qualification: member.requirement.observed_qualification.clone(),
                control_mode: member.requirement.control_mode.clone(),
                parameters: member.requirement.parameters.clone(),
            },
            task: TaskReview {
                id: member.task.id.clone(),
                operation: member.task.operation.to_string(),
                inputs: member.task.inputs.clone(),
                outputs: member.task.outputs.clone(),
                parameters: member.task.parameters.clone(),
                materials: member.task.materials.clone(),
            },
            execution,
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
            requirement: member.requirement.id.clone(),
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
    requirement: &AllocatedRequirementBinding,
) -> Result<
    (
        &'static str,
        StarTaskExecution,
        crate::backend::hamilton::star::plan::StarExecutionPlan,
    ),
    String,
> {
    match task.operation.as_str() {
        SETUP_GOLDEN_GATE => plan_setup(profile, task, requirement),
        SERIAL_DILUTION => plan_dilution(profile, task, requirement),
        operation => Err(format!(
            "STAR invocation contains unsupported Procedure operation '{operation}' in task '{}'; the STAR adapter implements liquid-handling tasks only",
            task.id
        )),
    }
}

fn plan_setup(
    profile: &StarAdapterProfile,
    task: &AllocatedProcedureTask,
    requirement: &AllocatedRequirementBinding,
) -> Result<
    (
        &'static str,
        StarTaskExecution,
        crate::backend::hamilton::star::plan::StarExecutionPlan,
    ),
    String,
> {
    let procedure = setup_golden_gate("STAR", task, requirement)?;
    let view = ProcedureTaskView::new("STAR", task);
    let source_keys = procedure
        .additions
        .iter()
        .map(|addition| addition.material.symbol.clone())
        .collect::<BTreeSet<_>>();
    let source_wells = assign_source_wells(
        BACKEND,
        "setup-golden-gate-reaction",
        source_keys,
        profile.deck.source_rack.capacity,
    )
    .map_err(|error| error.to_string())?;
    let known_reaction_wells = plate_wells(profile.deck.reaction_plate.capacity);
    if known_reaction_wells.is_empty() || procedure.replicates > known_reaction_wells.len() {
        return Err(view.capacity_error(
            "reaction plate",
            procedure.replicates,
            known_reaction_wells.len(),
        ));
    }
    let reaction_wells = known_reaction_wells
        .into_iter()
        .take(procedure.replicates)
        .collect::<Vec<_>>();
    let additions = procedure
        .additions
        .iter()
        .map(|addition| MaterialAddition {
            placement: MaterialPlacement {
                role: addition.role.to_owned(),
                input: addition.material.input.clone(),
                symbol: addition.material.symbol.clone(),
                source: addition.material.source.clone(),
                source_well: source_wells[&addition.material.symbol].clone(),
            },
            volume_ul: addition.volume_ul,
        })
        .collect::<Vec<_>>();
    let device_additions = procedure
        .additions
        .iter()
        .map(|addition| SetupAddition {
            symbol: addition.material.symbol.clone(),
            volume_ul: f64::from(addition.volume_ul),
        })
        .collect::<Vec<_>>();
    let device_plan = plan_setup_invocation(
        profile,
        source_wells,
        reaction_wells.clone(),
        &device_additions,
        (procedure.mix_cycles, f64::from(procedure.mix_volume_ul)),
    )
    .map_err(|error| error.to_string())?;
    Ok((
        "setup-golden-gate-reaction",
        StarTaskExecution::SetupGoldenGateReaction {
            artifact: procedure.artifact,
            reaction_wells,
            additions,
            reaction_volume_ul: procedure.reaction_volume_ul,
            mix_cycles: procedure.mix_cycles,
            mix_volume_ul: procedure.mix_volume_ul,
        },
        device_plan,
    ))
}

fn plan_dilution(
    profile: &StarAdapterProfile,
    task: &AllocatedProcedureTask,
    requirement: &AllocatedRequirementBinding,
) -> Result<
    (
        &'static str,
        StarTaskExecution,
        crate::backend::hamilton::star::plan::StarExecutionPlan,
    ),
    String,
> {
    let procedure = serial_dilution("STAR", task, requirement)?;
    let view = ProcedureTaskView::new("STAR", task);
    let mut allocator = PlateAllocator::new(
        BACKEND,
        "serial-dilution",
        "dilution_plate",
        &profile.stages.plating.dilution_plate,
    );
    let dilution_wells = allocator
        .take(procedure.serial_dilutions)
        .map_err(|error| error.to_string())?;
    let culture_wells = plate_wells(profile.deck.reaction_plate.capacity);
    if culture_wells.is_empty() {
        return Err(view.capacity_error("culture staging plate", 1, 0));
    }
    let culture_well = culture_wells[0].clone();
    let device_plan = plan_dilution_invocation(
        profile,
        culture_well.clone(),
        dilution_wells.clone(),
        f64::from(procedure.medium_volume_ul),
        f64::from(procedure.culture_volume_ul),
        (procedure.mix_cycles, f64::from(procedure.mix_volume_ul)),
    )
    .map_err(|error| error.to_string())?;
    Ok((
        "serial-dilution",
        StarTaskExecution::SerialDilution {
            culture_source: procedure.culture_source.clone(),
            culture_well,
            medium: MaterialPlacement {
                role: "medium".to_owned(),
                input: procedure.medium.input.clone(),
                symbol: procedure.medium.symbol.clone(),
                source: procedure.medium.source.clone(),
                source_well: profile.stages.plating.media_rack.medium_well.clone(),
            },
            dilution_wells,
            medium_volume_ul: procedure.medium_volume_ul,
            culture_volume_ul: procedure.culture_volume_ul,
            mix_cycles: procedure.mix_cycles,
            mix_volume_ul: procedure.mix_volume_ul,
        },
        device_plan,
    ))
}

fn render_manual(plan: &StarTaskPlan) -> Doc {
    let title = match &plan.execution {
        StarTaskExecution::SetupGoldenGateReaction { artifact, .. } => {
            format!("Set up Golden Gate reaction for {artifact}")
        }
        StarTaskExecution::SerialDilution { .. } => "Serially dilute recovered culture".to_owned(),
    };
    let mut doc = Doc::new(DocMeta::new(
        title,
        "Operator instructions for one facility-allocated Procedure requirement",
        &plan.adapter_profile,
        "Hamilton STAR/STARlet",
    ));
    doc.notice([
        bold("Allocation boundary. "),
        text("This reviewed STAR run implements only requirement "),
        code(plan.requirement.id.as_str()),
        text(" on the exact Asset below. Adjacent Procedure work remains in separate plan nodes."),
    ]);
    doc.heading(1, [text("Reviewed allocation")]);
    doc.table(
        [Column::left("Field"), Column::left("Exact value")],
        [
            vec![vec![text("Asset")], vec![code(&plan.asset)]],
            vec![
                vec![text("Capability offering")],
                vec![code(&plan.requirement.offering)],
            ],
            vec![
                vec![text("Capability kind")],
                vec![code(&plan.requirement.capability_kind)],
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
    doc.heading(1, [text("Run this task")]);
    doc.para([
        text("Review "),
        code("automation_run.json"),
        text(" and this exact task manifest together, stage only the listed sources, then execute the run through the STAR runtime."),
    ]);
    match &plan.execution {
        StarTaskExecution::SetupGoldenGateReaction {
            reaction_wells,
            additions,
            ..
        } => {
            doc.para_text(format!(
                "Preserve reaction wells {} for the separately allocated thermal-cycling task.",
                reaction_wells.join(", ")
            ));
            doc.table(
                [
                    Column::left("Role"),
                    Column::left("Material"),
                    Column::left("Physical source"),
                    Column::left("Well"),
                    Column::right("Volume / reaction"),
                ],
                additions.iter().map(|addition| {
                    vec![
                        vec![text(&addition.placement.role)],
                        vec![code(&addition.placement.symbol)],
                        vec![code(material_source(&addition.placement.source))],
                        vec![code(&addition.placement.source_well)],
                        vec![text(format!("{} µL", addition.volume_ul))],
                    ]
                }),
            );
        }
        StarTaskExecution::SerialDilution {
            culture_source,
            culture_well,
            medium,
            dilution_wells,
            ..
        } => {
            doc.para_text(format!(
                "Stage {} in reaction-plate well {culture_well}; load {} from {} in media-rack well {}; preserve dilution wells {} for downstream work. This run performs no plating.",
                value_source(culture_source),
                medium.symbol,
                material_source(&medium.source),
                medium.source_well,
                well_list(dilution_wells)
            ));
        }
    }
    doc
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

fn value_source(source: &PlanningValueSource) -> String {
    match source {
        PlanningValueSource::ChoiceInput { input } => format!("choice input {input}"),
        PlanningValueSource::ChoiceOutput { choice, output } => {
            format!("Method choice {choice} output {output}")
        }
        PlanningValueSource::TaskOutput { task, output } => {
            format!("Procedure task {task} output {output}")
        }
    }
}

fn well_list(wells: &[Well]) -> String {
    wells
        .iter()
        .map(|well| format!("plate {} {}", well.plate + 1, well.well))
        .collect::<Vec<_>>()
        .join(", ")
}
