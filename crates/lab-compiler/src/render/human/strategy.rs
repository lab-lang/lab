use std::fmt::Write;

use crate::{AcceptanceCriterion, OperationKind, ProtocolPlan};

pub(in crate::render::human) fn write_strategy(output: &mut String, plan: &ProtocolPlan) {
    writeln!(output, "Selected strategy").unwrap();
    if let Some(method) = parameter(plan, OperationKind::Assemble, "method") {
        writeln!(output, "  Assembly: {method}").unwrap();
    }
    if let Some(host) = parameter(plan, OperationKind::Transform, "host") {
        writeln!(output, "  Propagation host: {host}").unwrap();
    }
    if let Some(method) = parameter(plan, OperationKind::Screen, "method") {
        writeln!(output, "  Screening method: {method}").unwrap();
    }

    let mut verification = Vec::new();
    for obligation in &plan.acceptance {
        let name = match obligation.criterion {
            AcceptanceCriterion::ExactSequence => "sequence identity",
            AcceptanceCriterion::MinimumConcentration { .. } => "concentration",
            AcceptanceCriterion::MinimumVolume { .. } => "volume",
        };
        if !verification.contains(&name) {
            verification.push(name);
        }
    }
    if !verification.is_empty() {
        writeln!(output, "  Verification: {}", verification.join(", ")).unwrap();
    }
    writeln!(output).unwrap();
}

fn parameter<'a>(plan: &'a ProtocolPlan, operation: OperationKind, name: &str) -> Option<&'a str> {
    plan.steps
        .iter()
        .find(|step| step.operation == operation)
        .and_then(|step| step.parameters.get(name))
        .map(String::as_str)
}
