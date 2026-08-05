//! Modules in the `std.lab` namespace.

mod plasmid_actions;

use crate::standard_library::catalog::StandardModule;

pub(in crate::standard_library) fn modules() -> Vec<StandardModule> {
    vec![plasmid_actions::module()]
}
