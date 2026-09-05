//! `std.lab.plasmid` durable action contracts.

use crate::checked::OwnershipMode;
use crate::standard_library::catalog::StandardModule;
use crate::standard_library::contract::{
    ActionContractSpec, ContractType, Lineage, PhrasePart, ResultSpec,
};
use crate::type_system::Ty;

pub(in crate::standard_library::lab) fn module() -> StandardModule {
    let copy = OwnershipMode::Copy;
    let borrow = OwnershipMode::Borrow;
    let take = OwnershipMode::Take;
    let operand = |name: &str, r#type, mode| PhrasePart::operand(name, r#type, mode);
    // Most results are the same material further along. The two that are not
    // say so: a transformation establishes an organism, and each picked colony
    // is an independent transformant.
    let result = |name: &str, r#type| ResultSpec {
        name: name.to_owned(),
        r#type,
        lineage: Lineage::Continues,
    };
    let begins = |name: &str, r#type| ResultSpec {
        name: name.to_owned(),
        r#type,
        lineage: Lineage::Begins,
    };
    let concrete = ContractType::Concrete;
    let named = Ty::named;
    let material = Ty::material;
    // A culture and a picked colony are one organism at different points in
    // being grown, and a plate is a medium that has been poured. Each was a
    // fieldless type of its own, which is why none could name what it was made
    // of. Naming the state instead keeps the design underneath readable.
    let in_state = |subject: Ty, state: &str| Ty::InState(Box::new(subject), state.to_owned());
    let strain = |state: &str| material(in_state(named("Strain"), state));
    let plate = |state: &str| material(in_state(named("Medium"), state));

    let actions = vec![
        ActionContractSpec {
            operation: "std.lab.plasmid.capture".to_owned(),
            phrase: vec![
                PhrasePart::word("capture"),
                PhrasePart::word("image"),
                PhrasePart::word("of"),
                operand("plate", concrete(plate("inoculated")), borrow),
            ],
            inert: Vec::new(),
            results: vec![result("image", concrete(named("Image")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.synthesize".to_owned(),
            phrase: vec![
                PhrasePart::word("synthesize"),
                operand("design", concrete(named("Plasmid")), copy),
            ],
            inert: Vec::new(),
            results: vec![result(
                "fragments",
                concrete(Ty::List(Box::new(named("Fragment")))),
            )],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.assemble".to_owned(),
            phrase: vec![
                PhrasePart::word("assemble"),
                operand(
                    "fragments",
                    concrete(Ty::List(Box::new(named("Fragment")))),
                    take,
                ),
            ],
            inert: Vec::new(),
            results: vec![result("construct", concrete(material(named("Plasmid"))))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.provision".to_owned(),
            phrase: vec![
                PhrasePart::word("provision"),
                operand("item", ContractType::AnyValue, copy),
            ],
            // Whether this laboratory bought the thing or made it last month is
            // not provision's business: it says what to fetch, and whether one
            // is available is a question for the plan.
            inert: Vec::new(),
            results: vec![result(
                "material",
                ContractType::MaterialOf("item".to_owned()),
            )],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.transform".to_owned(),
            phrase: vec![
                PhrasePart::word("transform"),
                operand("design", concrete(named("Strain")), copy),
                PhrasePart::word("from"),
                operand(
                    "plasmids",
                    concrete(Ty::List(Box::new(material(named("Plasmid"))))),
                    take,
                ),
                PhrasePart::word("into"),
                // Cells that were never made competent take up nothing, so the
                // state is required rather than assumed. A chassis fetched off
                // the shelf carries it because its declaration states it.
                operand(
                    "cells",
                    concrete(material(Ty::InState(
                        Box::new(named("Chassis")),
                        "competent".to_owned(),
                    ))),
                    take,
                ),
            ],
            inert: Vec::new(),
            results: vec![
                begins("strain", concrete(material(named("Strain")))),
                begins("culture", concrete(strain("transformed"))),
            ],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.recover".to_owned(),
            phrase: vec![
                PhrasePart::word("recover"),
                operand("culture", concrete(strain("transformed")), take),
                PhrasePart::word("for"),
                PhrasePart::quantity("duration", false, &["min", "h"]),
            ],
            inert: Vec::new(),
            results: vec![result("culture", concrete(strain("recovered")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.dilute".to_owned(),
            phrase: vec![
                PhrasePart::word("dilute"),
                operand("culture", concrete(strain("recovered")), take),
            ],
            inert: Vec::new(),
            results: vec![result("culture", concrete(strain("diluted")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.plate".to_owned(),
            phrase: vec![
                PhrasePart::word("plate"),
                // A culture is plated whether or not it was thinned first.
                // Diluting matters for counting what grows, not for the act of
                // spreading it, so both states are spreadable.
                operand(
                    "culture",
                    concrete(Ty::Union(vec![strain("recovered"), strain("diluted")])),
                    take,
                ),
                PhrasePart::word("on"),
                // What a culture is spread on is a medium that has been poured,
                // so plating on the wrong one is now something to see rather
                // than a name nobody checked.
                operand("medium", concrete(plate("poured")), take),
            ],
            inert: vec!["medium".to_owned()],
            results: vec![result("plate", concrete(plate("inoculated")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.pick".to_owned(),
            phrase: vec![
                PhrasePart::word("pick"),
                PhrasePart::integer("count", false),
                PhrasePart::word("isolated"),
                PhrasePart::word("colonies"),
                PhrasePart::word("from"),
                operand("plate", concrete(plate("inoculated")), borrow),
            ],
            inert: Vec::new(),
            results: vec![begins(
                "candidates",
                concrete(Ty::List(Box::new(strain("isolated")))),
            )],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.screen".to_owned(),
            phrase: vec![
                PhrasePart::word("screen"),
                operand(
                    "candidates",
                    concrete(Ty::List(Box::new(strain("isolated")))),
                    take,
                ),
                PhrasePart::word("against"),
                operand("design", concrete(named("Plasmid")), copy),
            ],
            inert: Vec::new(),
            results: vec![result("screening", concrete(named("Screening")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.culture".to_owned(),
            phrase: vec![
                PhrasePart::word("culture"),
                operand("clone", concrete(strain("isolated")), take),
                PhrasePart::word("at"),
                PhrasePart::quantity("temperature", true, &["C"]),
                PhrasePart::word("for"),
                PhrasePart::quantity("duration", false, &["h"]),
            ],
            inert: Vec::new(),
            results: vec![result("culture", concrete(strain("grown")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.purify".to_owned(),
            phrase: vec![
                PhrasePart::word("purify"),
                operand("culture", concrete(strain("grown")), take),
            ],
            inert: Vec::new(),
            results: vec![result("plasmid", concrete(material(named("Plasmid"))))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.split".to_owned(),
            phrase: vec![
                PhrasePart::word("split"),
                operand("material", concrete(material(named("Plasmid"))), take),
            ],
            inert: Vec::new(),
            results: vec![
                result("retained", ContractType::SameAs("material".to_owned())),
                result("aliquot", ContractType::SameAs("material".to_owned())),
            ],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.sequence".to_owned(),
            phrase: vec![
                PhrasePart::word("sequence"),
                operand("aliquot", concrete(material(named("Plasmid"))), take),
            ],
            inert: Vec::new(),
            results: vec![result("result", concrete(named("SequenceCheck")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.quantify".to_owned(),
            phrase: vec![
                PhrasePart::word("quantify"),
                operand("material", concrete(material(named("Plasmid"))), borrow),
            ],
            inert: Vec::new(),
            results: vec![result("evidence", concrete(named("Evidence")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.store".to_owned(),
            phrase: vec![
                PhrasePart::word("store"),
                operand("material", concrete(material(named("Plasmid"))), take),
                PhrasePart::word("at"),
                PhrasePart::quantity("temperature", true, &["C"]),
            ],
            inert: Vec::new(),
            results: vec![result(
                "material",
                ContractType::SameAs("material".to_owned()),
            )],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.dispose".to_owned(),
            phrase: vec![
                PhrasePart::word("dispose"),
                operand("material", ContractType::AnyMaterial, take),
            ],
            inert: Vec::new(),
            results: Vec::new(),
        },
    ];

    StandardModule::new("std.lab.plasmid").with_actions(actions)
}
