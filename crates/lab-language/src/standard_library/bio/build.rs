//! Artifact-realization operations in `std.bio.build`.

use crate::checked::OwnershipMode;
use crate::standard_library::capability;
use crate::standard_library::catalog::StandardModule;
use crate::standard_library::contract::{
    ActionContractSpec, ContractType, Lineage, PhrasePart, ResultSpec,
};
use crate::type_system::Ty;

pub(in crate::standard_library::bio) fn module() -> StandardModule {
    let named = Ty::named;
    let material = Ty::material;
    let concrete = ContractType::Concrete;
    let action = ActionContractSpec {
        operation: "std.bio.build.realize",
        capability: capability::ARTIFACT_REALIZATION,
        phrase: vec![
            PhrasePart::Word("realize"),
            PhrasePart::Operand {
                name: "design",
                r#type: concrete(named("Plasmid")),
                mode: OwnershipMode::Copy,
            },
            // A realization with no artifact inputs writes nothing, so leaving
            // the clause out says the same thing as passing an empty list.
            PhrasePart::Optional(vec![
                PhrasePart::Word("from"),
                PhrasePart::Operand {
                    name: "dependencies",
                    r#type: concrete(Ty::List(Box::new(material(named("Plasmid"))))),
                    mode: OwnershipMode::Take,
                },
            ]),
        ],
        // Realizing a design assembles DNA rather than establishing an
        // organism, so the product carries the lineage of what went into it.
        results: vec![ResultSpec {
            name: "product",
            r#type: concrete(material(named("Plasmid"))),
            lineage: Lineage::Continues,
        }],
    };
    StandardModule::new("std.bio.build").with_actions([action])
}
