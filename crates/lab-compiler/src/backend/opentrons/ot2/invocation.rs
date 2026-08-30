//! Requirement-scoped lowering from exact facility allocations to standalone OT-2 protocols.

use std::collections::BTreeSet;

use lab_instruments::ThermalProfile;
use lab_method::LocalId;
use lab_procedure::{MixTechnique, TransferTechnique};
use lab_runfmt::OPENTRONS_PYTHON_PROTOCOL_FORMAT;
use serde::Serialize;

use crate::backend::adapters::{AdapterInvocationDocument, AdapterInvocationLowering};
use crate::backend::document::{Column, Doc, DocMeta, bold, code, text};
use crate::backend::invocation::{ProcedureTaskView, exact_invocation_tasks};
use crate::backend::opentrons::ot2::BACKEND;
use crate::backend::opentrons::ot2::profile::Ot2AdapterProfile;
use crate::backend::opentrons::ot2::schedule::{
    Ot2BatchExecution, Ot2BatchPlan, Ot2BatchRun, try_plan_golden_gate_batches,
};
use crate::backend::procedure::{
    ADD_RECOVERY_MEDIUM, CYCLE_GOLDEN_GATE, HEAT_SHOCK_TRANSFORMATION, INCUBATE_RECOVERY_CULTURE,
    PLATE_DILUTED_CULTURE, PREPARE_CHEMICAL_TRANSFORMATION, SERIAL_DILUTION, SETUP_GOLDEN_GATE,
    normalized_chemical_transformation, normalized_golden_gate_setup, normalized_recovery_medium,
    normalized_selective_plating, normalized_serial_dilution, normalized_thermal_program,
};
use crate::backend::profile::Plates;
use crate::backend::resources::{Well, assign_source_wells, plate_wells};
use crate::backend::typst;
use crate::planning::{
    AdapterInvocation, AdapterInvocationPlan, AllocatedExecutionGroup, AllocatedProcedureTask,
    AllocatedRequirementBinding, PlanningProcedureParameter, PlanningTaskInput, PlanningTaskOutput,
    PlanningValueSource, SelectedCapabilityParameter, SelectedMaterialBinding,
    SelectedMaterialSource,
};
use crate::{ArtifactBundle, GeneratedArtifact};

const TASK_PLAN_SCHEMA: &str = "lab.opentrons-ot2-task.v3";
const RUN_PLAN_SCHEMA: &str = "lab.opentrons-ot2-run.v1";

const SETUP_TEMPLATE: &str = include_str!("invocation/setup_reaction.py");
const CYCLE_TEMPLATE: &str = include_str!("invocation/thermal_cycle.py");
const TRANSFORMATION_TEMPLATE: &str = include_str!("invocation/prepare_transformation.py");
const RECOVERY_TEMPLATE: &str = include_str!("invocation/add_recovery_medium.py");
const DILUTION_TEMPLATE: &str = include_str!("invocation/serial_dilution.py");
const PLATING_TEMPLATE: &str = include_str!("invocation/plate_diluted_culture.py");
const ASSEMBLY_BATCH_TEMPLATE: &str = include_str!("invocation/assembly_batch.py");
const TRANSFORMATION_BATCH_TEMPLATE: &str = include_str!("invocation/transformation_batch.py");
const PLATING_BATCH_TEMPLATE: &str = include_str!("invocation/plating_batch.py");
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

#[derive(Serialize)]
struct Ot2RunPlan {
    schema_version: String,
    facility: String,
    asset: String,
    adapter: String,
    adapter_profile: String,
    adapter_profile_sha256: String,
    schedule_sha256: String,
    group: AllocatedExecutionGroup,
    requirements: Vec<RequirementReview>,
    tasks: Vec<TaskReview>,
    deck: Ot2AdapterProfile,
    execution: Ot2BatchExecution,
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
    SetupGoldenGateReaction {
        artifact: String,
        reaction_wells: Vec<String>,
        additions: Vec<MaterialAddition>,
        reaction_volume_ul: u32,
        mix_cycles: u32,
        mix_volume_ul: u32,
    },
    ThermalProgram {
        title: String,
        sample_wells: Vec<String>,
        volume_each_ul: f64,
        lid_temperature_c: Option<f64>,
        profile: ThermalProfile,
        final_hold_celsius: Option<f64>,
    },
    PrepareChemicalTransformation {
        artifact: String,
        cell_source: PlanningValueSource,
        cell_source_well: String,
        cell_source_volume_ul: u32,
        dna: Vec<MaterialPlacement>,
        reaction_wells: Vec<String>,
        cell_mix_cycles: u32,
        cell_mix_volume_ul: u32,
        cell_mix_technique: MixTechnique,
        cell_volume_ul: u32,
        cell_transfer_technique: TransferTechnique,
        dna_mix_cycles: u32,
        dna_mix_volume_ul: u32,
        dna_mix_technique: MixTechnique,
        dna_volume_ul: u32,
        dna_transfer_technique: TransferTechnique,
        bubble_clear_cycles: u32,
        bubble_clear_volume_ul: u32,
        bubble_clear_technique: MixTechnique,
    },
    AddRecoveryMedium {
        artifact: String,
        culture_source: PlanningValueSource,
        culture_wells: Vec<String>,
        medium: MaterialPlacement,
        initial_volume_ul: u32,
        recovery_volume_ul: u32,
        technique: TransferTechnique,
    },
    SerialDilution {
        artifact: String,
        culture_source: PlanningValueSource,
        culture_wells: Vec<String>,
        medium: MaterialPlacement,
        dilution_wells: Vec<Well>,
        initial_volume_ul: u32,
        medium_volume_ul: u32,
        culture_volume_ul: u32,
        mix_cycles: u32,
        mix_volume_ul: u32,
        medium_technique: TransferTechnique,
        transfer_technique: TransferTechnique,
        mix_technique: MixTechnique,
    },
    PlateDilutedCulture {
        artifact: String,
        culture_source: PlanningValueSource,
        selection: MaterialReview,
        dilution_wells: Vec<Well>,
        agar_wells: Vec<Well>,
        initial_volume_by_dilution_ul: Vec<u32>,
        culture_replicates: usize,
        serial_dilutions: usize,
        plating_replicates: usize,
        colony_volume_ul: u32,
        technique: TransferTechnique,
        plate_map: Vec<PlateMapEntry>,
    },
}

#[derive(Clone, Serialize)]
pub(super) struct MaterialReview {
    pub(super) role: String,
    pub(super) input: LocalId,
    pub(super) symbol: String,
    pub(super) source: SelectedMaterialSource,
}

#[derive(Clone, Serialize)]
pub(super) struct MaterialPlacement {
    #[serde(flatten)]
    pub(super) material: MaterialReview,
    pub(super) source_well: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) load_volume_ul: Option<u32>,
}

#[derive(Clone, Serialize)]
pub(super) struct MaterialAddition {
    #[serde(flatten)]
    pub(super) placement: MaterialPlacement,
    pub(super) volume_ul: u32,
}

#[derive(Clone, Serialize)]
pub(super) struct PlateMapEntry {
    pub(super) subject: String,
    pub(super) dilution: usize,
    pub(super) dilution_ratio: String,
    pub(super) culture_replicate: usize,
    pub(super) plating_replicate: usize,
    pub(super) source: Well,
    pub(super) destination: Well,
}

#[derive(Serialize)]
struct PlateMapDocument<'a> {
    schema_version: &'static str,
    facility: &'a str,
    asset: &'a str,
    task: &'a LocalId,
    artifact: &'a str,
    culture_source: &'a PlanningValueSource,
    selection: &'a MaterialReview,
    entries: &'a [PlateMapEntry],
}

#[derive(Serialize)]
struct BatchPlateMapDocument<'a> {
    schema_version: &'static str,
    facility: &'a str,
    asset: &'a str,
    schedule_sha256: &'a str,
    tasks: &'a [LocalId],
    entries: &'a [PlateMapEntry],
}

/// Lower only the Procedure tasks and requirements allocated to this exact invocation.
pub(in crate::backend) fn lower_invocation(
    profile: &Ot2AdapterProfile,
    invocation_plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
) -> Result<AdapterInvocationLowering, String> {
    if let Some(batch) = try_plan_golden_gate_batches(profile, invocation_plan, invocation)? {
        return lower_batch_invocation(profile, invocation_plan, invocation, batch);
    }
    let tasks = exact_invocation_tasks("OT-2", invocation_plan, invocation)?;
    let mut artifacts = ArtifactBundle::new();
    let mut documents = Vec::new();

    for (ordinal, member) in tasks.into_iter().enumerate() {
        let (slug, execution) = plan_task(profile, member.task, &member.requirements)?;
        let directory = format!("tasks/{:03}-{slug}", ordinal + 1);
        let plan = Ot2TaskPlan {
            schema_version: TASK_PLAN_SCHEMA.to_owned(),
            facility: invocation_plan.facility.clone(),
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
        if let Ot2TaskExecution::PlateDilutedCulture {
            artifact,
            culture_source,
            selection,
            plate_map,
            ..
        } = &plan.execution
        {
            let map = PlateMapDocument {
                schema_version: "lab.plate-map.v1",
                facility: &plan.facility,
                asset: &plan.asset,
                task: &plan.task.id,
                artifact,
                culture_source,
                selection,
                entries: plate_map,
            };
            let mut json = serde_json::to_string_pretty(&map).map_err(|error| error.to_string())?;
            json.push('\n');
            artifacts
                .insert_text(
                    format!("{directory}/plate_map.json"),
                    "application/json",
                    json,
                )
                .map_err(|error| error.to_string())?;
            artifacts
                .insert_text(
                    format!("{directory}/plate_map.typ"),
                    "text/x-typst",
                    typst::render(&render_plate_map(&plan)),
                )
                .map_err(|error| error.to_string())?;
        }
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

fn lower_batch_invocation(
    profile: &Ot2AdapterProfile,
    invocation_plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
    batch: Ot2BatchPlan,
) -> Result<AdapterInvocationLowering, String> {
    let schedule_sha256 = batch.schedule.sha256();
    let mut artifacts = ArtifactBundle::new();
    let mut schedule_json =
        serde_json::to_string_pretty(&batch.schedule).map_err(|error| error.to_string())?;
    schedule_json.push('\n');
    artifacts
        .insert_text("execution_schedule.json", "application/json", schedule_json)
        .map_err(|error| error.to_string())?;
    artifacts
        .insert(
            GeneratedArtifact::text(typst::STYLE_PATH, "text/x-typst", typst::STYLE)
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;

    let mut documents = Vec::with_capacity(batch.runs.len());
    let plating_tasks = batch
        .runs
        .iter()
        .find(|run| run.id == "plating")
        .map(|run| run.group.tasks.clone())
        .unwrap_or_default();
    for run in batch.runs {
        let plan = batch_run_plan(profile, invocation_plan, invocation, &schedule_sha256, run)?;
        let template = match &plan.execution {
            Ot2BatchExecution::Assembly { .. } => ASSEMBLY_BATCH_TEMPLATE,
            Ot2BatchExecution::Transformation { .. } => TRANSFORMATION_BATCH_TEMPLATE,
            Ot2BatchExecution::Plating { .. } => PLATING_BATCH_TEMPLATE,
        };
        let protocol_path = match &plan.execution {
            Ot2BatchExecution::Assembly { .. } => "assembly_protocol.py",
            Ot2BatchExecution::Transformation { .. } => "transformation_protocol.py",
            Ot2BatchExecution::Plating { .. } => "plating_protocol.py",
        };
        let manifest_path = match &plan.execution {
            Ot2BatchExecution::Assembly { .. } => "assembly_manifest.json",
            Ot2BatchExecution::Transformation { .. } => "transformation_manifest.json",
            Ot2BatchExecution::Plating { .. } => "plating_manifest.json",
        };
        let manual_path = match &plan.execution {
            Ot2BatchExecution::Assembly { .. } => "assembly_manual_protocol.typ",
            Ot2BatchExecution::Transformation { .. } => "transformation_manual_protocol.typ",
            Ot2BatchExecution::Plating { .. } => "plating_manual_protocol.typ",
        };
        artifacts
            .insert_text(
                protocol_path,
                "text/x-python",
                render_embedded_python_protocol(template, &plan.deck, &plan)?,
            )
            .map_err(|error| error.to_string())?;
        let mut manifest =
            serde_json::to_string_pretty(&plan).map_err(|error| error.to_string())?;
        manifest.push('\n');
        artifacts
            .insert_text(manifest_path, "application/json", manifest)
            .map_err(|error| error.to_string())?;
        artifacts
            .insert_text(
                manual_path,
                "text/x-typst",
                typst::render(&render_batch_manual(&plan)),
            )
            .map_err(|error| error.to_string())?;
        documents.push(AdapterInvocationDocument {
            requirements: plan.group.requirements.clone(),
            path: protocol_path.to_owned(),
            format: OPENTRONS_PYTHON_PROTOCOL_FORMAT.to_owned(),
        });
    }

    let map = BatchPlateMapDocument {
        schema_version: "lab.plate-map.v2",
        facility: &invocation_plan.facility,
        asset: &invocation.asset,
        schedule_sha256: &schedule_sha256,
        tasks: &plating_tasks,
        entries: &batch.plate_map,
    };
    let mut map_json = serde_json::to_string_pretty(&map).map_err(|error| error.to_string())?;
    map_json.push('\n');
    artifacts
        .insert_text("plate_map.json", "application/json", map_json)
        .map_err(|error| error.to_string())?;
    artifacts
        .insert_text(
            "plate_map.typ",
            "text/x-typst",
            typst::render(&render_batch_plate_map(&map)),
        )
        .map_err(|error| error.to_string())?;

    Ok(AdapterInvocationLowering {
        artifacts,
        documents,
    })
}

fn batch_run_plan(
    profile: &Ot2AdapterProfile,
    invocation_plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
    schedule_sha256: &str,
    run: Ot2BatchRun,
) -> Result<Ot2RunPlan, String> {
    let tasks = run
        .group
        .tasks
        .iter()
        .map(|task_id| {
            invocation_plan
                .methods
                .iter()
                .flat_map(|method| &method.tasks)
                .find(|task| &task.id == task_id)
                .map(task_review)
                .ok_or_else(|| {
                    format!(
                        "OT-2 schedule group '{}' refers to missing task '{task_id}'",
                        run.group.id
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let requirements = run
        .group
        .requirements
        .iter()
        .map(|requirement_id| {
            invocation_plan
                .methods
                .iter()
                .flat_map(|method| &method.tasks)
                .flat_map(|task| &task.requirements)
                .find(|requirement| &requirement.id == requirement_id)
                .map(requirement_review)
                .ok_or_else(|| {
                    format!(
                        "OT-2 schedule group '{}' refers to missing requirement '{requirement_id}'",
                        run.group.id
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Ot2RunPlan {
        schema_version: RUN_PLAN_SCHEMA.to_owned(),
        facility: invocation_plan.facility.clone(),
        asset: invocation.asset.clone(),
        adapter: BACKEND.to_owned(),
        adapter_profile: profile.name.clone(),
        adapter_profile_sha256: invocation.adapter.profile_sha256.clone(),
        schedule_sha256: schedule_sha256.to_owned(),
        group: run.group,
        requirements,
        tasks,
        deck: profile.clone(),
        execution: run.execution,
    })
}

fn task_review(task: &AllocatedProcedureTask) -> TaskReview {
    TaskReview {
        id: task.id.clone(),
        operation: task.operation.to_string(),
        inputs: task.inputs.clone(),
        outputs: task.outputs.clone(),
        parameters: task.parameters.clone(),
        materials: task.materials.clone(),
    }
}

fn requirement_review(requirement: &AllocatedRequirementBinding) -> RequirementReview {
    RequirementReview {
        id: requirement.id.clone(),
        capability_kind: requirement.capability_kind.to_string(),
        offering: requirement.offering.clone(),
        observed_qualification: requirement.observed_qualification.clone(),
        control_mode: requirement.control_mode.clone(),
        parameters: requirement.parameters.clone(),
    }
}

pub(super) fn plan_task(
    profile: &Ot2AdapterProfile,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
) -> Result<(&'static str, Ot2TaskExecution), String> {
    match task.operation.as_str() {
        SETUP_GOLDEN_GATE => Ok((
            "setup-golden-gate-reaction",
            plan_setup(profile, task, requirements)?,
        )),
        CYCLE_GOLDEN_GATE => Ok((
            "thermal-cycle-golden-gate-reaction",
            plan_thermal(profile, task, requirements)?,
        )),
        PREPARE_CHEMICAL_TRANSFORMATION => Ok((
            "prepare-chemical-transformation",
            plan_transformation(profile, task, requirements)?,
        )),
        HEAT_SHOCK_TRANSFORMATION => Ok((
            "heat-shock-transformation",
            plan_thermal(profile, task, requirements)?,
        )),
        ADD_RECOVERY_MEDIUM => Ok((
            "add-recovery-medium",
            plan_recovery_medium(profile, task, requirements)?,
        )),
        INCUBATE_RECOVERY_CULTURE => Ok((
            "incubate-recovery-culture",
            plan_thermal(profile, task, requirements)?,
        )),
        SERIAL_DILUTION => Ok((
            "serial-dilution",
            plan_dilution(profile, task, requirements)?,
        )),
        PLATE_DILUTED_CULTURE => Ok((
            "plate-diluted-culture",
            plan_plating(profile, task, requirements)?,
        )),
        operation => Err(format!(
            "OT-2 invocation contains unsupported Procedure operation '{operation}' in task '{}'",
            task.id
        )),
    }
}

fn plan_setup(
    profile: &Ot2AdapterProfile,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
) -> Result<Ot2TaskExecution, String> {
    let procedure = normalized_golden_gate_setup("OT-2", task, requirements)?;
    let view = ProcedureTaskView::new("OT-2", task);
    let source_keys = procedure
        .additions
        .iter()
        .map(|addition| addition.material.symbol.clone())
        .collect::<BTreeSet<_>>();
    let source_wells = assign_source_wells(
        BACKEND,
        "setup-golden-gate-reaction",
        source_keys,
        profile.deck.temperature_module.capacity,
    )
    .map_err(|error| error.to_string())?;
    let additions = procedure
        .additions
        .into_iter()
        .map(|addition| MaterialAddition {
            placement: MaterialPlacement {
                material: material_review(addition.role, addition.material),
                source_well: source_wells[&addition.material.symbol].clone(),
                load_volume_ul: None,
            },
            volume_ul: addition.volume_ul,
        })
        .collect::<Vec<_>>();

    let reaction_plate = known_wells(task, "reaction plate", profile.deck.thermocycler.capacity)?;
    if procedure.replicates > reaction_plate.len() {
        return Err(view.capacity_error(
            "reaction plate",
            procedure.replicates,
            reaction_plate.len(),
        ));
    }
    let required_tips = (additions.len() + 1)
        .checked_mul(procedure.replicates)
        .ok_or_else(|| format!("OT-2 Procedure task '{}' tip count overflows", task.id))?;
    let tip_capacity = profile.stages.assembly.small_tips.total_capacity();
    if required_tips > tip_capacity {
        return Err(view.capacity_error("assembly small-tip racks", required_tips, tip_capacity));
    }

    Ok(Ot2TaskExecution::SetupGoldenGateReaction {
        artifact: procedure.artifact,
        reaction_wells: reaction_plate
            .into_iter()
            .take(procedure.replicates)
            .collect(),
        additions,
        reaction_volume_ul: procedure.reaction_volume_ul,
        mix_cycles: procedure.mix_cycles,
        mix_volume_ul: procedure.mix_volume_ul,
    })
}

fn plan_transformation(
    profile: &Ot2AdapterProfile,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
) -> Result<Ot2TaskExecution, String> {
    let procedure = normalized_chemical_transformation("OT-2", task, requirements)?;
    let view = ProcedureTaskView::new("OT-2", task);
    let reaction_wells = known_wells(
        task,
        "transformation reaction plate",
        profile.deck.thermocycler.capacity,
    )?;
    if procedure.replicates > reaction_wells.len() {
        return Err(view.capacity_error(
            "transformation reaction plate",
            procedure.replicates,
            reaction_wells.len(),
        ));
    }
    let dna_keys = procedure
        .dna
        .iter()
        .map(|material| material.symbol.clone())
        .collect::<BTreeSet<_>>();
    if dna_keys.len() != procedure.dna.len() {
        return Err(format!(
            "OT-2 Procedure task '{}' has duplicate DNA symbols that cannot be staged unambiguously",
            task.id
        ));
    }
    let dna_wells = assign_source_wells(
        BACKEND,
        "prepare-chemical-transformation",
        dna_keys,
        profile.stages.transformation.dna_plate.capacity,
    )
    .map_err(|error| error.to_string())?;
    let dna_source_volume_ul = procedure
        .dna_volume_ul
        .checked_mul(u32::try_from(procedure.replicates).map_err(|_| {
            format!(
                "OT-2 Procedure task '{}' replicate count does not fit a source-volume calculation",
                task.id
            )
        })?)
        .map(|volume| volume.max(procedure.dna_mix_volume_ul))
        .ok_or_else(|| format!("OT-2 Procedure task '{}' DNA volume overflows", task.id))?;
    let dna = procedure
        .dna
        .into_iter()
        .map(|material| MaterialPlacement {
            material: material_review("dependency".to_owned(), material),
            source_well: dna_wells[&material.symbol].clone(),
            load_volume_ul: Some(dna_source_volume_ul),
        })
        .collect::<Vec<_>>();
    let small_tips = procedure.replicates.checked_mul(dna.len()).ok_or_else(|| {
        format!(
            "OT-2 Procedure task '{}' small-tip count overflows",
            task.id
        )
    })?;
    let small_tip_capacity = profile.stages.transformation.small_tips.total_capacity();
    if small_tips > small_tip_capacity {
        return Err(view.capacity_error(
            "transformation small-tip racks",
            small_tips,
            small_tip_capacity,
        ));
    }
    if profile.stages.transformation.large_tips.total_capacity() == 0 {
        return Err(view.capacity_error("transformation large-tip racks", 1, 0));
    }
    let cell_source_well = known_wells(
        task,
        "competent-cell rack",
        profile.deck.temperature_module.capacity,
    )?
    .into_iter()
    .next()
    .expect("known wells is non-empty");
    let cell_source_volume_ul = procedure
        .cell_volume_ul
        .checked_mul(u32::try_from(procedure.replicates).map_err(|_| {
            format!(
                "OT-2 Procedure task '{}' replicate count does not fit a source-volume calculation",
                task.id
            )
        })?)
        .map(|volume| volume.max(procedure.cell_mix_volume_ul))
        .ok_or_else(|| {
            format!(
                "OT-2 Procedure task '{}' competent-cell volume overflows",
                task.id
            )
        })?;

    Ok(Ot2TaskExecution::PrepareChemicalTransformation {
        artifact: procedure.artifact,
        cell_source: procedure.cell_source.clone(),
        cell_source_well,
        cell_source_volume_ul,
        dna,
        reaction_wells: reaction_wells
            .into_iter()
            .take(procedure.replicates)
            .collect(),
        cell_mix_cycles: procedure.cell_mix_cycles,
        cell_mix_volume_ul: procedure.cell_mix_volume_ul,
        cell_mix_technique: procedure.cell_mix_technique,
        cell_volume_ul: procedure.cell_volume_ul,
        cell_transfer_technique: procedure.cell_transfer_technique,
        dna_mix_cycles: procedure.dna_mix_cycles,
        dna_mix_volume_ul: procedure.dna_mix_volume_ul,
        dna_mix_technique: procedure.dna_mix_technique,
        dna_volume_ul: procedure.dna_volume_ul,
        dna_transfer_technique: procedure.dna_transfer_technique,
        bubble_clear_cycles: procedure.bubble_clear_cycles,
        bubble_clear_volume_ul: procedure.bubble_clear_volume_ul,
        bubble_clear_technique: procedure.bubble_clear_technique,
    })
}

fn plan_recovery_medium(
    profile: &Ot2AdapterProfile,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
) -> Result<Ot2TaskExecution, String> {
    let procedure = normalized_recovery_medium("OT-2", task, requirements)?;
    let view = ProcedureTaskView::new("OT-2", task);
    let culture_wells = known_wells(
        task,
        "recovery culture plate",
        profile.deck.thermocycler.capacity,
    )?;
    if procedure.replicates > culture_wells.len() {
        return Err(view.capacity_error(
            "recovery culture plate",
            procedure.replicates,
            culture_wells.len(),
        ));
    }
    if profile.stages.transformation.large_tips.total_capacity() == 0 {
        return Err(view.capacity_error("recovery large-tip racks", 1, 0));
    }
    let source_well = known_wells(
        task,
        "recovery-medium rack",
        profile.deck.temperature_module.capacity,
    )?
    .into_iter()
    .next()
    .expect("known wells is non-empty");
    let load_volume_ul = procedure
        .recovery_volume_ul
        .checked_mul(u32::try_from(procedure.replicates).map_err(|_| {
            format!(
                "OT-2 Procedure task '{}' replicate count does not fit a source-volume calculation",
                task.id
            )
        })?)
        .ok_or_else(|| {
            format!(
                "OT-2 Procedure task '{}' recovery-medium volume overflows",
                task.id
            )
        })?;

    Ok(Ot2TaskExecution::AddRecoveryMedium {
        artifact: procedure.subject,
        culture_source: procedure.culture_source.clone(),
        culture_wells: culture_wells
            .into_iter()
            .take(procedure.replicates)
            .collect(),
        medium: MaterialPlacement {
            material: material_review("medium".to_owned(), procedure.medium),
            source_well,
            load_volume_ul: Some(load_volume_ul),
        },
        initial_volume_ul: procedure.initial_volume_ul,
        recovery_volume_ul: procedure.recovery_volume_ul,
        technique: procedure.technique,
    })
}

fn plan_thermal(
    profile: &Ot2AdapterProfile,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
) -> Result<Ot2TaskExecution, String> {
    let procedure = normalized_thermal_program("OT-2", task, requirements)?;
    let view = ProcedureTaskView::new("OT-2", task);
    let sample_wells = known_wells(task, "sample plate", profile.deck.thermocycler.capacity)?;
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

fn plan_dilution(
    profile: &Ot2AdapterProfile,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
) -> Result<Ot2TaskExecution, String> {
    let procedure = normalized_serial_dilution("OT-2", task, requirements)?;
    let view = ProcedureTaskView::new("OT-2", task);
    let dilution_wells = layered_plate_layout(
        task,
        "dilution plates",
        &profile.stages.plating.dilution_plate,
        procedure.serial_dilutions,
        procedure.culture_replicates,
    )?;
    let culture_wells = known_wells(
        task,
        "culture staging plate",
        profile.deck.thermocycler.capacity,
    )?;
    if procedure.culture_replicates > culture_wells.len() {
        return Err(view.capacity_error(
            "culture staging plate",
            procedure.culture_replicates,
            culture_wells.len(),
        ));
    }
    if procedure.culture_replicates > profile.stages.plating.small_tips.total_capacity() {
        return Err(view.capacity_error(
            "dilution small-tip racks",
            procedure.culture_replicates,
            profile.stages.plating.small_tips.total_capacity(),
        ));
    }
    if profile.stages.plating.large_tips.total_capacity() == 0 {
        return Err(view.capacity_error("dilution large-tip racks", 1, 0));
    }
    let chunk_count = dilution_wells
        .len()
        .div_ceil(profile.techniques.tracked_chunk_size);
    let medium_load_ul = procedure
        .medium_volume_ul
        .checked_mul(u32::try_from(dilution_wells.len()).map_err(|_| {
            format!(
                "OT-2 Procedure task '{}' dilution count does not fit a source-volume calculation",
                task.id
            )
        })?)
        .and_then(|volume| {
            profile
                .techniques
                .distribution_disposal_volume_ul
                .checked_mul(u32::try_from(chunk_count).ok()?)
                .and_then(|disposal| volume.checked_add(disposal))
        })
        .ok_or_else(|| {
            format!(
                "OT-2 Procedure task '{}' recovery-medium volume overflows",
                task.id
            )
        })?;
    if medium_load_ul > profile.techniques.tracked_source_volume_ul {
        return Err(format!(
            "OT-2 Procedure task '{}' needs {medium_load_ul} uL of dilution medium including calibrated disposal volume, but its tracked source is configured with {} uL",
            task.id, profile.techniques.tracked_source_volume_ul
        ));
    }

    Ok(Ot2TaskExecution::SerialDilution {
        artifact: procedure.subject,
        culture_source: procedure.culture_source.clone(),
        culture_wells: culture_wells
            .into_iter()
            .take(procedure.culture_replicates)
            .collect(),
        medium: MaterialPlacement {
            material: material_review("medium".to_owned(), procedure.medium),
            source_well: profile.stages.plating.media_rack.medium_well.clone(),
            load_volume_ul: Some(profile.techniques.tracked_source_volume_ul),
        },
        dilution_wells,
        initial_volume_ul: procedure.initial_volume_ul,
        medium_volume_ul: procedure.medium_volume_ul,
        culture_volume_ul: procedure.culture_volume_ul,
        mix_cycles: procedure.mix_cycles,
        mix_volume_ul: procedure.mix_volume_ul,
        medium_technique: procedure.medium_technique,
        transfer_technique: procedure.transfer_technique,
        mix_technique: procedure.mix_technique,
    })
}

fn plan_plating(
    profile: &Ot2AdapterProfile,
    task: &AllocatedProcedureTask,
    requirements: &[&AllocatedRequirementBinding],
) -> Result<Ot2TaskExecution, String> {
    let procedure = normalized_selective_plating("OT-2", task, requirements)?;
    let view = ProcedureTaskView::new("OT-2", task);
    let dilution_wells = layered_plate_layout(
        task,
        "dilution plates",
        &profile.stages.plating.dilution_plate,
        procedure.serial_dilutions,
        procedure.culture_replicates,
    )?;
    let agar_per_dilution = procedure
        .culture_replicates
        .checked_mul(procedure.plating_replicates)
        .ok_or_else(|| {
            format!(
                "OT-2 Procedure task '{}' agar-well count overflows",
                task.id
            )
        })?;
    let agar_wells = layered_plate_layout(
        task,
        "selective agar plates",
        &profile.stages.plating.agar_plate,
        procedure.serial_dilutions,
        agar_per_dilution,
    )?;
    let required_tips = procedure
        .culture_replicates
        .checked_mul(procedure.serial_dilutions)
        .ok_or_else(|| {
            format!(
                "OT-2 Procedure task '{}' plating tip count overflows",
                task.id
            )
        })?;
    let tip_capacity = profile.stages.plating.small_tips.total_capacity();
    if required_tips > tip_capacity {
        return Err(view.capacity_error("plating small-tip racks", required_tips, tip_capacity));
    }
    let mut plate_map = Vec::with_capacity(agar_wells.len());
    for dilution in 0..procedure.serial_dilutions {
        for culture_replicate in 0..procedure.culture_replicates {
            let source =
                dilution_wells[dilution * procedure.culture_replicates + culture_replicate].clone();
            for plating_replicate in 0..procedure.plating_replicates {
                let destination_index = dilution * agar_per_dilution
                    + culture_replicate * procedure.plating_replicates
                    + plating_replicate;
                plate_map.push(PlateMapEntry {
                    subject: procedure.subject.clone(),
                    dilution: dilution + 1,
                    dilution_ratio: dilution_ratio(
                        procedure.medium_volume_ul,
                        procedure.culture_volume_ul,
                        dilution + 1,
                    )?,
                    culture_replicate: culture_replicate + 1,
                    plating_replicate: plating_replicate + 1,
                    source: source.clone(),
                    destination: agar_wells[destination_index].clone(),
                });
            }
        }
    }

    Ok(Ot2TaskExecution::PlateDilutedCulture {
        artifact: procedure.subject,
        culture_source: procedure.culture_source.clone(),
        selection: material_review("selection".to_owned(), procedure.selection),
        dilution_wells,
        agar_wells,
        initial_volume_by_dilution_ul: procedure.initial_volume_by_dilution_ul,
        culture_replicates: procedure.culture_replicates,
        serial_dilutions: procedure.serial_dilutions,
        plating_replicates: procedure.plating_replicates,
        colony_volume_ul: procedure.colony_volume_ul,
        technique: procedure.technique,
        plate_map,
    })
}

pub(super) fn layered_plate_layout(
    task: &AllocatedProcedureTask,
    resource: &str,
    plates: &Plates,
    layers: usize,
    positions_per_layer: usize,
) -> Result<Vec<Well>, String> {
    let wells = plate_wells(plates.capacity);
    if wells.len() != plates.capacity || layers == 0 || positions_per_layer == 0 {
        return Err(format!(
            "OT-2 Procedure task '{}' cannot address {resource} with {} layers of {positions_per_layer} positions and declared per-plate capacity {}",
            task.id, layers, plates.capacity
        ));
    }
    let shared_region = plates.capacity / layers;
    if positions_per_layer <= shared_region {
        return Ok((0..layers)
            .flat_map(|layer| {
                let offset = layer * shared_region;
                (0..positions_per_layer).map({
                    let wells = &wells;
                    move |position| Well {
                        plate: 0,
                        well: wells[offset + position].clone(),
                    }
                })
            })
            .collect());
    }
    if layers <= plates.slots.len() && positions_per_layer <= plates.capacity {
        return Ok((0..layers)
            .flat_map(|layer| {
                (0..positions_per_layer).map({
                    let wells = &wells;
                    move |position| Well {
                        plate: layer,
                        well: wells[position].clone(),
                    }
                })
            })
            .collect());
    }
    Err(format!(
        "OT-2 Procedure task '{}' needs {layers} isolated {resource} regions of {positions_per_layer} positions, but the profile supplies {} plates with {} positions each",
        task.id,
        plates.slots.len(),
        plates.capacity
    ))
}

pub(super) fn dilution_ratio(
    medium_volume_ul: u32,
    culture_volume_ul: u32,
    steps: usize,
) -> Result<String, String> {
    let total = medium_volume_ul
        .checked_add(culture_volume_ul)
        .ok_or_else(|| "OT-2 dilution-ratio volume arithmetic overflows".to_owned())?;
    let divisor = greatest_common_divisor(culture_volume_ul, total);
    let numerator = u128::from(culture_volume_ul / divisor);
    let denominator = u128::from(total / divisor);
    let exponent = u32::try_from(steps)
        .map_err(|_| "OT-2 dilution-ratio exponent does not fit this platform".to_owned())?;
    let numerator = numerator
        .checked_pow(exponent)
        .ok_or_else(|| "OT-2 dilution-ratio numerator overflows".to_owned())?;
    let denominator = denominator
        .checked_pow(exponent)
        .ok_or_else(|| "OT-2 dilution-ratio denominator overflows".to_owned())?;
    Ok(format!("{numerator}/{denominator}"))
}

fn greatest_common_divisor(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left.max(1)
}

fn render_python_protocol(plan: &Ot2TaskPlan) -> Result<String, String> {
    let template = match &plan.execution {
        Ot2TaskExecution::SetupGoldenGateReaction { .. } => SETUP_TEMPLATE,
        Ot2TaskExecution::ThermalProgram { .. } => CYCLE_TEMPLATE,
        Ot2TaskExecution::PrepareChemicalTransformation { .. } => TRANSFORMATION_TEMPLATE,
        Ot2TaskExecution::AddRecoveryMedium { .. } => RECOVERY_TEMPLATE,
        Ot2TaskExecution::SerialDilution { .. } => DILUTION_TEMPLATE,
        Ot2TaskExecution::PlateDilutedCulture { .. } => PLATING_TEMPLATE,
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
        Ot2TaskExecution::SetupGoldenGateReaction { artifact, .. } => {
            format!("Set up Golden Gate reaction for {artifact}")
        }
        Ot2TaskExecution::ThermalProgram { title, .. } => title.clone(),
        Ot2TaskExecution::PrepareChemicalTransformation { artifact, .. } => {
            format!("Prepare chemical transformation for {artifact}")
        }
        Ot2TaskExecution::AddRecoveryMedium { artifact, .. } => {
            format!("Add recovery medium for {artifact}")
        }
        Ot2TaskExecution::SerialDilution { artifact, .. } => {
            format!("Serially dilute recovered culture for {artifact}")
        }
        Ot2TaskExecution::PlateDilutedCulture { artifact, .. } => {
            format!("Plate {artifact} cultures on selective medium")
        }
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
        text(" on the exact Asset below. Upstream and downstream Procedure tasks remain separate reviewed plan nodes."),
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
        Ot2TaskExecution::SetupGoldenGateReaction {
            reaction_wells,
            additions,
            reaction_volume_ul,
            mix_cycles,
            mix_volume_ul,
            ..
        } => {
            doc.heading(1, [text("Load sources")]);
            doc.para_text("Place the exact materials below into the stated chilled-rack wells. The physical source column preserves the reviewed MaterialLot or upstream Method output binding.");
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
                        vec![text(&addition.placement.material.role)],
                        vec![code(&addition.placement.material.symbol)],
                        vec![code(material_source(&addition.placement.material.source))],
                        vec![code(&addition.placement.source_well)],
                        vec![text(format!("{} µL", addition.volume_ul))],
                    ]
                }),
            );
            doc.heading(1, [text("Run the allocated setup")]);
            doc.bullets([
                vec![text(format!(
                    "Reaction wells: {}",
                    reaction_wells.join(", ")
                ))],
                vec![text(format!("Final volume: {reaction_volume_ul} µL"))],
                vec![text(format!(
                    "Mix each reaction {mix_cycles} times at {mix_volume_ul} µL"
                ))],
                vec![
                    text("Import "),
                    code("automation_protocol.py"),
                    text(" into the Opentrons App and review its deck map before starting."),
                ],
            ]);
            doc.para_text("Leave the reaction plate on the thermocycler after setup. The next reviewed thermal-cycling node consumes these reaction wells; this setup protocol does not cycle them.");
        }
        Ot2TaskExecution::ThermalProgram {
            sample_wells,
            volume_each_ul,
            lid_temperature_c,
            profile,
            final_hold_celsius,
            ..
        } => {
            doc.heading(1, [text("Stage the allocated reaction")]);
            doc.para([
                text("Confirm that the upstream setup task left only this reaction in wells "),
                code(sample_wells.join(", ")),
                text(" of the configured thermocycler plate."),
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
            doc.para_text("Remove and label the completed reaction before another independently allocated setup/cycle pair reuses the staging wells.");
        }
        Ot2TaskExecution::PrepareChemicalTransformation {
            cell_source,
            cell_source_well,
            dna,
            reaction_wells,
            cell_volume_ul,
            dna_volume_ul,
            bubble_clear_cycles,
            bubble_clear_volume_ul,
            ..
        } => {
            doc.heading(1, [text("Stage transformation inputs")]);
            let mut rows = vec![vec![
                vec![text("Competent cells")],
                vec![code(value_source(cell_source))],
                vec![code(format!("temperature rack {cell_source_well}"))],
            ]];
            rows.extend(dna.iter().map(|placement| {
                vec![
                    vec![code(&placement.material.symbol)],
                    vec![code(material_source(&placement.material.source))],
                    vec![code(format!("DNA plate {}", placement.source_well))],
                ]
            }));
            doc.table(
                [
                    Column::left("Input"),
                    Column::left("Physical source"),
                    Column::left("Location"),
                ],
                rows,
            );
            doc.heading(1, [text("Run the allocated transformation setup")]);
            doc.bullets([
                vec![text(format!(
                    "Reaction wells: {}",
                    reaction_wells.join(", ")
                ))],
                vec![text(format!(
                    "Add {cell_volume_ul} µL competent cells and {dna_volume_ul} µL of each DNA input to every reaction"
                ))],
                vec![text(format!(
                    "Clear bubbles with {bubble_clear_cycles} strokes at {bubble_clear_volume_ul} µL after every DNA addition"
                ))],
            ]);
            doc.para_text("The generated protocol preserves the reviewed source mixing, contamination paths, blowout, touch-tip, and vessel-relative bubble-clearing technique. Leave the plate on the thermocycler for the heat-shock task.");
        }
        Ot2TaskExecution::AddRecoveryMedium {
            culture_source,
            culture_wells,
            medium,
            initial_volume_ul,
            recovery_volume_ul,
            ..
        } => {
            doc.heading(1, [text("Stage recovery inputs")]);
            doc.table(
                [
                    Column::left("Input"),
                    Column::left("Physical source"),
                    Column::left("Location"),
                ],
                [
                    vec![
                        vec![text("Heat-shocked cultures")],
                        vec![code(value_source(culture_source))],
                        vec![code(format!(
                            "thermocycler plate {}",
                            culture_wells.join(", ")
                        ))],
                    ],
                    vec![
                        vec![code(&medium.material.symbol)],
                        vec![code(material_source(&medium.material.source))],
                        vec![code(format!("temperature rack {}", medium.source_well))],
                    ],
                ],
            );
            doc.heading(1, [text("Run the allocated medium addition")]);
            doc.bullets([
                vec![text(format!(
                    "Each culture begins at {initial_volume_ul} µL"
                ))],
                vec![text(format!(
                    "Add {recovery_volume_ul} µL medium above each culture without contacting it"
                ))],
                vec![text(
                    "Preserve the configured air gap and one contamination-safe source path",
                )],
            ]);
            doc.para_text("Leave the plate on the thermocycler for the independently reviewed recovery-incubation task.");
        }
        Ot2TaskExecution::SerialDilution {
            culture_source,
            culture_wells,
            medium,
            dilution_wells,
            medium_volume_ul,
            culture_volume_ul,
            mix_cycles,
            mix_volume_ul,
            ..
        } => {
            doc.heading(1, [text("Stage inputs")]);
            doc.table(
                [
                    Column::left("Input"),
                    Column::left("Physical source"),
                    Column::left("Location"),
                ],
                [
                    vec![
                        vec![text("Recovered culture")],
                        vec![code(value_source(culture_source))],
                        vec![code(format!(
                            "thermocycler plate {}",
                            culture_wells.join(", ")
                        ))],
                    ],
                    vec![
                        vec![code(&medium.material.symbol)],
                        vec![code(material_source(&medium.material.source))],
                        vec![code(format!("media rack {}", medium.source_well))],
                    ],
                ],
            );
            doc.heading(1, [text("Run the allocated dilution")]);
            doc.bullets([
                vec![text(format!(
                    "Dilution wells: {}",
                    well_list(dilution_wells)
                ))],
                vec![text(format!(
                    "Add {medium_volume_ul} µL medium and transfer {culture_volume_ul} µL culture at every dilution step"
                ))],
                vec![text(format!(
                    "Mix every dilution {mix_cycles} times at {mix_volume_ul} µL"
                ))],
                vec![
                    text("Import "),
                    code("automation_protocol.py"),
                    text(" and verify that no agar plate is requested; downstream plating remains a separate Procedure node."),
                ],
            ]);
            doc.para_text("Preserve and label the final dilution wells for the downstream task named by the reviewed execution plan.");
        }
        Ot2TaskExecution::PlateDilutedCulture {
            culture_source,
            selection,
            dilution_wells,
            agar_wells,
            colony_volume_ul,
            plating_replicates,
            ..
        } => {
            doc.heading(1, [text("Stage dilution and selective plates")]);
            doc.table(
                [Column::left("Input"), Column::left("Exact value")],
                [
                    vec![
                        vec![text("Diluted culture")],
                        vec![code(value_source(culture_source))],
                    ],
                    vec![
                        vec![text("Dilution wells")],
                        vec![code(well_list(dilution_wells))],
                    ],
                    vec![
                        vec![text("Selective medium")],
                        vec![code(format!(
                            "{} ({})",
                            selection.symbol,
                            material_source(&selection.source)
                        ))],
                    ],
                    vec![vec![text("Agar wells")], vec![code(well_list(agar_wells))]],
                ],
            );
            doc.heading(1, [text("Run the allocated plating")]);
            doc.bullets([
                vec![text(format!(
                    "Spot {colony_volume_ul} µL per well with {plating_replicates} plating replicates"
                ))],
                vec![text("Dispense at the calibrated material-surface offset and blow out after every spot")],
                vec![
                    text("Review "),
                    code("plate_map.pdf"),
                    text(" or "),
                    code("plate_map.json"),
                    text(" before starting."),
                ],
            ]);
        }
    }
    doc
}

fn render_batch_manual(plan: &Ot2RunPlan) -> Doc {
    let (title, summary) = match &plan.execution {
        Ot2BatchExecution::Assembly {
            setups,
            thermal_programs,
        } => (
            "Golden Gate assembly run",
            format!(
                "Set up {} assembly reactions and execute {} compatible thermal programs as one reviewed OT-2 run.",
                setups.len(),
                thermal_programs.len()
            ),
        ),
        Ot2BatchExecution::Transformation {
            preparations,
            heat_shocks,
            recovery_additions,
            recovery_incubations,
        } => (
            "Golden Gate transformation run",
            format!(
                "Prepare {} transformations, execute {} heat-shock programs, add recovery medium in {} operations, and execute {} recovery incubations without remapping the reaction plate.",
                preparations.len(),
                heat_shocks.len(),
                recovery_additions.len(),
                recovery_incubations.len()
            ),
        ),
        Ot2BatchExecution::Plating {
            dilutions,
            platings,
        } => (
            "Golden Gate dilution and plating run",
            format!(
                "Perform {} serial-dilution programs and {} selective-plating programs using the static plate map.",
                dilutions.len(),
                platings.len()
            ),
        ),
    };
    let mut doc = Doc::new(DocMeta::new(
        title,
        "Operator instructions for one allocated multi-task device run",
        &plan.adapter_profile,
        "Opentrons OT-2",
    ));
    doc.notice([
        bold("Reviewed schedule. "),
        text("This document realizes execution group "),
        code(&plan.group.id),
        text(" from immutable schedule "),
        code(&plan.schedule_sha256),
        text(" on Asset "),
        code(&plan.asset),
        text(". Do not substitute wells, labware, or hardware without rebuilding and reviewing the plan."),
    ]);
    doc.para_text(summary);
    doc.heading(1, [text("Procedure tasks")]);
    doc.table(
        [Column::left("Task"), Column::left("Operation")],
        plan.tasks
            .iter()
            .map(|task| vec![vec![code(task.id.as_str())], vec![code(&task.operation)]]),
    );
    doc.heading(1, [text("Facility allocations")]);
    doc.table(
        [
            Column::left("Requirement"),
            Column::left("Capability"),
            Column::left("Offering"),
        ],
        plan.requirements.iter().map(|requirement| {
            vec![
                vec![code(requirement.id.as_str())],
                vec![code(&requirement.capability_kind)],
                vec![code(&requirement.offering)],
            ]
        }),
    );
    doc.heading(1, [text("Run files")]);
    doc.bullets([
        vec![
            text("Review the exact deck, physical allocations, and operation parameters in the corresponding "),
            code(format!("{}_manifest.json", plan.group.id)),
            text(" file."),
        ],
        vec![
            text("Import the corresponding "),
            code(format!("{}_protocol.py", plan.group.id)),
            text(" into the Opentrons App and confirm that the loaded module is the installed Thermocycler Module GEN1."),
        ],
        vec![
            text("Verify the adapter profile digest is "),
            code(&plan.adapter_profile_sha256),
            text(" before starting motion."),
        ],
    ]);
    if matches!(plan.execution, Ot2BatchExecution::Plating { .. }) {
        doc.para([
            text("Review "),
            code("plate_map.pdf"),
            text(" before plating and use it as the static source-to-destination record."),
        ]);
    }
    doc
}

fn render_batch_plate_map(map: &BatchPlateMapDocument<'_>) -> Doc {
    let mut doc = Doc::new(DocMeta::new(
        "Golden Gate selective plating map",
        "Static physical allocation for the fused OT-2 plating run",
        "Facility-allocated schedule",
        "Opentrons OT-2",
    ));
    doc.notice([
        bold("Identity boundary. "),
        text("This map belongs to Asset "),
        code(map.asset),
        text(" and immutable schedule "),
        code(map.schedule_sha256),
        text("."),
    ]);
    doc.heading(1, [text("Allocated agar positions")]);
    doc.table(
        [
            Column::left("Culture"),
            Column::right("Dilution"),
            Column::right("Ratio"),
            Column::right("Culture replicate"),
            Column::right("Plating replicate"),
            Column::left("Source"),
            Column::left("Agar destination"),
        ],
        map.entries.iter().map(|entry| {
            vec![
                vec![code(&entry.subject)],
                vec![text(entry.dilution.to_string())],
                vec![text(&entry.dilution_ratio)],
                vec![text(entry.culture_replicate.to_string())],
                vec![text(entry.plating_replicate.to_string())],
                vec![code(format!(
                    "plate {} {}",
                    entry.source.plate + 1,
                    entry.source.well
                ))],
                vec![code(format!(
                    "plate {} {}",
                    entry.destination.plate + 1,
                    entry.destination.well
                ))],
            ]
        }),
    );
    doc
}

fn render_plate_map(plan: &Ot2TaskPlan) -> Doc {
    let Ot2TaskExecution::PlateDilutedCulture {
        artifact,
        selection,
        plate_map,
        ..
    } = &plan.execution
    else {
        unreachable!("plate maps are rendered only for plating tasks")
    };
    let mut doc = Doc::new(DocMeta::new(
        "Selective plating map",
        "Static evidence generated from the same reviewed allocation as the OT-2 protocol",
        &plan.adapter_profile,
        "Opentrons OT-2",
    ));
    doc.notice([
        bold("Identity boundary. "),
        text("This map belongs to Procedure task "),
        code(plan.task.id.as_str()),
        text(" for "),
        code(artifact),
        text(" on Asset "),
        code(&plan.asset),
        text(" and selective material "),
        code(format!(
            "{} ({})",
            selection.symbol,
            material_source(&selection.source)
        )),
        text("."),
    ]);
    doc.heading(1, [text("Allocated agar positions")]);
    doc.table(
        [
            Column::right("Dilution step"),
            Column::right("Ratio"),
            Column::right("Culture replicate"),
            Column::right("Plating replicate"),
            Column::left("Source"),
            Column::left("Agar destination"),
        ],
        plate_map.iter().map(|entry| {
            vec![
                vec![text(entry.dilution.to_string())],
                vec![text(&entry.dilution_ratio)],
                vec![text(entry.culture_replicate.to_string())],
                vec![text(entry.plating_replicate.to_string())],
                vec![code(format!(
                    "plate {} {}",
                    entry.source.plate + 1,
                    entry.source.well
                ))],
                vec![code(format!(
                    "plate {} {}",
                    entry.destination.plate + 1,
                    entry.destination.well
                ))],
            ]
        }),
    );
    doc
}

fn material_review(role: impl Into<String>, material: &SelectedMaterialBinding) -> MaterialReview {
    MaterialReview {
        role: role.into(),
        input: material.input.clone(),
        symbol: material.symbol.clone(),
        source: material.source.clone(),
    }
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

fn well_list(wells: &[Well]) -> String {
    wells
        .iter()
        .map(|well| format!("plate {} {}", well.plate + 1, well.well))
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

fn known_wells(
    task: &AllocatedProcedureTask,
    resource: &str,
    capacity: usize,
) -> Result<Vec<String>, String> {
    let wells = plate_wells(capacity);
    if wells.is_empty() {
        Err(format!(
            "OT-2 Procedure task '{}' cannot address {resource} with declared capacity {capacity}",
            task.id
        ))
    } else {
        Ok(wells)
    }
}
