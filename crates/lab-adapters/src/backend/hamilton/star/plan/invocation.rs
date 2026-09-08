//! Resource planning for one exact facility-allocated STAR invocation.
//!
//! These planners deliberately know nothing about the rest of the experiment. They receive one
//! typed Procedure operation, allocate only the wells and tips needed by that operation, and
//! produce the single STAR run that implements its exact capability requirement.

use std::collections::BTreeMap;

use crate::backend::AdapterConstraintError;
use crate::backend::hamilton::star::BACKEND;
use crate::backend::hamilton::star::liquid_classes::{
    LiquidClassEvidence, LiquidClassLibraryIdentity,
};
use crate::backend::hamilton::star::plan::choreograph::TipFeeder;
use crate::backend::hamilton::star::plan::error::StarPlanningError;
use crate::backend::hamilton::star::plan::execution::{
    SourceFill, StarExecutionPlan, StarRunPlan, StarWell,
};
use crate::backend::hamilton::star::plan::liquids::{DeckIndex, LiquidState};
use crate::backend::hamilton::star::profile::StarAdapterProfile;

pub(in crate::backend::hamilton::star) fn execution_plan(
    profile: &StarAdapterProfile,
    source_fills: Vec<SourceFill>,
    tip_usage: BTreeMap<String, usize>,
    liquid_class_library: LiquidClassLibraryIdentity,
    liquid_classes: Vec<LiquidClassEvidence>,
    run: StarRunPlan,
) -> StarExecutionPlan {
    StarExecutionPlan {
        schema_version: "lab.automation.v3".to_owned(),
        adapter: BACKEND.to_owned(),
        deck: profile.clone(),
        source_fills,
        tip_usage,
        liquid_class_library,
        liquid_classes,
        runs: vec![run],
    }
}

pub(in crate::backend::hamilton::star) fn source_fill(
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

pub(in crate::backend::hamilton::star) fn seeded_liquids(
    source_fills: &[SourceFill],
) -> LiquidState {
    let mut liquids = LiquidState::new();
    for fill in source_fills {
        liquids.seed(&fill.location, fill.load_ul);
    }
    liquids
}

pub(in crate::backend::hamilton::star) fn tip_usage(
    feeders: Vec<TipFeeder>,
) -> BTreeMap<String, usize> {
    feeders
        .into_iter()
        .flat_map(|feeder| feeder.usage())
        .collect()
}
