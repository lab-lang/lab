use std::fmt::Write;

use crate::{AcceptanceCriterion, ProtocolPlan};

pub(super) fn write_acceptance(output: &mut String, plan: &ProtocolPlan) {
    writeln!(output, "Acceptance requirements").unwrap();
    if plan.acceptance.is_empty() {
        writeln!(
            output,
            "  No acceptance requirements are attached to this plan."
        )
        .unwrap();
    }
    for obligation in &plan.acceptance {
        match &obligation.criterion {
            AcceptanceCriterion::ExactSequence => {
                writeln!(output, "  [ ] Exact sequence matches the requested design").unwrap();
                writeln!(output, "      Evidence: sequence verification").unwrap();
            }
            AcceptanceCriterion::MinimumConcentration {
                nanograms_per_microliter,
            } => {
                writeln!(
                    output,
                    "  [ ] Concentration is at least {} ng/µL",
                    nanograms_per_microliter
                )
                .unwrap();
                writeln!(output, "      Evidence: concentration measurement").unwrap();
            }
            AcceptanceCriterion::MinimumVolume { microliters } => {
                writeln!(
                    output,
                    "  [ ] Retained volume is at least {} µL",
                    microliters
                )
                .unwrap();
                writeln!(output, "      Evidence: volume measurement").unwrap();
            }
        }
    }
    writeln!(output).unwrap();
}
