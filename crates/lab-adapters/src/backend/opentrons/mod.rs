//! Opentrons backends.
//!
//! This module is the vendor family; [`ot2`] and [`flex`] are the machines
//! under it, each the containment boundary for its own deck vocabulary,
//! instrument choices, and emitted format. What the two machines share they
//! share with every liquid handler, so it lives beside the backend contracts
//! rather than here.

pub mod flex;
pub mod ot2;
