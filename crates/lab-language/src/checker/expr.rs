//! Expression type inference and lowering to checked expressions.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::ast::{Argument, BinaryOp, Expr, FieldValue, Path, UnaryOp};
use crate::checked::{CheckedArgument, CheckedExpression, CheckedFieldValue, TypedExpression};
use crate::semantic_error::SemanticError;
use crate::source::Span;
use crate::standard_library::ConstructorSpec;
use crate::type_system::{Substitutions, Ty, substitute, to_checked_type};

use super::Checker;
use super::context::Generics;

impl Checker {
    pub fn lower_checked_expr(
        &self,
        expression: &Expr,
        environment: &HashMap<String, Ty>,
        expected: Option<&Ty>,
    ) -> Result<TypedExpression, SemanticError> {
        let inferred = self.infer_expr(expression, environment)?;
        let ty = expected.unwrap_or(&inferred);
        let value = match expression {
            Expr::Path(path) => CheckedExpression::Reference {
                definition: self.definition_for_path(path),
                path: path
                    .segments
                    .iter()
                    .map(|segment| segment.value.clone())
                    .collect(),
            },
            Expr::Integer { value, .. } => CheckedExpression::Integer { value: *value },
            Expr::Decimal { text, .. } => CheckedExpression::Decimal { text: text.clone() },
            Expr::String { value, .. } => CheckedExpression::String {
                value: value.clone(),
            },
            Expr::Quantity {
                magnitude, unit, ..
            } => CheckedExpression::Quantity {
                magnitude: numeric_text(magnitude)?,
                unit: unit.clone(),
            },
            Expr::List { elements, .. } => CheckedExpression::List {
                elements: elements
                    .iter()
                    .map(|element| self.lower_checked_expr(element, environment, None))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            Expr::Call {
                callee, arguments, ..
            } => {
                let Expr::Path(path) = callee.as_ref() else {
                    return Err(SemanticError::new(
                        callee.span(),
                        "checked call target is not a resolved path",
                    ));
                };
                let name = super::path_text(path);
                let operation = if self.circuits.contains_key(&name) {
                    format!("circuit.{name}")
                } else if let Some(function) = self.pure_functions.get(&name) {
                    function.operation.to_owned()
                } else {
                    name
                };
                CheckedExpression::Call {
                    operation,
                    arguments: arguments
                        .iter()
                        .map(|argument| {
                            Ok(CheckedArgument {
                                name: argument.name.as_ref().map(|name| name.value.clone()),
                                value: self.lower_checked_expr(
                                    &argument.value,
                                    environment,
                                    None,
                                )?,
                            })
                        })
                        .collect::<Result<Vec<_>, SemanticError>>()?,
                }
            }
            Expr::Record {
                constructor,
                fields,
                ..
            } => {
                let name = super::path_text(constructor);
                let constructor = if let Some(parent) = self.cases.get(&name) {
                    format!("case.{parent}.{name}")
                } else if self.data.contains_key(&name) {
                    format!("data.{name}")
                } else if let Some(spec) = self.constructors.get(&name) {
                    spec.operation.to_owned()
                } else {
                    name
                };
                CheckedExpression::Construct {
                    constructor,
                    fields: fields
                        .iter()
                        .map(|field| {
                            Ok(CheckedFieldValue {
                                name: field.name.value.clone(),
                                value: self.lower_checked_expr(&field.value, environment, None)?,
                            })
                        })
                        .collect::<Result<Vec<_>, SemanticError>>()?,
                }
            }
            Expr::Field { subject, field, .. } => CheckedExpression::Field {
                subject: Box::new(self.lower_checked_expr(subject, environment, None)?),
                field: field.value.clone(),
            },
            Expr::Unary { op, operand, .. } => CheckedExpression::Unary {
                operator: match op {
                    UnaryOp::Negate => "negate",
                    UnaryOp::Not => "not",
                }
                .to_owned(),
                operand: Box::new(self.lower_checked_expr(operand, environment, None)?),
            },
            Expr::Binary {
                op, left, right, ..
            } => CheckedExpression::Binary {
                operator: binary_operator_name(*op).to_owned(),
                left: Box::new(self.lower_checked_expr(left, environment, None)?),
                right: Box::new(self.lower_checked_expr(right, environment, None)?),
            },
        };
        Ok(TypedExpression {
            r#type: to_checked_type(ty),
            value,
        })
    }

    pub fn infer_expr(
        &self,
        expression: &Expr,
        environment: &HashMap<String, Ty>,
    ) -> Result<Ty, SemanticError> {
        match expression {
            Expr::Path(path) => self.resolve_path(path, environment),
            Expr::Integer { .. } => Ok(Ty::Integer),
            Expr::Decimal { .. } => Ok(Ty::Decimal),
            Expr::String { .. } => Ok(Ty::String),
            Expr::Quantity { unit, .. } => Ok(Ty::Quantity(unit.clone())),
            Expr::List { elements, .. } => {
                let Some(first) = elements.first() else {
                    return Ok(Ty::EmptyList);
                };
                let mut element_type = self.infer_expr(first, environment)?;
                for element in &elements[1..] {
                    let found = self.infer_expr(element, environment)?;
                    element_type = self.common_type(element_type, found);
                }
                Ok(Ty::List(Box::new(element_type)))
            }
            Expr::Call {
                callee,
                arguments,
                span,
            } => self.infer_call(callee, arguments, *span, environment),
            Expr::Record {
                constructor,
                fields,
                span,
            } => self.infer_record(constructor, fields, *span, environment),
            Expr::Field {
                subject,
                field,
                span,
            } => {
                let subject = self.infer_expr(subject, environment)?;
                self.field_type(&subject, &field.value, *span)
            }
            Expr::Unary { op, operand, span } => {
                let operand = self.infer_expr(operand, environment)?;
                match op {
                    UnaryOp::Not if operand == Ty::Bool => Ok(Ty::Bool),
                    UnaryOp::Negate
                        if matches!(operand, Ty::Integer | Ty::Decimal | Ty::Quantity(_)) =>
                    {
                        Ok(operand)
                    }
                    _ => Err(SemanticError::new(
                        *span,
                        format!("invalid unary operand {operand}"),
                    )),
                }
            }
            Expr::Binary {
                op,
                left,
                right,
                span,
            } => {
                let left = self.infer_expr(left, environment)?;
                let right = self.infer_expr(right, environment)?;
                match op {
                    BinaryOp::Or | BinaryOp::And => {
                        if left == Ty::Bool && right == Ty::Bool {
                            Ok(Ty::Bool)
                        } else {
                            Err(SemanticError::new(
                                *span,
                                "boolean operator requires Bool operands",
                            ))
                        }
                    }
                    // Equality is otherwise permissive, but two measurements in
                    // different units are never equal and never unequal: the
                    // question cannot be asked until one is converted.
                    BinaryOp::Equal | BinaryOp::NotEqual => {
                        match mismatched_units(&left, &right, *span) {
                            Some(error) => Err(error),
                            None => Ok(Ty::Bool),
                        }
                    }
                    BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual => {
                        if self.comparable(&left, &right) {
                            Ok(Ty::Bool)
                        } else {
                            Err(mismatched_units(&left, &right, *span).unwrap_or_else(|| {
                                SemanticError::new(
                                    *span,
                                    format!("cannot compare {left} with {right}"),
                                )
                            }))
                        }
                    }
                    BinaryOp::Add => match (&left, &right) {
                        (Ty::List(left), Ty::List(right)) if self.compatible(left, right) => {
                            Ok(Ty::List(left.clone()))
                        }
                        (Ty::List(left), Ty::EmptyList) => Ok(Ty::List(left.clone())),
                        (Ty::EmptyList, Ty::List(right)) => Ok(Ty::List(right.clone())),
                        _ if self.comparable(&left, &right) => Ok(left),
                        _ => Err(mismatched_units(&left, &right, *span).unwrap_or_else(|| {
                            SemanticError::new(*span, format!("cannot add {left} and {right}"))
                        })),
                    },
                    BinaryOp::Subtract => {
                        if self.comparable(&left, &right) {
                            Ok(left)
                        } else {
                            Err(mismatched_units(&left, &right, *span).unwrap_or_else(|| {
                                SemanticError::new(
                                    *span,
                                    format!("cannot subtract {right} from {left}"),
                                )
                            }))
                        }
                    }
                    // Scaling a measurement by a count keeps its unit, which is
                    // how a recipe states a batch. Multiplying two measurements
                    // is a different operation: it yields a quantity in neither
                    // operand's unit, and until dimensions are computed the
                    // honest answer is that it cannot be written.
                    BinaryOp::Multiply | BinaryOp::Divide => {
                        match (&left, &right) {
                            (Ty::Quantity(_), other) if crate::type_system::dimensionless(other) => {
                                Ok(left)
                            }
                            (other, Ty::Quantity(_))
                                if crate::type_system::dimensionless(other)
                                    && matches!(op, BinaryOp::Multiply) =>
                            {
                                Ok(right)
                            }
                            (Ty::Quantity(left_unit), Ty::Quantity(right_unit)) => {
                                Err(SemanticError::new(
                                    *span,
                                    format!(
                                        "cannot multiply or divide {left} by {right}"
                                    ),
                                )
                                .help(format!(
                                    "the result is measured in neither '{left_unit}' nor '{right_unit}', and a quantity's dimension is not yet computed"
                                ))
                                .help(
                                    "scale a measurement by a plain number instead, such as '20 uL * 3'",
                                ))
                            }
                            _ if self.comparable(&left, &right) => Ok(left),
                            _ => Err(SemanticError::new(
                                *span,
                                format!("cannot combine {left} with {right} arithmetically"),
                            )),
                        }
                    }
                    BinaryOp::Range => Ok(Ty::List(Box::new(left))),
                }
            }
        }
    }

    pub fn infer_call(
        &self,
        callee: &Expr,
        arguments: &[Argument],
        span: Span,
        environment: &HashMap<String, Ty>,
    ) -> Result<Ty, SemanticError> {
        let Expr::Path(path) = callee else {
            return Err(SemanticError::new(
                span,
                "call target must resolve to a named operation",
            ));
        };
        let name = super::path_text(path);
        if let Some(signature) = self.circuits.get(&name) {
            if arguments.len() != signature.inputs.len() {
                return Err(SemanticError::new(
                    span,
                    format!(
                        "circuit '{name}' expects {} arguments",
                        signature.inputs.len()
                    ),
                ));
            }
            let mut operands = Vec::new();
            for (argument, parameter) in arguments.iter().zip(&signature.inputs) {
                let actual = self.infer_expr(&argument.value, environment)?;
                operands.push((parameter.clone(), actual, argument.value.span()));
            }
            let substitutions =
                self.infer_type_arguments(&signature.generics, &operands, "circuit", &name, span)?;
            return Ok(substitute(&signature.output, &substitutions));
        }
        if let Some(function) = self.pure_functions.get(&name) {
            let actual =
                self.require_call_arguments(arguments, function.parameters.len(), environment)?;
            for (actual, expected) in actual.iter().zip(&function.parameters) {
                if !self.compatible(actual, expected) {
                    return Err(SemanticError::new(
                        span,
                        format!(
                            "operation '{}' expects {expected}, found {actual}",
                            function.operation
                        ),
                    ));
                }
            }
            return Ok(function.result.clone());
        }
        let providers = self.standard_library.function_providers(&name);
        if let [required_module] = providers.as_slice() {
            return Err(SemanticError::new(
                span,
                format!("operation '{name}' requires 'use {required_module}'"),
            ));
        }
        Err(SemanticError::new(
            span,
            format!("unknown pure operation '{name}'"),
        ))
    }

    pub fn require_call_arguments(
        &self,
        arguments: &[Argument],
        expected: usize,
        environment: &HashMap<String, Ty>,
    ) -> Result<Vec<Ty>, SemanticError> {
        if arguments.len() != expected {
            let span = arguments
                .first()
                .map_or(Span::at(0), |argument| argument.span);
            return Err(SemanticError::new(
                span,
                format!("expected {expected} argument(s), found {}", arguments.len()),
            ));
        }
        arguments
            .iter()
            .map(|argument| self.infer_expr(&argument.value, environment))
            .collect()
    }

    pub fn infer_record(
        &self,
        constructor: &Path,
        fields: &[FieldValue],
        span: Span,
        environment: &HashMap<String, Ty>,
    ) -> Result<Ty, SemanticError> {
        let name = super::path_text(constructor);
        let result = if let Some(parent) = self.cases.get(&name) {
            let signature = &self.data[parent];
            let mut expected = signature.fields.clone();
            expected.extend(signature.cases[&name].clone());
            self.check_constructor_fields(&name, fields, &expected, environment, span)?;
            Ty::named(parent)
        } else if let Some(signature) = self.data.get(&name) {
            self.check_constructor_fields(&name, fields, &signature.fields, environment, span)?;
            Ty::named(&name)
        } else if let Some(constructor) = self.constructors.get(&name) {
            let expected = self.constructor_fields(constructor);
            self.check_constructor_fields(&name, fields, &expected, environment, span)?;
            constructor.result.clone()
        } else {
            return Err(SemanticError::new(
                span,
                format!("unknown constructor '{name}'"),
            ));
        };
        Ok(result)
    }

    pub fn constructor_fields(&self, constructor: &ConstructorSpec) -> BTreeMap<String, Ty> {
        constructor
            .fields
            .iter()
            .map(|(name, ty)| {
                let ty = if matches!(ty, Ty::List(element) if **element == Ty::named("Evidence")) {
                    let evidence = std::iter::once(Ty::named("Evidence"))
                        .chain(
                            self.data
                                .iter()
                                // Anything playing `Evidential` may be offered
                                // in support of a claim, which is what this
                                // field asks for.
                                .filter(|(name, _)| {
                                    self.type_roles
                                        .get(*name)
                                        .is_some_and(|roles| roles.contains("Evidential"))
                                })
                                .map(|(name, _)| Ty::named(name)),
                        )
                        .collect();
                    Ty::List(Box::new(Ty::Union(evidence)))
                } else {
                    ty.clone()
                };
                ((*name).to_owned(), ty)
            })
            .collect()
    }

    pub fn check_constructor_fields(
        &self,
        constructor: &str,
        fields: &[FieldValue],
        expected: &BTreeMap<String, Ty>,
        environment: &HashMap<String, Ty>,
        span: Span,
    ) -> Result<(), SemanticError> {
        let mut provided = BTreeSet::new();
        for field in fields {
            let Some(expected_type) = expected.get(&field.name.value) else {
                return Err(SemanticError::new(
                    field.name.span,
                    format!(
                        "constructor '{constructor}' has no field '{}'",
                        field.name.value
                    ),
                ));
            };
            if !provided.insert(field.name.value.clone()) {
                return Err(SemanticError::new(
                    field.name.span,
                    "duplicate constructor field",
                ));
            }
            let actual = self.infer_expr(&field.value, environment)?;
            if !self.compatible(&actual, expected_type) {
                return Err(SemanticError::new(
                    field.value.span(),
                    format!(
                        "field '{}.{}' expects {expected_type}, found {actual}",
                        constructor, field.name.value
                    ),
                ));
            }
        }
        let missing = expected
            .keys()
            .filter(|name| !provided.contains(*name))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(SemanticError::new(
                span,
                format!("constructor '{constructor}' is missing fields {missing:?}"),
            ));
        }
        Ok(())
    }

    pub fn resolve_path(
        &self,
        path: &Path,
        environment: &HashMap<String, Ty>,
    ) -> Result<Ty, SemanticError> {
        let first = &path.segments[0];
        let mut ty = environment
            .get(&first.value)
            .or_else(|| self.values.get(&first.value))
            .cloned()
            .ok_or_else(|| {
                SemanticError::new(first.span, format!("unknown name '{}'", first.value))
            })?;
        for field in &path.segments[1..] {
            ty = self.field_type(&ty, &field.value, field.span)?;
        }
        Ok(ty)
    }

    pub fn field_type(&self, subject: &Ty, field: &str, span: Span) -> Result<Ty, SemanticError> {
        let result = match (subject, field) {
            (Ty::Named(name, _), field) if self.data.contains_key(name) => {
                self.data[name].fields.get(field).cloned().or_else(|| {
                    self.data[name]
                        .cases
                        .values()
                        .find_map(|fields| fields.get(field).cloned())
                })
            }
            (Ty::Named(name, _), field) => self
                .standard_types
                .get(name)
                .and_then(|spec| spec.fields.get(field))
                .cloned(),
            _ => None,
        };
        result.ok_or_else(|| {
            SemanticError::new(span, format!("type {subject} has no field '{field}'"))
        })
    }

    pub fn require_bool(
        &self,
        expression: &Expr,
        environment: &HashMap<String, Ty>,
        context: &str,
    ) -> Result<(), SemanticError> {
        let ty = self.infer_expr(expression, environment)?;
        if ty != Ty::Bool {
            return Err(SemanticError::new(
                expression.span(),
                format!("{context} expects Bool, found {ty}"),
            ));
        }
        Ok(())
    }

    pub fn require_duration(
        &self,
        expression: &Expr,
        environment: &HashMap<String, Ty>,
    ) -> Result<(), SemanticError> {
        let ty = self.infer_expr(expression, environment)?;
        match ty {
            Ty::Quantity(ref unit) if matches!(unit.as_str(), "min" | "h") => Ok(()),
            _ => Err(SemanticError::new(
                expression.span(),
                format!("timer expects a duration, found {ty}"),
            )),
        }
    }

    /// Infer a callable's type arguments from its operands and check each
    /// against its bound.
    ///
    /// Circuits and workflows are called through different syntax but are
    /// generic in the same way, so they share this rather than each carrying
    /// their own copy of the rule.
    pub fn infer_type_arguments(
        &self,
        generics: &Generics,
        operands: &[(Ty, Ty, Span)],
        kind: &str,
        name: &str,
        span: Span,
    ) -> Result<Substitutions, SemanticError> {
        let mut substitutions = Substitutions::new();
        for (expected, actual, operand) in operands {
            self.unify(
                expected,
                actual,
                &generics.parameters,
                &mut substitutions,
                *operand,
            )
            .map_err(|error| {
                error.help(format!(
                    "{kind} '{name}' declares this operand as {expected}"
                ))
            })?;
        }
        for parameter in &generics.parameters {
            let Some(bound) = generics.bounds.get(parameter) else {
                continue;
            };
            let Some((inferred, operand)) = substitutions.get(parameter) else {
                return Err(SemanticError::new(
                    span,
                    format!("could not infer '{parameter}' for {kind} '{name}'"),
                ));
            };
            if !self.satisfies_bound(inferred, bound) {
                return Err(self.unsatisfied_bound(
                    inferred,
                    bound,
                    &format!("{kind} '{name}' requires its '{parameter}'"),
                    *operand,
                ));
            }
        }
        Ok(substitutions)
    }

    /// The innermost pair of types that actually disagree.
    ///
    /// A mismatch deep inside two large types otherwise prints both and leaves
    /// the reader to diff them; naming the part that failed is the difference
    /// between a report and an explanation.
    pub fn first_mismatch(&self, actual: &Ty, expected: &Ty) -> Option<(Ty, Ty)> {
        if self.compatible(actual, expected) {
            return None;
        }
        let inner = match (actual, expected) {
            (Ty::List(actual), Ty::List(expected)) => self.first_mismatch(actual, expected),
            // One bad alternative is what sinks a union, so report that one.
            (Ty::Union(alternatives), expected) if !matches!(expected, Ty::Union(_)) => {
                alternatives
                    .iter()
                    .find(|alternative| !self.compatible(alternative, expected))
                    .and_then(|alternative| self.first_mismatch(alternative, expected))
            }
            (
                Ty::Named(actual_name, actual_arguments),
                Ty::Named(expected_name, expected_arguments),
            ) if actual_name == expected_name
                && actual_arguments.len() == expected_arguments.len() =>
            {
                actual_arguments
                    .iter()
                    .zip(expected_arguments)
                    .find_map(|(actual, expected)| self.first_mismatch(actual, expected))
            }
            _ => None,
        };
        Some(inner.unwrap_or_else(|| (actual.clone(), expected.clone())))
    }

    /// Whether a type may stand for a parameter bounded by `bound`.
    ///
    /// A bound is satisfied by being the type itself or by playing it as a
    /// role. Membership declared in source and membership built into the
    /// standard library are the same relation, so both are read from one map.
    ///
    /// A declaration states the roles it plays, and its own type arguments do
    /// not bear on the question: `Both<TetR, LacI>` is a signal because `Both`
    /// is declared to be one. A role takes no arguments, so the bound is
    /// nullary either way.
    pub fn satisfies_bound(&self, actual: &Ty, bound: &Ty) -> bool {
        if self.compatible(actual, bound) {
            return true;
        }
        let (Ty::Named(actual, _), Ty::Named(bound, bound_arguments)) = (actual, bound) else {
            return false;
        };
        bound_arguments.is_empty()
            && self
                .type_roles
                .get(actual)
                .is_some_and(|roles| roles.contains(bound))
    }

    pub fn resolve_action_operand(
        &self,
        word: &str,
        environment: &HashMap<String, Ty>,
        span: Span,
    ) -> Result<Ty, SemanticError> {
        let segments = word.split('.').collect::<Vec<_>>();
        let mut ty = environment
            .get(segments[0])
            .or_else(|| self.values.get(segments[0]))
            .cloned()
            .ok_or_else(|| SemanticError::new(span, format!("unknown action operand '{word}'")))?;
        for field in &segments[1..] {
            ty = self.field_type(&ty, field, span)?;
        }
        Ok(ty)
    }

    pub fn type_contains_material(&self, ty: &Ty, visiting: &mut BTreeSet<String>) -> bool {
        match ty {
            Ty::Named(name, _) if name == "Material" => true,
            // A forgotten argument is a role tag, never a material. Packing
            // `Material<Tetracycline>` into `Material<any Signal>` leaves the
            // `Material` wrapper in place, so ownership is unaffected.
            Ty::Any(_) => false,
            Ty::List(element) => self.type_contains_material(element, visiting),
            Ty::Union(alternatives) => alternatives
                .iter()
                .any(|alternative| self.type_contains_material(alternative, visiting)),
            Ty::Named(name, _) if name == "Screening" => true,
            Ty::Named(name, arguments) => {
                if !visiting.insert(name.clone()) {
                    return false;
                }
                let contains = self.data.get(name).is_some_and(|signature| {
                    signature
                        .fields
                        .values()
                        .chain(signature.cases.values().flat_map(|fields| fields.values()))
                        .any(|field| self.type_contains_material(field, visiting))
                }) || arguments
                    .iter()
                    .any(|argument| self.type_contains_material(argument, visiting));
                visiting.remove(name);
                contains
            }
            _ => false,
        }
    }
}

pub(super) fn numeric_text(expression: &Expr) -> Result<String, SemanticError> {
    match expression {
        Expr::Integer { value, .. } => Ok(value.to_string()),
        Expr::Decimal { text, .. } => Ok(text.clone()),
        Expr::Unary {
            op: UnaryOp::Negate,
            operand,
            ..
        } => Ok(format!("-{}", numeric_text(operand)?)),
        _ => Err(SemanticError::new(
            expression.span(),
            "quantity magnitude must be numeric",
        )),
    }
}

/// The diagnostic for two measurements that meet in a unit neither shares.
///
/// This is the same mistake as writing `20 mL` where microlitres are required,
/// so it reads the same way: name both units and say that conversion is written
/// rather than assumed. Returns `None` when the operands are not both
/// quantities, so the caller keeps its own wording.
fn mismatched_units(left: &Ty, right: &Ty, span: Span) -> Option<SemanticError> {
    let (Ty::Quantity(left_unit), Ty::Quantity(right_unit)) = (left, right) else {
        return None;
    };
    Some(
        SemanticError::new(span, format!("'{left_unit}' and '{right_unit}' are different units"))
            .help(format!(
                "a measurement in '{left_unit}' and one in '{right_unit}' are not interchangeable, so neither converts on its own"
            ))
            .help("write both operands in the same unit"),
    )
}

pub(super) fn binary_operator_name(operator: BinaryOp) -> &'static str {
    match operator {
        BinaryOp::Or => "or",
        BinaryOp::And => "and",
        BinaryOp::Equal => "equal",
        BinaryOp::NotEqual => "not_equal",
        BinaryOp::Less => "less",
        BinaryOp::LessEqual => "less_equal",
        BinaryOp::Greater => "greater",
        BinaryOp::GreaterEqual => "greater_equal",
        BinaryOp::Range => "range",
        BinaryOp::Add => "add",
        BinaryOp::Subtract => "subtract",
        BinaryOp::Multiply => "multiply",
        BinaryOp::Divide => "divide",
    }
}
