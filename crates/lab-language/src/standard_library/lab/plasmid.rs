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
    let operand = |name, r#type, mode| PhrasePart::Operand { name, r#type, mode };
    // Most results are the same material further along. The two that are not
    // say so: a transformation establishes an organism, and each picked colony
    // is an independent transformant.
    let result = |name, r#type| ResultSpec {
        name,
        r#type,
        lineage: Lineage::Continues,
    };
    let begins = |name, r#type| ResultSpec {
        name,
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
            operation: "std.lab.plasmid.capture",
            phrase: vec![
                PhrasePart::Word("capture"),
                PhrasePart::Word("image"),
                PhrasePart::Word("of"),
                operand("plate", concrete(plate("inoculated")), borrow),
            ],
            inert: &[],
            results: vec![result("image", concrete(named("Image")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.synthesize",
            phrase: vec![
                PhrasePart::Word("synthesize"),
                operand("design", concrete(named("Plasmid")), copy),
            ],
            inert: &[],
            results: vec![result(
                "fragments",
                concrete(Ty::List(Box::new(named("Fragment")))),
            )],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.assemble",
            phrase: vec![
                PhrasePart::Word("assemble"),
                operand(
                    "fragments",
                    concrete(Ty::List(Box::new(named("Fragment")))),
                    take,
                ),
            ],
            inert: &[],
            results: vec![result("construct", concrete(material(named("Plasmid"))))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.provision",
            phrase: vec![
                PhrasePart::Word("provision"),
                operand("item", ContractType::AnyValue, copy),
            ],
            // Whether this laboratory bought the thing or made it last month is
            // not provision's business: it says what to fetch, and whether one
            // is available is a question for the plan.
            inert: &[],
            results: vec![result("material", ContractType::MaterialOf("item"))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.transform",
            phrase: vec![
                PhrasePart::Word("transform"),
                operand("design", concrete(named("Strain")), copy),
                PhrasePart::Word("from"),
                operand(
                    "plasmids",
                    concrete(Ty::List(Box::new(material(named("Plasmid"))))),
                    take,
                ),
                PhrasePart::Word("into"),
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
            inert: &[],
            results: vec![
                begins("strain", concrete(material(named("Strain")))),
                begins("culture", concrete(strain("transformed"))),
            ],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.recover",
            phrase: vec![
                PhrasePart::Word("recover"),
                operand("culture", concrete(strain("transformed")), take),
                PhrasePart::Word("for"),
                PhrasePart::Quantity {
                    name: "duration",
                    signed: false,
                    units: &["min", "h"],
                },
            ],
            inert: &[],
            results: vec![result("culture", concrete(strain("recovered")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.dilute",
            phrase: vec![
                PhrasePart::Word("dilute"),
                operand("culture", concrete(strain("recovered")), take),
            ],
            inert: &[],
            results: vec![result("culture", concrete(strain("diluted")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.plate",
            phrase: vec![
                PhrasePart::Word("plate"),
                // A culture is plated whether or not it was thinned first.
                // Diluting matters for counting what grows, not for the act of
                // spreading it, so both states are spreadable.
                operand(
                    "culture",
                    concrete(Ty::Union(vec![strain("recovered"), strain("diluted")])),
                    take,
                ),
                PhrasePart::Word("on"),
                // What a culture is spread on is a medium that has been poured,
                // so plating on the wrong one is now something to see rather
                // than a name nobody checked.
                operand("medium", concrete(plate("poured")), take),
            ],
            inert: &["medium"],
            results: vec![result("plate", concrete(plate("inoculated")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.pick",
            phrase: vec![
                PhrasePart::Word("pick"),
                PhrasePart::Integer {
                    name: "count",
                    signed: false,
                },
                PhrasePart::Word("isolated"),
                PhrasePart::Word("colonies"),
                PhrasePart::Word("from"),
                operand("plate", concrete(plate("inoculated")), borrow),
            ],
            inert: &[],
            results: vec![begins(
                "candidates",
                concrete(Ty::List(Box::new(strain("isolated")))),
            )],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.screen",
            phrase: vec![
                PhrasePart::Word("screen"),
                operand(
                    "candidates",
                    concrete(Ty::List(Box::new(strain("isolated")))),
                    take,
                ),
                PhrasePart::Word("against"),
                operand("design", concrete(named("Plasmid")), copy),
            ],
            inert: &[],
            results: vec![result("screening", concrete(named("Screening")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.grow",
            phrase: vec![
                PhrasePart::Word("grow"),
                operand("clone", concrete(strain("isolated")), take),
                PhrasePart::Word("at"),
                PhrasePart::Quantity {
                    name: "temperature",
                    signed: true,
                    units: &["C"],
                },
                PhrasePart::Word("for"),
                PhrasePart::Quantity {
                    name: "duration",
                    signed: false,
                    units: &["h"],
                },
            ],
            inert: &[],
            results: vec![result("culture", concrete(strain("grown")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.purify",
            phrase: vec![
                PhrasePart::Word("purify"),
                operand("culture", concrete(strain("grown")), take),
            ],
            inert: &[],
            results: vec![result("plasmid", concrete(material(named("Plasmid"))))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.split",
            phrase: vec![
                PhrasePart::Word("split"),
                operand("material", concrete(material(named("Plasmid"))), take),
            ],
            inert: &[],
            results: vec![
                result("retained", ContractType::SameAs("material")),
                result("aliquot", ContractType::SameAs("material")),
            ],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.sequence",
            phrase: vec![
                PhrasePart::Word("sequence"),
                operand("aliquot", concrete(material(named("Plasmid"))), take),
            ],
            inert: &[],
            results: vec![result("result", concrete(named("SequenceCheck")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.quantify",
            phrase: vec![
                PhrasePart::Word("quantify"),
                operand("material", concrete(material(named("Plasmid"))), borrow),
            ],
            inert: &[],
            results: vec![result("evidence", concrete(named("Evidence")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.store",
            phrase: vec![
                PhrasePart::Word("store"),
                operand("material", concrete(material(named("Plasmid"))), take),
                PhrasePart::Word("at"),
                PhrasePart::Quantity {
                    name: "temperature",
                    signed: true,
                    units: &["C"],
                },
            ],
            inert: &[],
            results: vec![result("material", ContractType::SameAs("material"))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.dispose",
            phrase: vec![
                PhrasePart::Word("dispose"),
                operand("material", ContractType::AnyMaterial, take),
            ],
            inert: &[],
            results: Vec::new(),
        },
    ];

    StandardModule::new("std.lab.plasmid").with_actions(actions)
}
