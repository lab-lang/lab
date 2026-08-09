//! OT-2 projections of a dependency-driven build. The renderers themselves
//! are backend-neutral and live in the shared planning module.

use serde::Serialize;

pub(in crate::backend::opentrons::ot2::package) use crate::backend::package::{
    render_full_build_instructions, render_report,
};

use crate::backend::opentrons::ot2::package::compile::DependencyBuildError;

pub(in crate::backend::opentrons::ot2::package) fn pretty_json(
    value: &impl Serialize,
) -> Result<String, DependencyBuildError> {
    serde_json::to_string_pretty(value)
        .map(|mut output| {
            output.push('\n');
            output
        })
        .map_err(|error| DependencyBuildError::Serialization(error.to_string()))
}
