//! Fixed demonstration exports for `std.bio.parts`.

use crate::standard_library::catalog::{StandardModule, TypeSpec};
use crate::type_system::Ty;

pub(in crate::standard_library::bio) fn module() -> StandardModule {
    let named = Ty::named;
    StandardModule::new("std.bio.parts")
        .documented("Named biological parts and their biological type relationships.")
        .with_type_specs([
            TypeSpec::nominal("Tetracycline").implements(["Signal"]),
            TypeSpec::nominal("GreenFluorescentProtein").implements(["Protein"]),
        ])
        .with_values([
            (
                "pTet",
                Ty::Named("Promoter".into(), vec![named("Tetracycline")]),
            ),
            (
                "sfGFP",
                Ty::Named("CDS".into(), vec![named("GreenFluorescentProtein")]),
            ),
            ("B0034", named("Part")),
            ("B0015", named("Part")),
            ("BsaI", named("RestrictionEnzyme")),
        ])
}
