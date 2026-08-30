//! Borrowed, validated views over one exact adapter invocation.
//!
//! Facility planning has already selected Methods, MaterialLots, capability offerings, Assets,
//! and adapters before this module runs. These helpers let a concrete adapter read only the
//! Procedure tasks and requirements assigned to its immutable invocation. They deliberately do
//! not reconstruct Workflow intent, perform allocation, or define an adapter-independent device
//! plan.

use std::collections::BTreeSet;

use lab_capability::ScalarValue;
use lab_method::ProcedureValue;

use crate::planning::{
    AdapterInvocation, AdapterInvocationPlan, AllocatedProcedureTask, AllocatedRequirementBinding,
    PlanningProcedureParameter, SelectedMaterialBinding,
};

/// One Procedure task paired with every requirement this invocation implements atomically.
///
/// The current built-in automation adapters lower independently executable documents. They must
/// therefore reject a task whose requirements were split across invocations. A normalized
/// Procedure program may have several derived capability clauses, but they arrive here only after
/// allocation proved that one exact Asset, adapter, and Procedure implementation owns them all.
pub(crate) struct ExactInvocationTask<'a> {
    pub(crate) task: &'a AllocatedProcedureTask,
    pub(crate) requirements: Vec<&'a AllocatedRequirementBinding>,
}

/// Resolve every task in an invocation to all of its requirements without accepting split work.
pub(crate) fn exact_invocation_tasks<'a>(
    adapter: &str,
    plan: &'a AdapterInvocationPlan,
    invocation: &AdapterInvocation,
) -> Result<Vec<ExactInvocationTask<'a>>, String> {
    let task_ids = invocation.tasks.iter().collect::<BTreeSet<_>>();
    let requirement_ids = invocation.requirements.iter().collect::<BTreeSet<_>>();
    let mut members = Vec::new();
    for task in plan
        .methods
        .iter()
        .flat_map(|method| method.tasks.iter())
        .filter(|task| task_ids.contains(&task.id))
    {
        let selected = task
            .requirements
            .iter()
            .filter(|requirement| requirement_ids.contains(&requirement.id))
            .collect::<Vec<_>>();
        if selected.is_empty() || selected.len() != task.requirements.len() {
            return Err(format!(
                "{adapter} Procedure task '{}' must be owned atomically by this invocation; found {} task requirements and {} in this invocation",
                task.id,
                task.requirements.len(),
                selected.len()
            ));
        }
        members.push(ExactInvocationTask {
            task,
            requirements: selected,
        });
    }
    let resolved_requirements = members
        .iter()
        .map(|member| member.requirements.len())
        .sum::<usize>();
    if members.len() != invocation.tasks.len()
        || resolved_requirements != invocation.requirements.len()
    {
        return Err(format!(
            "{adapter} invocation '{}' does not map every exact requirement to its complete Procedure task",
            invocation.id
        ));
    }
    Ok(members)
}

/// Typed access to one allocated Procedure task.
///
/// IDs remain the authoritative schema. Parameter access uses their stable local suffixes because
/// task IDs are namespaced by the chosen Method during LAIR projection.
pub(crate) struct ProcedureTaskView<'adapter, 'task> {
    adapter: &'adapter str,
    task: &'task AllocatedProcedureTask,
}

impl<'adapter, 'task> ProcedureTaskView<'adapter, 'task> {
    pub(crate) fn new(adapter: &'adapter str, task: &'task AllocatedProcedureTask) -> Self {
        Self { adapter, task }
    }

    pub(crate) fn require_material_roles(&self, allowed: &[&str]) -> Result<(), String> {
        if let Some((material, role)) = self
            .task
            .materials
            .iter()
            .filter_map(|material| material_role(material).map(|role| (material, role)))
            .find(|(_, role)| !allowed.contains(role))
        {
            return Err(format!(
                "{} Procedure task '{}' has unsupported material role '{}' at '{}'",
                self.adapter, self.task.id, role, material.input
            ));
        }
        if let Some(material) = self
            .task
            .materials
            .iter()
            .find(|material| material_role(material).is_none())
        {
            return Err(format!(
                "{} Procedure task '{}' has malformed material input '{}'",
                self.adapter, self.task.id, material.input
            ));
        }
        Ok(())
    }

    pub(crate) fn materials(&self, role: &str) -> Vec<&'task SelectedMaterialBinding> {
        self.task
            .materials
            .iter()
            .filter(|material| material_role(material) == Some(role))
            .collect()
    }

    pub(crate) fn one_material(
        &self,
        role: &str,
    ) -> Result<&'task SelectedMaterialBinding, String> {
        let materials = self.materials(role);
        if materials.len() != 1 {
            return Err(format!(
                "{} Procedure task '{}' requires exactly one '{role}' material, found {}",
                self.adapter,
                self.task.id,
                materials.len()
            ));
        }
        Ok(materials[0])
    }

    pub(crate) fn text_parameter(&self, name: &str) -> Result<String, String> {
        let parameter = self.parameter(name)?;
        let ProcedureValue::Scalar { value: property } = &parameter.value else {
            return Err(self.parameter_type_error(name, "a text scalar"));
        };
        let ScalarValue::Text(value) = &property.value else {
            return Err(self.parameter_type_error(name, "a text scalar"));
        };
        if value.is_empty() || property.unit.is_some() {
            return Err(self.parameter_type_error(name, "unitless non-empty text"));
        }
        Ok(value.clone())
    }

    pub(crate) fn capacity_error(
        &self,
        resource: &str,
        required: usize,
        capacity: usize,
    ) -> String {
        format!(
            "{} Procedure task '{}' requires {required} {resource} positions, but the exact adapter profile provides {capacity}",
            self.adapter, self.task.id
        )
    }

    fn parameter(&self, name: &str) -> Result<&'task PlanningProcedureParameter, String> {
        let suffix = format!("::parameter::{name}");
        let matches = self
            .task
            .parameters
            .iter()
            .filter(|parameter| parameter.id.as_str().ends_with(&suffix))
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(format!(
                "{} Procedure task '{}' requires exactly one parameter '{name}', found {}",
                self.adapter,
                self.task.id,
                matches.len()
            ));
        }
        Ok(matches[0])
    }

    fn parameter_type_error(&self, name: &str, expected: &str) -> String {
        format!(
            "{} Procedure task '{}' parameter '{name}' must be {expected}",
            self.adapter, self.task.id
        )
    }
}

pub(crate) fn material_role(material: &SelectedMaterialBinding) -> Option<&str> {
    material
        .input
        .as_str()
        .rsplit_once("::material::")
        .map(|(_, role)| role.split("::").next().unwrap_or(role))
}
