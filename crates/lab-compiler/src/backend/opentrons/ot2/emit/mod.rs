//! Concrete OT-2 artifact emitters.

mod manual;
mod python;

pub(in crate::backend::opentrons::ot2) use crate::backend::opentrons::ot2::emit::manual::render_manual_protocol;
pub(in crate::backend::opentrons::ot2) use crate::backend::opentrons::ot2::emit::python::{
    render_assembly_protocol, render_plating_protocol, render_transformation_protocol,
};
