//! Requirement-scoped Flex lowering to standalone Protocol Designer JSON documents.

use std::collections::BTreeSet;

use lab_method::LocalId;
use lab_runfmt::OPENTRONS_PROTOCOL_DESIGNER_FORMAT;
use opentrons_protocol::schema::Metadata;
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
    CYCLE_GOLDEN_GATE, SERIAL_DILUTION, SETUP_GOLDEN_GATE, serial_dilution, setup_golden_gate,
    thermal_cycle_golden_gate,
};
use crate::backend::resources::{PlateAllocator, Well, assign_source_wells, plate_wells};
use crate::backend::typst;
use crate::planning::{
    AdapterInvocation, AdapterInvocationPlan, AllocatedProcedureTask, AllocatedRequirementBinding,
    PlanningProcedureParameter, PlanningTaskInput, PlanningTaskOutput, PlanningValueSource,
    SelectedCapabilityParameter, SelectedMaterialBinding, SelectedMaterialSource,
};
use crate::{ArtifactBundle, GeneratedArtifact};

const TASK_PLAN_SCHEMA: &str = "lab.opentrons-flex-task.v1";

#[derive(Serialize)]
struct FlexTaskPlan {
    schema_version: String,
    facility: String,
    asset: String,
    adapter: String,
    adapter_profile: String,
    adapter_profile_sha256: String,
    requirement: RequirementReview,
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
    SetupGoldenGateReaction {
        artifact: String,
        reaction_wells: Vec<String>,
        additions: Vec<MaterialAddition>,
        reaction_volume_ul: u32,
        mix_cycles: u32,
        mix_volume_ul: u32,
    },
    ThermalCycleGoldenGateReaction {
        artifact: String,
        reaction_wells: Vec<String>,
        reaction_volume_ul: u32,
        cycles: u32,
        digest_temperature_c: u32,
        digest_minutes: u32,
        ligate_temperature_c: u32,
        ligate_minutes: u32,
        lid_temperature_c: u32,
        final_digest_temperature_c: u32,
        final_digest_minutes: u32,
        heat_inactivation_temperature_c: u32,
        heat_inactivation_minutes: u32,
        hold_temperature_c: u32,
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

/// Lower only the Procedure tasks and requirements allocated to this exact Flex invocation.
pub(in crate::backend) fn lower_invocation(
    profile: &FlexAdapterProfile,
    invocation_plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
) -> Result<AdapterInvocationLowering, String> {
    let tasks = exact_invocation_tasks("Flex", invocation_plan, invocation)?;
    let mut artifacts = ArtifactBundle::new();
    let mut documents = Vec::new();

    for (ordinal, member) in tasks.into_iter().enumerate() {
        let (slug, execution) = plan_task(profile, member.task, member.requirement)?;
        let directory = format!("tasks/{:03}-{slug}", ordinal + 1);
        let plan = FlexTaskPlan {
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
            requirement: member.requirement.id.clone(),
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
    requirement: &AllocatedRequirementBinding,
) -> Result<(&'static str, FlexTaskExecution), String> {
    match task.operation.as_str() {
        SETUP_GOLDEN_GATE => Ok((
            "setup-golden-gate-reaction",
            plan_setup(profile, task, requirement)?,
        )),
        CYCLE_GOLDEN_GATE => Ok((
            "thermal-cycle-golden-gate-reaction",
            plan_cycle(profile, task, requirement)?,
        )),
        SERIAL_DILUTION => Ok((
            "serial-dilution",
            plan_dilution(profile, task, requirement)?,
        )),
        operation => Err(format!(
            "Flex invocation contains unsupported Procedure operation '{operation}' in task '{}'",
            task.id
        )),
    }
}

fn plan_setup(
    profile: &FlexAdapterProfile,
    task: &AllocatedProcedureTask,
    requirement: &AllocatedRequirementBinding,
) -> Result<FlexTaskExecution, String> {
    let procedure = setup_golden_gate("Flex", task, requirement)?;
    let view = ProcedureTaskView::new("Flex", task);
    known_wells(
        task,
        "source rack",
        profile.deck.temperature_module.capacity,
    )?;
    known_wells(
        task,
        "assembly tip rack",
        profile.stages.assembly.small_tips.capacity,
    )?;
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
                role: addition.role.to_owned(),
                input: addition.material.input.clone(),
                symbol: addition.material.symbol.clone(),
                source: addition.material.source.clone(),
                source_well: source_wells[&addition.material.symbol].clone(),
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
        .ok_or_else(|| format!("Flex Procedure task '{}' tip count overflows", task.id))?;
    let tip_capacity = profile.stages.assembly.small_tips.total_capacity();
    if required_tips > tip_capacity {
        return Err(view.capacity_error("assembly small-tip racks", required_tips, tip_capacity));
    }
    let working = working_volume_ul(
        &profile.instruments.small,
        &profile.stages.assembly.small_tips,
    )?;
    if f64::from(procedure.mix_volume_ul) > working {
        return Err(format!(
            "Flex Procedure task '{}' requires a {} uL mix, but the configured small pipette and tip provide {working} uL",
            task.id, procedure.mix_volume_ul
        ));
    }

    Ok(FlexTaskExecution::SetupGoldenGateReaction {
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

fn plan_cycle(
    profile: &FlexAdapterProfile,
    task: &AllocatedProcedureTask,
    requirement: &AllocatedRequirementBinding,
) -> Result<FlexTaskExecution, String> {
    let procedure = thermal_cycle_golden_gate("Flex", task, requirement)?;
    let view = ProcedureTaskView::new("Flex", task);
    let reaction_wells = known_wells(task, "reaction plate", profile.deck.thermocycler.capacity)?;
    if procedure.replicates > reaction_wells.len() {
        return Err(view.capacity_error(
            "reaction plate",
            procedure.replicates,
            reaction_wells.len(),
        ));
    }
    for (name, value) in [
        ("digest_temperature_c", procedure.digest_temperature_c),
        ("ligate_temperature_c", procedure.ligate_temperature_c),
        ("lid_temperature_c", procedure.lid_temperature_c),
        (
            "final_digest_temperature_c",
            procedure.final_digest_temperature_c,
        ),
        (
            "heat_inactivation_temperature_c",
            procedure.heat_inactivation_temperature_c,
        ),
        ("hold_temperature_c", procedure.hold_temperature_c),
    ] {
        if value > 110 {
            return Err(format!(
                "Flex Procedure task '{}' parameter '{name}' is {value} °C, above the adapter's 110 °C limit",
                task.id
            ));
        }
    }

    Ok(FlexTaskExecution::ThermalCycleGoldenGateReaction {
        artifact: procedure.artifact,
        reaction_wells: reaction_wells
            .into_iter()
            .take(procedure.replicates)
            .collect(),
        reaction_volume_ul: procedure.reaction_volume_ul,
        cycles: procedure.cycles,
        digest_temperature_c: procedure.digest_temperature_c,
        digest_minutes: procedure.digest_minutes,
        ligate_temperature_c: procedure.ligate_temperature_c,
        ligate_minutes: procedure.ligate_minutes,
        lid_temperature_c: procedure.lid_temperature_c,
        final_digest_temperature_c: procedure.final_digest_temperature_c,
        final_digest_minutes: procedure.final_digest_minutes,
        heat_inactivation_temperature_c: procedure.heat_inactivation_temperature_c,
        heat_inactivation_minutes: procedure.heat_inactivation_minutes,
        hold_temperature_c: procedure.hold_temperature_c,
    })
}

fn plan_dilution(
    profile: &FlexAdapterProfile,
    task: &AllocatedProcedureTask,
    requirement: &AllocatedRequirementBinding,
) -> Result<FlexTaskExecution, String> {
    let procedure = serial_dilution("Flex", task, requirement)?;
    let view = ProcedureTaskView::new("Flex", task);
    known_wells(
        task,
        "dilution plate",
        profile.stages.plating.dilution_plate.capacity,
    )?;
    known_wells(
        task,
        "dilution small-tip rack",
        profile.stages.plating.small_tips.capacity,
    )?;
    known_wells(
        task,
        "dilution large-tip rack",
        profile.stages.plating.large_tips.capacity,
    )?;
    let mut allocator = PlateAllocator::new(
        BACKEND,
        "serial-dilution",
        "dilution_plate",
        &profile.stages.plating.dilution_plate,
    );
    let dilution_wells = allocator
        .take(procedure.serial_dilutions)
        .map_err(|error| error.to_string())?;
    let culture_wells = known_wells(
        task,
        "culture staging plate",
        profile.deck.thermocycler.capacity,
    )?;
    if procedure.serial_dilutions > profile.stages.plating.small_tips.total_capacity() {
        return Err(view.capacity_error(
            "dilution small-tip racks",
            procedure.serial_dilutions,
            profile.stages.plating.small_tips.total_capacity(),
        ));
    }
    if profile.stages.plating.large_tips.total_capacity() == 0 {
        return Err(view.capacity_error("dilution large-tip racks", 1, 0));
    }
    let small_working = working_volume_ul(
        &profile.instruments.small,
        &profile.stages.plating.small_tips,
    )?;
    if f64::from(procedure.mix_volume_ul) > small_working {
        return Err(format!(
            "Flex Procedure task '{}' requires a {} uL mix, but the configured small pipette and tip provide {small_working} uL",
            task.id, procedure.mix_volume_ul
        ));
    }
    let large_working = working_volume_ul(
        &profile.instruments.large,
        &profile.stages.plating.large_tips,
    )?;
    if f64::from(procedure.medium_volume_ul) > large_working {
        return Err(format!(
            "Flex Procedure task '{}' requires a {} uL medium dispense, but the configured large pipette and tip provide {large_working} uL",
            task.id, procedure.medium_volume_ul
        ));
    }

    Ok(FlexTaskExecution::SerialDilution {
        culture_source: procedure.culture_source.clone(),
        culture_well: culture_wells[0].clone(),
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
    })
}

fn render_protocol(plan: &FlexTaskPlan) -> Result<String, String> {
    match &plan.execution {
        FlexTaskExecution::SetupGoldenGateReaction { .. } => render_setup(plan),
        FlexTaskExecution::ThermalCycleGoldenGateReaction { .. } => render_cycle(plan),
        FlexTaskExecution::SerialDilution { .. } => render_dilution(plan),
    }
}

fn render_setup(plan: &FlexTaskPlan) -> Result<String, String> {
    let FlexTaskExecution::SetupGoldenGateReaction {
        reaction_wells,
        additions,
        mix_cycles,
        mix_volume_ul,
        ..
    } = &plan.execution
    else {
        unreachable!("setup renderer receives only a setup plan")
    };
    let profile = &plan.deck;
    let mut builder = stage_builder(profile, "Lab allocated Golden Gate setup");
    let temperature = builder
        .load_module::<TemperatureModule>(slot(&profile.deck.temperature_module.slot))
        .map_err(protocol_error)?;
    let sources = builder
        .load_labware_on_module(&profile.deck.temperature_module.labware, temperature)
        .map_err(protocol_error)?;
    let thermocycler = builder
        .load_module::<Thermocycler>(FlexSlot::B1)
        .map_err(protocol_error)?;
    let reactions = builder
        .load_labware_on_module(&profile.deck.thermocycler.labware, thermocycler)
        .map_err(protocol_error)?;
    let mut tips = TipFeeder::load(&mut builder, &profile.stages.assembly.small_tips)?;
    let pipette = load_instrument(&mut builder, &profile.instruments.small)?;
    builder
        .temperature_module_set_target(temperature, 4.0)
        .map_err(protocol_error)?;
    builder
        .temperature_module_wait_for_temperature(temperature)
        .map_err(protocol_error)?;
    builder.thermocycler_open_lid(thermocycler);

    for destination in reaction_wells {
        for addition in additions {
            transfer(
                &mut builder,
                &mut tips,
                &pipette,
                (sources, &addition.placement.source_well),
                (reactions, destination),
                f64::from(addition.volume_ul),
                None,
            )?;
        }
        let (rack, well) = tips.next();
        builder
            .pick_up_tip(pipette.id, rack, &well)
            .map_err(protocol_error)?;
        builder
            .mix(
                pipette.id,
                reactions,
                destination,
                *mix_cycles,
                f64::from(*mix_volume_ul),
                pipette.flow_rate,
            )
            .map_err(protocol_error)?;
        builder
            .drop_tip_into_trash(pipette.id)
            .map_err(protocol_error)?;
    }
    builder.comment("Allocated setup complete. Preserve the reaction wells for the separately reviewed thermal-cycling task.");
    render(builder)
}

fn render_cycle(plan: &FlexTaskPlan) -> Result<String, String> {
    let FlexTaskExecution::ThermalCycleGoldenGateReaction {
        reaction_volume_ul,
        cycles,
        digest_temperature_c,
        digest_minutes,
        ligate_temperature_c,
        ligate_minutes,
        lid_temperature_c,
        final_digest_temperature_c,
        final_digest_minutes,
        heat_inactivation_temperature_c,
        heat_inactivation_minutes,
        hold_temperature_c,
        ..
    } = &plan.execution
    else {
        unreachable!("cycle renderer receives only a cycle plan")
    };
    let profile = &plan.deck;
    let mut builder = stage_builder(profile, "Lab allocated Golden Gate thermal cycle");
    let thermocycler = builder
        .load_module::<Thermocycler>(FlexSlot::B1)
        .map_err(protocol_error)?;
    builder
        .load_labware_on_module(&profile.deck.thermocycler.labware, thermocycler)
        .map_err(protocol_error)?;
    builder.thermocycler_close_lid(thermocycler);
    builder
        .thermocycler_set_lid_temperature(thermocycler, f64::from(*lid_temperature_c))
        .map_err(protocol_error)?;
    builder
        .thermocycler_wait_for_lid_temperature(thermocycler)
        .map_err(protocol_error)?;
    let mut steps = Vec::with_capacity(*cycles as usize * 2 + 2);
    for _ in 0..*cycles {
        steps.push((
            f64::from(*digest_temperature_c),
            f64::from(*digest_minutes) * 60.0,
        ));
        steps.push((
            f64::from(*ligate_temperature_c),
            f64::from(*ligate_minutes) * 60.0,
        ));
    }
    steps.push((
        f64::from(*final_digest_temperature_c),
        f64::from(*final_digest_minutes) * 60.0,
    ));
    steps.push((
        f64::from(*heat_inactivation_temperature_c),
        f64::from(*heat_inactivation_minutes) * 60.0,
    ));
    builder
        .thermocycler_run_profile(thermocycler, &steps, Some(f64::from(*reaction_volume_ul)))
        .map_err(protocol_error)?;
    builder
        .thermocycler_set_block_temperature(
            thermocycler,
            f64::from(*hold_temperature_c),
            None,
            None,
        )
        .map_err(protocol_error)?;
    builder.thermocycler_deactivate_lid(thermocycler);
    builder.thermocycler_open_lid(thermocycler);
    builder.comment("Allocated thermal cycle complete. Remove and label this reaction before another task reuses the staging wells.");
    render(builder)
}

fn render_dilution(plan: &FlexTaskPlan) -> Result<String, String> {
    let FlexTaskExecution::SerialDilution {
        culture_well,
        dilution_wells,
        medium_volume_ul,
        culture_volume_ul,
        mix_cycles,
        mix_volume_ul,
        ..
    } = &plan.execution
    else {
        unreachable!("dilution renderer receives only a dilution plan")
    };
    let profile = &plan.deck;
    let stage = &profile.stages.plating;
    let mut builder = stage_builder(profile, "Lab allocated serial dilution");
    let thermocycler = builder
        .load_module::<Thermocycler>(FlexSlot::B1)
        .map_err(protocol_error)?;
    let cultures = builder
        .load_labware_on_module(&profile.deck.thermocycler.labware, thermocycler)
        .map_err(protocol_error)?;
    let dilution_plates = load_plates(
        &mut builder,
        &stage.dilution_plate.labware,
        &stage.dilution_plate.slots,
    )?;
    let media = builder
        .load_labware(&stage.media_rack.labware, slot(&stage.media_rack.slot))
        .map_err(protocol_error)?;
    let mut small_tips = TipFeeder::load(&mut builder, &stage.small_tips)?;
    let mut large_tips = TipFeeder::load(&mut builder, &stage.large_tips)?;
    let small = load_instrument(&mut builder, &profile.instruments.small)?;
    let large = load_instrument(&mut builder, &profile.instruments.large)?;
    builder
        .thermocycler_set_block_temperature(thermocycler, 4.0, None, None)
        .map_err(protocol_error)?;
    builder
        .thermocycler_wait_for_block_temperature(thermocycler)
        .map_err(protocol_error)?;
    builder.thermocycler_open_lid(thermocycler);

    let (rack, well) = large_tips.next();
    builder
        .pick_up_tip(large.id, rack, &well)
        .map_err(protocol_error)?;
    let working = large.max_volume.min(large_tips.tip_volume);
    let wells_per_aspirate = ((working / f64::from(*medium_volume_ul)).floor() as usize).max(1);
    for chunk in dilution_wells.chunks(wells_per_aspirate) {
        builder
            .aspirate(
                large.id,
                media,
                &stage.media_rack.medium_well,
                f64::from(*medium_volume_ul) * chunk.len() as f64,
                large.flow_rate,
                None,
            )
            .map_err(protocol_error)?;
        for destination in chunk {
            builder
                .dispense(
                    large.id,
                    dilution_plates[destination.plate],
                    &destination.well,
                    f64::from(*medium_volume_ul),
                    large.flow_rate,
                    None,
                )
                .map_err(protocol_error)?;
        }
    }
    builder
        .drop_tip_into_trash(large.id)
        .map_err(protocol_error)?;

    let mut source = (cultures, culture_well.clone());
    for destination in dilution_wells {
        let target = (dilution_plates[destination.plate], destination.well.clone());
        transfer(
            &mut builder,
            &mut small_tips,
            &small,
            (source.0, &source.1),
            (target.0, &target.1),
            f64::from(*culture_volume_ul),
            Some((*mix_cycles, f64::from(*mix_volume_ul))),
        )?;
        source = target;
    }
    builder.comment("Allocated dilution complete. This protocol performs no plating; preserve the final dilution for its downstream Procedure task.");
    render(builder)
}

fn render_manual(plan: &FlexTaskPlan) -> Doc {
    let title = match &plan.execution {
        FlexTaskExecution::SetupGoldenGateReaction { artifact, .. } => {
            format!("Set up Golden Gate reaction for {artifact}")
        }
        FlexTaskExecution::ThermalCycleGoldenGateReaction { artifact, .. } => {
            format!("Thermal cycle Golden Gate reaction for {artifact}")
        }
        FlexTaskExecution::SerialDilution { .. } => "Serially dilute recovered culture".to_owned(),
    };
    let mut doc = Doc::new(DocMeta::new(
        title,
        "Operator instructions for one facility-allocated Procedure requirement",
        &plan.adapter_profile,
        "Opentrons Flex",
    ));
    doc.notice([
        bold("Allocation boundary. "),
        text("This Protocol Designer JSON implements only requirement "),
        code(plan.requirement.id.as_str()),
        text(" on the exact Asset below. Adjacent Procedure work remains in separate reviewed plan nodes."),
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
    doc.heading(1, [text("Run this task")]);
    doc.para([
        text("Import "),
        code("automation_protocol.json"),
        text(" into the Opentrons App, inspect its generated deck map and commands, and confirm that the staged material identities match this reviewed task manifest."),
    ]);
    match &plan.execution {
        FlexTaskExecution::SetupGoldenGateReaction {
            reaction_wells,
            additions,
            ..
        } => {
            doc.para_text(format!(
                "Load the exact sources below and preserve reaction wells {} for the separate thermal-cycling node.",
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
        FlexTaskExecution::ThermalCycleGoldenGateReaction { reaction_wells, .. } => {
            doc.para_text(format!(
                "Confirm that the upstream setup task left only this reaction in wells {}. Remove and label it after the run.",
                reaction_wells.join(", ")
            ));
        }
        FlexTaskExecution::SerialDilution {
            culture_source,
            culture_well,
            medium,
            dilution_wells,
            ..
        } => {
            doc.para_text(format!(
                "Stage {} in thermocycler well {culture_well}; load {} from {} in media-rack well {}; preserve dilution wells {} for the downstream task. No agar plate is used.",
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

fn transfer(
    builder: &mut FlexProtocolBuilder,
    tips: &mut TipFeeder,
    instrument: &Instrument,
    source: (LabwareId, &str),
    destination: (LabwareId, &str),
    volume: f64,
    mix_after: Option<(u32, f64)>,
) -> Result<(), String> {
    let (rack, well) = tips.next();
    builder
        .pick_up_tip(instrument.id, rack, &well)
        .map_err(protocol_error)?;
    let working = instrument.max_volume.min(tips.tip_volume);
    let chunks = (volume / working).ceil().max(1.0);
    let chunk_volume = volume / chunks;
    for _ in 0..chunks as usize {
        builder
            .aspirate(
                instrument.id,
                source.0,
                source.1,
                chunk_volume,
                instrument.flow_rate,
                None,
            )
            .map_err(protocol_error)?;
        builder
            .dispense(
                instrument.id,
                destination.0,
                destination.1,
                chunk_volume,
                instrument.flow_rate,
                None,
            )
            .map_err(protocol_error)?;
    }
    if let Some((repetitions, mix_volume)) = mix_after {
        builder
            .mix(
                instrument.id,
                destination.0,
                destination.1,
                repetitions,
                mix_volume,
                instrument.flow_rate,
            )
            .map_err(protocol_error)?;
    }
    builder
        .drop_tip_into_trash(instrument.id)
        .map_err(protocol_error)
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
    task: &AllocatedProcedureTask,
    resource: &str,
    capacity: usize,
) -> Result<Vec<String>, String> {
    let wells = plate_wells(capacity);
    if wells.is_empty() {
        Err(format!(
            "Flex Procedure task '{}' cannot address {resource} with declared capacity {capacity}",
            task.id
        ))
    } else {
        Ok(wells)
    }
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
