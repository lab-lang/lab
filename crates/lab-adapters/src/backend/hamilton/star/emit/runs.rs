//! `lab.star-run.v0` rendering: each run becomes one JSON document of
//! ordered, id-less firmware frames built by the driver crate's validated
//! encoders, each annotated for the operator. What a reviewer approves here
//! is byte-for-byte what `lab run` sends; the session adds only command
//! ids.

use hamilton_star::catalog::TipType;
use hamilton_star::commands::Command;
use hamilton_star::commands::pipetting::{
    Aspirate, AspirateChannel, ChannelTarget, Dispense, DispenseChannel, DispenseMode, LldMode,
    TipDiscard, TipDiscardMethod, TipPickup,
};
use hamilton_star::commands::system::{DefineTipType, MoveAllChannelsToZSafety};

use crate::backend::hamilton::star::plan::{
    ChannelLiquid, StarEmissionError, StarExecutionPlan, StarOperation, StarRunPlan, TipClass,
};
use crate::backend::hamilton::star::profile::{LldPolicy, MachineVariant};

pub use lab_runfmt::RunStep;

/// The tip-type indices a run document defines: the small class is index 0
/// and the large class index 1, re-defined at every run start because the
/// firmware's tip table is volatile.
const SMALL_TIP_INDEX: u32 = 0;
const LARGE_TIP_INDEX: u32 = 1;

/// The tip-waste X per machine variant, millimeters: 25 mm right of the
/// second-to-last rail, where the waste chute sits on a deck without a
/// waste block.
fn tip_waste_x(variant: MachineVariant) -> f64 {
    let last_rail = f64::from(variant.rails() - 1);
    100.0 + (last_rail - 1.0) * 22.5 + 25.0
}

/// The waste Y spread: eight channels fanned from 405.0 mm down to
/// 217.5 mm over the chute.
fn waste_y(channel: usize) -> u32 {
    4050 - (channel as u32) * (4050 - 2175) / 7
}

/// Renders one run to its `lab.star-run.v0` JSON document.
pub(in crate::backend::hamilton::star) fn render_run(
    plan: &StarExecutionPlan,
    run: &StarRunPlan,
) -> Result<String, StarEmissionError> {
    let document = lab_runfmt::StarRunDocument {
        format: lab_runfmt::STAR_RUN_FORMAT.to_string(),
        run: run.id.clone(),
        title: run.title.clone(),
        machine: plan.deck.machine.variant.name().to_string(),
        channels: plan.deck.machine.channels,
        steps: run_steps(plan, run)?,
        manual_after: run.manual_after.clone(),
    };
    serde_json::to_string_pretty(&document)
        .map(|mut text| {
            text.push('\n');
            text
        })
        .map_err(|error| StarEmissionError::Serialization(error.to_string()))
}

/// The full step list: tip-type definitions, an opening retract, the
/// lowered operations, and a closing retract.
pub(in crate::backend::hamilton::star) fn run_steps(
    plan: &StarExecutionPlan,
    run: &StarRunPlan,
) -> Result<Vec<RunStep>, StarEmissionError> {
    let mut steps = Vec::new();
    for (class, index) in [
        (TipClass::Small, SMALL_TIP_INDEX),
        (TipClass::Large, LARGE_TIP_INDEX),
    ] {
        if let Some(tip) = run_tip(plan, run, class) {
            let define = DefineTipType::new(
                index,
                tip.has_filter,
                tip.wire_length(),
                tip.wire_volume(),
                tip.size,
                tip.pickup_method,
            )?;
            steps.push(step(
                &define,
                format!(
                    "define the {} tip ({:.1} mm, {:.0} µL{}) as firmware tip type {index}",
                    class_name(class),
                    tip.total_length.0,
                    tip.max_volume,
                    if tip.has_filter { ", filtered" } else { "" },
                ),
            ));
        }
    }
    steps.push(step(
        &MoveAllChannelsToZSafety,
        "retract all channels to Z-safety before any motion".to_string(),
    ));
    for operation in &run.operations {
        steps.push(operation_step(plan, operation)?);
    }
    steps.push(step(
        &MoveAllChannelsToZSafety,
        "retract all channels to Z-safety to finish the run".to_string(),
    ));
    Ok(steps)
}

/// The tip the run's operations of a class use, from the profile's stage
/// racks (every rack of a class feeds one tip type).
fn run_tip(plan: &StarExecutionPlan, run: &StarRunPlan, class: TipClass) -> Option<TipType> {
    let uses_class = run.operations.iter().any(|operation| match operation {
        StarOperation::PickUpTips { tip, .. } => *tip == class,
        _ => false,
    });
    if !uses_class {
        return None;
    }
    let rack_resource = run
        .operations
        .iter()
        .find_map(|operation| match operation {
            StarOperation::PickUpTips { tip, positions, .. } if *tip == class => positions
                .first()
                .map(|position| position.location.resource.clone()),
            _ => None,
        })?;
    let prefix = rack_resource.split('/').next()?;
    let labware = match prefix {
        "assembly_small_tips" => &plan.deck.stages.assembly.small_tips.labware,
        "transformation_small_tips" => &plan.deck.stages.transformation.small_tips.labware,
        "transformation_large_tips" => &plan.deck.stages.transformation.large_tips.labware,
        "plating_small_tips" => &plan.deck.stages.plating.small_tips.labware,
        "plating_large_tips" => &plan.deck.stages.plating.large_tips.labware,
        _ => return None,
    };
    crate::backend::hamilton::star::catalog::labware(labware)?.tip()
}

fn class_name(class: TipClass) -> &'static str {
    match class {
        TipClass::Small => "small",
        TipClass::Large => "large",
    }
}

fn tip_index(class: TipClass) -> u32 {
    match class {
        TipClass::Small => SMALL_TIP_INDEX,
        TipClass::Large => LARGE_TIP_INDEX,
    }
}

fn step<C: Command>(command: &C, description: String) -> RunStep {
    let frame = command.to_wire(None);
    RunStep {
        module: frame[..2].to_string(),
        code: frame[2..4].to_string(),
        frame,
        description,
    }
}

fn operation_step(
    plan: &StarExecutionPlan,
    operation: &StarOperation,
) -> Result<RunStep, StarEmissionError> {
    let profile = &plan.deck;
    let machine_channels = profile.machine.channels;
    let traverse = (profile.run.traverse_height_mm * 10.0).round() as u32;
    match operation {
        StarOperation::PickUpTips {
            tip,
            begin_z,
            end_z,
            positions,
        } => {
            let targets: Vec<ChannelTarget> = positions
                .iter()
                .map(|position| ChannelTarget {
                    channel: position.channel,
                    x: position.x,
                    y: position.y,
                })
                .collect();
            let tip_type = run_pickup_tip(plan, positions)?;
            let command = TipPickup::new(
                &targets,
                machine_channels,
                tip_index(*tip),
                *begin_z,
                *end_z,
                traverse,
                tip_type.pickup_method,
            )?;
            let wells = positions
                .iter()
                .map(|position| {
                    format!("{} {}", position.location.resource, position.location.well)
                })
                .collect::<Vec<_>>()
                .join(", ");
            Ok(step(
                &command,
                format!(
                    "pick up {} {} tip{} from {wells}",
                    positions.len(),
                    class_name(*tip),
                    if positions.len() == 1 { "" } else { "s" },
                ),
            ))
        }
        StarOperation::Aspirate { channels, .. } => {
            let lowered: Vec<AspirateChannel> = channels
                .iter()
                .map(|liquid| {
                    let mut channel = AspirateChannel::at(liquid.channel, liquid.x, liquid.y);
                    channel.lld_search_height = liquid.lld_search_z;
                    channel.liquid_surface = liquid.position_z;
                    channel.minimum_height = liquid.minimum_z;
                    channel.volume = liquid.corrected_volume;
                    channel.mix_volume = liquid.mix_volume;
                    channel.mix_cycles = liquid.mix_cycles;
                    channel.lld_mode = lld_mode(profile.run.lld);
                    channel
                })
                .collect();
            let command = Aspirate::new(lowered, machine_channels, traverse, traverse)?;
            Ok(step(
                &command,
                describe_liquid("aspirate", channels, "from"),
            ))
        }
        StarOperation::Dispense { mode, channels, .. } => {
            let lowered: Vec<DispenseChannel> = channels
                .iter()
                .map(|liquid| {
                    let mut channel = DispenseChannel::at(liquid.channel, liquid.x, liquid.y);
                    channel.mode = dispense_mode(*mode);
                    channel.lld_search_height = liquid.lld_search_z;
                    channel.liquid_surface = liquid.position_z;
                    channel.minimum_height = liquid.minimum_z;
                    channel.volume = liquid.corrected_volume;
                    channel.mix_volume = liquid.mix_volume;
                    channel.mix_cycles = liquid.mix_cycles;
                    channel
                })
                .collect();
            let command = Dispense::new(lowered, machine_channels, traverse, traverse, 0)?;
            let verb = match mode {
                1 => "dispense (blow-out jet)",
                _ => "dispense (partial jet)",
            };
            Ok(step(&command, describe_liquid(verb, channels, "into")))
        }
        StarOperation::DiscardTips { channels } => {
            let variant = profile.machine.variant;
            let waste_x = (tip_waste_x(variant) * 10.0).round() as u32;
            let targets: Vec<ChannelTarget> = channels
                .iter()
                .map(|&channel| ChannelTarget {
                    channel,
                    x: waste_x,
                    y: waste_y(channel),
                })
                .collect();
            // The working deposit window over the waste chute: drop from
            // 245.0 down to 122.0 mm, no surface to reference.
            let command = TipDiscard::new(
                &targets,
                machine_channels,
                2450,
                1220,
                traverse,
                traverse,
                TipDiscardMethod::Drop,
            )?;
            Ok(step(
                &command,
                format!(
                    "drop {} tip{} into the tip waste",
                    channels.len(),
                    if channels.len() == 1 { "" } else { "s" },
                ),
            ))
        }
    }
}

/// The driver tip type behind a pickup's rack resource, for its pickup
/// method.
fn run_pickup_tip(
    plan: &StarExecutionPlan,
    positions: &[crate::backend::hamilton::star::plan::TipPickupPosition],
) -> Result<TipType, StarEmissionError> {
    let resource = &positions
        .first()
        .expect("a pickup operation always lists at least one position")
        .location
        .resource;
    let prefix = resource.split('/').next().unwrap_or(resource);
    let labware = match prefix {
        "assembly_small_tips" => &plan.deck.stages.assembly.small_tips.labware,
        "transformation_small_tips" => &plan.deck.stages.transformation.small_tips.labware,
        "transformation_large_tips" => &plan.deck.stages.transformation.large_tips.labware,
        "plating_small_tips" => &plan.deck.stages.plating.small_tips.labware,
        "plating_large_tips" => &plan.deck.stages.plating.large_tips.labware,
        other => {
            return Err(StarEmissionError::Serialization(format!(
                "operation references unknown tip resource '{other}'"
            )));
        }
    };
    crate::backend::hamilton::star::catalog::labware(labware)
        .and_then(|labware| labware.tip())
        .ok_or_else(|| {
            StarEmissionError::Serialization(format!(
                "tip resource '{resource}' does not resolve to a catalog tip rack"
            ))
        })
}

fn lld_mode(policy: LldPolicy) -> LldMode {
    match policy {
        LldPolicy::Off => LldMode::Off,
        LldPolicy::Gamma => LldMode::Gamma,
    }
}

fn dispense_mode(mode: u32) -> DispenseMode {
    match mode {
        1 => DispenseMode::BlowOutJet,
        _ => DispenseMode::PartialJet,
    }
}

fn describe_liquid(verb: &str, channels: &[ChannelLiquid], preposition: &str) -> String {
    if channels.len() == 1 {
        let liquid = &channels[0];
        let mix = if liquid.mix_cycles > 0 {
            format!(
                ", then mix {} × {:.1} µL",
                liquid.mix_cycles,
                f64::from(liquid.mix_volume) / 10.0
            )
        } else {
            String::new()
        };
        let volume = if liquid.corrected_volume > 0 {
            format!(
                "{:.1} µL (for {:.1} µL requested) ",
                f64::from(liquid.corrected_volume) / 10.0,
                liquid.target_ul,
            )
        } else {
            String::new()
        };
        format!(
            "{verb} {volume}{preposition} {} {} on channel {}{mix}",
            liquid.location.resource,
            liquid.location.well,
            liquid.channel + 1,
        )
    } else {
        let first = &channels[0];
        let last = &channels[channels.len() - 1];
        format!(
            "{verb} {preposition} {} {}–{} across {} channels",
            first.location.resource,
            first.location.well,
            last.location.well,
            channels.len(),
        )
    }
}
