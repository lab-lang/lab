//! Quantity inference and deterministic stock binding for provisioned liquid inputs.

use std::collections::{BTreeMap, BTreeSet};

use lab_capability::ExactDecimal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::method::LocalId;
use crate::planning::{
    FacilityPlanningSolution, PlanningProblem, PlanningValueSource, SelectedMaterialSource,
};
use crate::procedure::{
    Location, PipettingProgramV1, PipettingStep, ProcedureLocalId, ProcedureProgram, VesselRole,
    Volume,
};

/// One independently owned reservation, derived from a downstream liquid program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaterialProvision {
    pub choice: LocalId,
    pub material: LocalId,
    pub symbol: String,
    pub material_lot: String,
    pub consumer: LocalId,
    pub vessel: ProcedureLocalId,
    pub required_volume: Volume,
    pub consumed_volume: Volume,
    pub stock_volume_each: Volume,
    pub stock_dead_volume_each: Option<Volume>,
    /// Distinct zero-based aliquots within the selected lot, reserved only for this call.
    pub stock_positions: Vec<u32>,
}

/// A downstream input whose source is an explicit `provision` call.
#[derive(Clone, Debug)]
pub struct ProvisionDemand {
    pub choice: LocalId,
    pub material: LocalId,
    pub consumer: LocalId,
    pub vessel: ProcedureLocalId,
    pub required_volume: Volume,
    pub consumed_volume: Volume,
    pub program: PipettingProgramV1,
}

/// Follow value edges, never names or biological artifact identities, back to provisioning.
pub fn provision_demands(
    problem: &PlanningProblem,
    solution: &FacilityPlanningSolution,
) -> Result<Vec<ProvisionDemand>, String> {
    let mut result = Vec::new();
    for selected in &solution.selections {
        let choice = problem
            .choices
            .iter()
            .find(|c| c.id == selected.choice)
            .ok_or("unknown selected choice")?;
        let candidate = choice
            .candidates
            .iter()
            .find(|c| c.method == selected.method)
            .ok_or("unknown selected method")?;
        for task in &candidate.tasks {
            let Some(program) = &task.program else {
                continue;
            };
            if program.contract.as_str() != crate::procedure::vocabulary::PIPETTING_PROGRAM_V1 {
                continue;
            }
            let program = serde_json::from_value::<PipettingProgramV1>(program.body.clone())
                .map_err(|e| e.to_string())?;
            let validated = program.clone().validate().map_err(|e| e.to_string())?;
            let ledger = validated.liquid_ledger();
            for vessel in &program.vessels {
                let VesselRole::ProcedureInput { input } = vessel.role else {
                    continue;
                };
                let input = task
                    .inputs
                    .get(input as usize)
                    .ok_or("missing procedure input")?;
                let PlanningValueSource::ChoiceInput { input } = &input.source else {
                    continue;
                };
                let Some(PlanningValueSource::ChoiceOutput {
                    choice: source,
                    output,
                }) = choice
                    .inputs
                    .iter()
                    .find(|p| &p.name == input)
                    .and_then(|p| p.source.as_ref())
                else {
                    continue;
                };
                let producer = problem
                    .choices
                    .iter()
                    .find(|c| &c.id == source)
                    .ok_or("missing producer")?;
                if producer.source_operation.as_str() != "std.lab.plasmid.provision" {
                    continue;
                }
                let selection = solution
                    .selections
                    .iter()
                    .find(|s| &s.choice == source)
                    .ok_or("missing provisioning selection")?;
                let method = producer
                    .candidates
                    .iter()
                    .find(|c| c.method == selection.method)
                    .ok_or("missing provisioning method")?;
                let Some(PlanningValueSource::TaskOutput { task: provider, .. }) = method
                    .yields
                    .iter()
                    .find(|y| &y.output == output)
                    .map(|y| &y.source)
                else {
                    continue;
                };
                let provider = selection
                    .tasks
                    .iter()
                    .find(|t| &t.task == provider)
                    .ok_or("missing provisioning task")?;
                if provider.materials.len() != 1 {
                    return Err(
                        "quantity inference requires one material per provisioning output".into(),
                    );
                }
                if vessel.positions != 1 {
                    return Err(format!(
                        "{}: provisioned source {} must have one logical position before stock allocation",
                        task.id, vessel.id
                    ));
                }
                let location = Location {
                    vessel: vessel.id.clone(),
                    position: 0,
                };
                let Some(required) = ledger.required_initial_volume(&location) else {
                    continue;
                };
                let consumed = ledger.withdrawn(&location).cloned().unwrap_or_else(zero);
                if consumed.is_zero() {
                    return Err(format!(
                        "{}: cannot infer consumed volume for a provisioned source that has no withdrawals",
                        task.id
                    ));
                }
                result.push(ProvisionDemand {
                    choice: source.clone(),
                    material: provider.materials[0].input.clone(),
                    consumer: task.id.clone(),
                    vessel: vessel.id.clone(),
                    required_volume: volume(required.clone()),
                    consumed_volume: volume(consumed),
                    program: program.clone(),
                });
            }
        }
    }
    Ok(result)
}

fn zero() -> ExactDecimal {
    ExactDecimal::parse("0").expect("zero")
}
fn volume(value: ExactDecimal) -> Volume {
    Volume::parse_microlitres(value.to_string()).expect("positive inferred volume")
}

/// Bind one logical load-only source to stock aliquots without changing any destination dose.
/// Aliquots are never pooled. A transfer uses one source; distributions split at source boundaries.
pub fn bind_stock(
    program: &PipettingProgramV1,
    source: &ProcedureLocalId,
    fill: &Volume,
    dead: Option<&Volume>,
) -> Result<PipettingProgramV1, String> {
    if program.steps.iter().any(|step| match step {
        PipettingStep::Transfer { destination, .. } => &destination.vessel == source,
        PipettingStep::Distribute { destinations, .. } => {
            destinations.iter().any(|to| &to.vessel == source)
        }
        _ => false,
    }) {
        return Err("stock binding requires a source that receives no incoming transfers".into());
    }
    let mut bound = program.clone();
    let vessel = bound
        .vessels
        .iter_mut()
        .find(|v| &v.id == source)
        .ok_or("unknown provisioned source")?;
    if vessel.positions != 1 {
        return Err("stock binding requires one logical source position".into());
    }
    let dead = vessel
        .dead_volume_each
        .as_ref()
        .into_iter()
        .chain(dead)
        .max_by(|a, b| a.value().cmp(b.value()))
        .cloned();
    let dead_value = dead
        .as_ref()
        .map(|v| v.value().clone())
        .unwrap_or_else(zero);
    if fill.value() <= &dead_value {
        return Err("stock aliquot cannot satisfy the source dead volume".into());
    }
    let usable = fill.value().subtracted_by(&dead_value);
    let mut remaining = usable.clone();
    let mut position = 0u32;
    let mut moved = Vec::new();
    for step in &program.steps {
        let group = path_group(step).cloned();
        match step {
            PipettingStep::Transfer {
                source: from,
                volume: dose,
                ..
            } if &from.vessel == source => {
                let at = take_stock(dose.value(), &usable, &mut remaining, &mut position)?;
                let mut step = step.clone();
                if let PipettingStep::Transfer {
                    source,
                    fluid_path_group,
                    ..
                } = &mut step
                {
                    source.position = at;
                    suffix_group(fluid_path_group, at);
                }
                moved.push((group, step));
            }
            PipettingStep::Distribute {
                source: from,
                destinations,
                volume_each,
                ..
            } if &from.vessel == source => {
                let mut groups = BTreeMap::<u32, Vec<Location>>::new();
                for destination in destinations {
                    let at =
                        take_stock(volume_each.value(), &usable, &mut remaining, &mut position)?;
                    groups.entry(at).or_default().push(destination.clone());
                }
                for (at, destinations) in groups {
                    let mut step = step.clone();
                    if let PipettingStep::Distribute {
                        id,
                        source,
                        destinations: into,
                        fluid_path_group,
                        ..
                    } = &mut step
                    {
                        if at > 0 {
                            *id = suffix(id, at);
                        }
                        source.position = at;
                        *into = destinations;
                        suffix_group(fluid_path_group, at);
                    }
                    moved.push((group.clone(), step));
                }
            }
            _ => moved.push((group, step.clone())),
        }
    }
    vessel.positions = position.checked_add(1).ok_or("stock count overflow")?;
    vessel.initial_volume_each = Some(fill.clone());
    vessel.dead_volume_each = dead;
    let mut expanded = Vec::new();
    for (group, step) in moved {
        if let PipettingStep::Mix { targets, .. } = &step
            && targets.iter().any(|t| &t.vessel == source)
        {
            if targets.len() != 1 || &targets[0].vessel != source {
                return Err(
                    "stock binding cannot expand a mix that combines provisioned and other sources"
                        .into(),
                );
            }
            for at in 0..=position {
                let mut mix = step.clone();
                if let PipettingStep::Mix {
                    id,
                    targets,
                    fluid_path_group,
                    ..
                } = &mut mix
                {
                    if at > 0 {
                        *id = suffix(id, at);
                    }
                    targets[0].position = at;
                    suffix_group(fluid_path_group, at);
                }
                expanded.push((group.clone(), mix));
            }
            continue;
        }
        expanded.push((group, step));
    }
    // A source-only fluid path is independent for each stock position. Preserve the original
    // step order within each position, keeping its mix and subsequent draws together so an
    // adapter can realize the same tip-use contract. Never reorder across other operations.
    let mut start = 0;
    while start < expanded.len() {
        let mut end = start + 1;
        if expanded[start].0.is_some() && stock_position(&expanded[start].1, source).is_some() {
            while end < expanded.len()
                && expanded[end].0 == expanded[start].0
                && stock_position(&expanded[end].1, source).is_some()
            {
                end += 1;
            }
            expanded[start..end].sort_by_key(|(_, step)| stock_position(step, source));
        }
        start = end;
    }
    bound.steps = expanded.into_iter().map(|(_, step)| step).collect();
    bound.clone().validate().map_err(|e| {
        format!("stock aliquots cannot realize this source's transfer and mixing requirements: {e}")
    })?;
    Ok(bound)
}

fn path_group(step: &PipettingStep) -> Option<&ProcedureLocalId> {
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

fn stock_position(step: &PipettingStep, stock: &ProcedureLocalId) -> Option<u32> {
    match step {
        PipettingStep::Transfer { source, .. } | PipettingStep::Distribute { source, .. }
            if &source.vessel == stock =>
        {
            Some(source.position)
        }
        PipettingStep::Mix { targets, .. } if targets.len() == 1 && &targets[0].vessel == stock => {
            Some(targets[0].position)
        }
        _ => None,
    }
}

fn take_stock(
    dose: &ExactDecimal,
    usable: &ExactDecimal,
    remaining: &mut ExactDecimal,
    position: &mut u32,
) -> Result<u32, String> {
    if dose > usable {
        return Err(format!(
            "a {dose} uL transfer exceeds a stock aliquot's {usable} uL usable volume"
        ));
    }
    if dose > remaining {
        *position = position.checked_add(1).ok_or("stock count overflow")?;
        *remaining = usable.clone();
    }
    *remaining = remaining.subtracted_by(dose);
    Ok(*position)
}
fn suffix(id: &ProcedureLocalId, position: u32) -> ProcedureLocalId {
    ProcedureLocalId::new(format!("{id}-stock-{position:04}")).expect("valid suffix")
}
fn suffix_group(group: &mut Option<ProcedureLocalId>, position: u32) {
    if position > 0 {
        *group = group.as_ref().map(|id| suffix(id, position));
    }
}

/// Recompute the only program changes allowed by a stock reservation.
pub fn provisioned_programs(
    problem: &PlanningProblem,
    solution: &FacilityPlanningSolution,
) -> Result<BTreeMap<LocalId, ProcedureProgram>, String> {
    let demands = provision_demands(problem, solution)?;
    let mut programs = BTreeMap::<LocalId, ProcedureProgram>::new();
    let mut reservations = BTreeSet::new();
    let mut consumers = BTreeSet::new();
    for provision in &solution.provisions {
        let demand = demands
            .iter()
            .find(|d| {
                d.choice == provision.choice
                    && d.material == provision.material
                    && d.consumer == provision.consumer
                    && d.vessel == provision.vessel
            })
            .ok_or("reservation has no matching downstream demand")?;
        if !consumers.insert((&provision.consumer, &provision.vessel)) {
            return Err("duplicate source reservation".into());
        }
        if demand.required_volume != provision.required_volume
            || demand.consumed_volume != provision.consumed_volume
        {
            return Err("reservation demand differs from the canonical transfers".into());
        }
        let binding = solution
            .selections
            .iter()
            .flat_map(|s| &s.tasks)
            .flat_map(|t| &t.materials)
            .find(|m| m.input == provision.material)
            .ok_or("missing reserved material")?;
        if binding.symbol != provision.symbol
            || !matches!(&binding.source, SelectedMaterialSource::MaterialLot { material_lot, .. } if material_lot == &provision.material_lot)
        {
            return Err("reservation does not match its selected material lot".into());
        }
        for position in &provision.stock_positions {
            if !reservations.insert((&provision.material_lot, *position)) {
                return Err("stock aliquot reserved by more than one provisioning call".into());
            }
        }
        let program = match programs.get(&provision.consumer) {
            Some(p) => serde_json::from_value::<PipettingProgramV1>(p.body.clone())
                .map_err(|e| e.to_string())?,
            None => demand.program.clone(),
        };
        let bound = bind_stock(
            &program,
            &provision.vessel,
            &provision.stock_volume_each,
            provision.stock_dead_volume_each.as_ref(),
        )?;
        let positions = bound
            .vessels
            .iter()
            .find(|v| v.id == provision.vessel)
            .unwrap()
            .positions;
        if positions as usize != provision.stock_positions.len() {
            return Err("reserved stock count does not match the source allocation".into());
        }
        let validated = bound.validate().map_err(|e| e.to_string())?;
        let original = program.validate().map_err(|e| e.to_string())?;
        if validated.capability_formula() != original.capability_formula() {
            return Err("stock allocation changes the task's capability requirements".into());
        }
        programs.insert(
            provision.consumer.clone(),
            ProcedureProgram::from_pipetting(&validated),
        );
    }
    for demand in &demands {
        let source = demand
            .program
            .vessels
            .iter()
            .find(|v| v.id == demand.vessel)
            .unwrap();
        if source.initial_volume_each.is_none()
            && !consumers.contains(&(&demand.consumer, &demand.vessel))
        {
            return Err(format!(
                "{}: inferred provisioning has no stock reservation",
                demand.choice
            ));
        }
    }
    Ok(programs)
}

pub(crate) fn validate_allocated_provisions(
    allocated: &super::AllocatedProgram,
) -> Result<(), String> {
    // Planning may carry unfilled inputs so its ledger can derive demand. Executable allocation
    // must close that obligation, including inputs that did not originate at `provision`.
    for task in allocated.methods.iter().flat_map(|method| &method.tasks) {
        let Some(document) = &task.program else {
            continue;
        };
        if document.contract.as_str() != crate::procedure::vocabulary::PIPETTING_PROGRAM_V1 {
            continue;
        }
        let program: PipettingProgramV1 =
            serde_json::from_value(document.body.clone()).map_err(|e| e.to_string())?;
        let validated = program.clone().validate().map_err(|e| e.to_string())?;
        for vessel in &program.vessels {
            if matches!(vessel.role, VesselRole::ProcedureInput { .. })
                && vessel.initial_volume_each.is_none()
                && (0..vessel.positions).any(|position| {
                    let location = Location {
                        vessel: vessel.id.clone(),
                        position,
                    };
                    validated
                        .liquid_ledger()
                        .required_initial_volume(&location)
                        .is_some()
                        || validated.liquid_ledger().withdrawn(&location).is_some()
                })
            {
                return Err(format!(
                    "{}: procedure input {} has no allocated source fill",
                    task.id, vessel.id
                ));
            }
        }
    }
    let mut reserved = BTreeSet::new();
    let mut sources = BTreeSet::new();
    for provision in &allocated.provisions {
        if !sources.insert((&provision.consumer, &provision.vessel)) {
            return Err("duplicate provisioned source".into());
        }
        let producer = allocated
            .methods
            .iter()
            .find(|m| m.choice == provision.choice)
            .ok_or("missing provisioning choice")?;
        if producer.source_operation.as_str() != "std.lab.plasmid.provision" {
            return Err("reservation is not owned by a provisioning call".into());
        }
        let binding = producer
            .tasks
            .iter()
            .find_map(|task| {
                task.materials
                    .iter()
                    .find(|m| m.input == provision.material)
                    .map(|binding| (task, binding))
            })
            .ok_or("missing provisioned material binding")?;
        let (provider, binding) = binding;
        if binding.symbol != provision.symbol
            || !matches!(&binding.source, SelectedMaterialSource::MaterialLot { material_lot, .. } if material_lot == &provision.material_lot)
        {
            return Err("reserved lot differs from the material binding".into());
        }
        let (method, task) = allocated
            .methods
            .iter()
            .flat_map(|m| m.tasks.iter().map(move |t| (m, t)))
            .find(|(_, t)| t.id == provision.consumer)
            .ok_or("missing provisioning consumer")?;
        let document = task
            .program
            .as_ref()
            .ok_or("provisioning consumer has no liquid program")?;
        let program: PipettingProgramV1 =
            serde_json::from_value(document.body.clone()).map_err(|e| e.to_string())?;
        let source = program
            .vessels
            .iter()
            .find(|v| v.id == provision.vessel)
            .ok_or("missing reserved source vessel")?;
        let VesselRole::ProcedureInput { input } = source.role else {
            return Err("reserved source is not a procedure input".into());
        };
        let Some(crate::planning::PlanningTaskInput {
            source: PlanningValueSource::ChoiceInput { input },
            ..
        }) = task.inputs.get(input as usize)
        else {
            return Err("reserved source is not a method input".into());
        };
        let Some(PlanningValueSource::ChoiceOutput { choice, output }) = method
            .inputs
            .iter()
            .find(|port| &port.name == input)
            .and_then(|port| port.source.as_ref())
        else {
            return Err("reservation does not follow its material value edge".into());
        };
        if choice != &provision.choice || !producer.yields.iter().any(|value| {
            &value.output == output && matches!(&value.source, PlanningValueSource::TaskOutput { task, .. } if task == &provider.id)
        }) {
            return Err("reservation does not follow its provisioning output".into());
        }
        if source.initial_volume_each.as_ref() != Some(&provision.stock_volume_each)
            || source.positions as usize != provision.stock_positions.len()
            || provision.stock_positions.is_empty()
            || provision.stock_positions.windows(2).any(|w| w[0] >= w[1])
        {
            return Err("reserved aliquots differ from the executable source load".into());
        }
        let dead = source
            .dead_volume_each
            .as_ref()
            .map(|v| v.value().clone())
            .unwrap_or_else(zero);
        if dead
            < provision
                .stock_dead_volume_each
                .as_ref()
                .map(|v| v.value().clone())
                .unwrap_or_else(zero)
            || dead >= *provision.stock_volume_each.value()
        {
            return Err("reserved aliquot dead volume differs from the executable source".into());
        }
        let positions = source.positions;
        let validated = program.validate().map_err(|e| e.to_string())?;
        let total = (0..positions).fold(zero(), |sum, position| {
            sum.added_to(
                &validated
                    .liquid_ledger()
                    .withdrawn(&Location {
                        vessel: provision.vessel.clone(),
                        position,
                    })
                    .cloned()
                    .unwrap_or_else(zero),
            )
        });
        if &total != provision.consumed_volume.value() || provision.required_volume.value() < &total
        {
            return Err("reserved consumption differs from the executable transfers".into());
        }
        for position in &provision.stock_positions {
            if !reserved.insert((&provision.material_lot, *position)) {
                return Err("stock aliquot reserved by more than one provisioning call".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procedure::{
        FluidPathPolicy, MixTechnique, PipettingConstraints, TransferTechnique, Vessel,
    };

    fn id(s: &str) -> ProcedureLocalId {
        ProcedureLocalId::new(s).unwrap()
    }
    fn vol(s: &str) -> Volume {
        Volume::parse_microlitres(s).unwrap()
    }
    fn source_program() -> PipettingProgramV1 {
        let source = Location {
            vessel: id("stock"),
            position: 0,
        };
        PipettingProgramV1::new(
            Vec::new(),
            Vec::new(),
            vec![
                Vessel {
                    id: id("stock"),
                    role: VesselRole::ProcedureInput { input: 0 },
                    positions: 1,
                    initial_volume_each: None,
                    working_capacity_each: None,
                    dead_volume_each: None,
                    temperature: None,
                },
                Vessel {
                    id: id("outputs"),
                    role: VesselRole::Intermediate,
                    positions: 6,
                    initial_volume_each: None,
                    working_capacity_each: None,
                    dead_volume_each: None,
                    temperature: None,
                },
            ],
            vec![
                PipettingStep::Mix {
                    id: id("mix"),
                    targets: vec![source.clone()],
                    cycles: 3,
                    volume: vol("50"),
                    fluid_path: FluidPathPolicy::IsolatedDestinations,
                    fluid_path_group: Some(id("cells")),
                    technique: MixTechnique::default(),
                },
                PipettingStep::Distribute {
                    id: id("distribute"),
                    source,
                    destinations: (0..6)
                        .map(|position| Location {
                            vessel: id("outputs"),
                            position,
                        })
                        .collect(),
                    volume_each: vol("20"),
                    fluid_path: FluidPathPolicy::SharedSourceNoReentry,
                    fluid_path_group: Some(id("cells")),
                    technique: TransferTechnique::default(),
                },
            ],
            PipettingConstraints::default(),
        )
    }

    #[test]
    fn stock_binding_preserves_every_dose_and_separates_sources() {
        let bound = bind_stock(&source_program(), &id("stock"), &vol("100"), None)
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(bound.as_program().vessels[0].positions, 2);
        let mut paths = bound
            .as_program()
            .steps
            .iter()
            .filter_map(path_group)
            .collect::<Vec<_>>();
        paths.dedup();
        assert_eq!(
            paths.len(),
            paths.iter().collect::<BTreeSet<_>>().len(),
            "stock expansion must keep each fluid path contiguous"
        );
        let ledger = bound.liquid_ledger();
        for position in 0..6 {
            assert_eq!(
                ledger
                    .final_volume(&Location {
                        vessel: id("outputs"),
                        position
                    })
                    .unwrap()
                    .to_string(),
                "20"
            );
        }
        assert_eq!(
            ledger
                .withdrawn(&Location {
                    vessel: id("stock"),
                    position: 0
                })
                .unwrap()
                .to_string(),
            "100"
        );
        assert_eq!(
            ledger
                .withdrawn(&Location {
                    vessel: id("stock"),
                    position: 1
                })
                .unwrap()
                .to_string(),
            "20"
        );
    }

    #[test]
    fn inaccessible_volume_changes_source_allocation() {
        let bound = bind_stock(
            &source_program(),
            &id("stock"),
            &vol("100"),
            Some(&vol("50")),
        )
        .unwrap();
        assert_eq!(bound.vessels[0].positions, 3);
        assert_eq!(bound.vessels[0].dead_volume_each, Some(vol("50")));
    }

    #[test]
    fn a_stock_aliquot_must_support_the_source_mix() {
        assert!(
            bind_stock(&source_program(), &id("stock"), &vol("40"), None)
                .unwrap_err()
                .contains("mix")
        );
    }

    #[test]
    fn one_transfer_cannot_be_split_across_stock_aliquots() {
        assert!(
            bind_stock(&source_program(), &id("stock"), &vol("10"), None)
                .unwrap_err()
                .contains("transfer exceeds")
        );
    }
}
