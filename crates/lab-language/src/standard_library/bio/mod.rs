//! Modules in the `std.bio` namespace.

mod backbones;
mod build;
mod inventory;
mod parts;

use super::catalog::StandardModule;

pub(super) fn modules() -> Vec<StandardModule> {
    vec![
        parts::module(),
        backbones::module(),
        inventory::module(),
        build::module(),
    ]
}
