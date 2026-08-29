//! Requirement-scoped lowering from exact facility allocations to standalone OT-2 protocols.

use std::collections::BTreeMap;

use lab_method::LocalId;
use lab_runfmt::OPENTRONS_PYTHON_PROTOCOL_FORMAT;
use serde::Serialize;

use crate::backend::adapters::{AdapterInvocationDocument, AdapterInvocationLowering};
use crate::backend::document::{Column, Doc, DocMeta, bold, code, text};
use crate::backend::invocation::{
    DEGREE_CELSIUS, MICROLITRE, MINUTE, ProcedureTaskView, exact_invocation_tasks, material_symbols,
};
use crate::backend::opentrons::ot2::BACKEND;
use crate::backend::opentrons::ot2::emit::python_string_expression;
use crate::backend::opentrons::ot2::profile::Ot2AdapterProfile;
use crate::backend::resources::{PlateAllocator, Well, assign_source_wells, plate_wells};
use crate::backend::typst;
use crate::planning::{
    AdapterInvocation, AdapterInvocationPlan, AllocatedProcedureTask, AllocatedRequirementBinding,
    PlanningProcedureParameter, PlanningTaskInput, PlanningTaskOutput, PlanningValueSource,
    SelectedCapabilityParameter, SelectedMaterialBinding, SelectedMaterialSource,
};
use crate::{ArtifactBundle, GeneratedArtifact};

const SETUP_GOLDEN_GATE: &str = "https://www.lab-compiler.org/ns/procedure#SetupGoldenGateReaction";
const CYCLE_GOLDEN_GATE: &str =
    "https://www.lab-compiler.org/ns/procedure#ThermalCycleGoldenGateReaction";
const SERIAL_DILUTION: &str = "https://www.lab-compiler.org/ns/procedure#SeriallyDiluteCulture";
const LIQUID_HANDLING: &str = "https://sbol.io/ns/capability#LiquidHandling";
const THERMAL_CYCLING: &str = "https://sbol.io/ns/capability#ThermalCycling";
const TASK_PLAN_SCHEMA: &str = "lab.opentrons-ot2-task.v1";

const SETUP_TEMPLATE: &str = include_str!("invocation/setup_reaction.py");
const CYCLE_TEMPLATE: &str = include_str!("invocation/thermal_cycle.py");
const DILUTION_TEMPLATE: &str = include_str!("invocation/serial_dilution.py");
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
    requirement: RequirementReview,
    task: TaskReview,
    deck: Ot2AdapterProfile,
    execution: Ot2TaskExecution,
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
enum Ot2TaskExecution {
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

/// Lower only the Procedure tasks and requirements allocated to this exact invocation.
pub(in crate::backend) fn lower_invocation(
    profile: &Ot2AdapterProfile,
    invocation_plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
) -> Result<AdapterInvocationLowering, String> {
    let tasks = exact_invocation_tasks("OT-2", invocation_plan, invocation)?;
    let mut artifacts = ArtifactBundle::new();
    let mut documents = Vec::new();

    for (ordinal, member) in tasks.into_iter().enumerate() {
        let (slug, execution) = plan_task(profile, member.task, member.requirement)?;
        let directory = format!("tasks/{:03}-{slug}", ordinal + 1);
        let plan = Ot2TaskPlan {
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
            requirement: member.requirement.id.clone(),
            path: protocol_path,
            format: OPENTRONS_PYTHON_PROTOCOL_FORMAT.to_owned(),
        });
    }

    Ok(AdapterInvocationLowering {
        artifacts,
        documents,
    })
}

fn plan_task(
    profile: &Ot2AdapterProfile,
    task: &AllocatedProcedureTask,
    requirement: &AllocatedRequirementBinding,
) -> Result<(&'static str, Ot2TaskExecution), String> {
    let view = ProcedureTaskView::new("OT-2", task);
    match task.operation.as_str() {
        SETUP_GOLDEN_GATE => {
            view.require_capability(requirement, LIQUID_HANDLING)?;
            Ok(("setup-golden-gate-reaction", plan_setup(profile, task)?))
        }
        CYCLE_GOLDEN_GATE => {
            view.require_capability(requirement, THERMAL_CYCLING)?;
            Ok((
                "thermal-cycle-golden-gate-reaction",
                plan_cycle(profile, task)?,
            ))
        }
        SERIAL_DILUTION => {
            view.require_capability(requirement, LIQUID_HANDLING)?;
            Ok(("serial-dilution", plan_dilution(profile, task)?))
        }
        operation => Err(format!(
            "OT-2 invocation contains unsupported Procedure operation '{operation}' in task '{}'",
            task.id
        )),
    }
}

fn plan_setup(
    profile: &Ot2AdapterProfile,
    task: &AllocatedProcedureTask,
) -> Result<Ot2TaskExecution, String> {
    let view = ProcedureTaskView::new("OT-2", task);
    view.require_material_roles(&[
        "backbone",
        "components",
        "dependencies",
        "restriction-enzyme",
        "ligase",
        "buffer",
        "water",
    ])?;
    let artifact = view.text_parameter("artifact")?;
    let backbone = view.text_parameter("backbone")?;
    let components = view.text_list_parameter("components")?;
    let dependencies = view.text_list_parameter("dependencies")?;
    let restriction_enzyme = view.text_parameter("restriction_enzyme")?;
    let replicates = view.usize_parameter("assembly_replicates", None)?;
    view.require_nonzero("assembly_replicates", replicates as u32)?;
    let reaction_volume_ul = view.integer_parameter("reaction_volume_ul", Some(MICROLITRE))?;
    let part_volume_ul = view.integer_parameter("part_volume_ul", Some(MICROLITRE))?;
    let enzyme_volume_ul = view.integer_parameter("enzyme_volume_ul", Some(MICROLITRE))?;
    let ligase_volume_ul = view.integer_parameter("ligase_volume_ul", Some(MICROLITRE))?;
    let buffer_volume_ul = view.integer_parameter("buffer_volume_ul", Some(MICROLITRE))?;
    let mix_cycles = view.integer_parameter("mix_cycles", None)?;
    let mix_volume_ul = view.integer_parameter("mix_volume_ul", Some(MICROLITRE))?;
    for (name, value) in [
        ("reaction_volume_ul", reaction_volume_ul),
        ("part_volume_ul", part_volume_ul),
        ("enzyme_volume_ul", enzyme_volume_ul),
        ("ligase_volume_ul", ligase_volume_ul),
        ("buffer_volume_ul", buffer_volume_ul),
        ("mix_cycles", mix_cycles),
        ("mix_volume_ul", mix_volume_ul),
    ] {
        view.require_nonzero(name, value)?;
    }
    if mix_volume_ul > reaction_volume_ul {
        return Err(format!(
            "OT-2 Procedure task '{}' mix volume {} uL exceeds its {} uL reaction",
            task.id, mix_volume_ul, reaction_volume_ul
        ));
    }

    let backbone_material = view.one_material("backbone")?;
    if backbone_material.symbol != backbone {
        return Err(view.material_parameter_mismatch("backbone"));
    }
    let component_materials = view.materials("components");
    if material_symbols(&component_materials) != components {
        return Err(view.material_parameter_mismatch("components"));
    }
    let dependency_materials = view.materials("dependencies");
    if material_symbols(&dependency_materials) != dependencies {
        return Err(view.material_parameter_mismatch("dependencies"));
    }
    let enzyme_material = view.one_material("restriction-enzyme")?;
    if enzyme_material.symbol != restriction_enzyme {
        return Err(view.material_parameter_mismatch("restriction_enzyme"));
    }
    let ligase_material = view.one_material("ligase")?;
    let buffer_material = view.one_material("buffer")?;
    let water_material = view.one_material("water")?;

    let dna_piece_count = u32::try_from(component_materials.len() + 1)
        .map_err(|_| format!("OT-2 Procedure task '{}' has too many DNA pieces", task.id))?;
    let consumed = buffer_volume_ul
        .checked_add(ligase_volume_ul)
        .and_then(|value| value.checked_add(enzyme_volume_ul))
        .and_then(|value| value.checked_add(part_volume_ul.checked_mul(dna_piece_count)?))
        .ok_or_else(|| {
            format!(
                "OT-2 Procedure task '{}' reaction volume overflows",
                task.id
            )
        })?;
    let water_volume_ul = reaction_volume_ul.checked_sub(consumed).ok_or_else(|| {
        format!(
            "OT-2 Procedure task '{}' requires {consumed} uL before water in a {reaction_volume_ul} uL reaction",
            task.id
        )
    })?;

    let mut additions = vec![
        ("water", water_material, water_volume_ul),
        ("buffer", buffer_material, buffer_volume_ul),
        ("ligase", ligase_material, ligase_volume_ul),
        ("restriction-enzyme", enzyme_material, enzyme_volume_ul),
        ("backbone", backbone_material, part_volume_ul),
    ];
    additions.extend(
        component_materials
            .iter()
            .copied()
            .map(|material| ("component", material, part_volume_ul)),
    );

    let mut sources = BTreeMap::<String, &SelectedMaterialSource>::new();
    for (_, material, _) in &additions {
        if let Some(previous) = sources.insert(material.symbol.clone(), &material.source)
            && previous != &material.source
        {
            return Err(format!(
                "OT-2 Procedure task '{}' assigns material '{}' to several physical sources",
                task.id, material.symbol
            ));
        }
    }
    let source_wells = assign_source_wells(
        BACKEND,
        "setup-golden-gate-reaction",
        sources.keys().cloned().collect(),
        profile.deck.temperature_module.capacity,
    )
    .map_err(|error| error.to_string())?;
    let additions = additions
        .into_iter()
        .map(|(role, material, volume_ul)| MaterialAddition {
            placement: MaterialPlacement {
                role: role.to_owned(),
                input: material.input.clone(),
                symbol: material.symbol.clone(),
                source: material.source.clone(),
                source_well: source_wells[&material.symbol].clone(),
            },
            volume_ul,
        })
        .collect::<Vec<_>>();

    let reaction_plate = known_wells(task, "reaction plate", profile.deck.thermocycler.capacity)?;
    if replicates > reaction_plate.len() {
        return Err(view.capacity_error("reaction plate", replicates, reaction_plate.len()));
    }
    let tips_per_reaction = additions
        .iter()
        .filter(|addition| addition.volume_ul > 0)
        .count()
        + 1;
    let required_tips = tips_per_reaction
        .checked_mul(replicates)
        .ok_or_else(|| format!("OT-2 Procedure task '{}' tip count overflows", task.id))?;
    let tip_capacity = profile.stages.assembly.small_tips.total_capacity();
    if required_tips > tip_capacity {
        return Err(view.capacity_error("assembly small-tip racks", required_tips, tip_capacity));
    }

    Ok(Ot2TaskExecution::SetupGoldenGateReaction {
        artifact,
        reaction_wells: reaction_plate.into_iter().take(replicates).collect(),
        additions,
        reaction_volume_ul,
        mix_cycles,
        mix_volume_ul,
    })
}

fn plan_cycle(
    profile: &Ot2AdapterProfile,
    task: &AllocatedProcedureTask,
) -> Result<Ot2TaskExecution, String> {
    let view = ProcedureTaskView::new("OT-2", task);
    view.require_material_roles(&[])?;
    let artifact = view.text_parameter("artifact")?;
    let replicates = view.usize_parameter("assembly_replicates", None)?;
    view.require_nonzero("assembly_replicates", replicates as u32)?;
    let reaction_wells = known_wells(task, "reaction plate", profile.deck.thermocycler.capacity)?;
    if replicates > reaction_wells.len() {
        return Err(view.capacity_error("reaction plate", replicates, reaction_wells.len()));
    }
    let reaction_volume_ul = view.integer_parameter("reaction_volume_ul", Some(MICROLITRE))?;
    let cycles = view.integer_parameter("cycles", None)?;
    let digest_temperature_c =
        view.integer_parameter("digest_temperature_c", Some(DEGREE_CELSIUS))?;
    let digest_minutes = view.integer_parameter("digest_minutes", Some(MINUTE))?;
    let ligate_temperature_c =
        view.integer_parameter("ligate_temperature_c", Some(DEGREE_CELSIUS))?;
    let ligate_minutes = view.integer_parameter("ligate_minutes", Some(MINUTE))?;
    let lid_temperature_c = view.integer_parameter("lid_temperature_c", Some(DEGREE_CELSIUS))?;
    let final_digest_temperature_c =
        view.integer_parameter("final_digest_temperature_c", Some(DEGREE_CELSIUS))?;
    let final_digest_minutes = view.integer_parameter("final_digest_minutes", Some(MINUTE))?;
    let heat_inactivation_temperature_c =
        view.integer_parameter("heat_inactivation_temperature_c", Some(DEGREE_CELSIUS))?;
    let heat_inactivation_minutes =
        view.integer_parameter("heat_inactivation_minutes", Some(MINUTE))?;
    let hold_temperature_c = view.integer_parameter("hold_temperature_c", Some(DEGREE_CELSIUS))?;
    for (name, value) in [
        ("reaction_volume_ul", reaction_volume_ul),
        ("cycles", cycles),
        ("digest_minutes", digest_minutes),
        ("ligate_minutes", ligate_minutes),
        ("lid_temperature_c", lid_temperature_c),
        ("final_digest_minutes", final_digest_minutes),
        ("heat_inactivation_minutes", heat_inactivation_minutes),
    ] {
        view.require_nonzero(name, value)?;
    }
    for (name, value) in [
        ("digest_temperature_c", digest_temperature_c),
        ("ligate_temperature_c", ligate_temperature_c),
        ("lid_temperature_c", lid_temperature_c),
        ("final_digest_temperature_c", final_digest_temperature_c),
        (
            "heat_inactivation_temperature_c",
            heat_inactivation_temperature_c,
        ),
        ("hold_temperature_c", hold_temperature_c),
    ] {
        if value > 110 {
            return Err(format!(
                "OT-2 Procedure task '{}' parameter '{name}' is {value} °C, above the adapter's 110 °C limit",
                task.id
            ));
        }
    }

    Ok(Ot2TaskExecution::ThermalCycleGoldenGateReaction {
        artifact,
        reaction_wells: reaction_wells.into_iter().take(replicates).collect(),
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
    })
}

fn plan_dilution(
    profile: &Ot2AdapterProfile,
    task: &AllocatedProcedureTask,
) -> Result<Ot2TaskExecution, String> {
    let view = ProcedureTaskView::new("OT-2", task);
    view.require_material_roles(&["medium"])?;
    if task.inputs.len() != 1 {
        return Err(format!(
            "OT-2 serial-dilution task '{}' must have exactly one culture input",
            task.id
        ));
    }
    let serial_dilutions = view.usize_parameter("serial_dilutions", None)?;
    view.require_nonzero("serial_dilutions", serial_dilutions as u32)?;
    let medium_volume_ul = view.integer_parameter("medium_volume_ul", Some(MICROLITRE))?;
    let culture_volume_ul = view.integer_parameter("culture_volume_ul", Some(MICROLITRE))?;
    let mix_cycles = view.integer_parameter("mix_cycles", None)?;
    let mix_volume_ul = view.integer_parameter("mix_volume_ul", Some(MICROLITRE))?;
    for (name, value) in [
        ("medium_volume_ul", medium_volume_ul),
        ("culture_volume_ul", culture_volume_ul),
        ("mix_cycles", mix_cycles),
        ("mix_volume_ul", mix_volume_ul),
    ] {
        view.require_nonzero(name, value)?;
    }
    let diluted_volume = medium_volume_ul
        .checked_add(culture_volume_ul)
        .ok_or_else(|| {
            format!(
                "OT-2 Procedure task '{}' dilution volume overflows",
                task.id
            )
        })?;
    if mix_volume_ul > diluted_volume {
        return Err(format!(
            "OT-2 Procedure task '{}' mix volume {} uL exceeds its {} uL dilution",
            task.id, mix_volume_ul, diluted_volume
        ));
    }

    let medium = view.one_material("medium")?;
    let mut allocator = PlateAllocator::new(
        BACKEND,
        "serial-dilution",
        "dilution_plate",
        &profile.stages.plating.dilution_plate,
    );
    let dilution_wells = allocator
        .take(serial_dilutions)
        .map_err(|error| error.to_string())?;
    let culture_wells = known_wells(
        task,
        "culture staging plate",
        profile.deck.thermocycler.capacity,
    )?;
    if serial_dilutions > profile.stages.plating.small_tips.total_capacity() {
        return Err(view.capacity_error(
            "dilution small-tip racks",
            serial_dilutions,
            profile.stages.plating.small_tips.total_capacity(),
        ));
    }
    if profile.stages.plating.large_tips.total_capacity() == 0 {
        return Err(view.capacity_error("dilution large-tip racks", 1, 0));
    }

    Ok(Ot2TaskExecution::SerialDilution {
        culture_source: task.inputs[0].source.clone(),
        culture_well: culture_wells[0].clone(),
        medium: MaterialPlacement {
            role: "medium".to_owned(),
            input: medium.input.clone(),
            symbol: medium.symbol.clone(),
            source: medium.source.clone(),
            source_well: profile.stages.plating.media_rack.medium_well.clone(),
        },
        dilution_wells,
        medium_volume_ul,
        culture_volume_ul,
        mix_cycles,
        mix_volume_ul,
    })
}

fn render_python_protocol(plan: &Ot2TaskPlan) -> Result<String, String> {
    let template = match &plan.execution {
        Ot2TaskExecution::SetupGoldenGateReaction { .. } => SETUP_TEMPLATE,
        Ot2TaskExecution::ThermalCycleGoldenGateReaction { .. } => CYCLE_TEMPLATE,
        Ot2TaskExecution::SerialDilution { .. } => DILUTION_TEMPLATE,
    };
    let api_level =
        serde_json::to_string(&plan.deck.protocol.api_level).map_err(|error| error.to_string())?;
    let output = replace_once(
        template,
        API_LEVEL_SENTINEL,
        &format!("{api_level},  # LAB:API_LEVEL"),
    )?;
    let plan_json = serde_json::to_string(plan).map_err(|error| error.to_string())?;
    let plan_literal = python_string_expression(&plan_json).map_err(|error| error.to_string())?;
    replace_once(
        &output,
        PLAN_SENTINEL,
        &format!("{plan_literal}  # LAB:INVOCATION_PLAN"),
    )
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
        Ot2TaskExecution::ThermalCycleGoldenGateReaction { artifact, .. } => {
            format!("Thermal cycle Golden Gate reaction for {artifact}")
        }
        Ot2TaskExecution::SerialDilution { .. } => "Serially dilute recovered culture".to_owned(),
    };
    let mut doc = Doc::new(DocMeta::new(
        title,
        "Operator instructions for one facility-allocated Procedure requirement",
        &plan.adapter_profile,
        "Opentrons OT-2",
    ));
    doc.notice([
        bold("Allocation boundary. "),
        text("This protocol implements only requirement "),
        code(plan.requirement.id.as_str()),
        text(" on the exact Asset below. Upstream and downstream Procedure tasks remain separate reviewed plan nodes."),
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
                        vec![text(&addition.placement.role)],
                        vec![code(&addition.placement.symbol)],
                        vec![code(material_source(&addition.placement.source))],
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
        Ot2TaskExecution::ThermalCycleGoldenGateReaction {
            reaction_wells,
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
        } => {
            doc.heading(1, [text("Stage the allocated reaction")]);
            doc.para([
                text("Confirm that the upstream setup task left only this reaction in wells "),
                code(reaction_wells.join(", ")),
                text(" of the configured thermocycler plate."),
            ]);
            doc.heading(1, [text("Review the thermal program")]);
            doc.table(
                [
                    Column::left("Step"),
                    Column::right("Temperature"),
                    Column::right("Duration / repeats"),
                ],
                [
                    vec![
                        vec![text("Lid")],
                        vec![text(format!("{lid_temperature_c} °C"))],
                        vec![text("during cycling")],
                    ],
                    vec![
                        vec![text("Digest")],
                        vec![text(format!("{digest_temperature_c} °C"))],
                        vec![text(format!("{digest_minutes} min × {cycles}"))],
                    ],
                    vec![
                        vec![text("Ligate")],
                        vec![text(format!("{ligate_temperature_c} °C"))],
                        vec![text(format!("{ligate_minutes} min × {cycles}"))],
                    ],
                    vec![
                        vec![text("Final digest")],
                        vec![text(format!("{final_digest_temperature_c} °C"))],
                        vec![text(format!("{final_digest_minutes} min"))],
                    ],
                    vec![
                        vec![text("Heat inactivation")],
                        vec![text(format!("{heat_inactivation_temperature_c} °C"))],
                        vec![text(format!("{heat_inactivation_minutes} min"))],
                    ],
                    vec![
                        vec![text("Hold")],
                        vec![text(format!("{hold_temperature_c} °C"))],
                        vec![text("until recovery")],
                    ],
                ],
            );
            doc.para([
                text("Import "),
                code("automation_protocol.py"),
                text(format!(
                    " and verify a {reaction_volume_ul} µL block volume before starting."
                )),
            ]);
            doc.para_text("Remove and label the completed reaction before another independently allocated setup/cycle pair reuses the staging wells.");
        }
        Ot2TaskExecution::SerialDilution {
            culture_source,
            culture_well,
            medium,
            dilution_wells,
            medium_volume_ul,
            culture_volume_ul,
            mix_cycles,
            mix_volume_ul,
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
                        vec![code(format!("thermocycler plate {culture_well}"))],
                    ],
                    vec![
                        vec![code(&medium.symbol)],
                        vec![code(material_source(&medium.source))],
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
    }
    doc
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
