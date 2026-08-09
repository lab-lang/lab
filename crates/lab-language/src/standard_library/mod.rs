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

pub(crate) use catalog::{
    ConstructorSpec, PureFunctionSpec, StandardLibrary, StandardModule, TypeSpec,
};
pub(crate) use contract::{ActionContractSpec, ContractType, Lineage, PhrasePart};

pub(crate) fn render_markdown() -> String {
    StandardLibrary::bundled().render_markdown()
}
