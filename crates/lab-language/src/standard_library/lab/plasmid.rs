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

    let actions = vec![
        ActionContractSpec {
            operation: "std.lab.plasmid.capture",
            phrase: vec![
                PhrasePart::Word("capture"),
                PhrasePart::Word("image"),
                PhrasePart::Word("of"),
                operand("plate", concrete(material(named("Plate"))), borrow),
            ],
            results: vec![result("image", concrete(named("Image")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.synthesize",
            phrase: vec![
                PhrasePart::Word("synthesize"),
                operand("design", concrete(named("Plasmid")), copy),
            ],
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
                operand("cells", concrete(material(named("Chassis"))), take),
            ],
            results: vec![
                begins("strain", concrete(material(named("Strain")))),
                begins("culture", concrete(material(named("Culture")))),
            ],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.recover",
            phrase: vec![
                PhrasePart::Word("recover"),
                operand("culture", concrete(material(named("Culture"))), take),
                PhrasePart::Word("for"),
                PhrasePart::Quantity {
                    name: "duration",
                    signed: false,
                    units: &["min", "h"],
                },
            ],
            results: vec![result("culture", concrete(material(named("Culture"))))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.dilute",
            phrase: vec![
                PhrasePart::Word("dilute"),
                operand("culture", concrete(material(named("Culture"))), take),
            ],
            results: vec![result("culture", concrete(material(named("Culture"))))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.plate",
            phrase: vec![
                PhrasePart::Word("plate"),
                operand("culture", concrete(material(named("Culture"))), take),
                PhrasePart::Word("on"),
                operand("antibiotic", concrete(named("Antibiotic")), copy),
            ],
            results: vec![result("plate", concrete(material(named("Plate"))))],
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
                operand("plate", concrete(material(named("Plate"))), borrow),
            ],
            results: vec![begins(
                "candidates",
                concrete(Ty::List(Box::new(material(named("Clone"))))),
            )],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.screen",
            phrase: vec![
                PhrasePart::Word("screen"),
                operand(
                    "candidates",
                    concrete(Ty::List(Box::new(material(named("Clone"))))),
                    take,
                ),
                PhrasePart::Word("against"),
                operand("design", concrete(named("Plasmid")), copy),
            ],
            results: vec![result("screening", concrete(named("Screening")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.grow",
            phrase: vec![
                PhrasePart::Word("grow"),
                operand("clone", concrete(material(named("Clone"))), take),
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
            results: vec![result("culture", concrete(material(named("Culture"))))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.purify",
            phrase: vec![
                PhrasePart::Word("purify"),
                operand("culture", concrete(material(named("Culture"))), take),
            ],
            results: vec![result("plasmid", concrete(material(named("Plasmid"))))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.split",
            phrase: vec![
                PhrasePart::Word("split"),
                operand("material", concrete(material(named("Plasmid"))), take),
            ],
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
            results: vec![result("result", concrete(named("SequenceCheck")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.quantify",
            phrase: vec![
                PhrasePart::Word("quantify"),
                operand("material", concrete(material(named("Plasmid"))), borrow),
            ],
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
            results: vec![result("material", ContractType::SameAs("material"))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.dispose",
            phrase: vec![
                PhrasePart::Word("dispose"),
                operand("material", ContractType::AnyMaterial, take),
            ],
            results: Vec::new(),
        },
    ];

    StandardModule::new("std.lab.plasmid").with_actions(actions)
}
