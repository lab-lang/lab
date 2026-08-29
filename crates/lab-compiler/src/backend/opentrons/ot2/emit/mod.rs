//! Concrete OT-2 artifact emitters.

mod manual;
mod python;

pub(in crate::backend) use crate::backend::opentrons::ot2::emit::manual::{
    bench_blocks, boundary_blocks, render_manual_protocol, run_blocks,
};
pub(in crate::backend::opentrons::ot2) use crate::backend::opentrons::ot2::emit::python::{
    python_string_expression, render_assembly_protocol, render_plating_protocol,
    render_transformation_protocol,
};
