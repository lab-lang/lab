//! Versioned execution groups and physical locations over one allocated adapter invocation.
//!
//! Facility planning owns Method, capability, Asset, and material selection. A device scheduler
//! may then group compatible allocated Procedure tasks into one reviewed run document and assign
//! stable physical locations. This record preserves task and requirement identity while making
//! cross-task resource reuse explicit and reviewable.

use std::collections::{BTreeMap, BTreeSet};

use lab_method::LocalId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{AdapterInvocation, AdapterInvocationPlan, InvocationAdapter};

pub const ALLOCATED_PROCEDURE_SCHEDULE_SCHEMA_VERSION: &str = "lab.allocated-procedure-schedule.v1";

/// One validated device schedule for one exact Asset/adapter invocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AllocatedProcedureSchedule {
    pub schema_version: String,
    pub invocation_plan_sha256: String,
    pub invocation: String,
    pub asset: String,
    pub adapter: InvocationAdapter,
    pub groups: Vec<AllocatedExecutionGroup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<ScheduledPhysicalLocation>,
}

/// Procedure tasks that one reviewed run document executes atomically.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AllocatedExecutionGroup {
    pub id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
    pub tasks: Vec<LocalId>,
    pub requirements: Vec<LocalId>,
}

/// A logical allocated value and the adapter-defined physical positions that carry it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ScheduledPhysicalLocation {
    pub value: ScheduledValueRef,
    /// Adapter-defined resource ID resolved by the checked implementation profile.
    pub resource: String,
    /// Addresses within the resource, in semantic replicate order.
    pub positions: Vec<String>,
}

/// Stable references into the selected Method graph retained by `AdapterInvocationPlan`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduledValueRef {
    ChoiceInput { choice: LocalId, input: LocalId },
    TaskOutput { task: LocalId, output: LocalId },
    MaterialInput { task: LocalId, input: LocalId },
}

impl AllocatedProcedureSchedule {
    pub fn new(
        plan: &AdapterInvocationPlan,
        invocation: &AdapterInvocation,
        groups: Vec<AllocatedExecutionGroup>,
        locations: Vec<ScheduledPhysicalLocation>,
    ) -> Result<Self, AllocatedProcedureScheduleError> {
        let schedule = Self {
            schema_version: ALLOCATED_PROCEDURE_SCHEDULE_SCHEMA_VERSION.to_owned(),
            invocation_plan_sha256: plan.sha256(),
            invocation: invocation.id.clone(),
            asset: invocation.asset.clone(),
            adapter: invocation.adapter.clone(),
            groups,
            locations,
        };
        schedule.validate_against(plan, invocation)?;
        Ok(schedule)
    }

    /// Revalidate a deserialized schedule against the exact invocation it claims to realize.
    pub fn validate_against(
        &self,
        plan: &AdapterInvocationPlan,
        invocation: &AdapterInvocation,
    ) -> Result<(), AllocatedProcedureScheduleError> {
        plan.validate().map_err(|error| {
            AllocatedProcedureScheduleError::InvalidInvocationPlan {
                message: error.to_string(),
            }
        })?;
        if self.schema_version != ALLOCATED_PROCEDURE_SCHEDULE_SCHEMA_VERSION {
            return Err(AllocatedProcedureScheduleError::WrongSchema {
                found: self.schema_version.clone(),
            });
        }
        if self.invocation_plan_sha256 != plan.sha256()
            || self.invocation != invocation.id
            || self.asset != invocation.asset
            || self.adapter != invocation.adapter
            || !plan
                .invocations
                .iter()
                .any(|candidate| candidate == invocation)
        {
            return Err(AllocatedProcedureScheduleError::InvocationMismatch);
        }
        if self.groups.is_empty() {
            return Err(AllocatedProcedureScheduleError::EmptySchedule);
        }

        let task_records = plan
            .methods
            .iter()
            .flat_map(|method| &method.tasks)
            .map(|task| (task.id.clone(), task))
            .collect::<BTreeMap<_, _>>();
        let expected_tasks = invocation.tasks.iter().cloned().collect::<BTreeSet<_>>();
        let expected_requirements = invocation
            .requirements
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut group_ids = BTreeSet::new();
        let mut scheduled_tasks = BTreeSet::new();
        let mut scheduled_requirements = BTreeSet::new();
        let mut task_group = BTreeMap::new();
        let mut requirement_group = BTreeMap::new();

        for group in &self.groups {
            if !valid_group_id(&group.id) || !group_ids.insert(group.id.as_str()) {
                return Err(AllocatedProcedureScheduleError::InvalidGroup {
                    group: group.id.clone(),
                    message: "the ID is empty, repeated, or not a lowercase slug".to_owned(),
                });
            }
            if group.tasks.is_empty() || group.requirements.is_empty() {
                return Err(AllocatedProcedureScheduleError::InvalidGroup {
                    group: group.id.clone(),
                    message: "tasks and requirements must both be non-empty".to_owned(),
                });
            }
            let mut local_tasks = BTreeSet::new();
            for task in &group.tasks {
                if !expected_tasks.contains(task)
                    || !local_tasks.insert(task)
                    || !scheduled_tasks.insert(task.clone())
                {
                    return Err(AllocatedProcedureScheduleError::InvalidGroup {
                        group: group.id.clone(),
                        message: format!("task '{task}' is unknown or scheduled more than once"),
                    });
                }
                task_group.insert(task.clone(), group.id.as_str());
            }
            let mut local_requirements = BTreeSet::new();
            for requirement in &group.requirements {
                if !expected_requirements.contains(requirement)
                    || !local_requirements.insert(requirement)
                    || !scheduled_requirements.insert(requirement.clone())
                {
                    return Err(AllocatedProcedureScheduleError::InvalidGroup {
                        group: group.id.clone(),
                        message: format!(
                            "requirement '{requirement}' is unknown or scheduled more than once"
                        ),
                    });
                }
                requirement_group.insert(requirement.clone(), group.id.as_str());
            }
        }
        if scheduled_tasks != expected_tasks || scheduled_requirements != expected_requirements {
            return Err(AllocatedProcedureScheduleError::IncompleteCoverage);
        }
        for task in &invocation.tasks {
            let task_record = task_records.get(task).ok_or_else(|| {
                AllocatedProcedureScheduleError::InvalidInvocationPlan {
                    message: format!("invocation references missing task '{task}'"),
                }
            })?;
            let group = task_group[task];
            for requirement in &task_record.requirements {
                if expected_requirements.contains(&requirement.id)
                    && requirement_group.get(&requirement.id).copied() != Some(group)
                {
                    return Err(AllocatedProcedureScheduleError::SplitTask { task: task.clone() });
                }
            }
        }

        for group in &self.groups {
            let mut dependencies = BTreeSet::new();
            for dependency in &group.after {
                if dependency == &group.id
                    || !group_ids.contains(dependency.as_str())
                    || !dependencies.insert(dependency)
                {
                    return Err(AllocatedProcedureScheduleError::InvalidGroup {
                        group: group.id.clone(),
                        message: format!(
                            "dependency '{dependency}' is unknown, repeated, or self-referential"
                        ),
                    });
                }
            }
        }
        validate_acyclic(&self.groups)?;
        validate_locations(self, plan, invocation)?;
        Ok(())
    }
}

fn validate_locations(
    schedule: &AllocatedProcedureSchedule,
    plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
) -> Result<(), AllocatedProcedureScheduleError> {
    let scheduled_tasks = invocation.tasks.iter().collect::<BTreeSet<_>>();
    let methods = plan
        .methods
        .iter()
        .filter(|method| {
            method
                .tasks
                .iter()
                .any(|task| scheduled_tasks.contains(&task.id))
        })
        .map(|method| (method.choice.clone(), method))
        .collect::<BTreeMap<_, _>>();
    let tasks = methods
        .values()
        .flat_map(|method| &method.tasks)
        .filter(|task| scheduled_tasks.contains(&task.id))
        .map(|task| (task.id.clone(), task))
        .collect::<BTreeMap<_, _>>();
    let mut values = BTreeSet::new();
    for location in &schedule.locations {
        if location.resource.is_empty()
            || location.positions.is_empty()
            || !values.insert(&location.value)
            || location
                .positions
                .iter()
                .any(|position| position.is_empty())
            || location.positions.iter().collect::<BTreeSet<_>>().len() != location.positions.len()
        {
            return Err(AllocatedProcedureScheduleError::InvalidLocation {
                value: location.value.clone(),
            });
        }
        let valid = match &location.value {
            ScheduledValueRef::ChoiceInput { choice, input } => methods
                .get(choice)
                .is_some_and(|method| method.inputs.iter().any(|port| &port.name == input)),
            ScheduledValueRef::TaskOutput { task, output } => tasks
                .get(task)
                .is_some_and(|task| task.outputs.iter().any(|port| &port.name == output)),
            ScheduledValueRef::MaterialInput { task, input } => {
                tasks.get(task).is_some_and(|task| {
                    task.materials
                        .iter()
                        .any(|material| &material.input == input)
                })
            }
        };
        if !valid {
            return Err(AllocatedProcedureScheduleError::InvalidLocation {
                value: location.value.clone(),
            });
        }
    }
    Ok(())
}

fn validate_acyclic(
    groups: &[AllocatedExecutionGroup],
) -> Result<(), AllocatedProcedureScheduleError> {
    let mut indegree = groups
        .iter()
        .map(|group| (group.id.as_str(), group.after.len()))
        .collect::<BTreeMap<_, _>>();
    let mut dependents = BTreeMap::<&str, Vec<&str>>::new();
    for group in groups {
        for dependency in &group.after {
            dependents
                .entry(dependency.as_str())
                .or_default()
                .push(group.id.as_str());
        }
    }
    let mut ready = indegree
        .iter()
        .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
        .collect::<Vec<_>>();
    let mut visited = 0;
    while let Some(id) = ready.pop() {
        visited += 1;
        for dependent in dependents.get(id).into_iter().flatten() {
            let degree = indegree
                .get_mut(dependent)
                .expect("every dependency target was validated");
            *degree -= 1;
            if *degree == 0 {
                ready.push(dependent);
            }
        }
    }
    if visited != groups.len() {
        return Err(AllocatedProcedureScheduleError::CyclicGroups);
    }
    Ok(())
}

fn valid_group_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('-')
        && !id.ends_with('-')
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum AllocatedProcedureScheduleError {
    #[error("allocated Procedure schedule contains an invalid invocation plan: {message}")]
    InvalidInvocationPlan { message: String },
    #[error(
        "allocated Procedure schedule declares schema `{found}`, expected `{ALLOCATED_PROCEDURE_SCHEDULE_SCHEMA_VERSION}`"
    )]
    WrongSchema { found: String },
    #[error("allocated Procedure schedule does not match its exact invocation")]
    InvocationMismatch,
    #[error("allocated Procedure schedule contains no execution groups")]
    EmptySchedule,
    #[error("allocated execution group `{group}` is invalid: {message}")]
    InvalidGroup { group: String, message: String },
    #[error("allocated Procedure schedule does not cover every task and requirement exactly once")]
    IncompleteCoverage,
    #[error("allocated Procedure task `{task}` has requirements split across execution groups")]
    SplitTask { task: LocalId },
    #[error("allocated Procedure schedule execution groups contain a cycle")]
    CyclicGroups,
    #[error("allocated Procedure schedule contains invalid physical location `{value:?}`")]
    InvalidLocation { value: ScheduledValueRef },
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use lab_capability::{
        AbsoluteIri, CapabilityKind, ControlMode, MethodId, OperationId, QualificationLevel,
    };
    use lab_method::{IntentOperationId, PortType};

    use super::*;
    use crate::planning::{
        ADAPTER_INVOCATIONS_SCHEMA_VERSION, AdapterInvocationPlan, AllocatedMethod,
        AllocatedProcedureTask, AllocatedRequirementBinding, MaterialLotBuildInventory,
        PlanningTaskInput, PlanningTaskOutput, PlanningValueSource,
    };

    fn id(value: &str) -> LocalId {
        LocalId::new(value).unwrap()
    }

    fn fixture() -> (AdapterInvocationPlan, AdapterInvocation) {
        let adapter = InvocationAdapter {
            driver: "opentrons.ot2".to_owned(),
            profile_path: PathBuf::from("adapters/ot2.toml"),
            profile_sha256: "a".repeat(64),
            features: BTreeSet::new(),
            accepted_run_formats: BTreeSet::new(),
            emitted_run_formats: BTreeSet::from(["opentrons.python-protocol".to_owned()]),
        };
        let first_task = id("method::first");
        let second_task = id("method::second");
        let first_requirement = id("method::first::requirement::transfer");
        let second_requirement = id("method::second::requirement::temperature");
        let requirement = |id: LocalId, capability: &str| AllocatedRequirementBinding {
            id,
            capability_kind: CapabilityKind::new(capability).unwrap(),
            minimum_qualification: QualificationLevel::Executable,
            accepted_control_modes: BTreeSet::from([ControlMode::ReviewedFile]),
            offering: "https://example.org/facility/ot2/offering".to_owned(),
            asset: "https://example.org/facility/ot2".to_owned(),
            observed_qualification: QualificationLevel::Executable.to_string(),
            control_mode: ControlMode::ReviewedFile.to_string(),
            parameters: Vec::new(),
            procedure_implementation: None,
            adapter: Some(adapter.clone()),
        };
        let state = AbsoluteIri::new("https://example.org/material/sample").unwrap();
        let first = AllocatedProcedureTask {
            id: first_task.clone(),
            operation: OperationId::new("https://example.org/procedure/first").unwrap(),
            program: None,
            inputs: Vec::new(),
            outputs: vec![PlanningTaskOutput {
                name: id("sample"),
                port_type: PortType::Material {
                    state: state.clone(),
                },
            }],
            parameters: Vec::new(),
            materials: Vec::new(),
            requirements: vec![requirement(
                first_requirement.clone(),
                "https://sbol.io/ns/capability#LiquidTransfer",
            )],
        };
        let second = AllocatedProcedureTask {
            id: second_task.clone(),
            operation: OperationId::new("https://example.org/procedure/second").unwrap(),
            program: None,
            inputs: vec![PlanningTaskInput {
                source: PlanningValueSource::TaskOutput {
                    task: first_task.clone(),
                    output: id("sample"),
                },
                port_type: PortType::Material { state },
            }],
            outputs: Vec::new(),
            parameters: Vec::new(),
            materials: Vec::new(),
            requirements: vec![requirement(
                second_requirement.clone(),
                "https://sbol.io/ns/capability#BlockTemperatureControl",
            )],
        };
        let invocation = AdapterInvocation {
            id: crate::planning::adapter_invocation_id(
                "https://example.org/facility/ot2",
                &adapter,
            ),
            asset: "https://example.org/facility/ot2".to_owned(),
            adapter,
            tasks: vec![first_task, second_task],
            requirements: vec![first_requirement, second_requirement],
        };
        let plan = AdapterInvocationPlan {
            schema_version: ADAPTER_INVOCATIONS_SCHEMA_VERSION.to_owned(),
            problem_sha256: "b".repeat(64),
            allocated_lair_sha256: "c".repeat(64),
            inventory_sha256: "d".repeat(64),
            facility: "https://example.org/facility".to_owned(),
            material_inventory: MaterialLotBuildInventory {
                source_sha256: "d".repeat(64),
                facility: "https://example.org/facility".to_owned(),
                materials: BTreeMap::new(),
                artifacts: BTreeMap::new(),
            },
            methods: vec![AllocatedMethod {
                choice: id("method"),
                source_operation: IntentOperationId::new("https://example.org/intent").unwrap(),
                method: MethodId::new("https://example.org/method").unwrap(),
                after: Vec::new(),
                inputs: Vec::new(),
                outputs: Vec::new(),
                yields: Vec::new(),
                tasks: vec![first, second],
            }],
            invocations: vec![invocation.clone()],
        };
        plan.validate().unwrap();
        (plan, invocation)
    }

    fn groups(invocation: &AdapterInvocation) -> Vec<AllocatedExecutionGroup> {
        vec![
            AllocatedExecutionGroup {
                id: "prepare".to_owned(),
                after: Vec::new(),
                tasks: vec![invocation.tasks[0].clone()],
                requirements: vec![invocation.requirements[0].clone()],
            },
            AllocatedExecutionGroup {
                id: "process".to_owned(),
                after: vec!["prepare".to_owned()],
                tasks: vec![invocation.tasks[1].clone()],
                requirements: vec![invocation.requirements[1].clone()],
            },
        ]
    }

    #[test]
    fn validates_grouped_tasks_and_persistent_locations() {
        let (plan, invocation) = fixture();
        let schedule = AllocatedProcedureSchedule::new(
            &plan,
            &invocation,
            groups(&invocation),
            vec![ScheduledPhysicalLocation {
                value: ScheduledValueRef::TaskOutput {
                    task: invocation.tasks[0].clone(),
                    output: id("sample"),
                },
                resource: "thermocycler-plate".to_owned(),
                positions: vec!["A1".to_owned(), "B1".to_owned()],
            }],
        )
        .unwrap();

        assert_eq!(
            schedule.schema_version,
            ALLOCATED_PROCEDURE_SCHEDULE_SCHEMA_VERSION
        );
        schedule.validate_against(&plan, &invocation).unwrap();
    }

    #[test]
    fn rejects_requirement_membership_that_splits_tasks() {
        let (plan, invocation) = fixture();
        let mut schedule =
            AllocatedProcedureSchedule::new(&plan, &invocation, groups(&invocation), Vec::new())
                .unwrap();
        let first = schedule.groups[0].requirements[0].clone();
        schedule.groups[0].requirements[0] = schedule.groups[1].requirements[0].clone();
        schedule.groups[1].requirements[0] = first;

        assert!(matches!(
            schedule.validate_against(&plan, &invocation),
            Err(AllocatedProcedureScheduleError::SplitTask { .. })
        ));
    }

    #[test]
    fn rejects_cycles_between_execution_groups() {
        let (plan, invocation) = fixture();
        let mut scheduled_groups = groups(&invocation);
        scheduled_groups[0].after.push("process".to_owned());
        let schedule = AllocatedProcedureSchedule {
            schema_version: ALLOCATED_PROCEDURE_SCHEDULE_SCHEMA_VERSION.to_owned(),
            invocation_plan_sha256: plan.sha256(),
            invocation: invocation.id.clone(),
            asset: invocation.asset.clone(),
            adapter: invocation.adapter.clone(),
            groups: scheduled_groups,
            locations: Vec::new(),
        };

        assert_eq!(
            schedule.validate_against(&plan, &invocation),
            Err(AllocatedProcedureScheduleError::CyclicGroups)
        );
    }
}
