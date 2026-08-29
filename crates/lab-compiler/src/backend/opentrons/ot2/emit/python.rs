//! Rendering for standalone typed Opentrons protocol modules.

use crate::backend::opentrons::ot2::plan::{Ot2EmissionError, Ot2ExecutionPlan};

const PLAN_TYPES_SOURCE: &str = include_str!("../python/src/lab_opentrons_ot2/plan_types.py");
const ASSEMBLY_SOURCE: &str = include_str!("../python/src/lab_opentrons_ot2/protocols/assembly.py");
const TRANSFORMATION_SOURCE: &str =
    include_str!("../python/src/lab_opentrons_ot2/protocols/transformation.py");
const PLATING_SOURCE: &str = include_str!("../python/src/lab_opentrons_ot2/protocols/plating.py");

const SOURCE_IMPORT_BLOCK: &str = "from typing import cast\n\nfrom opentrons import protocol_api\n\nfrom lab_opentrons_ot2.plan_types import Ot2ExecutionPlan";
const BUNDLED_IMPORT_BLOCK: &str =
    "from typing import TypedDict, cast\n\nfrom opentrons import protocol_api";
const TYPE_START: &str = "# LAB:BUNDLE_TYPES_START";
const TYPE_END: &str = "# LAB:BUNDLE_TYPES_END";
const API_LEVEL_SENTINEL: &str = "\"2.21\",  # LAB:API_LEVEL";
const PLAN_SENTINEL: &str = "\"{}\"  # LAB:EXECUTION_PLAN";

pub(in crate::backend::opentrons::ot2) fn render_assembly_protocol(
    plan: &Ot2ExecutionPlan,
) -> Result<String, Ot2EmissionError> {
    render_protocol("assembly", ASSEMBLY_SOURCE, plan)
}

pub(in crate::backend::opentrons::ot2) fn render_transformation_protocol(
    plan: &Ot2ExecutionPlan,
) -> Result<String, Ot2EmissionError> {
    render_protocol("transformation", TRANSFORMATION_SOURCE, plan)
}

pub(in crate::backend::opentrons::ot2) fn render_plating_protocol(
    plan: &Ot2ExecutionPlan,
) -> Result<String, Ot2EmissionError> {
    render_protocol("plating", PLATING_SOURCE, plan)
}

fn render_protocol(
    name: &'static str,
    source: &str,
    plan: &Ot2ExecutionPlan,
) -> Result<String, Ot2EmissionError> {
    let type_definitions = bundled_type_definitions()?;
    let bundled_imports = format!("{BUNDLED_IMPORT_BLOCK}\n\n\n{type_definitions}");
    let mut output = replace_once(name, source, SOURCE_IMPORT_BLOCK, &bundled_imports)?;

    let api_level = serde_json::to_string(&plan.api_level)
        .map_err(|error| Ot2EmissionError::Serialization(error.to_string()))?;
    output = replace_once(
        name,
        &output,
        API_LEVEL_SENTINEL,
        &format!("{api_level},  # LAB:API_LEVEL"),
    )?;

    let plan_json = serde_json::to_string(plan)
        .map_err(|error| Ot2EmissionError::Serialization(error.to_string()))?;
    let plan_literal = python_string_expression(&plan_json)?;
    output = replace_once(
        name,
        &output,
        PLAN_SENTINEL,
        &format!("{plan_literal}  # LAB:EXECUTION_PLAN"),
    )?;
    Ok(output)
}

pub(in crate::backend::opentrons::ot2) fn python_string_expression(
    value: &str,
) -> Result<String, Ot2EmissionError> {
    const MAX_LITERAL_WIDTH: usize = 88;

    let mut chunks = Vec::new();
    let mut current = String::new();
    for character in value.chars() {
        let mut candidate = current.clone();
        candidate.push(character);
        let encoded = serde_json::to_string(&candidate)
            .map_err(|error| Ot2EmissionError::Serialization(error.to_string()))?;
        if encoded.len() > MAX_LITERAL_WIDTH && !current.is_empty() {
            chunks.push(current);
            current = character.to_string();
        } else {
            current = candidate;
        }
    }
    chunks.push(current);

    let literals = chunks
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| Ot2EmissionError::Serialization(error.to_string()))?;
    Ok(format!("(\n    {}\n)", literals.join("\n    ")))
}

fn bundled_type_definitions() -> Result<&'static str, Ot2EmissionError> {
    let (_, after_start) =
        PLAN_TYPES_SOURCE
            .split_once(TYPE_START)
            .ok_or_else(|| Ot2EmissionError::Template {
                template: "plan_types",
                message: format!("missing marker {TYPE_START}"),
            })?;
    let (definitions, _) =
        after_start
            .split_once(TYPE_END)
            .ok_or_else(|| Ot2EmissionError::Template {
                template: "plan_types",
                message: format!("missing marker {TYPE_END}"),
            })?;
    Ok(definitions.trim_matches('\n'))
}

fn replace_once(
    template: &'static str,
    source: &str,
    needle: &str,
    replacement: &str,
) -> Result<String, Ot2EmissionError> {
    match source.matches(needle).count() {
        1 => Ok(source.replacen(needle, replacement, 1)),
        count => Err(Ot2EmissionError::Template {
            template,
            message: format!("expected exactly one {needle:?} marker, found {count}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use crate::backend::opentrons::ot2::emit::python::*;
    use crate::test_support::golden_gate_protocol;

    #[test]
    fn emitted_protocol_is_standalone_typed_python() {
        let protocol = golden_gate_protocol();
        let plan =
            crate::backend::opentrons::ot2::plan_build(&protocol, &Default::default()).unwrap();
        let protocol = render_assembly_protocol(&plan).unwrap();

        assert!(!protocol.contains("from lab_opentrons_ot2"));
        assert!(protocol.contains("class Ot2ExecutionPlan(TypedDict):"));
        assert!(protocol.contains("def run(protocol: protocol_api.ProtocolContext) -> None:"));
        // The injected plan names the backend that produced it.
        assert!(protocol.contains(crate::backend::opentrons::ot2::BACKEND));
    }
}
