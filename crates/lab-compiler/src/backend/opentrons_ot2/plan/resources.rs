//! Deterministic OT-2 source-rack and plate-well allocation primitives.

use std::collections::{BTreeMap, BTreeSet};

use crate::backend::TargetConstraintError;

use super::trace::ProtocolTrace;
use super::{Ot2PlanningError, TARGET};

const SOURCE_RACK_CAPACITY: usize = 24;

pub(super) fn assembly_source_keys(
    traces: &[ProtocolTrace],
    context: &pliron::context::Context,
) -> BTreeSet<String> {
    let mut keys = BTreeSet::from([
        "reagent:nuclease_free_water".into(),
        "reagent:T4_DNA_ligase".into(),
        "reagent:T4_DNA_ligase_buffer".into(),
    ]);
    for trace in traces {
        keys.insert(format!("dna:{}", trace.backbone(context)));
        keys.extend(
            trace
                .components(context)
                .iter()
                .map(|component| format!("dna:{component}")),
        );
        keys.insert(format!("enzyme:{}", trace.restriction_enzyme(context)));
    }
    keys
}

pub(super) fn transformation_source_keys(
    traces: &[ProtocolTrace],
    context: &pliron::context::Context,
) -> BTreeSet<String> {
    let mut keys = BTreeSet::from(["reagent:recovery_medium".into()]);
    keys.extend(
        traces
            .iter()
            .map(|trace| format!("cells:{}", trace.host(context))),
    );
    keys
}

pub(super) fn assign_source_wells(
    stage: &'static str,
    keys: BTreeSet<String>,
) -> Result<BTreeMap<String, String>, Ot2PlanningError> {
    if keys.len() > SOURCE_RACK_CAPACITY {
        return Err(TargetConstraintError::CapacityExceeded {
            target: TARGET.into(),
            operation: stage.into(),
            subject: "automation_batch".into(),
            resource: "source_rack".into(),
            required: keys.len() as u64,
            capacity: SOURCE_RACK_CAPACITY as u64,
            unit: "wells".into(),
        }
        .into());
    }
    Ok(keys
        .into_iter()
        .zip(source_rack_wells())
        .collect::<BTreeMap<_, _>>())
}

pub(super) fn plate_wells() -> Vec<String> {
    (1..=12)
        .flat_map(|column| (b'A'..=b'H').map(move |row| format!("{}{column}", char::from(row))))
        .collect()
}

fn source_rack_wells() -> Vec<String> {
    (1..=6)
        .flat_map(|column| (b'A'..=b'D').map(move |row| format!("{}{column}", char::from(row))))
        .collect()
}
