use std::collections::{BTreeMap, BTreeSet};

use lab_language::{
    CheckedActionArgument, CheckedDeclaration, CheckedExpression, CheckedField, CheckedModule,
    CheckedStatement, CheckedType, OwnershipMode, ResolvedAction, TypedExpression, is_absolute_iri,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CAPABILITY_REQUIREMENTS_SCHEMA_VERSION: &str = "lab.capability-requirements.v2";
pub const CAPABILITY_REQUIREMENT_INSTANCES_SCHEMA_VERSION: &str =
    "lab.capability-requirement-instances.v2";

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

    /// Instantiates only requirement templates reachable from one exact entry workflow.
    ///
    /// A workflow invoked twice produces two instances. Structural branches and loops are
    /// retained conservatively as potential work, while recursive workflow expansion is rejected
    /// because it cannot produce a finite reviewed plan.
    pub fn instantiate_reachable(
        &self,
        modules: &[&CheckedModule],
        entry_module: &str,
        entry_workflow: &str,
    ) -> Result<CapabilityRequirementInstances, CapabilityInstantiationError> {
        let entry = WorkflowIdentity {
            module: entry_module.to_owned(),
            workflow: entry_workflow.to_owned(),
        };
        let workflows = workflow_bodies(modules)?;
        if !workflows.contains_key(&(entry.module.clone(), entry.workflow.clone())) {
            return Err(CapabilityInstantiationError::MissingEntryWorkflow {
                module: entry.module,
                workflow: entry.workflow,
            });
        }
        let templates = self
            .requirements
            .iter()
            .map(|requirement| (requirement.id.clone(), requirement))
            .collect::<BTreeMap<_, _>>();
        let mut instances = Vec::new();
        instantiate_workflow(
            &entry,
            &entry,
            &workflows,
            &templates,
            &mut Vec::new(),
            &mut Vec::new(),
            &mut instances,
        )?;
        let mut seen = BTreeSet::new();
        if let Some(id) = instances
            .iter()
            .find_map(|instance| (!seen.insert(instance.id.clone())).then(|| instance.id.clone()))
        {
            return Err(CapabilityInstantiationError::DuplicateInstanceId { id });
        }
        Ok(CapabilityRequirementInstances {
            schema_version: CAPABILITY_REQUIREMENT_INSTANCES_SCHEMA_VERSION.to_owned(),
            requirements_schema_version: self.schema_version.clone(),
            entry,
            instances,
        })
    }
}

/// Requirement occurrences reachable from one exact package entry workflow.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRequirementInstances {
    pub schema_version: String,
    pub requirements_schema_version: String,
    pub entry: WorkflowIdentity,
    pub instances: Vec<CapabilityRequirementInstance>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WorkflowIdentity {
    pub module: String,
    pub workflow: String,
}

/// One distinct use of a requirement template in the entry workflow's static call expansion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRequirementInstance {
    pub id: String,
    pub template: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub call_path: Vec<WorkflowCallSite>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowCallSite {
    pub caller: WorkflowIdentity,
    pub statement_path: Vec<StatementPathSegment>,
    pub callee: WorkflowIdentity,
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
    #[serde(rename = "https://sbol.io/ns/facility#Discovered")]
    Discovered,
    #[serde(rename = "https://sbol.io/ns/facility#Described")]
    Described,
    #[serde(rename = "https://sbol.io/ns/facility#Plannable")]
    Plannable,
    #[serde(rename = "https://sbol.io/ns/facility#Simulatable")]
    Simulatable,
    #[serde(rename = "https://sbol.io/ns/facility#Executable")]
    Executable,
    #[serde(rename = "https://sbol.io/ns/facility#Qualified")]
    Qualified,
}

impl RequirementQualification {
    pub const fn iri(self) -> &'static str {
        match self {
            Self::Discovered => "https://sbol.io/ns/facility#Discovered",
            Self::Described => "https://sbol.io/ns/facility#Described",
            Self::Plannable => "https://sbol.io/ns/facility#Plannable",
            Self::Simulatable => "https://sbol.io/ns/facility#Simulatable",
            Self::Executable => "https://sbol.io/ns/facility#Executable",
            Self::Qualified => "https://sbol.io/ns/facility#Qualified",
        }
    }
}

/// The closed SBOLInventory control-mode vocabulary, used here as a typed accepted set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RequirementControlMode {
    #[serde(rename = "https://sbol.io/ns/facility#UnspecifiedControl")]
    Unspecified,
    #[serde(rename = "https://sbol.io/ns/facility#ManualControl")]
    Manual,
    #[serde(rename = "https://sbol.io/ns/facility#ReviewedFileControl")]
    ReviewedFile,
    #[serde(rename = "https://sbol.io/ns/facility#VendorSessionControl")]
    VendorSession,
    #[serde(rename = "https://sbol.io/ns/facility#ApiControl")]
    Api,
    #[serde(rename = "https://sbol.io/ns/facility#SiLA2Control")]
    Sila2,
    #[serde(rename = "https://sbol.io/ns/facility#OpcUaControl")]
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

    pub const fn iri(self) -> &'static str {
        match self {
            Self::Unspecified => "https://sbol.io/ns/facility#UnspecifiedControl",
            Self::Manual => "https://sbol.io/ns/facility#ManualControl",
            Self::ReviewedFile => "https://sbol.io/ns/facility#ReviewedFileControl",
            Self::VendorSession => "https://sbol.io/ns/facility#VendorSessionControl",
            Self::Api => "https://sbol.io/ns/facility#ApiControl",
            Self::Sila2 => "https://sbol.io/ns/facility#SiLA2Control",
            Self::OpcUa => "https://sbol.io/ns/facility#OpcUaControl",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityParameterConstraint {
    pub argument: String,
    pub property_kind: String,
    pub relation: ParameterRelation,
    pub value: TypedExpression,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
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
    #[error(
        "action operation `{operation}` parameter `{argument}` has non-absolute property kind `{property_kind}`"
    )]
    InvalidParameterKind {
        operation: String,
        argument: String,
        property_kind: String,
    },
    #[error(
        "action operation `{operation}` parameter `{argument}` uses unit `{unit}` without a canonical RDF unit IRI"
    )]
    UnknownParameterUnit {
        operation: String,
        argument: String,
        unit: String,
    },
    #[error("capability requirement ID `{id}` occurs more than once")]
    DuplicateId { id: String },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CapabilityInstantiationError {
    #[error("entry module `{module}` does not declare workflow `{workflow}`")]
    MissingEntryWorkflow { module: String, workflow: String },
    #[error("workflow `{module}::{workflow}` occurs more than once in checked modules")]
    DuplicateWorkflow { module: String, workflow: String },
    #[error(
        "workflow call at `{caller}` resolves to `{module}::{workflow}`, but its checked body is unavailable"
    )]
    MissingWorkflowBody {
        caller: String,
        module: String,
        workflow: String,
    },
    #[error("workflow call `{operation}` at `{caller}` has no resolved callee identity")]
    MissingCalleeIdentity { operation: String, caller: String },
    #[error("reachable capability template `{template}` is absent from the extracted catalog")]
    MissingTemplate { template: String },
    #[error("recursive workflow expansion cannot produce a finite plan: {cycle}")]
    RecursiveWorkflow { cycle: String },
    #[error("capability requirement instance ID `{id}` occurs more than once")]
    DuplicateInstanceId { id: String },
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
    let PartitionedArguments {
        materials: material_inputs,
        values: value_inputs,
        parameters: parameter_constraints,
    } = partition_arguments(action)?;
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

struct PartitionedArguments {
    materials: Vec<CapabilityMaterialInput>,
    values: Vec<CapabilityValueInput>,
    parameters: Vec<CapabilityParameterConstraint>,
}

fn partition_arguments(
    action: &ResolvedAction,
) -> Result<PartitionedArguments, CapabilityRequirementError> {
    let mut materials = Vec::new();
    let mut values = Vec::new();
    let mut parameters = Vec::new();
    for argument in &action.arguments {
        if contains_material(&argument.value.r#type) {
            materials.push(CapabilityMaterialInput {
                argument: argument.name.clone(),
                ownership: argument.mode,
                value: argument.value.clone(),
            });
        } else if let Some(property_kind) = &argument.parameter_kind {
            if !is_absolute_iri(property_kind) {
                return Err(CapabilityRequirementError::InvalidParameterKind {
                    operation: action.operation.clone(),
                    argument: argument.name.clone(),
                    property_kind: property_kind.clone(),
                });
            }
            parameters.push(CapabilityParameterConstraint {
                argument: argument.name.clone(),
                property_kind: property_kind.clone(),
                relation: ParameterRelation::Exact,
                value: argument.value.clone(),
                unit: canonical_parameter_unit(action, argument)?,
            });
        } else {
            values.push(CapabilityValueInput {
                argument: argument.name.clone(),
                ownership: argument.mode,
                value: argument.value.clone(),
            });
        }
    }
    Ok(PartitionedArguments {
        materials,
        values,
        parameters,
    })
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

fn canonical_parameter_unit(
    action: &ResolvedAction,
    argument: &CheckedActionArgument,
) -> Result<Option<String>, CapabilityRequirementError> {
    let CheckedExpression::Quantity { unit, .. } = &argument.value.value else {
        return Ok(None);
    };
    let iri = match unit.as_str() {
        "C" => "http://qudt.org/vocab/unit/DEG_C",
        "h" => "http://qudt.org/vocab/unit/HR",
        "min" => "http://qudt.org/vocab/unit/MIN",
        "uL" => "http://qudt.org/vocab/unit/MicroL",
        _ => {
            return Err(CapabilityRequirementError::UnknownParameterUnit {
                operation: action.operation.clone(),
                argument: argument.name.clone(),
                unit: unit.clone(),
            });
        }
    };
    Ok(Some(iri.to_owned()))
}

fn requirement_id(module: &str, workflow: &str, path: &[StatementPathSegment]) -> String {
    let path = render_statement_path(path);
    format!("{module}::{workflow}::{path}")
}

fn render_statement_path(path: &[StatementPathSegment]) -> String {
    path.iter()
        .map(|segment| format!("{}[{}]", segment.block.id_label(), segment.index))
        .collect::<Vec<_>>()
        .join("/")
}

fn workflow_bodies<'a>(
    modules: &[&'a CheckedModule],
) -> Result<BTreeMap<(String, String), &'a [CheckedStatement]>, CapabilityInstantiationError> {
    let mut workflows = BTreeMap::new();
    for module in modules {
        for declaration in &module.declarations {
            let CheckedDeclaration::Workflow { name, body, .. } = declaration else {
                continue;
            };
            let key = (module.module.as_str().to_owned(), name.clone());
            if workflows.insert(key.clone(), body.as_slice()).is_some() {
                return Err(CapabilityInstantiationError::DuplicateWorkflow {
                    module: key.0,
                    workflow: key.1,
                });
            }
        }
    }
    Ok(workflows)
}

#[allow(clippy::too_many_arguments)]
fn instantiate_workflow(
    entry: &WorkflowIdentity,
    current: &WorkflowIdentity,
    workflows: &BTreeMap<(String, String), &[CheckedStatement]>,
    templates: &BTreeMap<String, &CapabilityRequirement>,
    active: &mut Vec<WorkflowIdentity>,
    call_path: &mut Vec<WorkflowCallSite>,
    instances: &mut Vec<CapabilityRequirementInstance>,
) -> Result<(), CapabilityInstantiationError> {
    if let Some(index) = active.iter().position(|workflow| workflow == current) {
        let cycle = active[index..]
            .iter()
            .chain(std::iter::once(current))
            .map(|workflow| format!("{}::{}", workflow.module, workflow.workflow))
            .collect::<Vec<_>>()
            .join(" -> ");
        return Err(CapabilityInstantiationError::RecursiveWorkflow { cycle });
    }
    let key = (current.module.clone(), current.workflow.clone());
    let body =
        workflows
            .get(&key)
            .ok_or_else(|| CapabilityInstantiationError::MissingWorkflowBody {
                caller: call_path
                    .last()
                    .map(render_call_site)
                    .unwrap_or_else(|| format!("{}::{}", entry.module, entry.workflow)),
                module: current.module.clone(),
                workflow: current.workflow.clone(),
            })?;
    active.push(current.clone());
    let result = instantiate_block(
        entry,
        current,
        StatementBlock::WorkflowBody,
        body,
        workflows,
        templates,
        active,
        call_path,
        &mut Vec::new(),
        instances,
    );
    active.pop();
    result
}

#[allow(clippy::too_many_arguments)]
fn instantiate_block(
    entry: &WorkflowIdentity,
    current: &WorkflowIdentity,
    block: StatementBlock,
    statements: &[CheckedStatement],
    workflows: &BTreeMap<(String, String), &[CheckedStatement]>,
    templates: &BTreeMap<String, &CapabilityRequirement>,
    active: &mut Vec<WorkflowIdentity>,
    call_path: &mut Vec<WorkflowCallSite>,
    path: &mut Vec<StatementPathSegment>,
    instances: &mut Vec<CapabilityRequirementInstance>,
) -> Result<(), CapabilityInstantiationError> {
    for (index, statement) in statements.iter().enumerate() {
        path.push(StatementPathSegment { block, index });
        match statement {
            CheckedStatement::Effect { action, .. } => {
                if action.capability.is_some() {
                    let template = requirement_id(&current.module, &current.workflow, path);
                    if !templates.contains_key(&template) {
                        return Err(CapabilityInstantiationError::MissingTemplate { template });
                    }
                    instances.push(CapabilityRequirementInstance {
                        id: requirement_instance_id(entry, call_path, &template),
                        template,
                        call_path: call_path.clone(),
                    });
                }
                if let Some(callee) = &action.callee {
                    let callee = WorkflowIdentity {
                        module: callee.module.as_str().to_owned(),
                        workflow: callee.local.clone(),
                    };
                    let call_site = WorkflowCallSite {
                        caller: current.clone(),
                        statement_path: path.clone(),
                        callee: callee.clone(),
                    };
                    if !workflows.contains_key(&(callee.module.clone(), callee.workflow.clone())) {
                        return Err(CapabilityInstantiationError::MissingWorkflowBody {
                            caller: render_call_site(&call_site),
                            module: callee.module,
                            workflow: callee.workflow,
                        });
                    }
                    call_path.push(call_site);
                    let result = instantiate_workflow(
                        entry, &callee, workflows, templates, active, call_path, instances,
                    );
                    call_path.pop();
                    result?;
                } else if action.operation.starts_with("workflow.") {
                    return Err(CapabilityInstantiationError::MissingCalleeIdentity {
                        operation: action.operation.clone(),
                        caller: format!(
                            "{}::{}::{}",
                            current.module,
                            current.workflow,
                            render_statement_path(path)
                        ),
                    });
                }
            }
            CheckedStatement::If {
                body, else_body, ..
            } => {
                instantiate_block(
                    entry,
                    current,
                    StatementBlock::IfBody,
                    body,
                    workflows,
                    templates,
                    active,
                    call_path,
                    path,
                    instances,
                )?;
                instantiate_block(
                    entry,
                    current,
                    StatementBlock::ElseBody,
                    else_body,
                    workflows,
                    templates,
                    active,
                    call_path,
                    path,
                    instances,
                )?;
            }
            CheckedStatement::Match { cases, .. } => {
                for (case, branch) in cases.iter().enumerate() {
                    instantiate_block(
                        entry,
                        current,
                        StatementBlock::MatchCase { case },
                        &branch.body,
                        workflows,
                        templates,
                        active,
                        call_path,
                        path,
                        instances,
                    )?;
                }
            }
            CheckedStatement::For { body, .. } => instantiate_block(
                entry,
                current,
                StatementBlock::ForBody,
                body,
                workflows,
                templates,
                active,
                call_path,
                path,
                instances,
            )?,
            CheckedStatement::When { body, .. } => instantiate_block(
                entry,
                current,
                StatementBlock::WhenBody,
                body,
                workflows,
                templates,
                active,
                call_path,
                path,
                instances,
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

fn requirement_instance_id(
    entry: &WorkflowIdentity,
    call_path: &[WorkflowCallSite],
    template: &str,
) -> String {
    let mut parts = vec![format!("{}::{}", entry.module, entry.workflow)];
    parts.extend(call_path.iter().map(render_call_site));
    parts.push(template.to_owned());
    parts.join("/")
}

fn render_call_site(call: &WorkflowCallSite) -> String {
    format!(
        "{}::{}::{}=>{}::{}",
        call.caller.module,
        call.caller.workflow,
        render_statement_path(&call.statement_path),
        call.callee.module,
        call.callee.workflow
    )
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
            "https://sbol.io/ns/capability#ColdStorage"
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
            requirement.parameter_constraints[0].property_kind,
            "https://sbol.io/ns/capability#Temperature"
        );
        assert_eq!(
            requirement.parameter_constraints[0].unit.as_deref(),
            Some("http://qudt.org/vocab/unit/DEG_C")
        );
        assert_eq!(
            requirement.parameter_constraints[0].value.r#type,
            CheckedType::Quantity {
                unit: "C".to_owned()
            }
        );
        assert!(requirement.parent_requirement.is_none());

        let json = serde_json::to_string(&catalog).unwrap();
        assert!(json.contains("https://sbol.io/ns/facility#Plannable"));
        assert!(json.contains("https://sbol.io/ns/facility#ManualControl"));
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
    fn instantiates_only_reachable_requirements_and_distinguishes_two_call_sites() {
        let module = compile_module(
            r#"use std.lab.plasmid

workflow preserve(plasmid: Material<Plasmid>) -> Material<Plasmid>:
  stored <- store plasmid at -80 C
  return stored

workflow never_called(plasmid: Material<Plasmid>) -> Material<Plasmid>:
  stored <- store plasmid at -20 C
  return stored

workflow main(plasmid: Material<Plasmid>) -> Material<Plasmid>:
  first <- preserve plasmid
  second <- preserve first
  return second
"#,
        )
        .unwrap();
        let catalog = CapabilityRequirements::extract(&[&module]).unwrap();

        let instances = catalog
            .instantiate_reachable(&[&module], "standalone", "main")
            .unwrap();

        assert_eq!(
            instances.schema_version,
            CAPABILITY_REQUIREMENT_INSTANCES_SCHEMA_VERSION
        );
        assert_eq!(instances.entry.module, "standalone");
        assert_eq!(instances.entry.workflow, "main");
        assert_eq!(instances.instances.len(), 2);
        assert_ne!(instances.instances[0].id, instances.instances[1].id);
        assert!(
            instances
                .instances
                .iter()
                .all(|instance| instance.template == "standalone::preserve::body[0]")
        );
        assert_eq!(instances.instances[0].call_path.len(), 1);
        assert_eq!(
            instances.instances[0].call_path[0].callee,
            WorkflowIdentity {
                module: "standalone".to_owned(),
                workflow: "preserve".to_owned(),
            }
        );
        let json = serde_json::to_string(&instances).unwrap();
        assert_eq!(
            serde_json::from_str::<CapabilityRequirementInstances>(&json).unwrap(),
            instances
        );
    }

    #[test]
    fn rejects_recursive_workflow_expansion() {
        let module = compile_module(
            r#"use std.lab.plasmid

workflow first(plasmid: Material<Plasmid>) -> Material<Plasmid>:
  next <- second plasmid
  return next

workflow second(plasmid: Material<Plasmid>) -> Material<Plasmid>:
  next <- first plasmid
  return next

workflow main(plasmid: Material<Plasmid>) -> Material<Plasmid>:
  result <- first plasmid
  return result
"#,
        )
        .unwrap();
        let catalog = CapabilityRequirements::extract(&[&module]).unwrap();

        let error = catalog
            .instantiate_reachable(&[&module], "standalone", "main")
            .unwrap_err();

        assert_eq!(
            error,
            CapabilityInstantiationError::RecursiveWorkflow {
                cycle: "standalone::first -> standalone::second -> standalone::first".to_owned(),
            }
        );
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

    #[test]
    fn rejects_a_non_absolute_parameter_kind_even_in_previously_checked_ir() {
        let mut module = compile_module(SOURCE).unwrap();
        let CheckedDeclaration::Workflow { body, .. } = &mut module.declarations[0] else {
            panic!("the fixture begins with a workflow")
        };
        let CheckedStatement::Effect { action, .. } = &mut body[0] else {
            panic!("the workflow begins with an effect")
        };
        action.arguments[1].parameter_kind = Some("temperature".to_owned());

        let error = CapabilityRequirements::extract(&[&module]).unwrap_err();

        assert_eq!(
            error,
            CapabilityRequirementError::InvalidParameterKind {
                operation: "std.lab.plasmid.store".to_owned(),
                argument: "temperature".to_owned(),
                property_kind: "temperature".to_owned(),
            }
        );
    }
}
