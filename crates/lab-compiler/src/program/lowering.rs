//! Lower checked Lab modules into facility-independent Design and Workflow intent.

use std::collections::{BTreeMap, BTreeSet};

use lab_capability::{ExactDecimal, ExactInteger, PropertyValue, ScalarValue, UnitIri};
use lab_language::{
    CheckedDeclaration, CheckedExpression, CheckedField, CheckedModule, CheckedStatement,
    CheckedType, DefinitionId, OwnershipMode, ResolvedAction, ResolvedActionCallee, ResultLineage,
    TypedExpression,
};
use thiserror::Error;

use crate::design::{ARTIFACT_DESIGN_SCHEMA_VERSION, ArtifactDesign};
use crate::method::{ProcedureValue, ScalarType};
use crate::workflow::ir::{IntentAction, IntentSource};

pub(crate) type WorkflowActionIntent = IntentAction;

struct ArtifactParameterContext<'a> {
    supplier_identities: &'a BTreeMap<DefinitionId, String>,
    stated: &'a BTreeMap<DefinitionId, Vec<lab_language::CheckedProperty>>,
    bindings: &'a BTreeMap<DefinitionId, TypedExpression>,
}

#[derive(Debug, Error)]
pub enum SourceLoweringError {
    #[error("entry workflow does not contain any designs or actions")]
    EmptyEntry,
    #[error("realize workflow for artifact '{0}' has unsupported dependency dataflow")]
    InvalidDependencyFlow(String),
    #[error("workflow action '{operation}' for artifact '{artifact}' has invalid result bindings")]
    InvalidActionResults { artifact: String, operation: String },
    #[error(
        "workflow '{workflow}' contains unsupported control statement '{control}' at statement path {path:?}; Design/Intent lowering never flattens conditional, repeated, reactive, or emitted effects"
    )]
    UnsupportedWorkflowControl {
        workflow: String,
        control: &'static str,
        path: Vec<usize>,
    },
    #[error(
        "workflow action '{operation}' contains invalid semantic parameter '{parameter}': {message}"
    )]
    InvalidActionParameter {
        operation: String,
        parameter: String,
        message: String,
    },
    #[error("entry module '{entry}' does not declare workflow 'main'")]
    MissingEntryWorkflow { entry: String },
    #[error("entry workflow '{entry}.main' requires inputs and cannot be run without arguments")]
    EntryWorkflowHasInputs { entry: String },
    #[error("workflow body for '{workflow}' is not present in the checked program")]
    MissingWorkflowDefinition { workflow: String },
    #[error("recursive workflow call to '{workflow}' cannot be lowered as a straight-line program")]
    RecursiveWorkflowCall { workflow: String },
    #[error(
        "workflow '{workflow}' declares durable state; Design/Intent lowering does not model stateful execution"
    )]
    UnsupportedWorkflowState { workflow: String },
    #[error(
        "workflow action '{operation}' uses unsupported projected SSA operand '{argument}' at path {path:?}"
    )]
    UnsupportedProjectedOperand {
        operation: String,
        argument: String,
        path: Vec<String>,
    },
    #[error(
        "workflow action '{operation}' SSA operand '{argument}' references unavailable binding '{binding}'"
    )]
    UnboundSsaOperand {
        operation: String,
        argument: String,
        binding: String,
    },
}

/// One rooted, straight-line execution after workflow calls have been
/// expanded. Designs are still emitted before actions in LAIR, while `actions`
/// retain the exact order in which the entry workflow executes them.
pub(crate) struct RootedProgramIntent {
    pub designs: Vec<ArtifactDesign>,
    pub actions: Vec<WorkflowActionIntent>,
}

/// Lower the exact execution rooted at `<entry>.main`.
///
/// Workflow calls are source-level composition, not durable execution
/// operations, so this expands them recursively in statement order. Pure
/// bindings and workflow returns are substituted into the checked action
/// values. Only declared actions survive as Intent operations.
pub(crate) fn lower_rooted_program_intent(
    modules: &[&CheckedModule],
    entry: &str,
) -> Result<RootedProgramIntent, SourceLoweringError> {
    let supplier_identities = supplier_identities(modules);
    let stated = inventory_properties(modules);
    let bindings = binding_values(modules);
    let parameter_context = ArtifactParameterContext {
        supplier_identities: &supplier_identities,
        stated: &stated,
        bindings: &bindings,
    };

    let mut artifact_records = BTreeMap::new();
    for declaration in declarations(modules) {
        let CheckedDeclaration::Artifact {
            definition,
            artifact,
            artifact_definitions,
            name,
            produces,
            type_definition,
            facets,
            sbol_identity,
            properties,
            requirements,
            acceptance,
            ..
        } = declaration
        else {
            continue;
        };
        let design = ArtifactDesign {
            schema_version: ARTIFACT_DESIGN_SCHEMA_VERSION.to_owned(),
            definition: definition.clone(),
            name: name.clone(),
            artifact: artifact.clone(),
            artifact_definitions: artifact_definitions.clone(),
            produces: produces.clone(),
            type_definition: type_definition.clone(),
            facets: facets.clone(),
            sbol_identity: sbol_identity.clone(),
            properties: properties.clone(),
            requirements: requirements.clone(),
            acceptance: acceptance.clone(),
        };
        let parameters = artifact_parameters(&design, &parameter_context)?;
        artifact_records.insert(definition.clone(), ArtifactRecord { design, parameters });
    }

    let workflows = workflow_definitions(modules);
    let main = modules
        .iter()
        .find(|module| module.module.as_str() == entry)
        .and_then(|module| module.interface.exports.get("main"))
        .and_then(|export| workflows.get(&export.definition).copied())
        .ok_or_else(|| SourceLoweringError::MissingEntryWorkflow {
            entry: entry.to_owned(),
        })?;
    if !main.inputs.is_empty() {
        return Err(SourceLoweringError::EntryWorkflowHasInputs {
            entry: entry.to_owned(),
        });
    }

    let mut lowerer = RootedWorkflowLowerer {
        workflows,
        artifacts: &artifact_records,
        identities: &supplier_identities,
        next_invocation: 0,
        stack: Vec::new(),
        actions: Vec::new(),
        dependencies: BTreeMap::new(),
        used_artifacts: BTreeSet::new(),
    };
    lowerer.inline_workflow(main, Vec::new(), None, &[])?;
    for intent in &mut lowerer.actions {
        let Some(owner) = intent
            .artifact
            .as_ref()
            .map(|artifact| artifact.definition.clone())
        else {
            continue;
        };
        intent.artifact_dependencies = lowerer
            .dependencies
            .get(&owner)
            .into_iter()
            .flatten()
            .map(definition_key)
            .collect();
    }
    lowerer.used_artifacts.extend(
        lowerer
            .dependencies
            .values()
            .flat_map(|dependencies| dependencies.iter().cloned()),
    );

    let mut designs = Vec::new();
    for declaration in declarations(modules) {
        let CheckedDeclaration::Artifact { definition, .. } = declaration else {
            continue;
        };
        if !lowerer.used_artifacts.contains(definition) {
            continue;
        }
        let record = artifact_records
            .get(definition)
            .expect("every checked artifact has a lowering record");
        designs.push(record.design.clone());
    }

    if lowerer.actions.is_empty() && designs.is_empty() {
        return Err(SourceLoweringError::EmptyEntry);
    }
    Ok(RootedProgramIntent {
        designs,
        actions: lowerer.actions,
    })
}

#[derive(Clone)]
struct ArtifactRecord {
    design: ArtifactDesign,
    parameters: BTreeMap<String, ProcedureValue>,
}

#[derive(Clone, Copy)]
struct WorkflowDefinition<'a> {
    module: &'a str,
    name: &'a str,
    definition: &'a DefinitionId,
    parameters: &'a [String],
    inputs: &'a [CheckedField],
    outputs: &'a [CheckedField],
    state: &'a [lab_language::CheckedState],
    body: &'a [CheckedStatement],
}

fn workflow_definitions<'a>(
    modules: &'a [&'a CheckedModule],
) -> BTreeMap<DefinitionId, WorkflowDefinition<'a>> {
    let mut workflows = BTreeMap::new();
    for module in modules {
        for declaration in &module.declarations {
            let CheckedDeclaration::Workflow {
                name,
                parameters,
                inputs,
                outputs,
                state,
                body,
                ..
            } = declaration
            else {
                continue;
            };
            let definition = &module
                .interface
                .exports
                .get(name)
                .expect("a checked workflow has an interface definition")
                .definition;
            workflows.insert(
                definition.clone(),
                WorkflowDefinition {
                    module: module.module.as_str(),
                    name,
                    definition,
                    parameters,
                    inputs,
                    outputs,
                    state,
                    body,
                },
            );
        }
    }
    workflows
}

#[derive(Clone)]
struct BoundValue {
    expression: TypedExpression,
    artifacts: BTreeSet<DefinitionId>,
    parameter: Option<ProcedureValue>,
    dataflow: bool,
}

struct RootedWorkflowLowerer<'a> {
    workflows: BTreeMap<DefinitionId, WorkflowDefinition<'a>>,
    artifacts: &'a BTreeMap<DefinitionId, ArtifactRecord>,
    identities: &'a BTreeMap<DefinitionId, String>,
    next_invocation: usize,
    stack: Vec<DefinitionId>,
    actions: Vec<WorkflowActionIntent>,
    dependencies: BTreeMap<DefinitionId, BTreeSet<DefinitionId>>,
    used_artifacts: BTreeSet<DefinitionId>,
}

impl RootedWorkflowLowerer<'_> {
    fn inline_workflow(
        &mut self,
        workflow: WorkflowDefinition<'_>,
        arguments: Vec<BoundValue>,
        inherited_artifact: Option<DefinitionId>,
        call_path: &[usize],
    ) -> Result<Vec<BoundValue>, SourceLoweringError> {
        if self.stack.contains(workflow.definition) {
            return Err(SourceLoweringError::RecursiveWorkflowCall {
                workflow: format!(
                    "{}::{}",
                    workflow.definition.module, workflow.definition.local
                ),
            });
        }
        if !workflow.state.is_empty() {
            return Err(SourceLoweringError::UnsupportedWorkflowState {
                workflow: workflow.name.to_owned(),
            });
        }
        reject_unsupported_control(workflow.name, workflow.body, call_path)?;

        let substitutions = infer_workflow_type_substitutions(workflow, &arguments);
        let mut environment = workflow
            .inputs
            .iter()
            .zip(arguments)
            .map(|(input, value)| (input.name.clone(), value))
            .collect::<BTreeMap<_, _>>();
        let local_artifact = self.realized_artifact(workflow, &environment, &substitutions)?;
        let artifact = local_artifact.or(inherited_artifact);
        if let Some(artifact) = &artifact {
            self.used_artifacts.insert(artifact.clone());
        }

        let invocation = self.next_invocation;
        self.next_invocation += 1;
        self.stack.push(workflow.definition.clone());
        let mut returned = None;
        for (statement_index, statement) in workflow.body.iter().enumerate() {
            let mut statement_path = call_path.to_vec();
            statement_path.push(statement_index);
            match statement {
                CheckedStatement::Binding(binding) => {
                    let value = resolve_expression(
                        &binding.value,
                        &environment,
                        self.artifacts,
                        self.identities,
                        &substitutions,
                    );
                    for target in &binding.targets {
                        environment.insert(target.name.clone(), value.clone());
                    }
                }
                CheckedStatement::Effect { results, action } => match &action.callee {
                    ResolvedActionCallee::Workflow { definition } => {
                        let callee = self.workflows.get(definition).copied().ok_or_else(|| {
                            SourceLoweringError::MissingWorkflowDefinition {
                                workflow: format!("{}::{}", definition.module, definition.local),
                            }
                        })?;
                        let call_arguments = action
                            .arguments
                            .iter()
                            .map(|argument| {
                                resolve_expression(
                                    &argument.value,
                                    &environment,
                                    self.artifacts,
                                    self.identities,
                                    &substitutions,
                                )
                            })
                            .collect();
                        let values = self.inline_workflow(
                            callee,
                            call_arguments,
                            artifact.clone(),
                            &statement_path,
                        )?;
                        if values.len() != results.len() {
                            return Err(SourceLoweringError::InvalidActionResults {
                                artifact: workflow.name.to_owned(),
                                operation: action.display_name().to_owned(),
                            });
                        }
                        for (binding, mut value) in results.iter().zip(values) {
                            value.expression.r#type =
                                specialize_type(&binding.r#type, &substitutions);
                            environment.insert(binding.name.clone(), value);
                        }
                    }
                    ResolvedActionCallee::Action { .. } => {
                        let resolved = action
                            .arguments
                            .iter()
                            .map(|argument| {
                                resolve_expression(
                                    &argument.value,
                                    &environment,
                                    self.artifacts,
                                    self.identities,
                                    &substitutions,
                                )
                            })
                            .collect::<Vec<_>>();
                        let mut concrete_action = action.clone();
                        for (argument, value) in concrete_action.arguments.iter_mut().zip(&resolved)
                        {
                            argument.value = value.expression.clone();
                        }
                        for result in &mut concrete_action.results {
                            result.r#type = specialize_type(&result.r#type, &substitutions);
                        }
                        let mut ssa_operands = Vec::new();
                        for (argument, value) in concrete_action.arguments.iter().zip(&resolved) {
                            if !value.dataflow {
                                continue;
                            }
                            let CheckedExpression::Reference { path, .. } = &value.expression.value
                            else {
                                return Err(SourceLoweringError::UnsupportedProjectedOperand {
                                    operation: concrete_action.display_name().to_owned(),
                                    argument: argument.name.clone(),
                                    path: Vec::new(),
                                });
                            };
                            if path.len() != 1 {
                                return Err(SourceLoweringError::UnsupportedProjectedOperand {
                                    operation: concrete_action.display_name().to_owned(),
                                    argument: argument.name.clone(),
                                    path: path.clone(),
                                });
                            }
                            ssa_operands.push(argument.name.clone());
                        }
                        let concrete_results = results
                            .iter()
                            .map(|binding| CheckedField {
                                name: format!(
                                    "call_{invocation}_statement_{statement_index}_{}",
                                    binding.name
                                ),
                                r#type: specialize_type(&binding.r#type, &substitutions),
                            })
                            .collect::<Vec<_>>();
                        let empty_bindings = BTreeMap::new();
                        let mut intent = perform_intent(
                            ActionSource {
                                module: workflow.module,
                                workflow: workflow.name,
                                definition: workflow.definition,
                                statement_path,
                            },
                            &concrete_results,
                            &concrete_action,
                            &empty_bindings,
                            self.identities,
                        )?;
                        intent.ssa_operands = ssa_operands;
                        for (argument, value) in concrete_action.arguments.iter().zip(&resolved) {
                            if let Some(parameter) = &value.parameter {
                                intent
                                    .parameters
                                    .insert(argument.name.clone(), parameter.clone());
                            }
                            self.used_artifacts.extend(value.artifacts.iter().cloned());
                        }
                        if let Some(owner) = &artifact {
                            let record = self
                                .artifacts
                                .get(owner)
                                .expect("a realized artifact is a checked artifact");
                            intent.artifact = Some(record.design.clone());
                            for (name, value) in &record.parameters {
                                intent
                                    .parameters
                                    .entry(name.clone())
                                    .or_insert_with(|| value.clone());
                            }
                            for (argument, value) in concrete_action.arguments.iter().zip(&resolved)
                            {
                                if argument.mode == OwnershipMode::Take
                                    && type_contains_material(&argument.value.r#type)
                                {
                                    self.dependencies.entry(owner.clone()).or_default().extend(
                                        value
                                            .artifacts
                                            .iter()
                                            .filter(|dependency| *dependency != owner)
                                            .cloned(),
                                    );
                                }
                            }
                        }

                        let result_values = concrete_results
                            .iter()
                            .zip(&concrete_action.results)
                            .map(|(binding, result)| {
                                let sources = match &result.lineage {
                                    ResultLineage::Begins => {
                                        artifact.iter().cloned().collect::<BTreeSet<_>>()
                                    }
                                    ResultLineage::Continues { from } => from
                                        .iter()
                                        .chain(std::iter::empty())
                                        .flat_map(|source| {
                                            concrete_action
                                                .arguments
                                                .iter()
                                                .zip(&resolved)
                                                .filter(move |(argument, _)| {
                                                    argument.name == *source
                                                })
                                                .flat_map(|(_, value)| {
                                                    value.artifacts.iter().cloned()
                                                })
                                        })
                                        .collect(),
                                    ResultLineage::IdentifiedBy { operands } => operands
                                        .iter()
                                        .flat_map(|source| {
                                            concrete_action
                                                .arguments
                                                .iter()
                                                .zip(&resolved)
                                                .filter(move |(argument, _)| {
                                                    argument.name == *source
                                                })
                                                .flat_map(|(_, value)| {
                                                    value.artifacts.iter().cloned()
                                                })
                                        })
                                        .collect(),
                                };
                                let parameter = (sources.len() == 1).then(|| {
                                    let source = sources.iter().next().expect("one source");
                                    let name = self
                                        .artifacts
                                        .get(source)
                                        .map(|artifact| artifact.design.name.clone())
                                        .unwrap_or_else(|| definition_key(source));
                                    ProcedureValue::Scalar {
                                        value: PropertyValue::unitless(ScalarValue::Text(name)),
                                    }
                                });
                                BoundValue {
                                    expression: TypedExpression {
                                        r#type: binding.r#type.clone(),
                                        value: CheckedExpression::Reference {
                                            definition: DefinitionId::exported(
                                                workflow.module,
                                                &binding.name,
                                            ),
                                            path: vec![binding.name.clone()],
                                        },
                                    },
                                    artifacts: sources,
                                    parameter,
                                    dataflow: true,
                                }
                            })
                            .collect::<Vec<_>>();
                        for (source, value) in results.iter().zip(&result_values) {
                            environment.insert(source.name.clone(), value.clone());
                        }
                        self.actions.push(intent);
                    }
                },
                CheckedStatement::Return { values } => {
                    returned = Some(
                        values
                            .iter()
                            .map(|value| {
                                resolve_expression(
                                    &value.value,
                                    &environment,
                                    self.artifacts,
                                    self.identities,
                                    &substitutions,
                                )
                            })
                            .collect::<Vec<_>>(),
                    );
                    break;
                }
                CheckedStatement::StateUpdate { .. } => {
                    return Err(SourceLoweringError::UnsupportedWorkflowControl {
                        workflow: workflow.name.to_owned(),
                        control: "state update",
                        path: statement_path,
                    });
                }
                CheckedStatement::If { .. }
                | CheckedStatement::Match { .. }
                | CheckedStatement::For { .. }
                | CheckedStatement::When { .. }
                | CheckedStatement::Emit { .. } => unreachable!(
                    "reject_unsupported_control rejects structured and emitted effects"
                ),
            }
        }
        self.stack.pop();
        let returned = returned.unwrap_or_default();
        if returned.len() != workflow.outputs.len() {
            return Err(SourceLoweringError::InvalidActionResults {
                artifact: workflow.name.to_owned(),
                operation: "return".to_owned(),
            });
        }
        Ok(returned)
    }

    fn realized_artifact(
        &self,
        workflow: WorkflowDefinition<'_>,
        environment: &BTreeMap<String, BoundValue>,
        substitutions: &BTreeMap<String, CheckedType>,
    ) -> Result<Option<DefinitionId>, SourceLoweringError> {
        let produced_kinds = self
            .artifacts
            .values()
            .map(|artifact| artifact.design.produces.subject().display_name())
            .collect::<BTreeSet<_>>();
        let mut realized = BTreeSet::new();
        for statement in workflow.body {
            let CheckedStatement::Effect { action, .. } = statement else {
                continue;
            };
            if action.operation().is_none()
                || !action
                    .results
                    .iter()
                    .any(|result| result.lineage == ResultLineage::Begins)
            {
                continue;
            }
            for argument in &action.arguments {
                let argument_type = specialize_type(&argument.value.r#type, substitutions);
                if argument.mode != OwnershipMode::Copy
                    || !produced_kinds.contains(&argument_type.subject().display_name())
                {
                    continue;
                }
                realized.extend(
                    resolve_expression(
                        &argument.value,
                        environment,
                        self.artifacts,
                        self.identities,
                        substitutions,
                    )
                    .artifacts,
                );
            }
        }
        if realized.len() > 1 {
            return Err(SourceLoweringError::InvalidDependencyFlow(
                workflow.name.to_owned(),
            ));
        }
        Ok(realized.into_iter().next())
    }
}

fn infer_workflow_type_substitutions(
    workflow: WorkflowDefinition<'_>,
    arguments: &[BoundValue],
) -> BTreeMap<String, CheckedType> {
    let parameters = workflow.parameters.iter().cloned().collect::<BTreeSet<_>>();
    let mut substitutions = BTreeMap::new();
    for (input, argument) in workflow.inputs.iter().zip(arguments) {
        infer_type_substitution(
            &input.r#type,
            &argument.expression.r#type,
            &parameters,
            &mut substitutions,
        );
    }
    substitutions
}

fn infer_type_substitution(
    declared: &CheckedType,
    actual: &CheckedType,
    parameters: &BTreeSet<String>,
    substitutions: &mut BTreeMap<String, CheckedType>,
) {
    match (declared, actual) {
        (CheckedType::Named { name, arguments }, actual)
            if arguments.is_empty() && parameters.contains(name) =>
        {
            substitutions
                .entry(name.clone())
                .or_insert_with(|| actual.clone());
        }
        (
            CheckedType::Named {
                name: declared_name,
                arguments: declared_arguments,
            },
            CheckedType::Named {
                name: actual_name,
                arguments: actual_arguments,
            },
        ) if declared_name == actual_name => {
            for (declared, actual) in declared_arguments.iter().zip(actual_arguments) {
                infer_type_substitution(declared, actual, parameters, substitutions);
            }
        }
        (CheckedType::List { element: declared }, CheckedType::List { element: actual })
        | (
            CheckedType::InState {
                subject: declared, ..
            },
            CheckedType::InState {
                subject: actual, ..
            },
        ) => infer_type_substitution(declared, actual, parameters, substitutions),
        _ => {}
    }
}

fn specialize_type(ty: &CheckedType, substitutions: &BTreeMap<String, CheckedType>) -> CheckedType {
    match ty {
        CheckedType::Named { name, arguments } if arguments.is_empty() => substitutions
            .get(name)
            .cloned()
            .unwrap_or_else(|| ty.clone()),
        CheckedType::Named { name, arguments } => CheckedType::Named {
            name: name.clone(),
            arguments: arguments
                .iter()
                .map(|argument| specialize_type(argument, substitutions))
                .collect(),
        },
        CheckedType::Union { alternatives } => CheckedType::Union {
            alternatives: alternatives
                .iter()
                .map(|alternative| specialize_type(alternative, substitutions))
                .collect(),
        },
        CheckedType::List { element } => CheckedType::List {
            element: Box::new(specialize_type(element, substitutions)),
        },
        CheckedType::InState { subject, state } => CheckedType::InState {
            subject: Box::new(specialize_type(subject, substitutions)),
            state: state.clone(),
        },
        _ => ty.clone(),
    }
}

fn resolve_expression(
    expression: &TypedExpression,
    environment: &BTreeMap<String, BoundValue>,
    artifacts: &BTreeMap<DefinitionId, ArtifactRecord>,
    identities: &BTreeMap<DefinitionId, String>,
    substitutions: &BTreeMap<String, CheckedType>,
) -> BoundValue {
    let specialized_type = specialize_type(&expression.r#type, substitutions);
    match &expression.value {
        CheckedExpression::Reference { definition, path } if !path.is_empty() => {
            if let Some(bound) = environment.get(&path[0]) {
                let mut resolved = bound.clone();
                if path.len() == 1 {
                    resolved.expression.r#type = specialized_type;
                    return resolved;
                }
                if let CheckedExpression::Reference {
                    path: resolved_path,
                    ..
                } = &mut resolved.expression.value
                {
                    resolved_path.extend(path.iter().skip(1).cloned());
                    resolved.expression.r#type = specialized_type;
                    resolved.parameter = None;
                    return resolved;
                }
            }
            let mut artifact_set = BTreeSet::new();
            if path.len() == 1 && artifacts.contains_key(definition) {
                artifact_set.insert(definition.clone());
            }
            let parameter = (path.len() == 1).then(|| ProcedureValue::Scalar {
                value: PropertyValue::unitless(ScalarValue::Text(
                    identities
                        .get(definition)
                        .cloned()
                        .unwrap_or_else(|| path[0].clone()),
                )),
            });
            BoundValue {
                expression: TypedExpression {
                    r#type: specialized_type,
                    value: CheckedExpression::Reference {
                        definition: definition.clone(),
                        path: path.clone(),
                    },
                },
                artifacts: artifact_set,
                parameter,
                dataflow: path.len() == 1 && artifacts.contains_key(definition),
            }
        }
        CheckedExpression::Reference { definition, path } => BoundValue {
            expression: TypedExpression {
                r#type: specialized_type,
                value: CheckedExpression::Reference {
                    definition: definition.clone(),
                    path: path.clone(),
                },
            },
            artifacts: BTreeSet::new(),
            parameter: None,
            dataflow: false,
        },
        CheckedExpression::List { elements } => {
            let resolved = elements
                .iter()
                .map(|element| {
                    resolve_expression(element, environment, artifacts, identities, substitutions)
                })
                .collect::<Vec<_>>();
            let artifact_set = resolved
                .iter()
                .flat_map(|value| value.artifacts.iter().cloned())
                .collect();
            let scalar_values = resolved
                .iter()
                .map(|value| match &value.parameter {
                    Some(ProcedureValue::Scalar { value }) => Some(value.clone()),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>();
            let parameter = scalar_values.and_then(|values| {
                let element_type = values
                    .first()
                    .map(|value| ScalarType::of(&value.value))
                    .unwrap_or(ScalarType::Text);
                values
                    .iter()
                    .all(|value| ScalarType::of(&value.value) == element_type)
                    .then_some(ProcedureValue::List {
                        element_type,
                        values,
                    })
            });
            BoundValue {
                expression: TypedExpression {
                    r#type: specialized_type,
                    value: CheckedExpression::List {
                        elements: resolved.into_iter().map(|value| value.expression).collect(),
                    },
                },
                artifacts: artifact_set,
                parameter,
                dataflow: false,
            }
        }
        CheckedExpression::Call {
            operation,
            arguments,
        } => BoundValue {
            expression: TypedExpression {
                r#type: specialized_type,
                value: CheckedExpression::Call {
                    operation: operation.clone(),
                    arguments: arguments
                        .iter()
                        .map(|argument| lab_language::CheckedArgument {
                            name: argument.name.clone(),
                            value: resolve_expression(
                                &argument.value,
                                environment,
                                artifacts,
                                identities,
                                substitutions,
                            )
                            .expression,
                        })
                        .collect(),
                },
            },
            artifacts: BTreeSet::new(),
            parameter: None,
            dataflow: false,
        },
        CheckedExpression::Construct {
            constructor,
            fields,
        } => BoundValue {
            expression: TypedExpression {
                r#type: specialized_type,
                value: CheckedExpression::Construct {
                    constructor: constructor.clone(),
                    fields: fields
                        .iter()
                        .map(|field| lab_language::CheckedFieldValue {
                            name: field.name.clone(),
                            value: resolve_expression(
                                &field.value,
                                environment,
                                artifacts,
                                identities,
                                substitutions,
                            )
                            .expression,
                        })
                        .collect(),
                },
            },
            artifacts: BTreeSet::new(),
            parameter: None,
            dataflow: false,
        },
        CheckedExpression::Field { subject, field } => {
            let subject =
                resolve_expression(subject, environment, artifacts, identities, substitutions);
            BoundValue {
                expression: TypedExpression {
                    r#type: specialized_type,
                    value: CheckedExpression::Field {
                        subject: Box::new(subject.expression),
                        field: field.clone(),
                    },
                },
                artifacts: subject.artifacts,
                parameter: None,
                dataflow: subject.dataflow,
            }
        }
        CheckedExpression::Unary { operator, operand } => BoundValue {
            expression: TypedExpression {
                r#type: specialized_type,
                value: CheckedExpression::Unary {
                    operator: operator.clone(),
                    operand: Box::new(
                        resolve_expression(
                            operand,
                            environment,
                            artifacts,
                            identities,
                            substitutions,
                        )
                        .expression,
                    ),
                },
            },
            artifacts: BTreeSet::new(),
            parameter: None,
            dataflow: false,
        },
        CheckedExpression::Binary {
            operator,
            left,
            right,
        } => BoundValue {
            expression: TypedExpression {
                r#type: specialized_type,
                value: CheckedExpression::Binary {
                    operator: operator.clone(),
                    left: Box::new(
                        resolve_expression(left, environment, artifacts, identities, substitutions)
                            .expression,
                    ),
                    right: Box::new(
                        resolve_expression(
                            right,
                            environment,
                            artifacts,
                            identities,
                            substitutions,
                        )
                        .expression,
                    ),
                },
            },
            artifacts: BTreeSet::new(),
            parameter: None,
            dataflow: false,
        },
        CheckedExpression::Integer { .. }
        | CheckedExpression::Decimal { .. }
        | CheckedExpression::String { .. }
        | CheckedExpression::Quantity { .. } => {
            let concrete = TypedExpression {
                r#type: specialized_type,
                value: expression.value.clone(),
            };
            BoundValue {
                parameter: procedure_value(&concrete, &BTreeMap::new(), identities)
                    .expect("checked scalar expressions project exactly"),
                expression: concrete,
                artifacts: BTreeSet::new(),
                dataflow: false,
            }
        }
    }
}

fn declarations<'a>(
    modules: &'a [&'a CheckedModule],
) -> impl Iterator<Item = &'a CheckedDeclaration> {
    modules.iter().flat_map(|module| module.declarations.iter())
}

/// Project the checked artifact document into the small scalar/list ABI Method
/// definitions use today. The complete document remains on every Intent action;
/// these are convenient, typed selectors rather than a replacement for it.
fn artifact_parameters(
    design: &ArtifactDesign,
    context: &ArtifactParameterContext<'_>,
) -> Result<BTreeMap<String, ProcedureValue>, SourceLoweringError> {
    let mut parameters = BTreeMap::new();
    insert_text_parameter(&mut parameters, "artifact", design.name.clone());
    insert_text_parameter(&mut parameters, "subject", design.name.clone());
    insert_text_parameter(&mut parameters, "artifact_kind", design.artifact.clone());
    insert_text_parameter(
        &mut parameters,
        "artifact_definition",
        definition_key(&design.definition),
    );
    insert_text_list_parameter(
        &mut parameters,
        "artifact_kind_definitions",
        design
            .artifact_definitions
            .iter()
            .map(definition_key)
            .collect(),
    );
    insert_text_parameter(
        &mut parameters,
        "artifact_type_definition",
        definition_key(&design.type_definition),
    );
    insert_text_list_parameter(
        &mut parameters,
        "artifact_facets",
        design
            .facets
            .iter()
            .map(|facet| format!("{}#{}", definition_key(&facet.definition), facet.state))
            .collect(),
    );
    for facet in &design.facets {
        insert_text_parameter(
            &mut parameters,
            &format!("facet_{}", facet.name),
            facet.state.clone(),
        );
    }
    if let Some(identity) = &design.sbol_identity {
        parameters.insert(
            "sbol_identity".to_owned(),
            ProcedureValue::Scalar {
                value: PropertyValue::unitless(ScalarValue::Iri(
                    lab_capability::AbsoluteIri::new(identity)
                        .expect("checked SBOL identities are absolute IRIs"),
                )),
            },
        );
    }
    insert_text_parameter(
        &mut parameters,
        "artifact_requirements",
        serde_json::to_string(&design.requirements)
            .expect("checked artifact requirements serialize infallibly"),
    );
    insert_text_parameter(
        &mut parameters,
        "artifact_acceptance",
        serde_json::to_string(&design.acceptance)
            .expect("checked artifact acceptance claims serialize infallibly"),
    );
    insert_text_parameter(
        &mut parameters,
        "artifact_design",
        serde_json::to_string(design).expect("checked artifact designs serialize infallibly"),
    );

    for property in &design.properties {
        project_artifact_expression(&mut parameters, &property.name, &property.value, context)?;
        let Some(owner) = reference_definition(&property.value) else {
            continue;
        };
        let Some(inherited) = context.stated.get(owner) else {
            continue;
        };
        for property in inherited {
            if parameters.contains_key(&property.name) {
                continue;
            }
            project_artifact_expression(&mut parameters, &property.name, &property.value, context)?;
        }
    }
    Ok(parameters)
}

fn project_artifact_expression(
    parameters: &mut BTreeMap<String, ProcedureValue>,
    name: &str,
    expression: &TypedExpression,
    context: &ArtifactParameterContext<'_>,
) -> Result<(), SourceLoweringError> {
    let value = artifact_procedure_value(expression, context).map_err(|message| {
        SourceLoweringError::InvalidActionParameter {
            operation: "design.artifact-property".to_owned(),
            parameter: name.to_owned(),
            message,
        }
    })?;
    if let Some(value) = value {
        if let ProcedureValue::List { values, .. } = &value {
            insert_integer_parameter(
                parameters,
                &format!("{name}_count"),
                u64::try_from(values.len()).unwrap_or(u64::MAX),
            );
        }
        parameters.entry(name.to_owned()).or_insert(value);
    }
    if let CheckedExpression::Quantity { magnitude, unit } = &expression.value {
        let suffix = unit
            .chars()
            .flat_map(char::to_lowercase)
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>();
        if !suffix.is_empty()
            && let Ok(value) = magnitude.parse::<u64>()
        {
            insert_integer_parameter(parameters, &format!("{name}_{suffix}"), value);
        }
    }
    Ok(())
}

fn artifact_procedure_value(
    expression: &TypedExpression,
    context: &ArtifactParameterContext<'_>,
) -> Result<Option<ProcedureValue>, String> {
    let scalar = |value| Some(ProcedureValue::Scalar { value });
    match &expression.value {
        CheckedExpression::Reference { definition, path } => {
            let local = path.last().cloned().unwrap_or_default();
            if let Some(binding) = context.bindings.get(definition) {
                return artifact_procedure_value(binding, context);
            }
            Ok(scalar(PropertyValue::unitless(ScalarValue::Text(
                context
                    .supplier_identities
                    .get(definition)
                    .cloned()
                    .unwrap_or(local),
            ))))
        }
        CheckedExpression::Integer { value } => {
            Ok(scalar(PropertyValue::unitless(ScalarValue::Integer(
                ExactInteger::parse(value.to_string()).expect("checked u64 is an exact integer"),
            ))))
        }
        CheckedExpression::Decimal { text } => Ok(scalar(PropertyValue::unitless(
            ScalarValue::Real(ExactDecimal::parse(text).map_err(|error| error.to_string())?),
        ))),
        CheckedExpression::String { value } => Ok(scalar(PropertyValue::unitless(
            ScalarValue::Text(value.clone()),
        ))),
        CheckedExpression::Quantity { magnitude, unit } => Ok(scalar(
            PropertyValue::new(
                ScalarValue::Real(
                    ExactDecimal::parse(magnitude).map_err(|error| error.to_string())?,
                ),
                Some(unit_iri(unit)?),
            )
            .map_err(|error| error.to_string())?,
        )),
        CheckedExpression::List { elements } => {
            let mut values = Vec::with_capacity(elements.len());
            let mut element_type = None;
            for element in elements {
                let Some(ProcedureValue::Scalar { value }) =
                    artifact_procedure_value(element, context)?
                else {
                    return Ok(None);
                };
                let actual = ScalarType::of(&value.value);
                if element_type.is_some_and(|expected| expected != actual) {
                    return Ok(None);
                }
                element_type = Some(actual);
                values.push(value);
            }
            Ok(Some(ProcedureValue::List {
                element_type: element_type.unwrap_or(ScalarType::Text),
                values,
            }))
        }
        CheckedExpression::Call { .. }
        | CheckedExpression::Construct { .. }
        | CheckedExpression::Field { .. }
        | CheckedExpression::Unary { .. }
        | CheckedExpression::Binary { .. } => Ok(None),
    }
}

fn definition_key(definition: &DefinitionId) -> String {
    format!("{}::{}", definition.module, definition.local)
}

fn insert_text_parameter(
    parameters: &mut BTreeMap<String, ProcedureValue>,
    name: &str,
    value: String,
) {
    parameters.insert(
        name.to_owned(),
        ProcedureValue::Scalar {
            value: PropertyValue::unitless(ScalarValue::Text(value)),
        },
    );
}

fn insert_text_list_parameter(
    parameters: &mut BTreeMap<String, ProcedureValue>,
    name: &str,
    values: Vec<String>,
) {
    parameters.insert(
        name.to_owned(),
        ProcedureValue::List {
            element_type: ScalarType::Text,
            values: values
                .into_iter()
                .map(|value| PropertyValue::unitless(ScalarValue::Text(value)))
                .collect(),
        },
    );
}

fn insert_integer_parameter(
    parameters: &mut BTreeMap<String, ProcedureValue>,
    name: &str,
    value: u64,
) {
    parameters.insert(
        name.to_owned(),
        ProcedureValue::Scalar {
            value: PropertyValue::unitless(ScalarValue::Integer(
                ExactInteger::parse(value.to_string()).expect("u64 is an exact integer"),
            )),
        },
    );
}

/// Top-level values are part of design intent. Keeping their checked
/// expressions available here lets a property reference a named sequence
/// instead of forcing every backend-facing design to contain an inline call.
fn binding_values(modules: &[&CheckedModule]) -> BTreeMap<DefinitionId, TypedExpression> {
    let mut values = BTreeMap::new();
    for module in modules {
        for declaration in &module.declarations {
            let CheckedDeclaration::Binding(binding) = declaration else {
                continue;
            };
            for target in &binding.targets {
                let definition = module
                    .interface
                    .exports
                    .get(&target.name)
                    .expect("a checked top-level binding has an interface definition")
                    .definition
                    .clone();
                values.insert(definition, binding.value.clone());
            }
        }
    }
    values
}

/// What each catalogued item states about itself.
///
/// An enzyme's working temperature and a chassis's heat shock belong to the
/// item rather than to every design that names it, so a design that says
/// nothing about them still gets them.
fn inventory_properties(
    modules: &[&CheckedModule],
) -> BTreeMap<DefinitionId, Vec<lab_language::CheckedProperty>> {
    let mut stated = BTreeMap::new();
    for module in modules {
        for declaration in &module.declarations {
            let CheckedDeclaration::Catalog {
                name, properties, ..
            } = declaration
            else {
                continue;
            };
            if properties.is_empty() {
                continue;
            }
            let definition = module
                .interface
                .exports
                .get(name)
                .expect("a checked catalog item has an interface definition")
                .definition
                .clone();
            stated.insert(definition, properties.clone());
        }
    }
    stated
}

/// What each catalogued symbol calls the item a supplier lists.
///
/// This deliberately ignores the separate SBOL Component IRI. Existing device
/// manifests require order identifiers, while inventory binding requires exact
/// biological-design identities.
fn supplier_identities(modules: &[&CheckedModule]) -> BTreeMap<DefinitionId, String> {
    let mut identities = BTreeMap::new();
    for module in modules {
        for declaration in &module.declarations {
            let CheckedDeclaration::Catalog {
                name,
                supplier_identity,
                ..
            } = declaration
            else {
                continue;
            };
            let definition = module
                .interface
                .exports
                .get(name)
                .expect("a checked catalog item has an interface definition")
                .definition
                .clone();
            identities.insert(definition, supplier_identity.clone());
        }
    }
    identities
}

fn type_contains_material(ty: &CheckedType) -> bool {
    match ty {
        CheckedType::Named { name, .. } if name == "Material" => true,
        CheckedType::Named { arguments, .. } => arguments.iter().any(type_contains_material),
        CheckedType::Union { alternatives } => alternatives.iter().any(type_contains_material),
        CheckedType::List { element }
        | CheckedType::InState {
            subject: element, ..
        } => type_contains_material(element),
        _ => false,
    }
}

/// Reject control the current LAIR cannot represent instead of traversing into
/// it and pretending every branch or iteration happened unconditionally.
fn reject_unsupported_control(
    workflow: &str,
    body: &[CheckedStatement],
    prefix: &[usize],
) -> Result<(), SourceLoweringError> {
    for (index, statement) in body.iter().enumerate() {
        let mut path = prefix.to_vec();
        path.push(index);
        let control = match statement {
            CheckedStatement::If { .. } => Some("if"),
            CheckedStatement::Match { .. } => Some("match"),
            CheckedStatement::For { .. } => Some("for"),
            CheckedStatement::When { .. } => Some("when"),
            CheckedStatement::Emit { .. } => Some("emit"),
            _ => None,
        };
        if let Some(control) = control {
            return Err(SourceLoweringError::UnsupportedWorkflowControl {
                workflow: workflow.to_owned(),
                control,
                path,
            });
        }
    }
    Ok(())
}

/// Lower every declared verb through the same lossless checked Intent envelope.
struct ActionSource<'a> {
    module: &'a str,
    workflow: &'a str,
    definition: &'a DefinitionId,
    statement_path: Vec<usize>,
}

fn perform_intent(
    source: ActionSource<'_>,
    results: &[CheckedField],
    action: &ResolvedAction,
    bindings: &BTreeMap<String, &TypedExpression>,
    identities: &BTreeMap<DefinitionId, String>,
) -> Result<WorkflowActionIntent, SourceLoweringError> {
    let operation = match &action.callee {
        ResolvedActionCallee::Action { operation, .. } => operation.clone(),
        ResolvedActionCallee::Workflow { .. } => {
            return Err(SourceLoweringError::InvalidActionResults {
                artifact: source.workflow.to_owned(),
                operation: action.display_name().to_owned(),
            });
        }
    };
    if results.len() != action.results.len()
        || results
            .iter()
            .zip(&action.results)
            .any(|(binding, declared)| binding.r#type != declared.r#type)
    {
        return Err(SourceLoweringError::InvalidActionResults {
            artifact: source.workflow.to_owned(),
            operation,
        });
    }
    let mut parameters = BTreeMap::new();
    for argument in &action.arguments {
        if let Some(value) =
            procedure_value(&argument.value, bindings, identities).map_err(|message| {
                SourceLoweringError::InvalidActionParameter {
                    operation: operation.clone(),
                    parameter: argument.name.clone(),
                    message,
                }
            })?
        {
            if let ProcedureValue::List { values, .. } = &value {
                insert_integer_parameter(
                    &mut parameters,
                    &format!("{}_count", argument.name),
                    u64::try_from(values.len()).unwrap_or(u64::MAX),
                );
            }
            parameters.insert(argument.name.clone(), value);
        }
    }
    Ok(IntentAction {
        source: IntentSource {
            module: source.module.to_owned(),
            workflow: source.definition.clone(),
            statement_path: source.statement_path,
        },
        action: action.clone(),
        ssa_operands: Vec::new(),
        result_bindings: results.to_vec(),
        artifact: None,
        artifact_dependencies: Vec::new(),
        parameters,
    })
}

fn procedure_value(
    expression: &TypedExpression,
    bindings: &BTreeMap<String, &TypedExpression>,
    identities: &BTreeMap<DefinitionId, String>,
) -> Result<Option<ProcedureValue>, String> {
    let scalar = |value| Some(ProcedureValue::Scalar { value });
    match &expression.value {
        CheckedExpression::Reference { definition, path } if path.len() == 1 => {
            if let Some(binding) = bindings.get(&path[0]) {
                return procedure_value(binding, bindings, identities);
            }
            if expression.r#type == CheckedType::Bool {
                return match path[0].as_str() {
                    "true" => Ok(scalar(PropertyValue::unitless(ScalarValue::Boolean(true)))),
                    "false" => Ok(scalar(PropertyValue::unitless(ScalarValue::Boolean(false)))),
                    _ => Ok(None),
                };
            }
            Ok(scalar(PropertyValue::unitless(ScalarValue::Text(
                identities
                    .get(definition)
                    .cloned()
                    .unwrap_or_else(|| path[0].clone()),
            ))))
        }
        CheckedExpression::Reference { path, .. } => Ok(scalar(PropertyValue::unitless(
            ScalarValue::Text(path.join(".")),
        ))),
        CheckedExpression::Integer { value } => {
            Ok(scalar(PropertyValue::unitless(ScalarValue::Integer(
                ExactInteger::parse(value.to_string()).expect("checked u64 is an exact integer"),
            ))))
        }
        CheckedExpression::Decimal { text } => Ok(scalar(PropertyValue::unitless(
            ScalarValue::Real(ExactDecimal::parse(text).map_err(|error| error.to_string())?),
        ))),
        CheckedExpression::String { value } => Ok(scalar(PropertyValue::unitless(
            ScalarValue::Text(value.clone()),
        ))),
        CheckedExpression::Quantity { magnitude, unit } => Ok(scalar(
            PropertyValue::new(
                ScalarValue::Real(
                    ExactDecimal::parse(magnitude).map_err(|error| error.to_string())?,
                ),
                Some(unit_iri(unit)?),
            )
            .map_err(|error| error.to_string())?,
        )),
        CheckedExpression::List { elements } => {
            let mut values = Vec::with_capacity(elements.len());
            let mut element_type = None;
            for element in elements {
                let Some(ProcedureValue::Scalar { value }) =
                    procedure_value(element, bindings, identities)?
                else {
                    return Ok(None);
                };
                let actual = ScalarType::of(&value.value);
                if element_type.is_some_and(|expected| expected != actual) {
                    return Ok(None);
                }
                element_type = Some(actual);
                values.push(value);
            }
            Ok(Some(ProcedureValue::List {
                element_type: element_type.unwrap_or(ScalarType::Text),
                values,
            }))
        }
        // The complete typed expression remains in `IntentAction.action`. A
        // Method that needs a structural expression should expose an explicit
        // scalar/list projection in its action contract rather than receiving
        // an untyped JSON blob as a Procedure parameter.
        CheckedExpression::Call { .. }
        | CheckedExpression::Construct { .. }
        | CheckedExpression::Field { .. }
        | CheckedExpression::Unary { .. }
        | CheckedExpression::Binary { .. } => Ok(None),
    }
}

fn unit_iri(unit: &str) -> Result<UnitIri, String> {
    let known = match unit {
        "C" => "http://qudt.org/vocab/unit/DEG_C".to_owned(),
        "h" => "http://qudt.org/vocab/unit/HR".to_owned(),
        "min" => "http://qudt.org/vocab/unit/MIN".to_owned(),
        "s" => "http://qudt.org/vocab/unit/SEC".to_owned(),
        "uL" => "http://qudt.org/vocab/unit/MicroL".to_owned(),
        "mL" => "http://qudt.org/vocab/unit/MilliL".to_owned(),
        "g" => "http://qudt.org/vocab/unit/GM".to_owned(),
        "g/L" => "http://qudt.org/vocab/unit/GM-PER-L".to_owned(),
        "mM" => "http://qudt.org/vocab/unit/MilliMOL-PER-L".to_owned(),
        other => format!(
            "https://www.lab-compiler.org/ns/unit#{}",
            other
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        ),
    };
    UnitIri::new(known).map_err(|error| error.to_string())
}

fn reference_definition(expression: &TypedExpression) -> Option<&DefinitionId> {
    let CheckedExpression::Reference { definition, path } = &expression.value else {
        return None;
    };
    (path.len() == 1).then_some(definition)
}
