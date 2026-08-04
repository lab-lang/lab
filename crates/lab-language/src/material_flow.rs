//! Affine physical-material verification for portable module IR.
//!
//! Types identify physical places, while resolved action contracts decide
//! whether an operand is copied, borrowed, or taken. This pass deliberately
//! runs after semantic checking so it never has to reinterpret source syntax.

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use thiserror::Error;

use super::checked::*;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("affine material-flow error in workflow '{workflow}' at {location}: {message}")]
pub struct MaterialFlowError {
    pub workflow: String,
    pub location: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Place(Vec<String>);

impl Place {
    fn root(name: &str) -> Self {
        Self(vec![name.to_owned()])
    }

    fn field(&self, name: &str) -> Self {
        let mut path = self.0.clone();
        path.push(name.to_owned());
        Self(path)
    }

    fn has_root(&self, name: &str) -> bool {
        self.0.first().is_some_and(|root| root == name)
    }
}

impl fmt::Display for Place {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0.join("."))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct FlowState {
    live: BTreeSet<Place>,
    consumed: BTreeSet<Place>,
}

#[derive(Clone, Debug)]
struct FlowResult {
    state: FlowState,
    terminated: bool,
}

#[derive(Default)]
struct MaterialShapes {
    data: HashMap<String, Vec<CheckedField>>,
}

impl MaterialShapes {
    fn from_module(module: &CheckedModule) -> Self {
        let mut data = HashMap::new();
        for declaration in &module.declarations {
            let CheckedDeclaration::Data {
                name,
                fields,
                cases,
                ..
            } = declaration
            else {
                continue;
            };
            let mut all_fields = fields.clone();
            for case in cases {
                all_fields.extend(case.fields.clone());
            }
            data.insert(name.clone(), all_fields);
        }
        Self { data }
    }

    fn contains_material(&self, r#type: &CheckedType) -> bool {
        !self
            .material_places(r#type, Place::root("value"))
            .is_empty()
    }

    fn material_places(&self, r#type: &CheckedType, prefix: Place) -> Vec<Place> {
        let mut places = self.material_places_inner(r#type, prefix, &mut BTreeSet::new());
        places.sort();
        places.dedup();
        places
    }

    fn material_places_inner(
        &self,
        r#type: &CheckedType,
        prefix: Place,
        visiting: &mut BTreeSet<String>,
    ) -> Vec<Place> {
        match r#type {
            CheckedType::Named { name, .. } if name == "Material" => vec![prefix],
            CheckedType::List { element } => {
                if !self
                    .material_places_inner(element, prefix.clone(), visiting)
                    .is_empty()
                {
                    vec![prefix]
                } else {
                    Vec::new()
                }
            }
            CheckedType::Union { alternatives } => {
                let contains_material = alternatives.iter().any(|alternative| {
                    !self
                        .material_places_inner(alternative, prefix.clone(), visiting)
                        .is_empty()
                });
                if contains_material {
                    vec![prefix]
                } else {
                    Vec::new()
                }
            }
            CheckedType::Named { name, .. } if name == "Screening" => {
                vec![prefix.field("clones").field("highest_confidence")]
            }
            CheckedType::Named { name, arguments } => {
                if !visiting.insert(name.clone()) {
                    return Vec::new();
                }
                let mut places = Vec::new();
                if let Some(fields) = self.data.get(name) {
                    for field in fields {
                        places.extend(self.material_places_inner(
                            &field.r#type,
                            prefix.field(&field.name),
                            visiting,
                        ));
                    }
                } else if arguments.iter().any(|argument| {
                    !self
                        .material_places_inner(argument, prefix.clone(), visiting)
                        .is_empty()
                }) {
                    places.push(prefix.clone());
                }
                visiting.remove(name);
                places
            }
            _ => Vec::new(),
        }
    }
}

pub(crate) fn verify_module(module: &CheckedModule) -> Result<(), MaterialFlowError> {
    let shapes = MaterialShapes::from_module(module);
    for declaration in &module.declarations {
        let CheckedDeclaration::Workflow {
            name,
            inputs,
            state,
            body,
            ..
        } = declaration
        else {
            continue;
        };
        MaterialFlowAnalyzer::new(name, &shapes, state).verify(inputs, state, body)?;
    }
    Ok(())
}

struct MaterialFlowAnalyzer<'a> {
    workflow: &'a str,
    shapes: &'a MaterialShapes,
    state_types: HashMap<String, CheckedType>,
}

impl<'a> MaterialFlowAnalyzer<'a> {
    fn new(workflow: &'a str, shapes: &'a MaterialShapes, state: &[CheckedState]) -> Self {
        Self {
            workflow,
            shapes,
            state_types: state
                .iter()
                .map(|state| (state.name.clone(), state.r#type.clone()))
                .collect(),
        }
    }

    fn verify(
        &self,
        inputs: &[CheckedField],
        states: &[CheckedState],
        body: &[CheckedStatement],
    ) -> Result<(), MaterialFlowError> {
        let mut flow = FlowState::default();
        for input in inputs {
            self.introduce_field(&mut flow, input, "input")?;
        }
        for state in states {
            self.use_expression(
                &mut flow,
                &state.initial,
                OwnershipMode::Take,
                "state initializer",
            )?;
            self.introduce_type(&mut flow, &state.name, &state.r#type, "state initializer")?;
        }
        let result = self.analyze_block(body, flow, "body")?;
        let reactive = body
            .iter()
            .any(|statement| matches!(statement, CheckedStatement::When { .. }));
        if !result.terminated && !reactive && !result.state.live.is_empty() {
            return Err(self.error(
                "workflow exit",
                format!(
                    "workflow may finish while still owning {}",
                    display_places(&result.state.live)
                ),
            ));
        }
        Ok(())
    }

    fn analyze_block(
        &self,
        statements: &[CheckedStatement],
        mut flow: FlowState,
        location: &str,
    ) -> Result<FlowResult, MaterialFlowError> {
        let mut terminated = false;
        for (index, statement) in statements.iter().enumerate() {
            if terminated {
                break;
            }
            let statement_location = format!("{location}.{index}");
            match statement {
                CheckedStatement::Binding(binding) => {
                    let material_targets = binding
                        .targets
                        .iter()
                        .filter(|target| self.shapes.contains_material(&target.r#type))
                        .count();
                    if material_targets > 1 {
                        return Err(self.error(
                            &statement_location,
                            "one physical value cannot be bound to multiple names",
                        ));
                    }
                    let mode = if material_targets == 0 {
                        OwnershipMode::Borrow
                    } else {
                        OwnershipMode::Take
                    };
                    self.use_expression(&mut flow, &binding.value, mode, &statement_location)?;
                    for target in &binding.targets {
                        self.ensure_not_shadowing(&flow, &target.name, &statement_location)?;
                        self.introduce_field(&mut flow, target, &statement_location)?;
                    }
                }
                CheckedStatement::StateUpdate { state, value } => {
                    let r#type = self.state_types.get(state).ok_or_else(|| {
                        self.error(&statement_location, format!("unknown state cell '{state}'"))
                    })?;
                    let mode = if self.shapes.contains_material(r#type) {
                        OwnershipMode::Take
                    } else {
                        OwnershipMode::Borrow
                    };
                    self.use_expression(&mut flow, value, mode, &statement_location)?;
                    self.ensure_not_shadowing(&flow, state, &statement_location)?;
                    self.introduce_type(&mut flow, state, r#type, &statement_location)?;
                }
                CheckedStatement::Effect { results, action } => {
                    for argument in &action.arguments {
                        if argument.mode == OwnershipMode::Copy
                            && self.shapes.contains_material(&argument.value.r#type)
                        {
                            return Err(self.error(
                                &statement_location,
                                format!(
                                    "action '{}' attempts to copy physical operand '{}'",
                                    action.operation, argument.name
                                ),
                            ));
                        }
                        self.use_expression(
                            &mut flow,
                            &argument.value,
                            argument.mode,
                            &statement_location,
                        )?;
                    }
                    for result in results {
                        self.ensure_not_shadowing(&flow, &result.name, &statement_location)?;
                        self.introduce_field(&mut flow, result, &statement_location)?;
                    }
                }
                CheckedStatement::Return { value } => {
                    self.use_expression(
                        &mut flow,
                        value,
                        OwnershipMode::Take,
                        &statement_location,
                    )?;
                    self.require_empty(&flow, &statement_location)?;
                    terminated = true;
                }
                CheckedStatement::If {
                    condition,
                    body,
                    else_body,
                } => {
                    self.use_expression(
                        &mut flow,
                        condition,
                        OwnershipMode::Borrow,
                        &statement_location,
                    )?;
                    let then_result = self.analyze_block(
                        body,
                        flow.clone(),
                        &format!("{statement_location}.then"),
                    )?;
                    let else_result = self.analyze_block(
                        else_body,
                        flow.clone(),
                        &format!("{statement_location}.else"),
                    )?;
                    let merged =
                        self.merge_branches(vec![then_result, else_result], &statement_location)?;
                    flow = merged.state;
                    terminated = merged.terminated;
                }
                CheckedStatement::Match { value, cases } => {
                    self.use_expression(
                        &mut flow,
                        value,
                        OwnershipMode::Borrow,
                        &statement_location,
                    )?;
                    let mut results = Vec::new();
                    for (case_index, case) in cases.iter().enumerate() {
                        results.push(self.analyze_block(
                            &case.body,
                            flow.clone(),
                            &format!("{statement_location}.case.{case_index}"),
                        )?);
                    }
                    let merged = self.merge_branches(results, &statement_location)?;
                    flow = merged.state;
                    terminated = merged.terminated;
                }
                CheckedStatement::For {
                    binding,
                    iterable,
                    body,
                } => {
                    if self.shapes.contains_material(&iterable.r#type) {
                        return Err(self.error(
                            &statement_location,
                            "iteration over physical material collections needs an explicit consuming iterator contract",
                        ));
                    }
                    self.use_expression(
                        &mut flow,
                        iterable,
                        OwnershipMode::Borrow,
                        &statement_location,
                    )?;
                    let mut loop_flow = flow.clone();
                    self.introduce_field(&mut loop_flow, binding, &statement_location)?;
                    let body_result =
                        self.analyze_block(body, loop_flow, &format!("{statement_location}.loop"))?;
                    if !body_result.terminated && body_result.state.live != flow.live {
                        return Err(self.error(
                            &statement_location,
                            "loop body changes physical ownership; use an explicit consuming iterator contract",
                        ));
                    }
                }
                CheckedStatement::When { trigger, body } => {
                    match trigger {
                        CheckedTrigger::Every { duration } | CheckedTrigger::After { duration } => {
                            self.use_expression(
                                &mut flow,
                                duration,
                                OwnershipMode::Borrow,
                                &statement_location,
                            )?
                        }
                        CheckedTrigger::Event { expression } => self.use_expression(
                            &mut flow,
                            expression,
                            OwnershipMode::Borrow,
                            &statement_location,
                        )?,
                    }
                    let handler = self.analyze_block(
                        body,
                        flow.clone(),
                        &format!("{statement_location}.handler"),
                    )?;
                    if !handler.terminated {
                        if handler.state.live != flow.live {
                            return Err(self.error(
                                &statement_location,
                                "a non-terminating reactive handler changes captured material ownership",
                            ));
                        }
                        if let Some(place) =
                            handler
                                .state
                                .consumed
                                .difference(&flow.consumed)
                                .find(|place| {
                                    flow.live.contains(*place)
                                        && place
                                            .0
                                            .first()
                                            .is_none_or(|root| !self.state_types.contains_key(root))
                                })
                        {
                            return Err(self.error(
                                &statement_location,
                                format!(
                                    "non-terminating handler consumes captured '{place}'; move evolving material into explicit state"
                                ),
                            ));
                        }
                    }
                }
                CheckedStatement::Emit { event } => {
                    if self.shapes.contains_material(&event.r#type) {
                        return Err(self.error(
                            &statement_location,
                            "events cannot copy physical material into the durable journal",
                        ));
                    }
                    self.use_expression(
                        &mut flow,
                        event,
                        OwnershipMode::Borrow,
                        &statement_location,
                    )?;
                }
            }
        }
        Ok(FlowResult {
            state: flow,
            terminated,
        })
    }

    fn merge_branches(
        &self,
        branches: Vec<FlowResult>,
        location: &str,
    ) -> Result<FlowResult, MaterialFlowError> {
        let mut continuing = branches
            .into_iter()
            .filter(|branch| !branch.terminated)
            .collect::<Vec<_>>();
        if continuing.is_empty() {
            return Ok(FlowResult {
                state: FlowState::default(),
                terminated: true,
            });
        }
        let mut expected = continuing.remove(0).state;
        for branch in continuing {
            if branch.state.live != expected.live {
                return Err(self.error(
                    location,
                    format!(
                        "continuing branches own different physical values: [{}] versus [{}]",
                        display_places(&expected.live),
                        display_places(&branch.state.live)
                    ),
                ));
            }
            expected.consumed.extend(branch.state.consumed);
        }
        Ok(FlowResult {
            state: expected,
            terminated: false,
        })
    }

    fn use_expression(
        &self,
        flow: &mut FlowState,
        expression: &TypedExpression,
        mode: OwnershipMode,
        location: &str,
    ) -> Result<(), MaterialFlowError> {
        match &expression.value {
            CheckedExpression::Reference { path } => {
                if self.shapes.contains_material(&expression.r#type) {
                    self.use_places(
                        flow,
                        self.shapes
                            .material_places(&expression.r#type, Place(path.clone())),
                        mode,
                        location,
                    )?;
                }
            }
            CheckedExpression::Field { subject, .. } => {
                if self.shapes.contains_material(&expression.r#type) {
                    let path = expression_path(expression).ok_or_else(|| {
                        self.error(
                            location,
                            "physical field expression has no stable ownership path",
                        )
                    })?;
                    self.use_places(
                        flow,
                        self.shapes.material_places(&expression.r#type, Place(path)),
                        mode,
                        location,
                    )?;
                } else if is_direct_material(&subject.r#type) {
                    self.use_expression(flow, subject, OwnershipMode::Borrow, location)?;
                }
            }
            CheckedExpression::List { elements } => {
                for element in elements {
                    self.use_expression(flow, element, mode, location)?;
                }
            }
            CheckedExpression::Construct { fields, .. } => {
                for field in fields {
                    self.use_expression(flow, &field.value, mode, location)?;
                }
            }
            CheckedExpression::Call {
                operation,
                arguments,
            } => {
                if self.shapes.contains_material(&expression.r#type) {
                    return Err(self.error(
                        location,
                        format!(
                            "pure operation '{operation}' cannot create physical material; use a typed action contract"
                        ),
                    ));
                }
                for argument in arguments {
                    self.use_expression(flow, &argument.value, OwnershipMode::Borrow, location)?;
                }
            }
            CheckedExpression::Unary { operand, .. } => {
                self.use_expression(flow, operand, OwnershipMode::Borrow, location)?;
            }
            CheckedExpression::Binary { left, right, .. } => {
                self.use_expression(flow, left, OwnershipMode::Borrow, location)?;
                self.use_expression(flow, right, OwnershipMode::Borrow, location)?;
            }
            CheckedExpression::Integer { .. }
            | CheckedExpression::Decimal { .. }
            | CheckedExpression::String { .. }
            | CheckedExpression::Quantity { .. } => {}
        }
        Ok(())
    }

    fn use_places(
        &self,
        flow: &mut FlowState,
        places: Vec<Place>,
        mode: OwnershipMode,
        location: &str,
    ) -> Result<(), MaterialFlowError> {
        for place in places {
            if !flow.live.contains(&place) {
                return Err(self.error(
                    location,
                    format!("physical value '{place}' is no longer available"),
                ));
            }
            match mode {
                OwnershipMode::Copy => {
                    return Err(self.error(
                        location,
                        format!("physical value '{place}' cannot be copied"),
                    ));
                }
                OwnershipMode::Borrow => {}
                OwnershipMode::Take => {
                    flow.live.remove(&place);
                    flow.consumed.insert(place);
                }
            }
        }
        Ok(())
    }

    fn introduce_field(
        &self,
        flow: &mut FlowState,
        field: &CheckedField,
        location: &str,
    ) -> Result<(), MaterialFlowError> {
        self.introduce_type(flow, &field.name, &field.r#type, location)
    }

    fn introduce_type(
        &self,
        flow: &mut FlowState,
        name: &str,
        r#type: &CheckedType,
        location: &str,
    ) -> Result<(), MaterialFlowError> {
        for place in self.shapes.material_places(r#type, Place::root(name)) {
            if !flow.live.insert(place.clone()) {
                return Err(self.error(
                    location,
                    format!("physical value '{place}' would be owned more than once"),
                ));
            }
        }
        Ok(())
    }

    fn ensure_not_shadowing(
        &self,
        flow: &FlowState,
        name: &str,
        location: &str,
    ) -> Result<(), MaterialFlowError> {
        if let Some(place) = flow.live.iter().find(|place| place.has_root(name)) {
            return Err(self.error(
                location,
                format!("binding '{name}' would hide still-owned physical value '{place}'"),
            ));
        }
        Ok(())
    }

    fn require_empty(&self, flow: &FlowState, location: &str) -> Result<(), MaterialFlowError> {
        if flow.live.is_empty() {
            return Ok(());
        }
        Err(self.error(
            location,
            format!(
                "terminating path still owns {}; return, store, transfer, or dispose it",
                display_places(&flow.live)
            ),
        ))
    }

    fn error(&self, location: &str, message: impl Into<String>) -> MaterialFlowError {
        MaterialFlowError {
            workflow: self.workflow.to_owned(),
            location: location.to_owned(),
            message: message.into(),
        }
    }
}

fn expression_path(expression: &TypedExpression) -> Option<Vec<String>> {
    match &expression.value {
        CheckedExpression::Reference { path } => Some(path.clone()),
        CheckedExpression::Field { subject, field } => {
            let mut path = expression_path(subject)?;
            path.push(field.clone());
            Some(path)
        }
        _ => None,
    }
}

fn is_direct_material(r#type: &CheckedType) -> bool {
    matches!(r#type, CheckedType::Named { name, .. } if name == "Material")
}

fn display_places(places: &BTreeSet<Place>) -> String {
    places
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::super::compile_module;
    use super::*;

    fn material(inner: &str) -> CheckedType {
        CheckedType::Named {
            name: "Material".to_owned(),
            arguments: vec![CheckedType::Named {
                name: inner.to_owned(),
                arguments: Vec::new(),
            }],
        }
    }

    #[test]
    fn rejects_use_after_take() {
        let error = compile_module(
            r#"use std.lab.plasmid_actions

workflow invalid:
  input sample: Material<Plasmid>
  output Material<Plasmid>
  <- dispose sample
  evidence <- quantify sample
  return sample
"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("sample' is no longer available"));
    }

    #[test]
    fn rejects_implicit_physical_copying() {
        let sample_type = material("Plasmid");
        let module = CheckedModule {
            imports: Vec::new(),
            declarations: vec![CheckedDeclaration::Workflow {
                name: "invalid".to_owned(),
                inputs: vec![CheckedField {
                    name: "sample".to_owned(),
                    r#type: sample_type.clone(),
                }],
                output: sample_type.clone(),
                state: Vec::new(),
                body: vec![CheckedStatement::Binding(CheckedBinding {
                    targets: vec![
                        CheckedField {
                            name: "first".to_owned(),
                            r#type: sample_type.clone(),
                        },
                        CheckedField {
                            name: "second".to_owned(),
                            r#type: sample_type.clone(),
                        },
                    ],
                    value: TypedExpression {
                        r#type: sample_type,
                        value: CheckedExpression::Reference {
                            path: vec!["sample".to_owned()],
                        },
                    },
                })],
            }],
        };
        let error = verify_module(&module).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("cannot be bound to multiple names"),
            "{error}"
        );
    }

    #[test]
    fn rejects_branch_dependent_material_loss() {
        let error = compile_module(
            r#"use std.lab.plasmid_actions

workflow invalid:
  input should_dispose: Bool
  input sample: Material<Plasmid>
  output None
  if should_dispose:
    <- dispose sample
  return None
"#,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("continuing branches own different physical values"),
            "{error}"
        );
    }

    #[test]
    fn permits_repeated_borrows_before_transfer() {
        compile_module(
            r#"use std.lab.plasmid_actions

workflow valid:
  input sample: Material<Plasmid>
  output Material<Plasmid>
  first <- quantify sample
  second <- quantify sample
  return sample
"#,
        )
        .unwrap();
    }

    #[test]
    fn tracks_material_projections_without_poisoning_sibling_data() {
        compile_module(
            r#"use std.lab.plasmid_actions

outcome Inspection:
  sample: Material<Plasmid>
  observations: List<Evidence>
  case Complete

workflow valid:
  input inspection: Inspection
  output List<Evidence>
  <- dispose inspection.sample
  return inspection.observations
"#,
        )
        .unwrap();
    }

    #[test]
    fn rejects_a_terminating_path_that_leaks_material() {
        let error = compile_module(
            r#"workflow invalid:
  input sample: Material<Plasmid>
  output None
  return None
"#,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("terminating path still owns sample"),
            "{error}"
        );
    }

    #[test]
    fn rejects_replacing_captured_material_in_a_repeating_handler() {
        let error = compile_module(
            r#"use std.lab.plasmid_actions

workflow invalid:
  input sample: Material<Plasmid>
  output Material<Plasmid>
  when every 1 h:
    sample <- store sample at -20 C
  when after 24 h:
    return sample
"#,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("move evolving material into explicit state"),
            "{error}"
        );
    }

    #[test]
    fn permits_replacing_material_held_in_explicit_state() {
        compile_module(
            r#"use std.lab.plasmid_actions

workflow valid:
  input initial: Material<Plasmid>
  output Material<Plasmid>
  state sample: Material<Plasmid> = initial
  when every 1 h:
    updated <- store sample at -20 C
    sample = updated
  when after 24 h:
    return sample
"#,
        )
        .unwrap();
    }
}
