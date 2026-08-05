//! Modules in the `std.bio` namespace.

mod backbones;
mod build;
mod inventory;
mod parts;

use crate::standard_library::catalog::StandardModule;

pub(in crate::standard_library) fn modules() -> Vec<StandardModule> {
    vec![
        parts::module(),
        backbones::module(),
        inventory::module(),
        build::module(),
    ]
}
