//! Concrete Flex artifact emitters.

mod manual;
mod protocols;

pub(in crate::backend::opentrons::flex) use crate::backend::opentrons::flex::emit::manual::render_manual_protocol;
pub(in crate::backend::opentrons::flex) use crate::backend::opentrons::flex::emit::protocols::{
    render_assembly_protocol, render_plating_protocol, render_transformation_protocol,
};
