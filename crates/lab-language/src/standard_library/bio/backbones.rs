//! Fixed demonstration exports for `std.bio.backbones`.

use super::super::catalog::StandardModule;
use crate::type_system::Ty;

pub(super) fn module() -> StandardModule {
    StandardModule::new("std.bio.backbones").with_values([("p15A_kan", Ty::named("Backbone"))])
}
