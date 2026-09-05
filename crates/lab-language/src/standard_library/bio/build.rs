//! Artifact-realization operations in `std.bio.build`.

use crate::checked::{OwnershipMode, ResultLineage};
use crate::standard_library::catalog::StandardModule;
use crate::standard_library::contract::{ActionContractSpec, ContractType, PhrasePart, ResultSpec};
use crate::type_system::Ty;

pub(in crate::standard_library::bio) fn module() -> StandardModule {
    let named = Ty::named;
    let material = Ty::material;
    let concrete = ContractType::Concrete;
    let action = ActionContractSpec {
        operation: "std.bio.build.realize".to_owned(),
        phrase: vec![
            PhrasePart::word("realize"),
            // Realizing is making the thing a declaration describes, whatever
            // kind of thing that is. The declaration carries the recipe, so a
            // plasmid realizes by assembly and a medium by weighing out, and
            // which is a Method's business rather than this contract's.
            PhrasePart::operand("design", ContractType::AnyValue, OwnershipMode::Copy),
            // A realization with no artifact inputs writes nothing, so leaving
            // the clause out says the same thing as passing an empty list.
            PhrasePart::Optional(vec![
                PhrasePart::word("from"),
                PhrasePart::operand(
                    "dependencies",
                    concrete(Ty::List(Box::new(material(named("Plasmid"))))),
                    OwnershipMode::Take,
                ),
            ]),
        ],
        // Each realization is a separate physical build event, even where two
        // events implement the same design.
        results: vec![ResultSpec {
            name: "product".to_owned(),
            r#type: ContractType::MaterialOf("design".to_owned()),
            lineage: ResultLineage::Begins,
        }],
    };
    StandardModule::new("std.bio.build").with_actions([action])
}
