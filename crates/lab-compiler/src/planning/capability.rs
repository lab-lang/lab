use std::collections::BTreeSet;

use lab_language::{
    CheckedActionArgument, CheckedDeclaration, CheckedField, CheckedModule, CheckedStatement,
    CheckedType, OwnershipMode, ResolvedAction, TypedExpression, is_absolute_iri,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CAPABILITY_REQUIREMENTS_SCHEMA_VERSION: &str = "lab.capability-requirements.v1";

/// Requirement templates derived from checked workflow definitions.
///
/// These are not allocations. A later planning pass instantiates reachable workflow templates,
/// refines composite requirements, and binds the resulting operational requirements to exact
/// SBOLInventory offerings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRequirements {
    pub schema_version: String,
    pub requirements: Vec<CapabilityRequirement>,
}

impl CapabilityRequirements {
    pub fn extract(modules: &[&CheckedModule]) -> Result<Self, CapabilityRequirementError> {
        let mut requirements = Vec::new();
        for module in modules {
            for declaration in &module.declarations {
                let CheckedDeclaration::Workflow { name, body, .. } = declaration else {
                    continue;
                };
                collect_block(
                    module.module.as_str(),
                    name,
                    StatementBlock::WorkflowBody,
                    body,
                    &mut Vec::new(),
                    &mut requirements,
                )?;
            }
        }
        requirements.sort_by(|left, right| left.id.cmp(&right.id));
        if let Some(duplicate) = requirements
            .windows(2)
            .find(|pair| pair[0].id == pair[1].id)
            .map(|pair| pair[0].id.clone())
        {
            return Err(CapabilityRequirementError::DuplicateId { id: duplicate });
        }
        Ok(Self {
            schema_version: CAPABILITY_REQUIREMENTS_SCHEMA_VERSION.to_owned(),
            requirements,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRequirement {
    pub id: String,
    pub source: CapabilityRequirementSource,
    pub capability_kind: String,
    pub minimum_qualification: RequirementQualification,
    pub accepted_control_modes: BTreeSet<RequirementControlMode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameter_constraints: Vec<CapabilityParameterConstraint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub value_inputs: Vec<CapabilityValueInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub material_inputs: Vec<CapabilityMaterialInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub value_outputs: Vec<CapabilityValueOutput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub material_outputs: Vec<CapabilityMaterialOutput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_requirement: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRequirementSource {
    pub module: String,
    pub workflow: String,
    pub statement_path: Vec<StatementPathSegment>,
    pub operation: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatementPathSegment {
    pub block: StatementBlock,
    pub index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StatementBlock {
    WorkflowBody,
    IfBody,
    ElseBody,
    MatchCase { case: usize },
    ForBody,
    WhenBody,
}

impl StatementBlock {
    fn id_label(self) -> String {
        match self {
            Self::WorkflowBody => "body".to_owned(),
            Self::IfBody => "then".to_owned(),
            Self::ElseBody => "else".to_owned(),
            Self::MatchCase { case } => format!("case-{case}"),
            Self::ForBody => "for".to_owned(),
            Self::WhenBody => "when".to_owned(),
        }
    }
}

/// The closed SBOLInventory qualification vocabulary, used here as a typed minimum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RequirementQualification {
    #[serde(rename = "https://draggon.org/ns/facility#Discovered")]
    Discovered,
    #[serde(rename = "https://draggon.org/ns/facility#Described")]
    Described,
    #[serde(rename = "https://draggon.org/ns/facility#Plannable")]
    Plannable,
    #[serde(rename = "https://draggon.org/ns/facility#Simulatable")]
    Simulatable,
    #[serde(rename = "https://draggon.org/ns/facility#Executable")]
    Executable,
    #[serde(rename = "https://draggon.org/ns/facility#Qualified")]
    Qualified,
}

/// The closed SBOLInventory control-mode vocabulary, used here as a typed accepted set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RequirementControlMode {
    #[serde(rename = "https://draggon.org/ns/facility#UnspecifiedControl")]
    Unspecified,
    #[serde(rename = "https://draggon.org/ns/facility#ManualControl")]
    Manual,
    #[serde(rename = "https://draggon.org/ns/facility#ReviewedFileControl")]
    ReviewedFile,
    #[serde(rename = "https://draggon.org/ns/facility#VendorSessionControl")]
    VendorSession,
    #[serde(rename = "https://draggon.org/ns/facility#ApiControl")]
    Api,
    #[serde(rename = "https://draggon.org/ns/facility#SiLA2Control")]
    Sila2,
    #[serde(rename = "https://draggon.org/ns/facility#OpcUaControl")]
    OpcUa,
}

impl RequirementControlMode {
    const CONCRETE: [Self; 6] = [
        Self::Manual,
        Self::ReviewedFile,
        Self::VendorSession,
        Self::Api,
        Self::Sila2,
        Self::OpcUa,
    ];
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityParameterConstraint {
    pub argument: String,
    pub relation: ParameterRelation,
    pub value: TypedExpression,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterRelation {
    Exact,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityMaterialInput {
    pub argument: String,
    pub ownership: OwnershipMode,
    pub value: TypedExpression,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityValueInput {
    pub argument: String,
    pub ownership: OwnershipMode,
    pub value: TypedExpression,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityMaterialOutput {
    pub binding: String,
    pub result: String,
    pub r#type: CheckedType,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityValueOutput {
    pub binding: String,
    pub result: String,
    pub r#type: CheckedType,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CapabilityRequirementError {
    #[error("action operation `{operation}` has non-absolute capability kind `{capability_kind}`")]
    InvalidCapabilityKind {
        operation: String,
        capability_kind: String,
    },
    #[error("capability requirement ID `{id}` occurs more than once")]
    DuplicateId { id: String },
}

fn collect_block(
    module: &str,
    workflow: &str,
    block: StatementBlock,
    statements: &[CheckedStatement],
    path: &mut Vec<StatementPathSegment>,
    requirements: &mut Vec<CapabilityRequirement>,
) -> Result<(), CapabilityRequirementError> {
    for (index, statement) in statements.iter().enumerate() {
        path.push(StatementPathSegment { block, index });
        match statement {
            CheckedStatement::Effect { results, action } => {
                if let Some(requirement) = requirement(module, workflow, path, results, action)? {
                    requirements.push(requirement);
                }
            }
            CheckedStatement::If {
                body, else_body, ..
            } => {
                collect_block(
                    module,
                    workflow,
                    StatementBlock::IfBody,
                    body,
                    path,
                    requirements,
                )?;
                collect_block(
                    module,
                    workflow,
                    StatementBlock::ElseBody,
                    else_body,
                    path,
                    requirements,
                )?;
            }
            CheckedStatement::Match { cases, .. } => {
                for (case, branch) in cases.iter().enumerate() {
                    collect_block(
                        module,
                        workflow,
                        StatementBlock::MatchCase { case },
                        &branch.body,
                        path,
                        requirements,
                    )?;
                }
            }
            CheckedStatement::For { body, .. } => collect_block(
                module,
                workflow,
                StatementBlock::ForBody,
                body,
                path,
                requirements,
            )?,
            CheckedStatement::When { body, .. } => collect_block(
                module,
                workflow,
                StatementBlock::WhenBody,
                body,
                path,
                requirements,
            )?,
            CheckedStatement::Binding(_)
            | CheckedStatement::StateUpdate { .. }
            | CheckedStatement::Return { .. }
            | CheckedStatement::Emit { .. } => {}
        }
        path.pop();
    }
    Ok(())
}

fn requirement(
    module: &str,
    workflow: &str,
    path: &[StatementPathSegment],
    bindings: &[CheckedField],
    action: &ResolvedAction,
) -> Result<Option<CapabilityRequirement>, CapabilityRequirementError> {
    let Some(capability_kind) = action.capability.as_ref() else {
        return Ok(None);
    };
    if !is_absolute_iri(capability_kind) {
        return Err(CapabilityRequirementError::InvalidCapabilityKind {
            operation: action.operation.clone(),
            capability_kind: capability_kind.clone(),
        });
    }
    let (material_inputs, value_inputs, parameter_constraints) =
        partition_arguments(&action.arguments);
    let mut material_outputs = Vec::new();
    let mut value_outputs = Vec::new();
    for (binding, result) in bindings.iter().zip(&action.results) {
        if contains_material(&binding.r#type) {
            material_outputs.push(CapabilityMaterialOutput {
                binding: binding.name.clone(),
                result: result.name.clone(),
                r#type: binding.r#type.clone(),
            });
        } else {
            value_outputs.push(CapabilityValueOutput {
                binding: binding.name.clone(),
                result: result.name.clone(),
                r#type: binding.r#type.clone(),
            });
        }
    }
    Ok(Some(CapabilityRequirement {
        id: requirement_id(module, workflow, path),
        source: CapabilityRequirementSource {
            module: module.to_owned(),
            workflow: workflow.to_owned(),
            statement_path: path.to_vec(),
            operation: action.operation.clone(),
        },
        capability_kind: capability_kind.clone(),
        minimum_qualification: RequirementQualification::Plannable,
        accepted_control_modes: RequirementControlMode::CONCRETE.into_iter().collect(),
        parameter_constraints,
        value_inputs,
        material_inputs,
        value_outputs,
        material_outputs,
        parent_requirement: None,
    }))
}

fn partition_arguments(
    arguments: &[CheckedActionArgument],
) -> (
    Vec<CapabilityMaterialInput>,
    Vec<CapabilityValueInput>,
    Vec<CapabilityParameterConstraint>,
) {
    let mut materials = Vec::new();
    let mut values = Vec::new();
    let mut parameters = Vec::new();
    for argument in arguments {
        if contains_material(&argument.value.r#type) {
            materials.push(CapabilityMaterialInput {
                argument: argument.name.clone(),
                ownership: argument.mode,
                value: argument.value.clone(),
            });
        } else if is_parameter_type(&argument.value.r#type) {
            parameters.push(CapabilityParameterConstraint {
                argument: argument.name.clone(),
                relation: ParameterRelation::Exact,
                value: argument.value.clone(),
            });
        } else {
            values.push(CapabilityValueInput {
                argument: argument.name.clone(),
                ownership: argument.mode,
                value: argument.value.clone(),
            });
        }
    }
    (materials, values, parameters)
}

fn contains_material(r#type: &CheckedType) -> bool {
    match r#type {
        CheckedType::Named { name, .. } => name == "Material",
        CheckedType::Union { alternatives } => alternatives.iter().any(contains_material),
        CheckedType::List { element } => contains_material(element),
        CheckedType::Quantity { .. }
        | CheckedType::Any { .. }
        | CheckedType::Integer
        | CheckedType::Decimal
        | CheckedType::String
        | CheckedType::Bool
        | CheckedType::None => false,
    }
}

fn is_parameter_type(r#type: &CheckedType) -> bool {
    match r#type {
        CheckedType::Quantity { .. }
        | CheckedType::Integer
        | CheckedType::Decimal
        | CheckedType::String
        | CheckedType::Bool => true,
        CheckedType::Union { alternatives } => alternatives.iter().all(is_parameter_type),
        CheckedType::List { element } => is_parameter_type(element),
        CheckedType::Named { .. } | CheckedType::Any { .. } | CheckedType::None => false,
    }
}

fn requirement_id(module: &str, workflow: &str, path: &[StatementPathSegment]) -> String {
    let path = path
        .iter()
        .map(|segment| format!("{}[{}]", segment.block.id_label(), segment.index))
        .collect::<Vec<_>>()
        .join("/");
    format!("{module}::{workflow}::{path}")
}

#[cfg(test)]
mod tests {
    use lab_language::{CheckedDeclaration, CheckedStatement, compile_module};

    use super::*;

    const SOURCE: &str = r#"use std.lab.plasmid

workflow preserve(plasmid: Material<Plasmid>) -> Material<Plasmid>:
  stored <- store plasmid at -80 C
  return stored
"#;

    #[test]
    fn extracts_typed_requirement_facts_without_allocating_an_asset() {
        let module = compile_module(SOURCE).unwrap();

        let catalog = CapabilityRequirements::extract(&[&module]).unwrap();

        assert_eq!(
            catalog.schema_version,
            CAPABILITY_REQUIREMENTS_SCHEMA_VERSION
        );
        assert_eq!(catalog.requirements.len(), 1);
        let requirement = &catalog.requirements[0];
        assert_eq!(requirement.id, "standalone::preserve::body[0]");
        assert_eq!(
            requirement.capability_kind,
            "https://draggon.org/ns/capability#ColdStorage"
        );
        assert_eq!(
            requirement.minimum_qualification,
            RequirementQualification::Plannable
        );
        assert_eq!(requirement.accepted_control_modes.len(), 6);
        assert_eq!(requirement.material_inputs.len(), 1);
        assert_eq!(requirement.material_inputs[0].argument, "material");
        assert_eq!(
            requirement.material_inputs[0].ownership,
            OwnershipMode::Take
        );
        assert_eq!(requirement.material_outputs.len(), 1);
        assert_eq!(requirement.material_outputs[0].binding, "stored");
        assert_eq!(requirement.material_outputs[0].result, "material");
        assert!(requirement.value_inputs.is_empty());
        assert!(requirement.value_outputs.is_empty());
        assert_eq!(requirement.parameter_constraints.len(), 1);
        assert_eq!(requirement.parameter_constraints[0].argument, "temperature");
        assert_eq!(
            requirement.parameter_constraints[0].value.r#type,
            CheckedType::Quantity {
                unit: "C".to_owned()
            }
        );
        assert!(requirement.parent_requirement.is_none());

        let json = serde_json::to_string(&catalog).unwrap();
        assert!(json.contains("https://draggon.org/ns/facility#Plannable"));
        assert!(json.contains("https://draggon.org/ns/facility#ManualControl"));
        assert_eq!(
            serde_json::from_str::<CapabilityRequirements>(&json).unwrap(),
            catalog
        );
    }

    #[test]
    fn workflow_calls_do_not_duplicate_the_callees_requirement_template() {
        let module = compile_module(
            r#"use std.lab.plasmid

workflow preserve(plasmid: Material<Plasmid>) -> Material<Plasmid>:
  stored <- store plasmid at -80 C
  return stored

workflow main(plasmid: Material<Plasmid>) -> Material<Plasmid>:
  stored <- preserve plasmid
  return stored
"#,
        )
        .unwrap();

        let catalog = CapabilityRequirements::extract(&[&module]).unwrap();

        assert_eq!(catalog.requirements.len(), 1);
        assert_eq!(catalog.requirements[0].source.workflow, "preserve");
    }

    #[test]
    fn rejects_a_non_absolute_capability_even_in_previously_checked_ir() {
        let mut module = compile_module(SOURCE).unwrap();
        let CheckedDeclaration::Workflow { body, .. } = &mut module.declarations[0] else {
            panic!("the fixture begins with a workflow")
        };
        let CheckedStatement::Effect { action, .. } = &mut body[0] else {
            panic!("the workflow begins with an effect")
        };
        action.capability = Some("cold_storage".to_owned());

        let error = CapabilityRequirements::extract(&[&module]).unwrap_err();

        assert_eq!(
            error,
            CapabilityRequirementError::InvalidCapabilityKind {
                operation: "std.lab.plasmid.store".to_owned(),
                capability_kind: "cold_storage".to_owned(),
            }
        );
    }
}
