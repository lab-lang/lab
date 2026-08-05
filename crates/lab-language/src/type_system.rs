//! Internal type representation, compatibility, and generic inference.

use std::collections::HashMap;
use std::fmt;

use crate::checked::CheckedType;
use crate::semantic_error::SemanticError;
use crate::source::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ty {
    Named(String, Vec<Ty>),
    Union(Vec<Ty>),
    List(Box<Ty>),
    Quantity(String),
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

pub(crate) fn compatible(actual: &Ty, expected: &Ty) -> bool {
    if actual == expected || matches!(actual, Ty::EmptyList) && matches!(expected, Ty::List(_)) {
        return true;
    }
    match expected {
        Ty::Union(alternatives) => alternatives.iter().any(|ty| compatible(actual, ty)),
        Ty::List(expected) => match actual {
            Ty::List(actual) => compatible(actual, expected),
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
                        .all(|(actual, expected)| compatible(actual, expected))
            }
            _ => false,
        },
        _ => false,
    }
}

pub(crate) fn common_type(left: Ty, right: Ty) -> Ty {
    if compatible(&right, &left) {
        return left;
    }
    if compatible(&left, &right) {
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

pub(crate) fn comparable(left: &Ty, right: &Ty) -> bool {
    compatible(left, right)
        || compatible(right, left)
        || matches!((left, right), (Ty::Quantity(_), Ty::Quantity(_)))
}

pub(crate) fn satisfies_bound(actual: &Ty, bound: &Ty) -> bool {
    compatible(actual, bound)
        || matches!(
            (actual, bound),
            (
                Ty::Named(actual, arguments),
                Ty::Named(bound, bound_arguments)
            ) if arguments.is_empty()
                && bound_arguments.is_empty()
                && matches!(
                    (actual.as_str(), bound.as_str()),
                    ("Tetracycline", "Signal")
                        | ("GreenFluorescentProtein", "Protein")
                )
        )
}

pub(crate) fn unify(
    template: &Ty,
    actual: &Ty,
    parameters: &[String],
    substitutions: &mut HashMap<String, Ty>,
    span: Span,
) -> Result<(), SemanticError> {
    if let Ty::Named(name, arguments) = template {
        if arguments.is_empty() && parameters.contains(name) {
            if let Some(previous) = substitutions.get(name) {
                if !compatible(actual, previous) {
                    return Err(SemanticError::new(
                        span,
                        format!("type parameter {name} inferred as both {previous} and {actual}"),
                    ));
                }
            } else {
                substitutions.insert(name.clone(), actual.clone());
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
            unify(template, actual, parameters, substitutions, span)?;
        }
        return Ok(());
    }
    if compatible(actual, template) {
        Ok(())
    } else {
        Err(SemanticError::new(
            span,
            format!("expected {template}, found {actual}"),
        ))
    }
}

pub(crate) fn substitute(ty: &Ty, substitutions: &HashMap<String, Ty>) -> Ty {
    match ty {
        Ty::Named(name, arguments) if arguments.is_empty() && substitutions.contains_key(name) => {
            substitutions[name].clone()
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
