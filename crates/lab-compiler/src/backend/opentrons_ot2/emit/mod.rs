//! Concrete OT-2 artifact emitters.

mod manual;
mod python;

pub(in crate::backend::opentrons_ot2) use manual::render_manual_protocol;
pub(in crate::backend::opentrons_ot2) use python::{
    render_assembly_protocol, render_plating_protocol, render_transformation_protocol,
};
