//! Typed external-identity constructors in `std.bio.inventory`.

use crate::standard_library::catalog::{PureFunctionSpec, StandardModule};
use crate::type_system::Ty;

pub(in crate::standard_library::bio) fn module() -> StandardModule {
    let named = Ty::named;
    StandardModule::new("std.bio.inventory").with_functions([
        PureFunctionSpec::new(
            "part",
            "std.bio.inventory.part",
            vec![Ty::String],
            named("Part"),
        ),
        PureFunctionSpec::new(
            "backbone",
            "std.bio.inventory.backbone",
            vec![Ty::String],
            named("Backbone"),
        ),
        PureFunctionSpec::new(
            "restriction_enzyme",
            "std.bio.inventory.restriction_enzyme",
            vec![Ty::String],
            named("RestrictionEnzyme"),
        ),
        PureFunctionSpec::new(
            "chassis",
            "std.bio.inventory.chassis",
            vec![Ty::String],
            named("Chassis"),
        ),
        PureFunctionSpec::new(
            "antibiotic",
            "std.bio.inventory.antibiotic",
            vec![Ty::String],
            named("Antibiotic"),
        ),
    ])
}
