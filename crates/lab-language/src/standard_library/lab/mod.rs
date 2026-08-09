//! Modules in the `std.lab` namespace.

mod plasmid;

use crate::standard_library::catalog::StandardModule;

pub(in crate::standard_library) fn modules() -> Vec<StandardModule> {
    vec![plasmid::module()]
}
