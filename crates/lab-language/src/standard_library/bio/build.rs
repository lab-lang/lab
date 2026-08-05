//! Artifact-realization operations in `std.bio.build`.

use crate::checked::OwnershipMode;
use crate::standard_library::catalog::StandardModule;
use crate::standard_library::contract::{ActionContractSpec, ContractType, PhrasePart, ResultSpec};
use crate::type_system::Ty;

pub(in crate::standard_library::bio) fn module() -> StandardModule {
    let named = Ty::named;
    let material = Ty::material;
    let concrete = ContractType::Concrete;
    let action = ActionContractSpec {
        operation: "std.bio.build.realize",
        capability: "artifact_realization",
        phrase: vec![
            PhrasePart::Word("realize"),
            PhrasePart::Operand {
                name: "design",
                r#type: concrete(named("Plasmid")),
                mode: OwnershipMode::Copy,
            },
            PhrasePart::Word("from"),
            PhrasePart::Operand {
                name: "dependencies",
                r#type: concrete(Ty::List(Box::new(material(named("Plasmid"))))),
                mode: OwnershipMode::Take,
            },
        ],
        results: vec![
            ResultSpec {
                name: "product",
                r#type: concrete(material(named("Plasmid"))),
            },
            ResultSpec {
                name: "construct",
                r#type: concrete(material(named("Construct"))),
            },
        ],
    };
    StandardModule::new("std.bio.build").with_actions([action])
}
