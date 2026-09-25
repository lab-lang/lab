//! Reserve stock independently for each explicit provisioning call.
use crate::MaterialLotInventory;
use lab_compiler::allocation::{
    MaterialProvision, bind_stock, provision_demands, provisioned_programs,
};
use lab_compiler::planning::{FacilityPlanningSolution, PlanningProblem, SelectedMaterialSource};
use std::collections::BTreeMap;

pub(crate) fn reserve_stock(
    problem: &PlanningProblem,
    solution: &mut FacilityPlanningSolution,
    inventory: &MaterialLotInventory,
) -> Result<(), String> {
    let demands = provision_demands(problem, solution)?;
    let mut used = BTreeMap::<String, u32>::new();
    for demand in demands {
        let binding = solution
            .selections
            .iter_mut()
            .flat_map(|s| &mut s.tasks)
            .flat_map(|t| &mut t.materials)
            .find(|m| m.input == demand.material)
            .ok_or("missing provisioned material")?;
        let SelectedMaterialSource::MaterialLot { material_lot, .. } = &binding.source else {
            return Err("provisioning must resolve to inventory".into());
        };
        let mut candidates = vec![material_lot.clone()];
        candidates.extend(binding.interchangeable_alternatives.clone());
        candidates.sort();
        // Legacy lots without stock annotations retain their stated Method load. An inferred
        // (open) provision must have physical stock evidence; silence is not unlimited inventory.
        if candidates
            .iter()
            .all(|lot| !inventory.stocks().contains_key(lot))
        {
            let vessel = demand
                .program
                .vessels
                .iter()
                .find(|v| v.id == demand.vessel)
                .unwrap();
            if vessel.initial_volume_each.is_some() {
                continue;
            }
            return Err(format!(
                "{} ({}) needs {} uL for {}; declare aliquotVolumeUl and aliquotCount on its inventory lot",
                binding.symbol,
                demand.choice,
                demand.required_volume.value(),
                demand.consumer
            ));
        }
        let mut selected = None;
        let mut reasons = Vec::new();
        for lot in &candidates {
            let Some(stock) = inventory.stocks().get(lot) else {
                continue;
            };
            let bound = match bind_stock(
                &demand.program,
                &demand.vessel,
                &stock.volume_each,
                stock.dead_volume_each.as_ref(),
            ) {
                Ok(bound) => bound,
                Err(message) => {
                    reasons.push(format!("{lot}: {message}"));
                    continue;
                }
            };
            let count = bound
                .vessels
                .iter()
                .find(|v| v.id == demand.vessel)
                .unwrap()
                .positions;
            let start = *used.get(lot).unwrap_or(&0);
            if start.checked_add(count).is_none_or(|end| end > stock.count) {
                reasons.push(format!(
                    "{lot}: requires {count} aliquots, only {} unreserved",
                    stock.count.saturating_sub(start)
                ));
                continue;
            }
            selected = Some((lot.clone(), stock, start, count));
            break;
        }
        let (lot, stock, start, count) = selected.ok_or_else(|| {
            format!(
                "insufficient compatible stock for {} ({}, {} uL): {}",
                binding.symbol,
                demand.choice,
                demand.required_volume.value(),
                reasons.join("; ")
            )
        })?;
        used.insert(lot.clone(), start + count);
        if let SelectedMaterialSource::MaterialLot { material_lot, .. } = &mut binding.source {
            *material_lot = lot.clone();
        }
        binding.interchangeable_alternatives = candidates
            .into_iter()
            .filter(|other| other != &lot)
            .collect();
        solution.provisions.push(MaterialProvision {
            choice: demand.choice,
            material: demand.material,
            symbol: binding.symbol.clone(),
            material_lot: lot,
            consumer: demand.consumer,
            vessel: demand.vessel,
            required_volume: demand.required_volume,
            consumed_volume: demand.consumed_volume,
            stock_volume_each: stock.volume_each.clone(),
            stock_dead_volume_each: stock.dead_volume_each.clone(),
            stock_positions: (start..start + count).collect(),
        });
    }
    provisioned_programs(problem, solution)?;
    Ok(())
}
