//! Renders a facility-planning failure into something a scientist can act on.
//!
//! The solver already records why every candidate was rejected and how two complete plans differ.
//! Those records are the explanation; this module is what puts them in front of the person who has
//! to change something.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use crate::planning::solver::{
    AlternativeMethod, FacilityPlanningError, PlanningAlternative,
    PlanningCandidateRejectionReason, PlanningMaterialRejectionReason, PlanningRejectedOffering,
    RejectedMethodCandidate,
};

/// A short local name for an absolute IRI, for reading rather than for identity.
fn short(iri: &str) -> &str {
    iri.rsplit(['#', '/']).next().unwrap_or(iri)
}

fn describe_offering_rejection(reason: &PlanningCandidateRejectionReason) -> String {
    match reason {
        PlanningCandidateRejectionReason::NoOfferingOfKind { assets_considered } => format!(
            "no Asset in this facility offers that capability ({assets_considered} considered)"
        ),
        PlanningCandidateRejectionReason::Inactive => "the offering is not active".to_owned(),
        PlanningCandidateRejectionReason::InsufficientQualification { required, observed } => {
            format!(
                "qualified only at {}, below the required {}",
                short(observed),
                short(required)
            )
        }
        PlanningCandidateRejectionReason::UnsupportedControlMode { accepted, observed } => format!(
            "offers {} control, but the Method accepts {}",
            short(observed),
            accepted
                .iter()
                .map(|mode| short(mode))
                .collect::<Vec<_>>()
                .join(" or ")
        ),
        PlanningCandidateRejectionReason::MissingParameter { property_kind } => {
            format!("declares no {}", short(property_kind))
        }
        PlanningCandidateRejectionReason::UnitMismatch {
            property_kind,
            required,
            observed,
        } => format!(
            "states {} in {}, but the Method requires {}",
            short(property_kind),
            observed.as_deref().map_or("no unit", short),
            required.as_deref().map_or("no unit", short)
        ),
        PlanningCandidateRejectionReason::ValueMismatch {
            property_kind,
            required,
            observed,
        } => format!(
            "{} is {observed}, which does not satisfy {required}",
            short(property_kind)
        ),
        PlanningCandidateRejectionReason::IncomparableValue { property_kind } => {
            format!("{} cannot be compared numerically", short(property_kind))
        }
        PlanningCandidateRejectionReason::MissingPlanningAdapter => {
            "no configured adapter can plan it".to_owned()
        }
        PlanningCandidateRejectionReason::AtomicBindingConflict { binding_scope } => format!(
            "this task's capabilities must be provided together ({binding_scope:?}), and no single \
             Asset and adapter provides all of them"
        ),
    }
}

fn describe_offering(offering: &PlanningRejectedOffering) -> String {
    let reasons = offering
        .reasons
        .iter()
        .map(describe_offering_rejection)
        .collect::<Vec<_>>()
        .join("; ");
    if offering.offering.is_empty() {
        reasons
    } else {
        format!(
            "{} on {}: {reasons}",
            short(&offering.offering),
            short(&offering.asset)
        )
    }
}

fn describe_candidate(report: &mut String, candidate: &RejectedMethodCandidate) {
    let _ = writeln!(report, "  method `{}` cannot run here", candidate.method);
    for material in &candidate.rejected_materials {
        let reason = match &material.reason {
            PlanningMaterialRejectionReason::UnknownSymbol => {
                "is not a symbol this package declares".to_owned()
            }
            PlanningMaterialRejectionReason::MissingDesignIdentity => {
                "has no SBOL identity to match a MaterialLot against".to_owned()
            }
            PlanningMaterialRejectionReason::NoActiveMaterialLot { component } => {
                format!(
                    "has no active MaterialLot built from `{}`",
                    short(component)
                )
            }
        };
        let _ = writeln!(report, "    material `{}` {reason}", material.symbol);
    }
    for requirement in &candidate.rejected_requirements {
        let _ = writeln!(
            report,
            "    requirement `{}` needs {}",
            requirement.requirement,
            short(requirement.capability_kind.as_str())
        );
        for offering in &requirement.candidates {
            let _ = writeln!(report, "      {}", describe_offering(offering));
        }
    }
}

/// The choices on which two otherwise complete plans disagree.
fn differing_choices(alternatives: &[PlanningAlternative]) -> Vec<String> {
    let [first, second, ..] = alternatives else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for (left, right) in first.methods.iter().zip(&second.methods) {
        if left.method != right.method {
            lines.push(format!(
                "  choice `{}` could use `{}` or `{}`",
                left.choice, left.method, right.method
            ));
            continue;
        }
        lines.extend(differing_bindings(left, right));
    }
    lines
}

fn differing_bindings(left: &AlternativeMethod, right: &AlternativeMethod) -> Vec<String> {
    let mut lines = Vec::new();
    for (one, other) in left.materials.iter().zip(&right.materials) {
        if one.source != other.source {
            lines.push(format!(
                "  material `{}` in choice `{}` could come from either active lot",
                one.symbol, left.choice
            ));
        }
    }
    for (one, other) in left.bindings.iter().zip(&right.bindings) {
        if one.asset != other.asset || one.offering != other.offering {
            lines.push(format!(
                "  requirement `{}` could bind {} or {}",
                one.requirement,
                short(&one.asset),
                short(&other.asset)
            ));
        } else if one.procedure_implementation != other.procedure_implementation {
            lines.push(format!(
                "  requirement `{}` could use either Procedure implementation on {}",
                one.requirement,
                short(&one.asset)
            ));
        }
    }
    lines
}

fn pin_suggestion(alternatives: &[PlanningAlternative]) -> Option<String> {
    let [first, second, ..] = alternatives else {
        return None;
    };
    if let Some((left, _)) = first
        .methods
        .iter()
        .zip(&second.methods)
        .find(|(left, right)| left.method != right.method)
    {
        return Some(format!(
            "\nPin one in lab.toml:\n\n    [[planning.methods]]\n    choice = \"{}\"\n    method = \"{}\"\n",
            left.choice, left.method
        ));
    }
    let (binding, _) = first
        .methods
        .iter()
        .zip(&second.methods)
        .flat_map(|(left, right)| left.bindings.iter().zip(&right.bindings))
        .find(|(left, right)| left.asset != right.asset)?;
    Some(format!(
        "\nName the one to use in lab.toml:\n\n    [[planning.assets]]\n    asset = \"{}\"\n\nThat binds every requirement it can serve. Use `requirement` or `capability-kind` in the same table to narrow it.\n",
        binding.asset
    ))
}

/// Expands a planning failure into a full explanation, or returns `None` when the error's own
/// message is already complete.
pub fn explain_facility_planning_error(error: &FacilityPlanningError) -> Option<String> {
    match error {
        FacilityPlanningError::NoFeasibleMethod { choice, candidates } => {
            let mut report = format!("no method for `{choice}` can run in this facility\n");
            for candidate in candidates {
                describe_candidate(&mut report, candidate);
            }
            Some(report)
        }
        FacilityPlanningError::AmbiguousPlan { alternatives } => {
            let differences = differing_choices(alternatives);
            let mut report = String::from(
                "this facility admits more than one complete plan, and Lab will not choose for you\n",
            );
            if differences.is_empty() {
                report.push_str("  the alternatives differ in a way this report cannot narrow\n");
            } else {
                let unique = differences.into_iter().collect::<BTreeSet<_>>();
                for line in unique {
                    let _ = writeln!(report, "{line}");
                }
            }
            if let Some(suggestion) = pin_suggestion(alternatives) {
                report.push_str(&suggestion);
            }
            Some(report)
        }
        _ => None,
    }
}
