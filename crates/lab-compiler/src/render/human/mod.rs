use std::fmt::Write;

use crate::ProtocolPlan;

mod acceptance;
mod strategy;
mod workflow;

pub fn render_human(plan: &ProtocolPlan) -> String {
    let mut output = String::new();
    writeln!(output, "LAB COMPILER SCIENTIFIC BUILD PLAN").unwrap();
    writeln!(output, "==================================").unwrap();
    writeln!(output, "Artifact: {}", plan.artifact).unwrap();
    writeln!(output, "Target laboratory: {}", plan.lab_profile).unwrap();
    writeln!(output, "Plan level: abstract biological workflow").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "Objective").unwrap();
    writeln!(
        output,
        "  Produce the plasmid artifact '{}' and collect the evidence required",
        plan.artifact
    )
    .unwrap();
    writeln!(output, "  to accept or reject it.").unwrap();
    writeln!(output).unwrap();

    strategy::write_strategy(&mut output, plan);
    acceptance::write_acceptance(&mut output, plan);
    workflow::write_workflow(&mut output, plan);

    writeln!(output, "Execution boundary").unwrap();
    writeln!(
        output,
        "  This is an abstract scientific build plan, not a bench-ready protocol."
    )
    .unwrap();
    writeln!(
        output,
        "  The current IR does not yet specify reagent quantities, temperatures,"
    )
    .unwrap();
    writeln!(
        output,
        "  durations, containers, instrument settings, inventory lots, or scheduling."
    )
    .unwrap();
    output
}

#[cfg(test)]
mod tests {
    use crate::{AcceptanceCriterion, AcceptanceObligation, OperationKind, PlanStep, PlanValue};

    use crate::render::human::*;

    #[test]
    fn renders_a_scientific_build_plan_without_compiler_identifiers() {
        let plan = ProtocolPlan {
            artifact: "p_test".into(),
            lab_profile: "reference-lab".into(),
            initial_values: vec![PlanValue::design("p_test.design")],
            steps: vec![PlanStep::new(
                "sequence",
                OperationKind::Sequence,
                ["p_test.design"],
                [PlanValue::evidence("p_test.sequence_evidence")],
            )],
            acceptance: vec![AcceptanceObligation {
                criterion: AcceptanceCriterion::ExactSequence,
                evidence_step: "sequence".into(),
                evidence_value: "p_test.sequence_evidence".into(),
            }],
        };

        let rendered = render_human(&plan);
        assert!(rendered.contains("LAB COMPILER SCIENTIFIC BUILD PLAN"));
        assert!(rendered.contains("Artifact: p_test"));
        assert!(rendered.contains("[ ] Exact sequence matches the requested design"));
        assert!(rendered.contains("Phase 4 — Verify the artifact"));
        assert!(rendered.contains("1. Verify sequence identity"));
        assert!(!rendered.contains("p_test.sequence_evidence"));
    }
}
