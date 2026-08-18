//! SBOL as a source and target for Lab designs.
//!
//! A laboratory writes its designs in Lab or in SBOL and its workflows in Lab.
//! That split is not a compromise between two formats; it is where the two
//! languages actually differ. SBOL describes what a thing is and where it came
//! from, and it is unordered, has no binder, no expression language, and no
//! notion of a value being consumed. Lab's workflows are exactly those things.
//! So designs cross the boundary intact and workflows do not cross it at all.
//!
//! This crate owns the crossing. It sits beside `lab-language` rather than
//! inside it because it carries an RDF stack, and `lab-language` is what the
//! editor's WebAssembly build compiles.

#![forbid(unsafe_code)]

mod kind;
mod read;

pub use kind::{KindIndex, LAB_KIND, LAB_NAMESPACE, Resolution};
pub use read::{Read, ReadError, Skipped, read_designs, read_module};
