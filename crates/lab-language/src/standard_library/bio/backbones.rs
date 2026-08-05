//! Fixed demonstration exports for `std.bio.backbones`.

use crate::standard_library::catalog::StandardModule;
use crate::type_system::Ty;

pub(in crate::standard_library::bio) fn module() -> StandardModule {
    StandardModule::new("std.bio.backbones").with_values([("p15A_kan", Ty::named("Backbone"))])
}
