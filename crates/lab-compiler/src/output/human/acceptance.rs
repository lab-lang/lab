use std::fmt::Write;

use crate::{AcceptanceCriterion, ExecutablePlan};

pub(super) fn write_acceptance(output: &mut String, plan: &ExecutablePlan) {
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
            AcceptanceCriterion::MinimumConcentration { concentration } => {
                writeln!(
                    output,
                    "  [ ] Concentration is at least {} ng/µL",
                    concentration.as_nanograms_per_microliter()
                )
                .unwrap();
                writeln!(output, "      Evidence: concentration measurement").unwrap();
            }
            AcceptanceCriterion::MinimumVolume { volume } => {
                writeln!(
                    output,
                    "  [ ] Retained volume is at least {} µL",
                    volume.as_microliters()
                )
                .unwrap();
                writeln!(output, "      Evidence: volume measurement").unwrap();
            }
        }
    }
    writeln!(output).unwrap();
}
