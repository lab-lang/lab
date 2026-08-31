//! Resource planning for one exact facility-allocated STAR invocation.
//!
//! These planners deliberately know nothing about the rest of the experiment. They receive one
//! typed Procedure operation, allocate only the wells and tips needed by that operation, and
//! produce the single STAR run that implements its exact capability requirement.

use std::collections::BTreeMap;

use crate::backend::AdapterConstraintError;
use crate::backend::hamilton::star::BACKEND;
use crate::backend::hamilton::star::plan::choreograph::{RunBuilder, TipFeeder, Transfer};
use crate::backend::hamilton::star::plan::error::StarPlanningError;
use crate::backend::hamilton::star::plan::execution::{
    SourceFill, StarExecutionPlan, StarRunPlan, StarWell, TipClass,
};
use crate::backend::hamilton::star::plan::liquids::{
    DeckIndex, LiquidState, PLATE_DEAD_VOLUME_UL, TROUGH_DEAD_VOLUME_UL, TUBE_DEAD_VOLUME_UL,
};
use crate::backend::hamilton::star::profile::StarAdapterProfile;
use crate::backend::resources::Well;

/// One material addition to every allocated reaction well.
pub(in crate::backend::hamilton::star) struct SetupAddition {
    pub(in crate::backend::hamilton::star) symbol: String,
    pub(in crate::backend::hamilton::star) volume_ul: f64,
}

/// Plan one Golden Gate setup task without claiming its separate thermal-cycling requirement.
pub(in crate::backend::hamilton::star) fn plan_setup_invocation(
    profile: &StarAdapterProfile,
    source_wells: BTreeMap<String, String>,
    reaction_wells: Vec<String>,
    additions: &[SetupAddition],
    mix: (u32, f64),
) -> Result<StarExecutionPlan, StarPlanningError> {
    profile.validate()?;
    let deck = DeckIndex::build(profile)?;
    ensure_well_volume(
        &deck,
        "reaction_plate",
        additions.iter().map(|item| item.volume_ul).sum(),
    )?;
    ensure_tip_volume(
        &deck,
        "assembly_small_tips/1",
        additions
            .iter()
            .map(|item| item.volume_ul)
            .chain(std::iter::once(mix.1)),
    )?;

    let choreograph = |liquids: &mut LiquidState| {
        let mut builder = RunBuilder::new(
            &deck,
            liquids,
            Some(TipFeeder::new(
                "assembly_small_tips",
                &deck,
                profile.stages.assembly.small_tips.slots.len(),
                profile.stages.assembly.small_tips.capacity,
            )),
            None,
        );
        for addition in additions {
            let source = StarWell::new(
                "assembly_sources",
                source_wells
                    .get(&addition.symbol)
                    .expect("the invocation assigned every material a source well")
                    .clone(),
            );
            let transfers = reaction_wells
                .iter()
                .map(|well| {
                    Transfer::new(
                        source.clone(),
                        StarWell::new("reaction_plate", well.clone()),
                        addition.volume_ul,
                    )
                })
                .collect::<Vec<_>>();
            builder.distribute(TipClass::Small, &transfers)?;
        }
        let targets = reaction_wells
            .iter()
            .map(|well| StarWell::new("reaction_plate", well.clone()))
            .collect::<Vec<_>>();
        builder.mix_wells(TipClass::Small, &targets, mix)?;
        Ok::<_, StarPlanningError>(builder.finish())
    };

    let mut discovery = LiquidState::new();
    choreograph(&mut discovery)?;
    let source_fills = source_wells
        .iter()
        .map(|(symbol, well)| {
            source_fill(
                &deck,
                &discovery,
                symbol.clone(),
                StarWell::new("assembly_sources", well.clone()),
                TUBE_DEAD_VOLUME_UL,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut liquids = seeded_liquids(&source_fills);
    let (operations, feeders) = choreograph(&mut liquids)?;

    Ok(execution_plan(
        profile,
        source_wells,
        source_fills,
        tip_usage(feeders),
        StarRunPlan {
            id: "setup_golden_gate_reaction".to_owned(),
            title: "Set up Golden Gate reaction".to_owned(),
            operations,
            manual_after: Vec::new(),
            thermal_after: Vec::new(),
        },
    ))
}

/// Plan one serial-dilution task without adding downstream plating work.
#[allow(clippy::too_many_arguments)]
pub(in crate::backend::hamilton::star) fn plan_dilution_invocation(
    profile: &StarAdapterProfile,
    culture_wells: Vec<String>,
    dilution_wells: Vec<Well>,
    serial_dilutions: usize,
    medium_volume_ul: f64,
    culture_volume_ul: f64,
    mix: (u32, f64),
) -> Result<StarExecutionPlan, StarPlanningError> {
    profile.validate()?;
    let deck = DeckIndex::build(profile)?;
    ensure_well_volume(
        &deck,
        "dilution_plate/1",
        medium_volume_ul + culture_volume_ul,
    )?;
    ensure_tip_volume(&deck, "plating_small_tips/1", [culture_volume_ul, mix.1])?;
    ensure_tip_volume(&deck, "plating_large_tips/1", [medium_volume_ul])?;
    let medium = StarWell::new(
        "media_rack",
        profile.stages.plating.media_rack.medium_well.clone(),
    );
    let cultures = culture_wells
        .into_iter()
        .map(|well| StarWell::new("reaction_plate", well))
        .collect::<Vec<_>>();
    let replicates = cultures.len();
    let targets = dilution_wells
        .into_iter()
        .map(|well| StarWell::new(format!("dilution_plate/{}", well.plate + 1), well.well))
        .collect::<Vec<_>>();

    let choreograph = |liquids: &mut LiquidState| {
        let mut builder = RunBuilder::new(
            &deck,
            liquids,
            Some(TipFeeder::new(
                "plating_small_tips",
                &deck,
                profile.stages.plating.small_tips.slots.len(),
                profile.stages.plating.small_tips.capacity,
            )),
            Some(TipFeeder::new(
                "plating_large_tips",
                &deck,
                profile.stages.plating.large_tips.slots.len(),
                profile.stages.plating.large_tips.capacity,
            )),
        );
        let medium_transfers = targets
            .iter()
            .map(|target| Transfer::new(medium.clone(), target.clone(), medium_volume_ul))
            .collect::<Vec<_>>();
        builder.distribute(TipClass::Large, &medium_transfers)?;
        // Each biological replicate is its own dilution series carried on one tip; the series
        // share a canonical fluid-path group, and replicates never share a path.
        for (replicate, culture) in cultures.iter().enumerate() {
            let mut source = culture.clone();
            let mut series = Vec::with_capacity(serial_dilutions);
            for dilution in 0..serial_dilutions {
                let target = targets[dilution * replicates + replicate].clone();
                series.push(
                    Transfer::new(source.clone(), target.clone(), culture_volume_ul).with_mix(mix),
                );
                source = target;
            }
            builder.chain(TipClass::Small, &series)?;
        }
        Ok::<_, StarPlanningError>(builder.finish())
    };

    let mut discovery = LiquidState::new();
    choreograph(&mut discovery)?;
    let mut source_fills = cultures
        .iter()
        .enumerate()
        .map(|(replicate, culture)| {
            source_fill(
                &deck,
                &discovery,
                format!("culture-{replicate}"),
                culture.clone(),
                PLATE_DEAD_VOLUME_UL,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    source_fills.extend([source_fill(
        &deck,
        &discovery,
        "medium".to_owned(),
        medium.clone(),
        TROUGH_DEAD_VOLUME_UL,
    )?]);
    let mut liquids = seeded_liquids(&source_fills);
    let (operations, feeders) = choreograph(&mut liquids)?;

    Ok(execution_plan(
        profile,
        BTreeMap::new(),
        source_fills,
        tip_usage(feeders),
        StarRunPlan {
            id: "serial_dilution".to_owned(),
            title: "Serially dilute recovered culture".to_owned(),
            operations,
            manual_after: Vec::new(),
            thermal_after: Vec::new(),
        },
    ))
}

fn execution_plan(
    profile: &StarAdapterProfile,
    assembly_source_wells: BTreeMap<String, String>,
    source_fills: Vec<SourceFill>,
    tip_usage: BTreeMap<String, usize>,
    run: StarRunPlan,
) -> StarExecutionPlan {
    StarExecutionPlan {
        schema_version: "lab.automation.v1".to_owned(),
        adapter: BACKEND.to_owned(),
        deck: profile.clone(),
        assembly_source_wells,
        transformation_source_wells: BTreeMap::new(),
        dna_source_wells: BTreeMap::new(),
        assemblies: Vec::new(),
        strains: Vec::new(),
        source_fills,
        tip_usage,
        runs: vec![run],
    }
}

fn source_fill(
    deck: &DeckIndex,
    discovery: &LiquidState,
    key: String,
    location: StarWell,
    dead_volume_ul: f64,
) -> Result<SourceFill, StarPlanningError> {
    let consumed_ul = discovery
        .drawn()
        .get(&(location.resource.clone(), location.well.clone()))
        .copied()
        .unwrap_or(0.0);
    let load_ul = consumed_ul + dead_volume_ul;
    let (_, capacity, _) = deck.vessel(&location.resource);
    if load_ul > capacity {
        return Err(AdapterConstraintError::CapacityExceeded {
            adapter: BACKEND.to_owned(),
            operation: "source_loading".to_owned(),
            subject: key.clone(),
            resource: location.resource.clone(),
            required: load_ul.ceil() as u64,
            capacity: capacity.floor() as u64,
            unit: "microlitres".to_owned(),
        }
        .into());
    }
    Ok(SourceFill {
        key,
        location,
        consumed_ul,
        load_ul,
    })
}

fn seeded_liquids(source_fills: &[SourceFill]) -> LiquidState {
    let mut liquids = LiquidState::new();
    for fill in source_fills {
        liquids.seed(&fill.location, fill.load_ul);
    }
    liquids
}

fn tip_usage(feeders: Vec<TipFeeder>) -> BTreeMap<String, usize> {
    feeders
        .into_iter()
        .flat_map(|feeder| feeder.usage())
        .collect()
}

fn ensure_well_volume(
    deck: &DeckIndex,
    resource: &str,
    required_ul: f64,
) -> Result<(), StarPlanningError> {
    let (_, capacity, _) = deck.vessel(resource);
    if required_ul > capacity {
        return Err(AdapterConstraintError::CapacityExceeded {
            adapter: BACKEND.to_owned(),
            operation: "liquid_handling".to_owned(),
            subject: resource.to_owned(),
            resource: resource.to_owned(),
            required: required_ul.ceil() as u64,
            capacity: capacity.floor() as u64,
            unit: "microlitres".to_owned(),
        }
        .into());
    }
    Ok(())
}

fn ensure_tip_volume(
    deck: &DeckIndex,
    resource: &str,
    volumes_ul: impl IntoIterator<Item = f64>,
) -> Result<(), StarPlanningError> {
    let capacity = deck
        .site(resource)
        .labware
        .tip()
        .expect("the validated profile placed a tip rack")
        .max_volume;
    if let Some(required) = volumes_ul.into_iter().find(|volume| *volume > capacity) {
        return Err(AdapterConstraintError::CapacityExceeded {
            adapter: BACKEND.to_owned(),
            operation: "pipetting".to_owned(),
            subject: resource.to_owned(),
            resource: resource.to_owned(),
            required: required.ceil() as u64,
            capacity: capacity.floor() as u64,
            unit: "microlitres".to_owned(),
        }
        .into());
    }
    Ok(())
}
