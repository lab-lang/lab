use std::fmt::Write;

use crate::{ExecutablePlan, OperationKind, PlanStep};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    PrepareAndAssemble,
    EstablishClone,
    ProduceMaterial,
    VerifyArtifact,
    AcceptArtifact,
}

impl Phase {
    fn for_operation(operation: OperationKind) -> Self {
        match operation {
            OperationKind::Provision | OperationKind::Synthesize | OperationKind::Assemble => {
                Self::PrepareAndAssemble
            }
            OperationKind::Transform
            | OperationKind::Recover
            | OperationKind::Select
            | OperationKind::Screen => Self::EstablishClone,
            OperationKind::Grow | OperationKind::Purify => Self::ProduceMaterial,
            OperationKind::Sample | OperationKind::Sequence | OperationKind::Quantify => {
                Self::VerifyArtifact
            }
            OperationKind::Accept => Self::AcceptArtifact,
        }
    }

    fn heading(self) -> &'static str {
        match self {
            Self::PrepareAndAssemble => "Phase 1 — Prepare and assemble",
            Self::EstablishClone => "Phase 2 — Establish a clone",
            Self::ProduceMaterial => "Phase 3 — Produce plasmid material",
            Self::VerifyArtifact => "Phase 4 — Verify the artifact",
            Self::AcceptArtifact => "Phase 5 — Accept or reject",
        }
    }
}

pub(super) fn write_workflow(output: &mut String, plan: &ExecutablePlan) {
    writeln!(output, "Workflow").unwrap();
    let mut previous_phase = None;
    for (index, step) in plan.steps.iter().enumerate() {
        let phase = Phase::for_operation(step.operation);
        if previous_phase != Some(phase) {
            if previous_phase.is_some() {
                writeln!(output).unwrap();
            }
            writeln!(output, "{}", phase.heading()).unwrap();
            previous_phase = Some(phase);
        }
        writeln!(output, "  {}. {}", index + 1, step_title(step.operation)).unwrap();
        writeln!(output, "     {}", step_description(step, &plan.artifact)).unwrap();
    }
    writeln!(output).unwrap();
}

fn step_title(operation: OperationKind) -> &'static str {
    match operation {
        OperationKind::Provision => "Provision host cells",
        OperationKind::Synthesize => "Synthesize DNA fragments",
        OperationKind::Assemble => "Assemble the construct",
        OperationKind::Transform => "Transform the propagation host",
        OperationKind::Recover => "Recover transformed cells",
        OperationKind::Select => "Select colonies",
        OperationKind::Screen => "Screen candidate colonies",
        OperationKind::Grow => "Expand the selected clone",
        OperationKind::Purify => "Purify plasmid DNA",
        OperationKind::Sample => "Prepare a verification aliquot",
        OperationKind::Sequence => "Verify sequence identity",
        OperationKind::Quantify => "Measure concentration and volume",
        OperationKind::Accept => "Evaluate acceptance",
    }
}

fn step_description(step: &PlanStep, artifact: &str) -> String {
    match step.operation {
        OperationKind::Provision => match parameter(step, "inventory item") {
            Some(item) => format!("Provision {item} from laboratory inventory."),
            None => "Provision the required host material from laboratory inventory.".into(),
        },
        OperationKind::Synthesize => {
            "Synthesize DNA fragments from the requested plasmid design.".into()
        }
        OperationKind::Assemble => match parameter(step, "method") {
            Some(method) => {
                format!(
                    "Assemble the DNA fragments into a circular construct using {method} assembly."
                )
            }
            None => "Assemble the DNA fragments into a circular construct.".into(),
        },
        OperationKind::Transform => match parameter(step, "host") {
            Some(host) => format!("Introduce the assembled construct into {host} cells."),
            None => "Introduce the assembled construct into the selected host cells.".into(),
        },
        OperationKind::Recover => {
            "Recover the transformed culture before applying selection.".into()
        }
        OperationKind::Select => "Select for colonies carrying the construct.".into(),
        OperationKind::Screen => match parameter(step, "method") {
            Some(method) => {
                format!("Screen the colony pool by {method} and retain a candidate clone.")
            }
            None => "Screen the colony pool and retain a candidate clone.".into(),
        },
        OperationKind::Grow => {
            "Expand the selected clone to produce a plasmid-bearing culture.".into()
        }
        OperationKind::Purify => "Purify plasmid DNA from the expanded culture.".into(),
        OperationKind::Sample => {
            "Retain the purified plasmid while separating an aliquot for verification.".into()
        }
        OperationKind::Sequence => {
            "Sequence the assay aliquot to produce identity evidence.".into()
        }
        OperationKind::Quantify => {
            "Measure the concentration and retained volume against the acceptance thresholds."
                .into()
        }
        OperationKind::Accept => {
            format!("Accept '{artifact}' only if every stated acceptance requirement passes.")
        }
    }
}

fn parameter<'a>(step: &'a PlanStep, name: &str) -> Option<&'a str> {
    step.parameters.get(name).map(String::as_str)
}
