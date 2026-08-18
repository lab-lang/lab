//! Bundled Lab standard-library catalog.
//!
//! Each standard module owns its exported types, values, pure functions, and
//! durable action contracts. The checker imports those declarations through
//! `StandardLibrary`; it does not assign biological meaning by spelling.

mod bio;
mod catalog;
mod contract;
mod lab;
pub mod manifest;
mod prelude;

pub(crate) use catalog::{
    ConstructorSpec, PureFunctionSpec, StandardLibrary, StandardModule, TypeSpec,
};
pub(crate) use contract::{ActionContractSpec, ContractType, Lineage, PhrasePart};

pub(crate) fn manifest() -> manifest::Library {
    manifest::library()
}

pub(crate) fn render_markdown() -> String {
    StandardLibrary::bundled().render_markdown()
}

/// The interfaces of the standard modules written in Lab.
///
/// A consumer that resolves a type to what it stands for needs these: the
/// grounding of `Plasmid` lives in `std.bio.designs` and the terms it names
/// live in `std.bio.ontology`, so a program that imports them states neither
/// itself.
pub(crate) fn authored_interfaces()
-> std::sync::Arc<std::collections::BTreeMap<&'static str, crate::semantics::ModuleInterface>> {
    catalog::authored_interfaces()
}
