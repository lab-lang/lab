//! Pliron-backed Lab intermediate representations and their transformations.

pub(crate) mod allocation;
pub(crate) mod analysis;
pub(crate) mod dialect;
pub mod pipeline;
pub mod program;
pub mod session;
pub mod stage;

pub(crate) mod planning_problem;
pub(crate) mod source_lowering;
