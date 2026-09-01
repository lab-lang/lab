//! Exact facility decisions applied to verifier-valid LAIR.

mod application;
pub(crate) mod ir;

pub use application::AllocationApplicationError;
pub(crate) use application::apply_facility_solution;
