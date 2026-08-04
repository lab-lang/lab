//! Compatibility lowering from the new source AST into the first artifact
//! specification pipeline.
//!
//! This module is deliberately strict: syntax that the source parser accepts
//! must either lower with its meaning preserved or produce `Unsupported`.

use super::ast::{BinaryOp, Expr, Item, Module, PlasmidMember};
use super::error::{ParseError, syntax_span};
use super::{
    AcceptanceCriterion, ArtifactSpec, Concentration, DnaSequence, PlasmidSpec, Topology, Volume,
};

pub(crate) fn lower_artifact(module: Module) -> Result<ArtifactSpec, ParseError> {
    if module.items.len() != 1 {
        let span = module.items.get(1).map_or(module.span, Item::span);
        return Err(ParseError::Unsupported {
            span,
            feature: "artifact lowering currently requires one plasmid declaration and no other module items"
                .to_owned(),
        });
    }
    let Item::Plasmid(plasmid) = &module.items[0] else {
        return Err(ParseError::Unsupported {
            span: module.items[0].span(),
            feature: "only a standalone plasmid declaration can enter the artifact pipeline"
                .to_owned(),
        });
    };

    let mut sequence = None;
    let mut topology = Topology::Circular;
    let mut acceptance = Vec::new();
    for member in &plasmid.members {
        match member {
            PlasmidMember::Binding(binding) if binding.names[0].value == "sequence" => {
                if sequence.is_some() {
                    return Err(syntax_span(binding.span, "duplicate sequence binding"));
                }
                sequence = Some(lower_dna(&binding.value)?);
            }
            PlasmidMember::Binding(binding) => {
                return Err(ParseError::Unsupported {
                    span: binding.span,
                    feature: format!(
                        "plasmid binding '{}' is parsed but not yet lowered",
                        binding.names[0].value
                    ),
                });
            }
            PlasmidMember::Requirement(claim) => {
                topology = lower_topology_requirement(&claim.predicate)?;
            }
            PlasmidMember::Acceptance(claim) => {
                acceptance.push(lower_acceptance(&claim.predicate)?);
            }
            PlasmidMember::Section(section) => {
                return Err(ParseError::Unsupported {
                    span: section.span,
                    feature: format!(
                        "plasmid section '{}' is not yet lowered",
                        section.name.value
                    ),
                });
            }
        }
    }

    let sequence = sequence.ok_or_else(|| {
        syntax_span(
            plasmid.span,
            "a lowered plasmid requires 'sequence = dna(\"...\")'",
        )
    })?;
    let plasmid_spec = PlasmidSpec::new(DnaSequence::new(sequence)?, topology)?;
    ArtifactSpec::plasmid(plasmid.name.value.clone(), plasmid_spec, 1, acceptance)
        .map_err(ParseError::from)
}

fn lower_dna(expression: &Expr) -> Result<String, ParseError> {
    let Expr::Call {
        callee,
        arguments,
        span,
    } = expression
    else {
        return Err(ParseError::Unsupported {
            span: expression.span(),
            feature: "DNA sequences must currently use dna(\"ACGT...\")".to_owned(),
        });
    };
    if !is_path(callee, &["dna"]) || arguments.len() != 1 || arguments[0].name.is_some() {
        return Err(ParseError::Unsupported {
            span: *span,
            feature: "DNA sequences must currently use dna(\"ACGT...\")".to_owned(),
        });
    }
    match &arguments[0].value {
        Expr::String { value, .. } => Ok(value.clone()),
        value => Err(ParseError::Unsupported {
            span: value.span(),
            feature: "dna currently requires a string literal".to_owned(),
        }),
    }
}

fn lower_topology_requirement(expression: &Expr) -> Result<Topology, ParseError> {
    let Expr::Binary {
        op: BinaryOp::Equal,
        left,
        right,
        ..
    } = expression
    else {
        return unsupported_claim(expression, "only 'require topology == circular' is lowered");
    };
    if !is_path(left, &["topology"]) {
        return unsupported_claim(expression, "only 'require topology == circular' is lowered");
    }
    if is_path(right, &["circular"]) {
        Ok(Topology::Circular)
    } else if is_path(right, &["linear"]) {
        Ok(Topology::Linear)
    } else {
        unsupported_claim(expression, "topology must be 'circular' or 'linear'")
    }
}

fn lower_acceptance(expression: &Expr) -> Result<AcceptanceCriterion, ParseError> {
    let Expr::Binary {
        op, left, right, ..
    } = expression
    else {
        return unsupported_claim(expression, "acceptance claims must be comparisons");
    };
    if *op == BinaryOp::Equal
        && is_path(left, &["sequence"])
        && is_path(right, &["design", "sequence"])
    {
        return Ok(AcceptanceCriterion::ExactSequence);
    }
    if *op == BinaryOp::GreaterEqual && is_path(left, &["concentration"]) {
        let value = lower_u32_quantity(right, "ng/uL")?;
        return Ok(AcceptanceCriterion::MinimumConcentration {
            concentration: Concentration::nanograms_per_microliter(value),
        });
    }
    if *op == BinaryOp::GreaterEqual && is_path(left, &["volume"]) {
        let value = lower_u32_quantity(right, "uL")?;
        return Ok(AcceptanceCriterion::MinimumVolume {
            volume: Volume::microliters(value),
        });
    }
    unsupported_claim(
        expression,
        "this acceptance predicate is parsed but not yet lowered",
    )
}

fn lower_u32_quantity(expression: &Expr, expected_unit: &str) -> Result<u32, ParseError> {
    let Expr::Quantity {
        magnitude, unit, ..
    } = expression
    else {
        return unsupported_claim(
            expression,
            format!("expected an integer quantity in {expected_unit}"),
        );
    };
    if unit != expected_unit {
        return unsupported_claim(
            expression,
            format!("expected unit {expected_unit}, found {unit}"),
        );
    }
    let Expr::Integer { value, span } = magnitude.as_ref() else {
        return unsupported_claim(
            expression,
            "quantity magnitude must currently be an integer",
        );
    };
    u32::try_from(*value).map_err(|_| syntax_span(*span, "quantity exceeds u32"))
}

fn is_path(expression: &Expr, expected: &[&str]) -> bool {
    let Expr::Path(path) = expression else {
        return false;
    };
    path.segments
        .iter()
        .map(|segment| segment.value.as_str())
        .eq(expected.iter().copied())
}

fn unsupported_claim<T>(expression: &Expr, feature: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError::Unsupported {
        span: expression.span(),
        feature: feature.into(),
    })
}
