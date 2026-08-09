//! Flex projections of a dependency-driven build. The renderers themselves
//! are backend-neutral and live in the shared planning module.

use serde::Serialize;

pub(in crate::backend::opentrons::flex::package) use crate::backend::package::{
    render_full_build_instructions, render_report,
};

use crate::backend::opentrons::flex::package::compile::FlexDependencyBuildError;

pub(in crate::backend::opentrons::flex::package) fn pretty_json(
    value: &impl Serialize,
) -> Result<String, FlexDependencyBuildError> {
    serde_json::to_string_pretty(value)
        .map(|mut output| {
            output.push('\n');
            output
        })
        .map_err(|error| FlexDependencyBuildError::Serialization(error.to_string()))
}
