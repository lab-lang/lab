//! Modules in the `std.lab` namespace.

mod plasmid_actions;

use super::catalog::StandardModule;

pub(super) fn modules() -> Vec<StandardModule> {
    vec![plasmid_actions::module()]
}
