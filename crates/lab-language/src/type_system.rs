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
    /// A measurement of a stated thing, in whatever unit it was written in.
    ///
    /// A field asks for this where it holds measurements an author chose the
    /// units of: a recipe carries grams per litre and millimolar together, and
    /// pinning one unit would refuse the other. Each value still names its own
    /// unit, so nothing converts on its own.
    Measuring(String),
    /// A type argument narrowed to one facet state.
    ///
    /// This only ever appears as an argument, the way `Any` does, so a material
    /// in a state is still `Material<..>` to the passes that detect one by its
    /// outermost name. Ownership and linearity are unaffected by a narrowing.
    InState(Box<Ty>, String),
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
            Self::Measuring(dimension) => write!(formatter, "Quantity<any {dimension}>"),
            Self::InState(subject, state) => write!(formatter, "{subject} is {state}"),
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
        Ty::Measuring(dimension) => CheckedType::Measuring {
            dimension: dimension.clone(),
        },
        Ty::InState(subject, state) => CheckedType::InState {
            subject: Box::new(to_checked_type(subject)),
            state: state.clone(),
        },
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
        CheckedType::Measuring { dimension } => Ty::Measuring(dimension.clone()),
        CheckedType::InState { subject, state } => {
            Ty::InState(Box::new(from_checked_type(subject)), state.clone())
        }
        CheckedType::Integer => Ty::Integer,
        CheckedType::Decimal => Ty::Decimal,
        CheckedType::String => Ty::String,
        CheckedType::Bool => Ty::Bool,
        CheckedType::None => Ty::None,
    }
}

/// Whether a type playing `role` is recorded as doing so.
///
/// A declaration states the roles it plays, so its type arguments do not bear
/// on the question: `Reading<Fluorescence>` is evidence because `Reading` is
/// declared to be, and a signal standing for several signals at once is a
/// signal whichever ones it combines.
fn plays_role(roles: &RoleTable, actual: &Ty, role: &str) -> bool {
    matches!(actual, Ty::Named(name, _)
        if roles.get(name).is_some_and(|played| played.contains(role)))
}

pub(crate) fn compatible(roles: &RoleTable, actual: &Ty, expected: &Ty) -> bool {
    if actual == expected || matches!(actual, Ty::EmptyList) && matches!(expected, Ty::List(_)) {
        return true;
    }
    // A value that is one of several things fits wherever every one of them
    // fits. This is what lets an inferred union settle into an existential, and
    // it is what makes a union a set rather than a sequence: `Plasmid | Part`
    // and `Part | Plasmid` describe the same values, so each satisfies the
    // other.
    if let Ty::Union(alternatives) = actual
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
        // A measurement fits a field asking for what it measures. This runs one
        // way: a field naming a unit still refuses every other unit, so the
        // thousandfold error 0025 refuses is refused here too.
        Ty::Measuring(dimension) => matches!(
            (actual, crate::units::Dimension::named(dimension)),
            (Ty::Quantity(unit), Some(wanted))
                if crate::units::measured(unit).is_some_and(|it| it.dimension == wanted)
        ),
        Ty::Union(alternatives) => alternatives.iter().any(|ty| compatible(roles, actual, ty)),
        Ty::List(expected) => match actual {
            Ty::List(actual) => compatible(roles, actual, expected),
            Ty::EmptyList => true,
            _ => false,
        },
        // Narrowing runs one way. A material known to be in a state may be used
        // where any state of that subject is accepted, because knowing more is
        // never a problem. The reverse is what this exists to refuse: an
        // unnarrowed material cannot stand in where a state is required, which
        // is how transforming into cells nobody made competent is caught.
        Ty::InState(expected_subject, expected_state) => match actual {
            Ty::InState(actual_subject, actual_state) => {
                actual_state == expected_state
                    && compatible(roles, actual_subject, expected_subject)
            }
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
            Ty::InState(actual_subject, _) => compatible(roles, actual_subject, expected),
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

/// Whether two types may be compared or added to one another.
///
/// Two quantities qualify only when they are measured in the same unit. Letting
/// any quantity meet any other made `20 uL + 5 mL` a microlitre volume and
/// `volume > 5 mL` a question worth asking of microlitres, which is the
/// thousandfold error [0025](../../docs/language/decisions/0025-quantity-types.md)
/// refuses when the same two units meet across an assignment.
pub(crate) fn comparable(roles: &RoleTable, left: &Ty, right: &Ty) -> bool {
    compatible(roles, left, right) || compatible(roles, right, left)
}

/// Whether a type counts without being measured in anything, and so may scale a
/// quantity.
pub(crate) fn dimensionless(ty: &Ty) -> bool {
    matches!(ty, Ty::Integer | Ty::Decimal)
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
