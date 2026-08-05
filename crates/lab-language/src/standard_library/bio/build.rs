//! Artifact-realization operations in `std.bio.build`.

use super::super::catalog::StandardModule;
use super::super::contract::{ActionContractSpec, ContractType, PhrasePart, ResultSpec};
use crate::checked::OwnershipMode;
use crate::type_system::Ty;

pub(super) fn module() -> StandardModule {
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
