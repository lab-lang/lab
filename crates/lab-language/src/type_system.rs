//! Internal type representation, compatibility, and generic inference.

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use crate::checked::CheckedType;
use crate::semantic_error::SemanticError;
use crate::source::Span;

/// The roles each type plays. Deciding whether one type fits another needs it,
/// because a forgotten type argument is satisfied by playing a role rather than
/// by being a particular type.
pub(crate) type RoleTable = HashMap<String, BTreeSet<String>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ty {
    Named(String, Vec<Ty>),
    Union(Vec<Ty>),
    List(Box<Ty>),
    Quantity(String),
    /// A type argument whose identity has been deliberately discarded,
    /// constrained to a role. `Circuit<any Signal, Fluorescence>` is a circuit
    /// driven by some signal nobody may name again.
    Any(String),
    Integer,
    Decimal,
    String,
    Bool,
    None,
    EmptyList,
}

impl Ty {
    pub(crate) fn named(name: impl Into<String>) -> Self {
        Self::Named(name.into(), Vec::new())
    }

    pub(crate) fn material(inner: Ty) -> Self {
        Self::Named("Material".to_owned(), vec![inner])
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named(name, arguments) if arguments.is_empty() => formatter.write_str(name),
            Self::Named(name, arguments) => {
                write!(formatter, "{name}<")?;
                for (index, argument) in arguments.iter().enumerate() {
                    if index != 0 {
                        formatter.write_str(", ")?;
                    }
                    write!(formatter, "{argument}")?;
                }
                formatter.write_str(">")
            }
            Self::Union(alternatives) => {
                for (index, alternative) in alternatives.iter().enumerate() {
                    if index != 0 {
                        formatter.write_str(" | ")?;
                    }
                    write!(formatter, "{alternative}")?;
                }
                Ok(())
            }
            Self::List(element) => write!(formatter, "List<{element}>"),
            Self::Quantity(unit) => write!(formatter, "Quantity<{unit}>"),
            Self::Any(role) => write!(formatter, "any {role}"),
            Self::Integer => formatter.write_str("Integer"),
            Self::Decimal => formatter.write_str("Decimal"),
            Self::String => formatter.write_str("String"),
            Self::Bool => formatter.write_str("Bool"),
            Self::None => formatter.write_str("None"),
            Self::EmptyList => formatter.write_str("List<_>"),
        }
    }
}

pub(crate) fn to_checked_type(ty: &Ty) -> CheckedType {
    match ty {
        Ty::Named(name, arguments) => CheckedType::Named {
            name: name.clone(),
            arguments: arguments.iter().map(to_checked_type).collect(),
        },
        Ty::Union(alternatives) => CheckedType::Union {
            alternatives: alternatives.iter().map(to_checked_type).collect(),
        },
        Ty::List(element) => CheckedType::List {
            element: Box::new(to_checked_type(element)),
        },
        Ty::Quantity(unit) => CheckedType::Quantity { unit: unit.clone() },
        Ty::Integer => CheckedType::Integer,
        Ty::Decimal => CheckedType::Decimal,
        Ty::String => CheckedType::String,
        Ty::Any(role) => CheckedType::Any { role: role.clone() },
        Ty::Bool => CheckedType::Bool,
        Ty::None => CheckedType::None,
        Ty::EmptyList => CheckedType::List {
            element: Box::new(CheckedType::Named {
                name: "_".to_owned(),
                arguments: Vec::new(),
            }),
        },
    }
}

pub(crate) fn from_checked_type(ty: &CheckedType) -> Ty {
    match ty {
        CheckedType::Named { name, arguments } => Ty::Named(
            name.clone(),
            arguments.iter().map(from_checked_type).collect(),
        ),
        CheckedType::Union { alternatives } => {
            Ty::Union(alternatives.iter().map(from_checked_type).collect())
        }
        CheckedType::List { element } => Ty::List(Box::new(from_checked_type(element))),
        CheckedType::Quantity { unit } => Ty::Quantity(unit.clone()),
        CheckedType::Any { role } => Ty::Any(role.clone()),
        CheckedType::Integer => Ty::Integer,
        CheckedType::Decimal => Ty::Decimal,
        CheckedType::String => Ty::String,
        CheckedType::Bool => Ty::Bool,
        CheckedType::None => Ty::None,
    }
}

/// Whether a type playing `role` is recorded as doing so.
fn plays_role(roles: &RoleTable, actual: &Ty, role: &str) -> bool {
    matches!(actual, Ty::Named(name, arguments)
        if arguments.is_empty()
            && roles.get(name).is_some_and(|played| played.contains(role)))
}

pub(crate) fn compatible(roles: &RoleTable, actual: &Ty, expected: &Ty) -> bool {
    if actual == expected || matches!(actual, Ty::EmptyList) && matches!(expected, Ty::List(_)) {
        return true;
    }
    // A value that is one of several things fits wherever every one of them
    // fits. This is what lets an inferred union settle into an existential.
    if let Ty::Union(alternatives) = actual
        && !matches!(expected, Ty::Union(_))
        && alternatives
            .iter()
            .all(|alternative| compatible(roles, alternative, expected))
    {
        return true;
    }
    match expected {
        // Packing: a concrete type may be forgotten into a role it plays.
        // Because arguments compare recursively, this also lets
        // `Circuit<Tetracycline, R>` become `Circuit<any Signal, R>` without a
        // separate rule, and it never runs in the other direction.
        Ty::Any(role) => plays_role(roles, actual, role),
        Ty::Union(alternatives) => alternatives.iter().any(|ty| compatible(roles, actual, ty)),
        Ty::List(expected) => match actual {
            Ty::List(actual) => compatible(roles, actual, expected),
            Ty::EmptyList => true,
            _ => false,
        },
        Ty::Named(expected_name, expected_args) => match actual {
            Ty::Named(actual_name, actual_args) => {
                actual_name == expected_name
                    && actual_args.len() == expected_args.len()
                    && actual_args
                        .iter()
                        .zip(expected_args)
                        .all(|(actual, expected)| compatible(roles, actual, expected))
            }
            _ => false,
        },
        _ => false,
    }
}

/// The type a collection of both `left` and `right` has.
///
/// This never produces an existential. Forgetting which type a value had is a
/// deliberate act the author writes down, so inference widens to a union — which
/// keeps the alternatives — and only an annotation turns that into `any`.
pub(crate) fn common_type(roles: &RoleTable, left: Ty, right: Ty) -> Ty {
    if compatible(roles, &right, &left) {
        return left;
    }
    if compatible(roles, &left, &right) {
        return right;
    }
    let mut alternatives = Vec::new();
    for ty in [left, right] {
        let candidates = match ty {
            Ty::Union(alternatives) => alternatives,
            other => vec![other],
        };
        for candidate in candidates {
            if !alternatives.contains(&candidate) {
                alternatives.push(candidate);
            }
        }
    }
    Ty::Union(alternatives)
}

pub(crate) fn comparable(roles: &RoleTable, left: &Ty, right: &Ty) -> bool {
    compatible(roles, left, right)
        || compatible(roles, right, left)
        || matches!((left, right), (Ty::Quantity(_), Ty::Quantity(_)))
}

/// What each type parameter was inferred as, and the operand that fixed it.
pub(crate) type Substitutions = HashMap<String, (Ty, Span)>;

pub(crate) fn unify(
    roles: &RoleTable,
    template: &Ty,
    actual: &Ty,
    parameters: &[String],
    substitutions: &mut Substitutions,
    span: Span,
) -> Result<(), SemanticError> {
    if let Ty::Named(name, arguments) = template {
        if arguments.is_empty() && parameters.contains(name) {
            // Binding a parameter to a forgotten type would let every other
            // occurrence of that parameter accept anything playing the role,
            // which is the mistake naming a parameter exists to catch.
            if let Ty::Any(role) = actual {
                return Err(SemanticError::new(
                    span,
                    format!("'{name}' cannot be inferred from a forgotten type"),
                )
                .help(format!(
                    "'any {role}' means some {role}, deliberately not recorded"
                ))
                .help(format!(
                    "there is nothing here for the other uses of '{name}' to be matched against"
                )));
            }
            if let Some((previous, previous_span)) = substitutions.get(name) {
                if !compatible(roles, actual, previous) {
                    // Both operands are named, because neither is wrong on its
                    // own — it is the disagreement that is the error.
                    return Err(SemanticError::new(
                        span,
                        format!("'{name}' cannot be both {previous} and {actual}"),
                    )
                    .related(*previous_span, format!("this fixes {name} = {previous}"))
                    .related(span, format!("this requires {name} = {actual}")));
                }
            } else {
                substitutions.insert(name.clone(), (actual.clone(), span));
            }
            return Ok(());
        }
        let Ty::Named(actual_name, actual_arguments) = actual else {
            return Err(SemanticError::new(
                span,
                format!("expected {template}, found {actual}"),
            ));
        };
        if name != actual_name || arguments.len() != actual_arguments.len() {
            return Err(SemanticError::new(
                span,
                format!("expected {template}, found {actual}"),
            ));
        }
        for (template, actual) in arguments.iter().zip(actual_arguments) {
            unify(roles, template, actual, parameters, substitutions, span)?;
        }
        return Ok(());
    }
    // A list is a type argument like any other, so a parameter introduced
    // inside one is inferred from the element rather than left unsolved.
    if let Ty::List(template) = template {
        return match actual {
            Ty::List(actual) => unify(roles, template, actual, parameters, substitutions, span),
            // An empty list determines nothing about its element.
            Ty::EmptyList => Ok(()),
            _ => Err(SemanticError::new(
                span,
                format!("expected List<{template}>, found {actual}"),
            )),
        };
    }
    if compatible(roles, actual, template) {
        Ok(())
    } else {
        Err(SemanticError::new(
            span,
            format!("expected {template}, found {actual}"),
        ))
    }
}

pub(crate) fn substitute(ty: &Ty, substitutions: &Substitutions) -> Ty {
    match ty {
        Ty::Named(name, arguments) if arguments.is_empty() && substitutions.contains_key(name) => {
            substitutions[name].0.clone()
        }
        Ty::Named(name, arguments) => Ty::Named(
            name.clone(),
            arguments
                .iter()
                .map(|argument| substitute(argument, substitutions))
                .collect(),
        ),
        Ty::Union(alternatives) => Ty::Union(
            alternatives
                .iter()
                .map(|alternative| substitute(alternative, substitutions))
                .collect(),
        ),
        Ty::List(element) => Ty::List(Box::new(substitute(element, substitutions))),
        other => other.clone(),
    }
}
