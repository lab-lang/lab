//! Bundled Lab standard-library catalog.
//!
//! Each standard module owns its exported types, values, pure functions, and
//! durable action contracts. The checker imports those declarations through
//! `StandardLibrary`; it does not assign biological meaning by spelling.

mod bio;
mod catalog;
mod contract;
mod lab;
mod prelude;

pub(super) use catalog::{PureFunctionSpec, StandardLibrary, StandardModule};
pub(super) use contract::{ActionContractSpec, ContractType, PhrasePart};
