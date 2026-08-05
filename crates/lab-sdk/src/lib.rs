//! Ergonomic Rust API for Lab.

mod api;
mod error;

pub use api::compile_lab_module;
pub use error::Error;
pub use lab_compiler::{CheckedModule, ModuleError};
