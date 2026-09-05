//! Workflow and statement checking: control flow, state, effects, and the
//! durable-workflow-call path of an effect statement (standard action
//! contracts are validated in `action_contract`).

use std::collections::{BTreeSet, HashMap};

use crate::ast::{BindingStmt, EffectStmt, Pattern, Stmt, Trigger, WorkflowDecl};
use crate::checked::{
    CheckedActionArgument, CheckedBinding, CheckedDeclaration, CheckedFieldValue, CheckedMatchCase,
    CheckedState, CheckedStatement, CheckedTrigger, OwnershipMode, ResolvedAction,
};
use crate::semantic_error::SemanticError;
use crate::type_system::{Ty, substitute, to_checked_type};

use super::Checker;
use super::action_contract;

impl Checker {
    pub fn check_workflow(
        &self,
        declaration: &WorkflowDecl,
    ) -> Result<CheckedDeclaration, SemanticError> {
        let signature = self
            .workflows
            .get(&declaration.name.value)
            .expect("workflow was collected");
        let mut environment = self.values.clone();
        environment.insert("workflow".to_owned(), Ty::named("WorkflowContext"));
        for (field, ty) in declaration.inputs.iter().zip(&signature.inputs) {
            environment.insert(field.name.value.clone(), ty.clone());
        }
        let mut state = Vec::new();
        let mut state_types = HashMap::new();
        let mut body_start = 0;
        for statement in &declaration.body {
            let Stmt::State(declaration) = statement else {
                break;
            };
            let ty = self.lower_type(&declaration.ty, &BTreeSet::new())?;
            let initial_ty = self.infer_expr(&declaration.initial, &environment)?;
            if !self.compatible(&initial_ty, &ty) {
                return Err(SemanticError::new(
                    declaration.initial.span(),
                    format!("state has type {initial_ty}, but declaration requires {ty}"),
                ));
            }
            if environment
                .insert(declaration.name.value.clone(), ty.clone())
                .is_some()
            {
                return Err(SemanticError::new(
                    declaration.name.span,
                    format!("duplicate state name '{}'", declaration.name.value),
                ));
            }
            state_types.insert(declaration.name.value.clone(), ty.clone());
            state.push(CheckedState {
                name: declaration.name.value.clone(),
                r#type: to_checked_type(&ty),
                initial: self.lower_checked_expr(&declaration.initial, &environment, Some(&ty))?,
            });
            body_start += 1;
        }
        let checked = self.check_block(
            &declaration.body[body_start..],
            &environment,
            &signature.outputs,
            &state_types,
        )?;
        if !checked.terminates
            && !declaration
                .body
                .iter()
                .any(|statement| matches!(statement, Stmt::When(_)))
        {
            return Err(SemanticError::new(
                declaration.span,
                format!(
                    "workflow '{}' may finish without returning its declared results",
                    declaration.name.value
                ),
            ));
        }
        Ok(CheckedDeclaration::Workflow {
            doc: declaration.doc.clone(),
            name: declaration.name.value.clone(),
            parameters: signature.generics.parameters.clone(),
            bounds: signature
                .generics
                .bounds
                .iter()
                .map(|(name, bound)| (name.clone(), to_checked_type(bound)))
                .collect(),
            inputs: declaration
                .inputs
                .iter()
                .zip(&signature.inputs)
                .map(|(field, ty)| super::checked_field(&field.name.value, ty))
                .collect(),
            outputs: signature
                .outputs
                .iter()
                .map(|(name, ty)| super::checked_field(name, ty))
                .collect(),
            state,
            body: checked.statements,
        })
    }

    pub fn check_block(
        &self,
        statements: &[Stmt],
        starting_environment: &HashMap<String, Ty>,
        outputs: &[(String, Ty)],
        state_types: &HashMap<String, Ty>,
    ) -> Result<CheckedBlock, SemanticError> {
        let mut environment = starting_environment.clone();
        let mut checked = Vec::new();
        let mut terminates = false;
        for statement in statements {
            if terminates {
                return Err(SemanticError::new(
                    statement.span(),
                    "unreachable statement",
                ));
            }
            match statement {
                Stmt::State(state) => {
                    return Err(SemanticError::new(
                        state.span,
                        "state declarations must appear before workflow statements",
                    ));
                }
                Stmt::Binding(binding) => {
                    let is_state_update = state_types.contains_key(&binding.names[0].value);
                    if environment.contains_key(&binding.names[0].value) && !is_state_update {
                        return Err(SemanticError::new(
                            binding.span,
                            format!(
                                "cannot reassign '{}'; declare durable workflow memory with 'state'",
                                binding.names[0].value
                            ),
                        ));
                    }
                    let (binding_ir, ty) = self.check_binding(binding, &mut environment)?;
                    if let Some(state_type) = state_types.get(&binding.names[0].value) {
                        if !self.compatible(&ty, state_type) {
                            return Err(SemanticError::new(
                                binding.span,
                                format!("state update expects {state_type}, found {ty}"),
                            ));
                        }
                        checked.push(CheckedStatement::StateUpdate {
                            state: binding.names[0].value.clone(),
                            value: binding_ir.value,
                        });
                    } else {
                        checked.push(CheckedStatement::Binding(binding_ir));
                    }
                }
                Stmt::Effect(effect) => {
                    let (action, result_types) = self.check_effect(effect, &environment)?;
                    if effect.names.len() != result_types.len() {
                        return Err(SemanticError::new(
                            effect.span,
                            format!(
                                "operation '{}' returns {} value(s), but {} name(s) were provided",
                                action.operation,
                                result_types.len(),
                                effect.names.len()
                            ),
                        ));
                    }
                    let results = effect
                        .names
                        .iter()
                        .zip(result_types)
                        .map(|(name, ty)| {
                            environment.insert(name.value.clone(), ty.clone());
                            super::checked_field(&name.value, &ty)
                        })
                        .collect();
                    checked.push(CheckedStatement::Effect { results, action });
                }
                Stmt::Return(statement) => {
                    if statement.values.len() != outputs.len() {
                        return Err(SemanticError::new(
                            statement.span,
                            format!(
                                "workflow returns {} value(s), expected {}",
                                statement.values.len(),
                                outputs.len()
                            ),
                        ));
                    }
                    let values = statement
                        .values
                        .iter()
                        .zip(outputs)
                        .map(|(value, (name, expected))| {
                            let actual = self.infer_expr(value, &environment)?;
                            if !self.compatible(&actual, expected) {
                                return Err(SemanticError::new(
                                    value.span(),
                                    format!(
                                        "workflow result '{name}' has type {actual}, expected {expected}"
                                    ),
                                ));
                            }
                            Ok(CheckedFieldValue {
                                name: name.clone(),
                                value: self.lower_checked_expr(
                                    value,
                                    &environment,
                                    Some(&actual),
                                )?,
                            })
                        })
                        .collect::<Result<Vec<_>, SemanticError>>()?;
                    checked.push(CheckedStatement::Return { values });
                    terminates = true;
                }
                Stmt::If(statement) => {
                    self.require_bool(&statement.condition, &environment, "if")?;
                    let then_block =
                        self.check_block(&statement.then_body, &environment, outputs, state_types)?;
                    let else_block =
                        self.check_block(&statement.else_body, &environment, outputs, state_types)?;
                    if !statement.else_body.is_empty()
                        && then_block.terminates
                        && else_block.terminates
                    {
                        terminates = true;
                    }
                    checked.push(CheckedStatement::If {
                        condition: self.lower_checked_expr(
                            &statement.condition,
                            &environment,
                            Some(&Ty::Bool),
                        )?,
                        body: then_block.statements,
                        else_body: else_block.statements,
                    });
                }
                Stmt::Match(statement) => {
                    let matched_type = self.infer_expr(&statement.value, &environment)?;
                    let mut cases = Vec::new();
                    let mut seen_patterns = BTreeSet::new();
                    let mut has_binding_pattern = false;
                    let mut continuing_environments = Vec::new();
                    for case in &statement.cases {
                        self.check_pattern(&case.pattern, &matched_type)?;
                        match &case.pattern {
                            Pattern::Name(_) => has_binding_pattern = true,
                            Pattern::Constructor { path, .. } => {
                                let name = super::path_text(path);
                                if !seen_patterns.insert(name.clone()) {
                                    return Err(SemanticError::new(
                                        path.span,
                                        format!("duplicate match case '{name}'"),
                                    ));
                                }
                            }
                        }
                        if let Some(guard) = &case.guard {
                            self.require_bool(guard, &environment, "case guard")?;
                        }
                        let block =
                            self.check_block(&case.body, &environment, outputs, state_types)?;
                        if !block.terminates {
                            continuing_environments.push(block.environment.clone());
                        }
                        cases.push(CheckedMatchCase {
                            pattern: self.checked_pattern(&case.pattern),
                            body: block.statements,
                            terminates: block.terminates,
                        });
                    }
                    if !has_binding_pattern
                        && let Ty::Named(name, _) = &matched_type
                        && let Some(signature) = self.data.get(name)
                        && !signature.cases.is_empty()
                    {
                        let missing = signature
                            .cases
                            .keys()
                            .filter(|name| !seen_patterns.contains(*name))
                            .cloned()
                            .collect::<Vec<_>>();
                        if !missing.is_empty() {
                            return Err(SemanticError::new(
                                statement.span,
                                format!("non-exhaustive match; missing cases {missing:?}"),
                            ));
                        }
                    }
                    if continuing_environments.is_empty() {
                        terminates = true;
                    } else {
                        promote_common_bindings(
                            &mut environment,
                            starting_environment,
                            &continuing_environments,
                        );
                    }
                    checked.push(CheckedStatement::Match {
                        value: self.lower_checked_expr(
                            &statement.value,
                            &environment,
                            Some(&matched_type),
                        )?,
                        cases,
                    });
                }
                Stmt::For(statement) => {
                    let iterable = self.infer_expr(&statement.iterable, &environment)?;
                    let element = match iterable {
                        Ty::List(element) => *element,
                        other => {
                            return Err(SemanticError::new(
                                statement.iterable.span(),
                                format!("for expects a collection, found {other}"),
                            ));
                        }
                    };
                    let mut loop_environment = environment.clone();
                    loop_environment.insert(statement.binding.value.clone(), element.clone());
                    let body =
                        self.check_block(&statement.body, &loop_environment, outputs, state_types)?;
                    checked.push(CheckedStatement::For {
                        binding: super::checked_field(&statement.binding.value, &element),
                        iterable: self.lower_checked_expr(
                            &statement.iterable,
                            &environment,
                            None,
                        )?,
                        body: body.statements,
                    });
                }
                Stmt::When(statement) => {
                    let trigger = match &statement.trigger {
                        Trigger::Every(expression) => {
                            self.require_duration(expression, &environment)?;
                            CheckedTrigger::Every {
                                duration: self.lower_checked_expr(
                                    expression,
                                    &environment,
                                    None,
                                )?,
                            }
                        }
                        Trigger::After(expression) => {
                            self.require_duration(expression, &environment)?;
                            CheckedTrigger::After {
                                duration: self.lower_checked_expr(
                                    expression,
                                    &environment,
                                    None,
                                )?,
                            }
                        }
                        Trigger::Event(expression) => CheckedTrigger::Event {
                            expression: self.lower_checked_expr(expression, &environment, None)?,
                        },
                    };
                    let body =
                        self.check_block(&statement.body, &environment, outputs, state_types)?;
                    checked.push(CheckedStatement::When {
                        trigger,
                        body: body.statements,
                    });
                }
                Stmt::Emit(statement) => {
                    let ty = self.infer_expr(&statement.event, &environment)?;
                    let is_event =
                        matches!(&ty, Ty::Named(name, _) if self.event_types.contains(name));
                    if !is_event {
                        return Err(SemanticError::new(
                            statement.event.span(),
                            format!("emit expects an event value, found {ty}"),
                        ));
                    }
                    checked.push(CheckedStatement::Emit {
                        event: self.lower_checked_expr(
                            &statement.event,
                            &environment,
                            Some(&ty),
                        )?,
                    });
                }
            }
        }
        Ok(CheckedBlock {
            statements: checked,
            environment,
            terminates,
        })
    }

    pub fn check_binding(
        &self,
        binding: &BindingStmt,
        environment: &mut HashMap<String, Ty>,
    ) -> Result<(CheckedBinding, Ty), SemanticError> {
        let inferred = self.infer_expr(&binding.value, environment)?;
        let ty = if let Some(annotation) = &binding.annotation {
            let declared = self.lower_type(annotation, &BTreeSet::new())?;
            if !self.compatible(&inferred, &declared) {
                let mut error = SemanticError::new(
                    binding.value.span(),
                    format!("binding has type {inferred}, but annotation requires {declared}"),
                );
                if let Some((actual, expected)) = self.first_mismatch(&inferred, &declared)
                    && (actual != inferred || expected != declared)
                {
                    error = error.help(format!("'{actual}' does not fit '{expected}'"));
                }
                return Err(error);
            }
            declared
        } else {
            inferred
        };
        if let Some(previous) = environment.get(&binding.names[0].value)
            && !self.compatible(&ty, previous)
        {
            return Err(SemanticError::new(
                binding.span,
                format!(
                    "cannot replace binding '{}' of type {previous} with {ty}",
                    binding.names[0].value
                ),
            ));
        }
        environment.insert(binding.names[0].value.clone(), ty.clone());
        Ok((
            CheckedBinding {
                doc: binding.doc.clone(),
                targets: binding
                    .names
                    .iter()
                    .map(|name| super::checked_field(&name.value, &ty))
                    .collect(),
                value: self.lower_checked_expr(&binding.value, environment, Some(&ty))?,
            },
            ty,
        ))
    }

    pub fn check_effect(
        &self,
        effect: &EffectStmt,
        environment: &HashMap<String, Ty>,
    ) -> Result<(ResolvedAction, Vec<Ty>), SemanticError> {
        let words = effect.action.split_whitespace().collect::<Vec<_>>();
        let operation = words
            .first()
            .copied()
            .ok_or_else(|| SemanticError::new(effect.span, "empty effect action"))?;
        if let Some(contract) = self.actions.get(operation).cloned() {
            let (mut action, types) =
                self.check_standard_action_contract(effect, &words, environment, contract)?;
            // A declared verb states the capability it needs; the call carries
            // it so the compiler can derive a method that requires it. The six
            // bundled verbs are not in this map and keep their capability in
            // their own lowering.
            action.capability = self
                .action_contracts
                .get(operation)
                .map(|contract| contract.capability.clone());
            return Ok((action, types));
        }
        let providers = self.standard_library.action_providers(operation);
        if let [required_module] = providers.as_slice() {
            return Err(SemanticError::new(
                effect.span,
                format!("operation '{operation}' requires 'use {required_module}'"),
            ));
        }
        let signature = self.workflows.get(operation).ok_or_else(|| {
            SemanticError::new(
                effect.span,
                format!("unknown durable operation '{operation}'"),
            )
        })?;
        if words.len() - 1 != signature.inputs.len() {
            return Err(SemanticError::new(
                effect.span,
                format!(
                    "workflow '{operation}' expects {} argument(s)",
                    signature.inputs.len()
                ),
            ));
        }
        // A workflow may be generic, so its operands both check against the
        // declared inputs and decide what its type parameters stand for.
        let spans = effect.words();
        let mut operands = Vec::new();
        for ((word, span), expected) in spans[1..].iter().zip(&signature.inputs) {
            let actual = self.resolve_action_operand(word, environment, *span)?;
            operands.push((expected.clone(), actual, *span));
        }
        let substitutions = self.infer_type_arguments(
            &signature.generics,
            &operands,
            "workflow",
            operation,
            effect.span,
        )?;

        let inputs = signature
            .inputs
            .iter()
            .map(|ty| substitute(ty, &substitutions))
            .collect::<Vec<_>>();
        let outputs = signature
            .outputs
            .iter()
            .map(|(name, ty)| (name.clone(), substitute(ty, &substitutions)))
            .collect::<Vec<_>>();
        let results = outputs.iter().map(|(_, ty)| ty.clone()).collect();
        let arguments = spans[1..]
            .iter()
            .zip(&inputs)
            .enumerate()
            .map(|(index, ((word, _), ty))| CheckedActionArgument {
                name: format!("input_{index}"),
                mode: if self.type_contains_material(ty, &mut BTreeSet::new()) {
                    OwnershipMode::Take
                } else {
                    OwnershipMode::Copy
                },
                value: action_contract::action_reference(
                    self.definition_for_action_word(word),
                    word,
                    ty,
                ),
            })
            .collect::<Vec<_>>();
        Ok((
            ResolvedAction {
                operation: format!("workflow.{operation}"),
                callee: Some(self.definition_for_action_word(operation)),
                capability: None,
                arguments,
                results: outputs
                    .iter()
                    .map(|(name, ty)| super::checked_field(name, ty))
                    .collect(),
            },
            results,
        ))
    }
}

pub(super) struct CheckedBlock {
    statements: Vec<CheckedStatement>,
    environment: HashMap<String, Ty>,
    terminates: bool,
}

fn promote_common_bindings(
    target: &mut HashMap<String, Ty>,
    original: &HashMap<String, Ty>,
    continuing: &[HashMap<String, Ty>],
) {
    let Some(first) = continuing.first() else {
        return;
    };
    for (name, ty) in first {
        if original.contains_key(name) {
            continue;
        }
        if continuing
            .iter()
            .skip(1)
            .all(|environment| environment.get(name) == Some(ty))
        {
            target.insert(name.clone(), ty.clone());
        }
    }
}
