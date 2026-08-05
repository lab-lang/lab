//! Concrete OT-2 artifact emitters.

mod manual;
mod python;

pub(super) use manual::render_manual_protocol;
pub(super) use python::{
    render_assembly_protocol, render_plating_protocol, render_transformation_protocol,
};
