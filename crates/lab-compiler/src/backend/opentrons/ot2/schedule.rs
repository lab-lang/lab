//! OT-2 resource scheduling for the complete Golden Gate vertical slice.
//!
//! This pass consumes only one exact facility-allocated invocation. It recognizes the canonical
//! setup/cycle, transformation/recovery, and dilution/plating dataflow, assigns persistent wells,
//! and emits three execution groups. Any other invocation stays on the standalone task path.

use std::collections::{BTreeMap, BTreeSet};

use lab_method::LocalId;
use serde::Serialize;

use super::BACKEND;
use super::invocation::{
    MaterialPlacement, Ot2TaskExecution, PlateMapEntry, dilution_ratio, layered_plate_layout,
    plan_task,
};
use super::profile::Ot2AdapterProfile;
use crate::backend::invocation::exact_invocation_tasks;
use crate::backend::procedure::{
    ADD_RECOVERY_MEDIUM, CYCLE_GOLDEN_GATE, HEAT_SHOCK_TRANSFORMATION, INCUBATE_RECOVERY_CULTURE,
    PLATE_DILUTED_CULTURE, PREPARE_CHEMICAL_TRANSFORMATION, SERIAL_DILUTION, SETUP_GOLDEN_GATE,
};
use crate::backend::resources::{assign_source_wells, plate_wells};
use crate::planning::{
    AdapterInvocation, AdapterInvocationPlan, AllocatedExecutionGroup, AllocatedMethod,
    AllocatedProcedureSchedule, AllocatedProcedureTask, AllocatedRequirementBinding,
    PlanningValueSource, ScheduledPhysicalLocation, ScheduledValueRef, SelectedMaterialSource,
};

#[derive(Clone, Serialize)]
pub(super) struct Ot2ScheduledTask {
    pub(super) task: LocalId,
    pub(super) requirements: Vec<LocalId>,
    pub(super) execution: Ot2TaskExecution,
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum Ot2BatchExecution {
    Assembly {
        source_temperature_c: Option<f64>,
        setups: Vec<Ot2ScheduledTask>,
        thermal_programs: Vec<Ot2ScheduledTask>,
    },
    Transformation {
        /// Shared setpoint for the module holding every staged competent-cell aliquot.
        cell_staging_temperature_c: f64,
        preparations: Vec<Ot2ScheduledTask>,
        heat_shocks: Vec<Ot2ScheduledTask>,
        recovery_additions: Vec<Ot2ScheduledTask>,
        recovery_incubations: Vec<Ot2ScheduledTask>,
    },
    Plating {
        dilutions: Vec<Ot2ScheduledTask>,
        platings: Vec<Ot2ScheduledTask>,
    },
}

pub(super) struct Ot2BatchRun {
    pub(super) id: &'static str,
    pub(super) group: AllocatedExecutionGroup,
    pub(super) execution: Ot2BatchExecution,
}

pub(super) struct Ot2BatchPlan {
    pub(super) schedule: AllocatedProcedureSchedule,
    pub(super) runs: Vec<Ot2BatchRun>,
    pub(super) plate_map: Vec<PlateMapEntry>,
}

type TransformationTasks = (
    Vec<Ot2ScheduledTask>,
    Vec<Ot2ScheduledTask>,
    Vec<Ot2ScheduledTask>,
    Vec<Ot2ScheduledTask>,
    f64,
);
type AssemblyTasks = (Vec<Ot2ScheduledTask>, Vec<Ot2ScheduledTask>, Option<f64>);
type PlatingTasks = (Vec<Ot2ScheduledTask>, Vec<Ot2ScheduledTask>);

struct PlannedTask<'a> {
    task: &'a AllocatedProcedureTask,
    requirements: Vec<&'a AllocatedRequirementBinding>,
    execution: Ot2TaskExecution,
}

struct TaskGraph<'a> {
    methods: BTreeMap<LocalId, &'a AllocatedMethod>,
    method_by_task: BTreeMap<LocalId, &'a AllocatedMethod>,
    task_by_id: BTreeMap<LocalId, &'a AllocatedProcedureTask>,
}

impl<'a> TaskGraph<'a> {
    fn new(plan: &'a AdapterInvocationPlan) -> Self {
        let methods = plan
            .methods
            .iter()
            .map(|method| (method.choice.clone(), method))
            .collect::<BTreeMap<_, _>>();
        let method_by_task = plan
            .methods
            .iter()
            .flat_map(|method| {
                method
                    .tasks
                    .iter()
                    .map(move |task| (task.id.clone(), method))
            })
            .collect::<BTreeMap<_, _>>();
        let task_by_id = plan
            .methods
            .iter()
            .flat_map(|method| &method.tasks)
            .map(|task| (task.id.clone(), task))
            .collect::<BTreeMap<_, _>>();
        Self {
            methods,
            method_by_task,
            task_by_id,
        }
    }

    fn task_producers(&self, task: &AllocatedProcedureTask) -> Result<BTreeSet<LocalId>, String> {
        let method = self.method_by_task.get(&task.id).copied().ok_or_else(|| {
            format!(
                "OT-2 scheduler cannot locate task '{}' in its Method",
                task.id
            )
        })?;
        let mut producers = BTreeSet::new();
        for input in &task.inputs {
            self.resolve_source(method, &input.source, &mut producers, &mut BTreeSet::new())?;
        }
        for material in &task.materials {
            if let SelectedMaterialSource::ChoiceOutput { choice } = &material.source {
                let producer = self.methods.get(choice).copied().ok_or_else(|| {
                    format!("OT-2 scheduler cannot resolve material producer choice '{choice}'")
                })?;
                for method_yield in &producer.yields {
                    self.resolve_source(
                        producer,
                        &method_yield.source,
                        &mut producers,
                        &mut BTreeSet::new(),
                    )?;
                }
            }
        }
        for dependency in &method.after {
            let producer = self.methods.get(dependency).copied().ok_or_else(|| {
                format!("OT-2 scheduler cannot resolve Method dependency '{dependency}'")
            })?;
            producers.extend(producer.tasks.iter().map(|task| task.id.clone()));
        }
        Ok(producers)
    }

    fn resolve_source(
        &self,
        method: &AllocatedMethod,
        source: &PlanningValueSource,
        producers: &mut BTreeSet<LocalId>,
        visiting: &mut BTreeSet<(LocalId, LocalId)>,
    ) -> Result<(), String> {
        match source {
            PlanningValueSource::TaskOutput { task, .. } => {
                if !self.task_by_id.contains_key(task) {
                    return Err(format!(
                        "OT-2 scheduler cannot resolve producer task '{task}'"
                    ));
                }
                producers.insert(task.clone());
            }
            PlanningValueSource::ChoiceInput { input } => {
                let port = method
                    .inputs
                    .iter()
                    .find(|port| &port.name == input)
                    .ok_or_else(|| {
                        format!(
                            "OT-2 scheduler cannot resolve input '{}::{input}'",
                            method.choice
                        )
                    })?;
                if let Some(source) = &port.source {
                    self.resolve_source(method, source, producers, visiting)?;
                }
            }
            PlanningValueSource::ChoiceOutput { choice, output } => {
                if !visiting.insert((choice.clone(), output.clone())) {
                    return Err(format!(
                        "OT-2 scheduler found a cycle while resolving '{choice}::{output}'"
                    ));
                }
                let producer = self.methods.get(choice).copied().ok_or_else(|| {
                    format!("OT-2 scheduler cannot resolve producer choice '{choice}'")
                })?;
                let method_yield = producer
                    .yields
                    .iter()
                    .find(|method_yield| &method_yield.output == output)
                    .ok_or_else(|| {
                        format!("OT-2 scheduler cannot resolve output '{choice}::{output}'")
                    })?;
                self.resolve_source(producer, &method_yield.source, producers, visiting)?;
                visiting.remove(&(choice.clone(), output.clone()));
            }
        }
        Ok(())
    }

    fn choice_for_task(&self, task: &LocalId) -> Result<LocalId, String> {
        self.method_by_task
            .get(task)
            .map(|method| method.choice.clone())
            .ok_or_else(|| format!("OT-2 scheduler cannot locate task '{task}'"))
    }

    fn choice_input_ref(
        &self,
        task: &AllocatedProcedureTask,
        input: &LocalId,
    ) -> Result<ScheduledValueRef, String> {
        let method = self
            .method_by_task
            .get(&task.id)
            .copied()
            .ok_or_else(|| format!("OT-2 scheduler cannot locate task '{}'", task.id))?;
        if !method.inputs.iter().any(|port| &port.name == input) {
            return Err(format!(
                "OT-2 scheduler cannot locate Method input '{}::{input}'",
                method.choice
            ));
        }
        Ok(ScheduledValueRef::ChoiceInput {
            choice: method.choice.clone(),
            input: input.clone(),
        })
    }

    fn physical_source_key(
        &self,
        task: &AllocatedProcedureTask,
        source: &PlanningValueSource,
    ) -> Result<String, String> {
        let method = self
            .method_by_task
            .get(&task.id)
            .copied()
            .ok_or_else(|| format!("OT-2 scheduler cannot locate task '{}'", task.id))?;
        let mut producers = BTreeSet::new();
        self.resolve_source(method, source, &mut producers, &mut BTreeSet::new())?;
        let sources = producers
            .iter()
            .filter_map(|producer| self.task_by_id.get(producer))
            .flat_map(|producer| &producer.materials)
            .map(|material| serde_json::to_string(&material.source))
            .collect::<Result<BTreeSet<_>, _>>()
            .map_err(|error| error.to_string())?;
        match sources.into_iter().collect::<Vec<_>>().as_slice() {
            [source] => Ok(source.clone()),
            [] if producers.len() == 1 => Ok(format!(
                "task-output:{}",
                producers.iter().next().expect("one producer exists")
            )),
            _ => Err(format!(
                "OT-2 scheduler cannot reduce task '{}' input to one physical source",
                task.id
            )),
        }
    }
}

pub(super) fn try_plan_golden_gate_batches(
    profile: &Ot2AdapterProfile,
    plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
) -> Result<Option<Ot2BatchPlan>, String> {
    let members = exact_invocation_tasks("OT-2", plan, invocation)?;
    let supported = BTreeSet::from([
        SETUP_GOLDEN_GATE,
        CYCLE_GOLDEN_GATE,
        PREPARE_CHEMICAL_TRANSFORMATION,
        HEAT_SHOCK_TRANSFORMATION,
        ADD_RECOVERY_MEDIUM,
        INCUBATE_RECOVERY_CULTURE,
        SERIAL_DILUTION,
        PLATE_DILUTED_CULTURE,
    ]);
    if members
        .iter()
        .any(|member| !supported.contains(member.task.operation.as_str()))
    {
        return Ok(None);
    }
    let mut tasks = members
        .into_iter()
        .map(|member| {
            let (_, execution) = plan_task(profile, member.task, &member.requirements)?;
            Ok(PlannedTask {
                task: member.task,
                requirements: member.requirements,
                execution,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let operation_count = |operation: &str| {
        tasks
            .iter()
            .filter(|task| task.task.operation.as_str() == operation)
            .count()
    };
    let assembly_count = operation_count(SETUP_GOLDEN_GATE);
    let transformation_count = operation_count(PREPARE_CHEMICAL_TRANSFORMATION);
    let plating_count = operation_count(SERIAL_DILUTION);
    if assembly_count == 0
        || transformation_count == 0
        || plating_count == 0
        || operation_count(CYCLE_GOLDEN_GATE) != assembly_count
        || operation_count(HEAT_SHOCK_TRANSFORMATION) != transformation_count
        || operation_count(ADD_RECOVERY_MEDIUM) != transformation_count
        || operation_count(INCUBATE_RECOVERY_CULTURE) != transformation_count
        || operation_count(PLATE_DILUTED_CULTURE) != plating_count
    {
        return Ok(None);
    }

    let graph = TaskGraph::new(plan);
    let producers = tasks
        .iter()
        .map(|task| Ok((task.task.id.clone(), graph.task_producers(task.task)?)))
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let assembly_pairs = pair_tasks(
        &graph,
        &tasks,
        &producers,
        SETUP_GOLDEN_GATE,
        CYCLE_GOLDEN_GATE,
        true,
    )?;
    let prepare_heat = pair_tasks(
        &graph,
        &tasks,
        &producers,
        PREPARE_CHEMICAL_TRANSFORMATION,
        HEAT_SHOCK_TRANSFORMATION,
        true,
    )?;
    let heat_recovery = pair_tasks(
        &graph,
        &tasks,
        &producers,
        HEAT_SHOCK_TRANSFORMATION,
        ADD_RECOVERY_MEDIUM,
        false,
    )?;
    let recovery_incubation = pair_tasks(
        &graph,
        &tasks,
        &producers,
        ADD_RECOVERY_MEDIUM,
        INCUBATE_RECOVERY_CULTURE,
        true,
    )?;
    let dilution_plating = pair_tasks(
        &graph,
        &tasks,
        &producers,
        SERIAL_DILUTION,
        PLATE_DILUTED_CULTURE,
        false,
    )?;

    let mut locations = Vec::new();
    let mut product_wells = BTreeMap::<LocalId, (String, u32)>::new();
    let (assembly_setups, assembly_thermal, assembly_source_temperature_c) = allocate_assembly(
        profile,
        &graph,
        &mut tasks,
        &assembly_pairs,
        &mut locations,
        &mut product_wells,
    )?;
    let mut incubation_wells = BTreeMap::<LocalId, Vec<String>>::new();
    let transformation = allocate_transformation(
        profile,
        &graph,
        &mut tasks,
        &prepare_heat,
        &heat_recovery,
        &recovery_incubation,
        &product_wells,
        &mut locations,
        &mut incubation_wells,
    )?;
    let (plating, plate_map) = allocate_plating(
        profile,
        &graph,
        &producers,
        &mut tasks,
        &dilution_plating,
        &incubation_wells,
        &mut locations,
    )?;

    let assembly_tasks = assembly_setups
        .iter()
        .chain(&assembly_thermal)
        .map(|task| task.task.clone())
        .collect::<Vec<_>>();
    let transformation_tasks = transformation
        .0
        .iter()
        .chain(&transformation.1)
        .chain(&transformation.2)
        .chain(&transformation.3)
        .map(|task| task.task.clone())
        .collect::<Vec<_>>();
    let plating_tasks = plating
        .0
        .iter()
        .chain(&plating.1)
        .map(|task| task.task.clone())
        .collect::<Vec<_>>();
    let mut groups = vec![
        execution_group("assembly", assembly_tasks, &tasks),
        execution_group("transformation", transformation_tasks, &tasks),
        execution_group("plating", plating_tasks, &tasks),
    ];
    let task_groups = groups
        .iter()
        .flat_map(|group| {
            group
                .tasks
                .iter()
                .map(move |task| (task.clone(), group.id.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    for group in &mut groups {
        group.after = group
            .tasks
            .iter()
            .flat_map(|task| producers[task].iter())
            .filter_map(|producer| task_groups.get(producer))
            .filter(|dependency| *dependency != &group.id)
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
    }
    let schedule = AllocatedProcedureSchedule::new(plan, invocation, groups.clone(), locations)
        .map_err(|error| error.to_string())?;
    let runs = vec![
        Ot2BatchRun {
            id: "assembly",
            group: groups[0].clone(),
            execution: Ot2BatchExecution::Assembly {
                source_temperature_c: assembly_source_temperature_c,
                setups: assembly_setups,
                thermal_programs: assembly_thermal,
            },
        },
        Ot2BatchRun {
            id: "transformation",
            group: groups[1].clone(),
            execution: Ot2BatchExecution::Transformation {
                cell_staging_temperature_c: transformation.4,
                preparations: transformation.0,
                heat_shocks: transformation.1,
                recovery_additions: transformation.2,
                recovery_incubations: transformation.3,
            },
        },
        Ot2BatchRun {
            id: "plating",
            group: groups[2].clone(),
            execution: Ot2BatchExecution::Plating {
                dilutions: plating.0,
                platings: plating.1,
            },
        },
    ];
    Ok(Some(Ot2BatchPlan {
        schedule,
        runs,
        plate_map,
    }))
}

fn pair_tasks(
    graph: &TaskGraph<'_>,
    tasks: &[PlannedTask<'_>],
    producers: &BTreeMap<LocalId, BTreeSet<LocalId>>,
    producer_operation: &str,
    consumer_operation: &str,
    within_method: bool,
) -> Result<Vec<(usize, usize)>, String> {
    tasks
        .iter()
        .enumerate()
        .filter(|(_, task)| task.task.operation.as_str() == producer_operation)
        .map(|(producer_index, producer)| {
            let producer_method = graph.method_by_task[&producer.task.id].choice.as_str();
            let consumers = tasks
                .iter()
                .enumerate()
                .filter(|(_, task)| {
                    task.task.operation.as_str() == consumer_operation
                        && producers[&task.task.id].contains(&producer.task.id)
                        && (!within_method
                            || graph.method_by_task[&task.task.id].choice.as_str()
                                == producer_method)
                })
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            let [consumer_index] = consumers.as_slice() else {
                return Err(format!(
                    "OT-2 scheduler requires one {consumer_operation} consumer of '{}', found {}",
                    producer.task.id,
                    consumers.len()
                ));
            };
            Ok((producer_index, *consumer_index))
        })
        .collect()
}

fn allocate_assembly(
    profile: &Ot2AdapterProfile,
    graph: &TaskGraph<'_>,
    tasks: &mut [PlannedTask<'_>],
    pairs: &[(usize, usize)],
    locations: &mut Vec<ScheduledPhysicalLocation>,
    product_wells: &mut BTreeMap<LocalId, (String, u32)>,
) -> Result<AssemblyTasks, String> {
    let source_keys = pairs
        .iter()
        .flat_map(|(setup, _)| match &tasks[*setup].execution {
            Ot2TaskExecution::SetupGoldenGateReaction { additions, .. } => additions
                .iter()
                .map(|addition| addition.placement.material.symbol.clone())
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect::<BTreeSet<_>>();
    let source_wells = assign_source_wells(
        BACKEND,
        "batched-golden-gate-assembly",
        source_keys,
        profile.deck.temperature_module.capacity,
    )
    .map_err(|error| error.to_string())?;
    let available_wells = plate_wells(profile.deck.thermocycler.capacity);
    let required_wells = pairs
        .iter()
        .map(|(setup, _)| match &tasks[*setup].execution {
            Ot2TaskExecution::SetupGoldenGateReaction { reaction_wells, .. } => {
                reaction_wells.len()
            }
            _ => 0,
        })
        .sum::<usize>();
    if required_wells > available_wells.len() {
        return Err(format!(
            "OT-2 assembly batch needs {required_wells} thermocycler wells, but the profile provides {}",
            available_wells.len()
        ));
    }
    let required_tips = pairs
        .iter()
        .map(|(setup, _)| match &tasks[*setup].execution {
            Ot2TaskExecution::SetupGoldenGateReaction {
                additions,
                reaction_wells,
                ..
            } => {
                let final_mix_tips = usize::from(
                    !additions
                        .last()
                        .is_some_and(|addition| addition.reuse_tip_for_final_mix),
                );
                (additions.len() + final_mix_tips) * reaction_wells.len()
            }
            _ => 0,
        })
        .sum::<usize>();
    if required_tips > profile.stages.assembly.small_tips.total_capacity() {
        return Err(format!(
            "OT-2 assembly batch needs {required_tips} small tips, but the profile provides {}",
            profile.stages.assembly.small_tips.total_capacity()
        ));
    }

    let mut cursor = 0;
    let mut shared_thermal = None;
    let mut shared_source_temperature = None;
    for (setup_index, thermal_index) in pairs {
        let setup_id = tasks[*setup_index].task.id.clone();
        let setup_outputs = tasks[*setup_index].task.outputs.clone();
        let setup_materials = tasks[*setup_index].task.materials.clone();
        let thermal_id = tasks[*thermal_index].task.id.clone();
        let thermal_outputs = tasks[*thermal_index].task.outputs.clone();
        let choice = graph.choice_for_task(&thermal_id)?;
        let Ot2TaskExecution::SetupGoldenGateReaction {
            reaction_wells,
            additions,
            reaction_volume_ul,
            source_temperature_c,
            ..
        } = &mut tasks[*setup_index].execution
        else {
            return Err("OT-2 assembly scheduler received a non-setup task".to_owned());
        };
        shared_source_temperature =
            merge_source_temperature(shared_source_temperature, *source_temperature_c)?;
        let count = reaction_wells.len();
        let reaction_volume = *reaction_volume_ul;
        let allocated = available_wells[cursor..cursor + count].to_vec();
        cursor += count;
        *reaction_wells = allocated.clone();
        for addition in additions {
            addition.placement.source_well =
                source_wells[&addition.placement.material.symbol].clone();
        }
        for material in setup_materials {
            let source_well = source_wells[&material.symbol].clone();
            locations.push(ScheduledPhysicalLocation {
                value: ScheduledValueRef::MaterialInput {
                    task: setup_id.clone(),
                    input: material.input,
                },
                resource: "assembly-source-rack".to_owned(),
                positions: vec![source_well],
            });
        }
        for output in setup_outputs {
            locations.push(ScheduledPhysicalLocation {
                value: ScheduledValueRef::TaskOutput {
                    task: setup_id.clone(),
                    output: output.name,
                },
                resource: "thermocycler-plate".to_owned(),
                positions: allocated.clone(),
            });
        }
        let Ot2TaskExecution::ThermalProgram {
            sample_wells,
            volume_each_ul,
            lid_temperature_c,
            profile: thermal_profile,
            final_hold_celsius,
            ..
        } = &mut tasks[*thermal_index].execution
        else {
            return Err("OT-2 assembly scheduler received a non-thermal task".to_owned());
        };
        if sample_wells.len() != count {
            return Err(format!(
                "OT-2 assembly tasks '{setup_id}' and '{thermal_id}' disagree about sample count"
            ));
        }
        let signature = (
            *volume_each_ul,
            *lid_temperature_c,
            thermal_profile.clone(),
            *final_hold_celsius,
        );
        if shared_thermal
            .as_ref()
            .is_some_and(|shared| shared != &signature)
        {
            return Err(
                "OT-2 cannot batch Golden Gate reactions with different thermal programs"
                    .to_owned(),
            );
        }
        shared_thermal = Some(signature);
        *sample_wells = allocated.clone();
        for output in thermal_outputs {
            locations.push(ScheduledPhysicalLocation {
                value: ScheduledValueRef::TaskOutput {
                    task: thermal_id.clone(),
                    output: output.name,
                },
                resource: "thermocycler-plate".to_owned(),
                positions: allocated.clone(),
            });
        }
        if allocated.len() != 1 {
            return Err(format!(
                "OT-2 transformation handoff currently requires one product well per assembly choice; '{choice}' has {}",
                allocated.len()
            ));
        }
        product_wells.insert(choice, (allocated[0].clone(), reaction_volume));
    }
    Ok((
        pairs
            .iter()
            .map(|(setup, _)| scheduled_task(&tasks[*setup]))
            .collect(),
        pairs
            .iter()
            .map(|(_, thermal)| scheduled_task(&tasks[*thermal]))
            .collect(),
        shared_source_temperature,
    ))
}

fn merge_source_temperature(
    shared: Option<f64>,
    candidate: Option<f64>,
) -> Result<Option<f64>, String> {
    match (shared, candidate) {
        (Some(shared), Some(candidate)) if shared != candidate => Err(
            "OT-2 cannot batch work whose staged materials require different temperatures"
                .to_owned(),
        ),
        (None, Some(candidate)) => Ok(Some(candidate)),
        (shared, _) => Ok(shared),
    }
}

#[allow(clippy::too_many_arguments)]
fn allocate_transformation(
    profile: &Ot2AdapterProfile,
    graph: &TaskGraph<'_>,
    tasks: &mut [PlannedTask<'_>],
    prepare_heat: &[(usize, usize)],
    heat_recovery: &[(usize, usize)],
    recovery_incubation: &[(usize, usize)],
    product_wells: &BTreeMap<LocalId, (String, u32)>,
    locations: &mut Vec<ScheduledPhysicalLocation>,
    incubation_wells: &mut BTreeMap<LocalId, Vec<String>>,
) -> Result<TransformationTasks, String> {
    let recovery_by_heat = heat_recovery.iter().copied().collect::<BTreeMap<_, _>>();
    let incubation_by_recovery = recovery_incubation
        .iter()
        .copied()
        .collect::<BTreeMap<_, _>>();
    let available_wells = plate_wells(profile.deck.thermocycler.capacity);
    let required_wells = prepare_heat
        .iter()
        .map(|(prepare, _)| match &tasks[*prepare].execution {
            Ot2TaskExecution::PrepareChemicalTransformation { reaction_wells, .. } => {
                reaction_wells.len()
            }
            _ => 0,
        })
        .sum::<usize>();
    if required_wells > available_wells.len() {
        return Err(format!(
            "OT-2 transformation batch needs {required_wells} thermocycler wells, but the profile provides {}",
            available_wells.len()
        ));
    }

    let mut cell_keys = BTreeMap::<usize, String>::new();
    let mut staging_temperature = None;
    let mut source_keys = BTreeSet::new();
    let mut chilled_keys = BTreeSet::new();
    let mut cell_withdrawals = BTreeMap::<String, u32>::new();
    let mut cell_mix_requirements = BTreeMap::<String, u32>::new();
    let mut dna_withdrawals = BTreeMap::<String, u32>::new();
    let mut dna_mix_requirements = BTreeMap::<String, u32>::new();
    let mut dna_final_withdrawals = BTreeMap::<String, u32>::new();
    let mut external_dna_keys = BTreeSet::new();
    let mut recovery_loads = BTreeMap::<String, u32>::new();
    for (prepare, heat) in prepare_heat {
        let recovery = *recovery_by_heat.get(heat).ok_or_else(|| {
            format!(
                "OT-2 scheduler cannot find recovery addition after '{}'",
                tasks[*heat].task.id
            )
        })?;
        let Ot2TaskExecution::PrepareChemicalTransformation {
            cell_source,
            cell_staging_temperature_c,
            cell_withdrawal_ul,
            cell_mix_volume_ul,
            reaction_wells,
            dna,
            dna_volume_ul,
            dna_mix_volume_ul,
            ..
        } = &tasks[*prepare].execution
        else {
            return Err("OT-2 transformation scheduler received a non-preparation task".to_owned());
        };
        staging_temperature =
            merge_source_temperature(staging_temperature, *cell_staging_temperature_c)?;
        let cell_key = format!(
            "cells:{}",
            graph.physical_source_key(tasks[*prepare].task, cell_source)?
        );
        chilled_keys.insert(cell_key.clone());
        // The canonical program's ledger already totalled this task's draw, so the batch adds
        // those figures rather than multiplying parameters that could drift from the steps.
        let withdrawal = cell_withdrawals.entry(cell_key.clone()).or_default();
        *withdrawal = withdrawal
            .checked_add(*cell_withdrawal_ul)
            .ok_or_else(|| "OT-2 competent-cell batch volume overflows".to_owned())?;
        cell_mix_requirements
            .entry(cell_key.clone())
            .and_modify(|required| *required = (*required).max(*cell_mix_volume_ul))
            .or_insert(*cell_mix_volume_ul);
        cell_keys.insert(*prepare, cell_key);
        for placement in dna {
            let key = dna_source_key(placement)?;
            if matches!(
                &placement.material.source,
                SelectedMaterialSource::MaterialLot { .. }
            ) {
                external_dna_keys.insert(key.clone());
            }
            let volume = dna_volume_ul
                .checked_mul(u32::try_from(reaction_wells.len()).map_err(|_| {
                    "OT-2 transformation replicate count does not fit DNA arithmetic".to_owned()
                })?)
                .ok_or_else(|| "OT-2 DNA source volume overflows".to_owned())?;
            let withdrawal = dna_withdrawals.entry(key.clone()).or_default();
            *withdrawal = withdrawal
                .checked_add(volume)
                .ok_or_else(|| "OT-2 DNA batch volume overflows".to_owned())?;
            dna_mix_requirements
                .entry(key.clone())
                .and_modify(|required| *required = (*required).max(*dna_mix_volume_ul))
                .or_insert(*dna_mix_volume_ul);
            // The smallest single draw is the least this well can have left before its last mix.
            dna_final_withdrawals
                .entry(key)
                .and_modify(|draw| *draw = (*draw).min(*dna_volume_ul))
                .or_insert(*dna_volume_ul);
        }
        let Ot2TaskExecution::AddRecoveryMedium {
            medium,
            recovery_volume_ul,
            culture_wells,
            ..
        } = &tasks[recovery].execution
        else {
            return Err("OT-2 transformation scheduler received a non-recovery task".to_owned());
        };
        let recovery_key = format!(
            "recovery:{}",
            serde_json::to_string(&medium.material.source).map_err(|error| error.to_string())?
        );
        source_keys.insert(recovery_key.clone());
        let volume = recovery_volume_ul
            .checked_mul(u32::try_from(culture_wells.len()).map_err(|_| {
                "OT-2 recovery replicate count does not fit source arithmetic".to_owned()
            })?)
            .ok_or_else(|| "OT-2 recovery source volume overflows".to_owned())?;
        let load = recovery_loads.entry(recovery_key).or_default();
        *load = load
            .checked_add(volume)
            .ok_or_else(|| "OT-2 recovery batch volume overflows".to_owned())?;
    }
    // One mix per competent-cell tube precedes its single distribute, while each DNA well is
    // remixed before every transfer out of it.
    let cell_loads = required_source_loads(
        &cell_withdrawals,
        &cell_mix_requirements,
        MixCadence::BeforeFirstDraw,
    );
    let dna_loads = required_source_loads(
        &dna_withdrawals,
        &dna_mix_requirements,
        MixCadence::BeforeEveryDraw(&dna_final_withdrawals),
    );
    let dna_plate_wells = plate_wells(profile.stages.transformation.dna_plate.capacity);
    let product_positions = product_wells
        .values()
        .map(|(well, _)| well.as_str())
        .collect::<BTreeSet<_>>();
    if product_positions
        .iter()
        .any(|well| !dna_plate_wells.iter().any(|candidate| candidate == well))
    {
        return Err(
            "OT-2 assembly output position does not exist in the configured transformation DNA plate"
                .to_owned(),
        );
    }
    let available_external_wells = dna_plate_wells
        .into_iter()
        .filter(|well| !product_positions.contains(well.as_str()))
        .collect::<Vec<_>>();
    if external_dna_keys.len() > available_external_wells.len() {
        return Err(format!(
            "OT-2 transformation batch needs {} external DNA wells in addition to {} assembly products, but the configured DNA plate has only {} free wells",
            external_dna_keys.len(),
            product_positions.len(),
            available_external_wells.len()
        ));
    }
    let external_dna_wells = external_dna_keys
        .into_iter()
        .zip(available_external_wells)
        .collect::<BTreeMap<_, _>>();
    let source_wells = assign_source_wells(
        BACKEND,
        "batched-transformation-sources",
        source_keys,
        profile.stages.transformation.source_rack.capacity,
    )
    .map_err(|error| error.to_string())?;
    // Competent cells carry a staging temperature, so they are addressed on the rack the
    // temperature module holds rather than on the ambient bench rack beside it.
    let chilled_wells = assign_source_wells(
        BACKEND,
        "batched-transformation-chilled-sources",
        chilled_keys,
        profile.deck.temperature_module.capacity,
    )
    .map_err(|error| error.to_string())?;
    let small_tips = prepare_heat
        .iter()
        .map(|(prepare, _)| match &tasks[*prepare].execution {
            Ot2TaskExecution::PrepareChemicalTransformation {
                reaction_wells,
                dna,
                ..
            } => reaction_wells.len() * dna.len(),
            _ => 0,
        })
        .sum::<usize>();
    if small_tips > profile.stages.transformation.small_tips.total_capacity() {
        return Err(format!(
            "OT-2 transformation batch needs {small_tips} small tips, but the profile provides {}",
            profile.stages.transformation.small_tips.total_capacity()
        ));
    }
    let large_tips = cell_loads.len() + recovery_loads.len();
    if large_tips > profile.stages.transformation.large_tips.total_capacity() {
        return Err(format!(
            "OT-2 transformation batch needs {large_tips} large tips, but the profile provides {}",
            profile.stages.transformation.large_tips.total_capacity()
        ));
    }

    let mut cursor = 0;
    let mut heat_signature = None;
    let mut incubation_signature = None;
    let mut preparations = Vec::new();
    let mut heat_shocks = Vec::new();
    let mut recoveries = Vec::new();
    let mut incubations = Vec::new();
    for (prepare, heat) in prepare_heat {
        let recovery = recovery_by_heat[heat];
        let incubation = incubation_by_recovery[&recovery];
        let prepare_id = tasks[*prepare].task.id.clone();
        let prepare_outputs = tasks[*prepare].task.outputs.clone();
        let prepare_materials = tasks[*prepare].task.materials.clone();
        let cell_input = tasks[*prepare]
            .task
            .inputs
            .iter()
            .find_map(|input| match &input.source {
                PlanningValueSource::ChoiceInput { input } if input.as_str() == "cells" => {
                    Some(input.clone())
                }
                _ => None,
            })
            .ok_or_else(|| format!("OT-2 preparation '{prepare_id}' has no cells input"))?;
        let heat_id = tasks[*heat].task.id.clone();
        let heat_outputs = tasks[*heat].task.outputs.clone();
        let recovery_id = tasks[recovery].task.id.clone();
        let recovery_outputs = tasks[recovery].task.outputs.clone();
        let incubation_id = tasks[incubation].task.id.clone();
        let incubation_outputs = tasks[incubation].task.outputs.clone();
        let Ot2TaskExecution::PrepareChemicalTransformation {
            cell_source_well,
            cell_source_volume_ul,
            dna,
            reaction_wells,
            ..
        } = &mut tasks[*prepare].execution
        else {
            unreachable!("transformation pair begins with preparation")
        };
        let count = reaction_wells.len();
        let allocated = available_wells[cursor..cursor + count].to_vec();
        cursor += count;
        *reaction_wells = allocated.clone();
        let cell_key = &cell_keys[prepare];
        *cell_source_well = chilled_wells[cell_key].clone();
        *cell_source_volume_ul = cell_loads[cell_key];
        locations.push(ScheduledPhysicalLocation {
            value: graph.choice_input_ref(tasks[*prepare].task, &cell_input)?,
            resource: "transformation-chilled-rack".to_owned(),
            positions: vec![cell_source_well.clone()],
        });
        for placement in dna {
            let key = dna_source_key(placement)?;
            let (well, available_volume) = match &placement.material.source {
                SelectedMaterialSource::ChoiceOutput { choice } => {
                    let (well, available_volume) = product_wells.get(choice).ok_or_else(|| {
                        format!(
                            "OT-2 cannot locate assembly output '{choice}' for DNA '{}'",
                            placement.material.symbol
                        )
                    })?;
                    if dna_loads[&key] > *available_volume {
                        return Err(format!(
                            "OT-2 transformations require {} uL of '{}' but assembly output '{choice}' contains {available_volume} uL",
                            dna_loads[&key], placement.material.symbol
                        ));
                    }
                    (well.clone(), *available_volume)
                }
                SelectedMaterialSource::MaterialLot { .. } => (
                    external_dna_wells.get(&key).cloned().ok_or_else(|| {
                        format!(
                            "OT-2 cannot allocate external DNA '{}' in the transformation plate",
                            placement.material.symbol
                        )
                    })?,
                    dna_loads[&key],
                ),
            };
            placement.source_well = well;
            placement.load_volume_ul = Some(available_volume);
        }
        for material in prepare_materials {
            let key = dna_material_source_key(&material.symbol, &material.source)?;
            let well = match &material.source {
                SelectedMaterialSource::ChoiceOutput { choice } => product_wells
                    .get(choice)
                    .map(|(well, _)| well.clone())
                    .ok_or_else(|| {
                        format!(
                            "OT-2 cannot locate assembly output '{choice}' for DNA '{}'",
                            material.symbol
                        )
                    })?,
                SelectedMaterialSource::MaterialLot { .. } => {
                    external_dna_wells.get(&key).cloned().ok_or_else(|| {
                        format!(
                            "OT-2 cannot allocate external DNA '{}' in the transformation plate",
                            material.symbol
                        )
                    })?
                }
            };
            locations.push(ScheduledPhysicalLocation {
                value: ScheduledValueRef::MaterialInput {
                    task: prepare_id.clone(),
                    input: material.input,
                },
                resource: "transformation-dna-plate".to_owned(),
                positions: vec![well],
            });
        }
        add_output_locations(
            locations,
            &prepare_id,
            prepare_outputs,
            "thermocycler-plate",
            &allocated,
        );

        let Ot2TaskExecution::ThermalProgram {
            sample_wells,
            volume_each_ul,
            lid_temperature_c,
            profile: thermal_profile,
            final_hold_celsius,
            ..
        } = &mut tasks[*heat].execution
        else {
            unreachable!("preparation consumer is heat shock")
        };
        if sample_wells.len() != count {
            return Err(format!(
                "OT-2 heat-shock task '{heat_id}' does not preserve transformation replicate count"
            ));
        }
        let signature = (
            *volume_each_ul,
            *lid_temperature_c,
            thermal_profile.clone(),
            *final_hold_celsius,
        );
        if heat_signature
            .as_ref()
            .is_some_and(|shared| shared != &signature)
        {
            return Err(
                "OT-2 cannot batch transformations with different heat-shock programs".to_owned(),
            );
        }
        heat_signature = Some(signature);
        *sample_wells = allocated.clone();
        add_output_locations(
            locations,
            &heat_id,
            heat_outputs,
            "thermocycler-plate",
            &allocated,
        );

        let Ot2TaskExecution::AddRecoveryMedium {
            culture_wells,
            medium,
            ..
        } = &mut tasks[recovery].execution
        else {
            unreachable!("heat-shock consumer is recovery addition")
        };
        if culture_wells.len() != count {
            return Err(format!(
                "OT-2 recovery task '{recovery_id}' does not preserve transformation replicate count"
            ));
        }
        *culture_wells = allocated.clone();
        let key = format!(
            "recovery:{}",
            serde_json::to_string(&medium.material.source).map_err(|error| error.to_string())?
        );
        medium.source_well = source_wells[&key].clone();
        medium.load_volume_ul = Some(recovery_loads[&key]);
        locations.push(ScheduledPhysicalLocation {
            value: ScheduledValueRef::MaterialInput {
                task: recovery_id.clone(),
                input: medium.material.input.clone(),
            },
            resource: "transformation-source-rack".to_owned(),
            positions: vec![medium.source_well.clone()],
        });
        add_output_locations(
            locations,
            &recovery_id,
            recovery_outputs,
            "thermocycler-plate",
            &allocated,
        );

        let Ot2TaskExecution::ThermalProgram {
            sample_wells,
            volume_each_ul,
            lid_temperature_c,
            profile: thermal_profile,
            final_hold_celsius,
            ..
        } = &mut tasks[incubation].execution
        else {
            unreachable!("recovery consumer is incubation")
        };
        if sample_wells.len() != count {
            return Err(format!(
                "OT-2 recovery-incubation task '{incubation_id}' does not preserve replicate count"
            ));
        }
        let signature = (
            *volume_each_ul,
            *lid_temperature_c,
            thermal_profile.clone(),
            *final_hold_celsius,
        );
        if incubation_signature
            .as_ref()
            .is_some_and(|shared| shared != &signature)
        {
            return Err(
                "OT-2 cannot batch transformations with different recovery-incubation programs"
                    .to_owned(),
            );
        }
        incubation_signature = Some(signature);
        *sample_wells = allocated.clone();
        incubation_wells.insert(incubation_id.clone(), allocated.clone());
        add_output_locations(
            locations,
            &incubation_id,
            incubation_outputs,
            "thermocycler-plate",
            &allocated,
        );
        preparations.push(scheduled_task(&tasks[*prepare]));
        heat_shocks.push(scheduled_task(&tasks[*heat]));
        recoveries.push(scheduled_task(&tasks[recovery]));
        incubations.push(scheduled_task(&tasks[incubation]));
    }
    // Every fused preparation shares one module, so a batch without a stated setpoint would leave
    // the aliquots at bench temperature. Refuse rather than guess.
    let cell_staging_temperature_c = staging_temperature.ok_or_else(|| {
        "OT-2 transformation batch stages competent cells without a stated temperature".to_owned()
    })?;
    Ok((
        preparations,
        heat_shocks,
        recoveries,
        incubations,
        cell_staging_temperature_c,
    ))
}

/// When a shared source is remixed, which decides how much must still be in it at the end.
enum MixCadence<'a> {
    /// One mix before any liquid leaves, so the mix only has to fit the starting load.
    BeforeFirstDraw,
    /// A mix before every draw, keyed by the smallest single draw from each source. Before the
    /// last mix the well is already down to `total - smallest draw`, and that remainder still has
    /// to cover a full mix.
    BeforeEveryDraw(&'a BTreeMap<String, u32>),
}

/// How much each shared source must hold for the whole batch.
///
/// The total withdrawal is one bound. The other depends on when the source is remixed: taking only
/// the larger of the total and one mix understates the load when a mix precedes every draw and
/// draws more than a single transfer does, which leaves the last mix short.
fn required_source_loads(
    withdrawals: &BTreeMap<String, u32>,
    mix_requirements: &BTreeMap<String, u32>,
    cadence: MixCadence<'_>,
) -> BTreeMap<String, u32> {
    withdrawals
        .iter()
        .map(|(source, withdrawal)| {
            let mix = mix_requirements.get(source).copied().unwrap_or_default();
            let before_last_mix = match &cadence {
                MixCadence::BeforeFirstDraw => 0,
                MixCadence::BeforeEveryDraw(final_withdrawals) => {
                    let smallest = final_withdrawals
                        .get(source)
                        .copied()
                        .unwrap_or(*withdrawal);
                    withdrawal.saturating_sub(smallest)
                }
            };
            (
                source.clone(),
                (*withdrawal).max(mix.saturating_add(before_last_mix)),
            )
        })
        .collect()
}

fn dna_source_key(placement: &MaterialPlacement) -> Result<String, String> {
    dna_material_source_key(&placement.material.symbol, &placement.material.source)
}

fn dna_material_source_key(
    symbol: &str,
    source: &SelectedMaterialSource,
) -> Result<String, String> {
    Ok(format!(
        "dna:{symbol}:{}",
        serde_json::to_string(source).map_err(|error| error.to_string())?
    ))
}

#[allow(clippy::too_many_arguments)]
fn allocate_plating(
    profile: &Ot2AdapterProfile,
    _graph: &TaskGraph<'_>,
    producers: &BTreeMap<LocalId, BTreeSet<LocalId>>,
    tasks: &mut [PlannedTask<'_>],
    pairs: &[(usize, usize)],
    incubation_wells: &BTreeMap<LocalId, Vec<String>>,
    locations: &mut Vec<ScheduledPhysicalLocation>,
) -> Result<(PlatingTasks, Vec<PlateMapEntry>), String> {
    let total_replicates = pairs
        .iter()
        .map(|(dilution, _)| match &tasks[*dilution].execution {
            Ot2TaskExecution::SerialDilution { culture_wells, .. } => culture_wells.len(),
            _ => 0,
        })
        .sum::<usize>();
    let first_dilution = pairs
        .first()
        .ok_or_else(|| "OT-2 plating schedule contains no dilution tasks".to_owned())?
        .0;
    let serial_dilutions = match &tasks[first_dilution].execution {
        Ot2TaskExecution::SerialDilution {
            dilution_wells,
            culture_wells,
            ..
        } => dilution_wells.len() / culture_wells.len(),
        _ => unreachable!("plating pair begins with dilution"),
    };
    if serial_dilutions != 2 {
        return Err(format!(
            "OT-2 interleaved dilution/plating scheduling requires exactly two serial dilutions, found {serial_dilutions}"
        ));
    }
    let dilution_layout = layered_plate_layout(
        tasks[first_dilution].task,
        "batched dilution plates",
        &profile.stages.plating.dilution_plate,
        serial_dilutions,
        total_replicates,
    )?;
    let first_plating = pairs[0].1;
    let plating_replicates = match &tasks[first_plating].execution {
        Ot2TaskExecution::PlateDilutedCulture {
            plating_replicates, ..
        } => *plating_replicates,
        _ => unreachable!("dilution consumer is plating"),
    };
    let agar_per_layer = total_replicates
        .checked_mul(plating_replicates)
        .ok_or_else(|| "OT-2 agar allocation overflows".to_owned())?;
    let agar_layout = layered_plate_layout(
        tasks[first_plating].task,
        "batched selective agar plates",
        &profile.stages.plating.agar_plate,
        serial_dilutions,
        agar_per_layer,
    )?;
    let small_tips = total_replicates
        .checked_mul(2)
        .ok_or_else(|| "OT-2 fused plating tip count overflows".to_owned())?;
    if small_tips > profile.stages.plating.small_tips.total_capacity() {
        return Err(format!(
            "OT-2 fused plating needs {small_tips} small tips, but the profile provides {}",
            profile.stages.plating.small_tips.total_capacity()
        ));
    }
    if profile.stages.plating.large_tips.total_capacity() == 0 {
        return Err("OT-2 fused plating needs one large tip for dilution medium".to_owned());
    }

    let mut offset = 0;
    let mut dilutions = Vec::new();
    let mut platings = Vec::new();
    let mut plate_map = Vec::new();
    let mut shared_dilution = None;
    let mut shared_selection = None;
    for (dilution, plating) in pairs {
        let dilution_id = tasks[*dilution].task.id.clone();
        let dilution_outputs = tasks[*dilution].task.outputs.clone();
        let dilution_materials = tasks[*dilution].task.materials.clone();
        let plating_id = tasks[*plating].task.id.clone();
        let plating_outputs = tasks[*plating].task.outputs.clone();
        let plating_materials = tasks[*plating].task.materials.clone();
        let producer_incubations = producers[&dilution_id]
            .iter()
            .filter(|producer| incubation_wells.contains_key(*producer))
            .collect::<Vec<_>>();
        let [producer_incubation] = producer_incubations.as_slice() else {
            return Err(format!(
                "OT-2 dilution task '{dilution_id}' must consume one scheduled recovery incubation"
            ));
        };
        let source_wells = &incubation_wells[*producer_incubation];
        let count = source_wells.len();
        let Ot2TaskExecution::SerialDilution {
            artifact,
            culture_wells,
            medium,
            dilution_wells,
            medium_volume_ul,
            culture_volume_ul,
            mix_cycles,
            mix_volume_ul,
            medium_technique,
            transfer_technique,
            mix_technique,
            ..
        } = &mut tasks[*dilution].execution
        else {
            unreachable!("plating pair begins with dilution")
        };
        if culture_wells.len() != count {
            return Err(format!(
                "OT-2 dilution task '{dilution_id}' does not preserve recovered-culture replicates"
            ));
        }
        *culture_wells = source_wells.clone();
        let allocated_dilutions = (0..serial_dilutions)
            .flat_map(|layer| {
                dilution_layout
                    [layer * total_replicates + offset..layer * total_replicates + offset + count]
                    .iter()
                    .cloned()
            })
            .collect::<Vec<_>>();
        *dilution_wells = allocated_dilutions.clone();
        let signature = (
            artifact.clone(),
            medium.material.source.clone(),
            *medium_volume_ul,
            *culture_volume_ul,
            *mix_cycles,
            *mix_volume_ul,
            medium_technique.clone(),
            transfer_technique.clone(),
            mix_technique.clone(),
        );
        let comparable = (
            signature.1.clone(),
            signature.2,
            signature.3,
            signature.4,
            signature.5,
            signature.6.clone(),
            signature.7.clone(),
            signature.8.clone(),
        );
        if shared_dilution
            .as_ref()
            .is_some_and(|shared| shared != &comparable)
        {
            return Err(
                "OT-2 cannot fuse dilution tasks with different media, volumes, or techniques"
                    .to_owned(),
            );
        }
        shared_dilution = Some(comparable);
        for material in dilution_materials {
            locations.push(ScheduledPhysicalLocation {
                value: ScheduledValueRef::MaterialInput {
                    task: dilution_id.clone(),
                    input: material.input,
                },
                resource: "plating-media-rack".to_owned(),
                positions: vec![medium.source_well.clone()],
            });
        }
        add_well_output_locations(
            locations,
            &dilution_id,
            dilution_outputs,
            "dilution-plates",
            &allocated_dilutions,
        );
        let dilution_artifact = artifact.clone();
        let dilution_medium_volume_ul = *medium_volume_ul;
        let dilution_culture_volume_ul = *culture_volume_ul;

        let Ot2TaskExecution::PlateDilutedCulture {
            artifact: plating_artifact,
            selection,
            dilution_wells: plating_dilutions,
            agar_wells,
            initial_volume_by_dilution_ul,
            culture_replicates,
            serial_dilutions: plating_dilution_count,
            plating_replicates: task_plating_replicates,
            colony_volume_ul,
            technique,
            plate_map: task_map,
            ..
        } = &mut tasks[*plating].execution
        else {
            unreachable!("dilution consumer is plating")
        };
        if plating_artifact != &dilution_artifact
            || *culture_replicates != count
            || *plating_dilution_count != serial_dilutions
            || *task_plating_replicates != plating_replicates
        {
            return Err(format!(
                "OT-2 plating task '{plating_id}' does not preserve its paired dilution shape"
            ));
        }
        *plating_dilutions = allocated_dilutions.clone();
        let allocated_agar = (0..serial_dilutions)
            .flat_map(|layer| {
                let start = layer * agar_per_layer + offset * plating_replicates;
                agar_layout[start..start + count * plating_replicates]
                    .iter()
                    .cloned()
            })
            .collect::<Vec<_>>();
        *agar_wells = allocated_agar.clone();
        let selection_signature = (
            selection.source.clone(),
            *colony_volume_ul,
            technique.clone(),
            plating_replicates,
        );
        if shared_selection
            .as_ref()
            .is_some_and(|shared| shared != &selection_signature)
        {
            return Err("OT-2 cannot fuse plating tasks with different selection material, volume, or technique".to_owned());
        }
        shared_selection = Some(selection_signature);
        task_map.clear();
        for layer in 0..serial_dilutions {
            for replicate in 0..count {
                let source = allocated_dilutions[layer * count + replicate].clone();
                for plating_replicate in 0..plating_replicates {
                    let destination = allocated_agar[layer * count * plating_replicates
                        + replicate * plating_replicates
                        + plating_replicate]
                        .clone();
                    task_map.push(PlateMapEntry {
                        subject: dilution_artifact.clone(),
                        dilution: layer + 1,
                        dilution_ratio: dilution_ratio(
                            dilution_medium_volume_ul,
                            dilution_culture_volume_ul,
                            layer + 1,
                        )?,
                        culture_replicate: replicate + 1,
                        plating_replicate: plating_replicate + 1,
                        source: source.clone(),
                        destination,
                    });
                }
            }
        }
        plate_map.extend(task_map.clone());
        for material in plating_materials {
            locations.push(ScheduledPhysicalLocation {
                value: ScheduledValueRef::MaterialInput {
                    task: plating_id.clone(),
                    input: material.input,
                },
                resource: "selective-agar-plates".to_owned(),
                positions: allocated_agar
                    .iter()
                    .map(|well| format!("{}:{}", well.plate, well.well))
                    .collect(),
            });
        }
        add_well_output_locations(
            locations,
            &plating_id,
            plating_outputs,
            "selective-agar-plates",
            &allocated_agar,
        );
        if initial_volume_by_dilution_ul.len() != serial_dilutions {
            return Err(format!(
                "OT-2 plating task '{plating_id}' has no initial volume for every dilution"
            ));
        }
        dilutions.push(scheduled_task(&tasks[*dilution]));
        platings.push(scheduled_task(&tasks[*plating]));
        offset += count;
    }
    Ok(((dilutions, platings), plate_map))
}

fn execution_group(
    id: &str,
    tasks: Vec<LocalId>,
    planned: &[PlannedTask<'_>],
) -> AllocatedExecutionGroup {
    let task_set = tasks.iter().collect::<BTreeSet<_>>();
    let requirements = planned
        .iter()
        .filter(|task| task_set.contains(&task.task.id))
        .flat_map(|task| {
            task.requirements
                .iter()
                .map(|requirement| requirement.id.clone())
        })
        .collect();
    AllocatedExecutionGroup {
        id: id.to_owned(),
        after: Vec::new(),
        tasks,
        requirements,
    }
}

fn scheduled_task(task: &PlannedTask<'_>) -> Ot2ScheduledTask {
    Ot2ScheduledTask {
        task: task.task.id.clone(),
        requirements: task
            .requirements
            .iter()
            .map(|requirement| requirement.id.clone())
            .collect(),
        execution: task.execution.clone(),
    }
}

fn add_output_locations(
    locations: &mut Vec<ScheduledPhysicalLocation>,
    task: &LocalId,
    outputs: Vec<crate::planning::PlanningTaskOutput>,
    resource: &str,
    positions: &[String],
) {
    locations.extend(outputs.into_iter().map(|output| ScheduledPhysicalLocation {
        value: ScheduledValueRef::TaskOutput {
            task: task.clone(),
            output: output.name,
        },
        resource: resource.to_owned(),
        positions: positions.to_vec(),
    }));
}

fn add_well_output_locations(
    locations: &mut Vec<ScheduledPhysicalLocation>,
    task: &LocalId,
    outputs: Vec<crate::planning::PlanningTaskOutput>,
    resource: &str,
    positions: &[crate::backend::resources::Well],
) {
    let positions = positions
        .iter()
        .map(|well| format!("{}:{}", well.plate, well.well))
        .collect::<Vec<_>>();
    add_output_locations(locations, task, outputs, resource, &positions);
}

#[cfg(test)]
mod tests {
    use super::{MixCadence, merge_source_temperature, required_source_loads};
    use std::collections::BTreeMap;

    #[test]
    fn a_shared_source_holds_enough_for_its_last_mix_as_well_as_every_draw() {
        // Four 2 uL draws with a 5 uL mix before each: the well is down to 2 uL before the final
        // mix, so it must start with 5 + 3 x 2 = 11 uL rather than the 8 uL total withdrawal.
        let withdrawals = BTreeMap::from([("dna".to_owned(), 8)]);
        let mix_requirements = BTreeMap::from([("dna".to_owned(), 5)]);
        let final_withdrawals = BTreeMap::from([("dna".to_owned(), 2)]);

        let loads = required_source_loads(
            &withdrawals,
            &mix_requirements,
            MixCadence::BeforeEveryDraw(&final_withdrawals),
        );

        assert_eq!(loads["dna"], 11);
    }

    #[test]
    fn a_source_mixed_once_only_has_to_hold_its_mix_at_the_start() {
        // The competent-cell tube is remixed once before its single distribute, so a 50 uL mix
        // needs no headroom beyond the 80 uL the batch draws.
        let withdrawals = BTreeMap::from([("cells".to_owned(), 80)]);
        let mix_requirements = BTreeMap::from([("cells".to_owned(), 50)]);

        let loads =
            required_source_loads(&withdrawals, &mix_requirements, MixCadence::BeforeFirstDraw);

        assert_eq!(loads["cells"], 80);
    }

    #[test]
    fn assembly_batch_requires_one_shared_source_temperature() {
        let shared = merge_source_temperature(None, None).unwrap();
        let shared = merge_source_temperature(shared, Some(4.0)).unwrap();
        let shared = merge_source_temperature(shared, None).unwrap();
        let shared = merge_source_temperature(shared, Some(4.0)).unwrap();

        assert_eq!(shared, Some(4.0));
        assert_eq!(
            merge_source_temperature(shared, Some(8.0)).unwrap_err(),
            "OT-2 cannot batch work whose staged materials require different temperatures"
        );
    }
}
