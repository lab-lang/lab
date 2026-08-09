//! Modules in the `std.bio` namespace.
//!
//! Catalogs are written in Lab under `standard_library/authored/`; what remains
//! here is the half with no source declaration form.

mod build;

use crate::standard_library::catalog::StandardModule;

pub(in crate::standard_library) fn modules() -> Vec<StandardModule> {
    vec![build::module()]
}
