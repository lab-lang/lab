use lab_capability::{ExactDecimal, ScalarValue};
use lab_method::ProcedureValue;
use lab_procedure::ProcedureLocalId;

use super::{ProcedureTaskInstance, ResolvedProcedureMaterial};

pub(super) struct TaskView<'task, 'instance> {
    task: &'task ProcedureTaskInstance<'instance>,
}

impl<'task, 'instance> TaskView<'task, 'instance> {
    pub(super) fn new(task: &'task ProcedureTaskInstance<'instance>) -> Self {
        Self { task }
    }

    fn parameter(&self, name: &str) -> Result<&ProcedureValue, String> {
        let suffix = format!("::parameter::{name}");
        let matches = self
            .task
            .parameters
            .iter()
            .filter(|parameter| parameter.id.as_str().ends_with(&suffix))
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(format!(
                "expected exactly one parameter `{name}`, found {}",
                matches.len()
            ));
        }
        Ok(&matches[0].value)
    }

    pub(super) fn integer_parameter(
        &self,
        name: &str,
        expected_unit: Option<&str>,
    ) -> Result<u32, String> {
        let ProcedureValue::Scalar { value } = self.parameter(name)? else {
            return Err(format!("parameter `{name}` must be an integer scalar"));
        };
        let ScalarValue::Integer(integer) = &value.value else {
            return Err(format!("parameter `{name}` must be an integer scalar"));
        };
        if value.unit.as_ref().map(|unit| unit.as_str()) != expected_unit {
            return Err(format!(
                "parameter `{name}` must use unit {expected_unit:?}, found {:?}",
                value.unit.as_ref().map(|unit| unit.as_str())
            ));
        }
        integer
            .as_str()
            .parse::<u32>()
            .map_err(|_| format!("parameter `{name}` must fit the unsigned 32-bit range"))
    }

    pub(super) fn decimal_parameter(
        &self,
        name: &str,
    ) -> Result<(ExactDecimal, Option<&str>), String> {
        let ProcedureValue::Scalar { value } = self.parameter(name)? else {
            return Err(format!("parameter `{name}` must be a numeric scalar"));
        };
        let decimal = match &value.value {
            ScalarValue::Integer(integer) => ExactDecimal::from_integer(integer),
            ScalarValue::Real(decimal) => decimal.clone(),
            _ => return Err(format!("parameter `{name}` must be a numeric scalar")),
        };
        Ok((decimal, value.unit.as_ref().map(|unit| unit.as_str())))
    }

    pub(super) fn text_parameter(&self, name: &str) -> Result<String, String> {
        let ProcedureValue::Scalar { value: property } = self.parameter(name)? else {
            return Err(format!("parameter `{name}` must be a text scalar"));
        };
        let ScalarValue::Text(value) = &property.value else {
            return Err(format!("parameter `{name}` must be a text scalar"));
        };
        if value.is_empty() || property.unit.is_some() {
            return Err(format!(
                "parameter `{name}` must be non-empty, unitless text"
            ));
        }
        Ok(value.clone())
    }

    pub(super) fn text_list_parameter(&self, name: &str) -> Result<Vec<String>, String> {
        let ProcedureValue::List { values, .. } = self.parameter(name)? else {
            return Err(format!("parameter `{name}` must be a text list"));
        };
        values
            .iter()
            .map(|property| {
                let ScalarValue::Text(value) = &property.value else {
                    return Err(format!("parameter `{name}` must be a text list"));
                };
                if value.is_empty() || property.unit.is_some() {
                    return Err(format!(
                        "parameter `{name}` must contain non-empty, unitless text"
                    ));
                }
                Ok(value.clone())
            })
            .collect()
    }

    pub(super) fn materials(&self, role: &str) -> Vec<&ResolvedProcedureMaterial> {
        self.task
            .materials
            .iter()
            .filter(|material| material_role(&material.id) == Some(role))
            .collect()
    }

    pub(super) fn one_material(&self, role: &str) -> Result<&ResolvedProcedureMaterial, String> {
        let materials = self.materials(role);
        if materials.len() != 1 {
            return Err(format!(
                "expected exactly one `{role}` material input, found {}",
                materials.len()
            ));
        }
        Ok(materials[0])
    }

    pub(super) fn require_material_roles(&self, allowed: &[&str]) -> Result<(), String> {
        for material in self.task.materials {
            let Some(role) = material_role(&material.id) else {
                return Err(format!(
                    "material input `{}` has no stable role",
                    material.id
                ));
            };
            if !allowed.contains(&role) {
                return Err(format!(
                    "material input `{}` has unsupported role `{role}`",
                    material.id
                ));
            }
        }
        Ok(())
    }
}

fn material_role(id: &lab_method::LocalId) -> Option<&str> {
    id.as_str()
        .rsplit_once("::material::")
        .map(|(_, role)| role.split("::").next().unwrap_or(role))
}

pub(super) fn material_symbols(materials: &[&ResolvedProcedureMaterial]) -> Vec<String> {
    materials
        .iter()
        .map(|material| material.symbol.clone())
        .collect()
}

pub(super) fn procedure_id(value: &str) -> Result<ProcedureLocalId, String> {
    ProcedureLocalId::new(value).map_err(|error| error.to_string())
}
