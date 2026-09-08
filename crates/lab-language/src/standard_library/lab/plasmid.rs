//! `std.lab.plasmid` durable action contracts.

use crate::checked::{OwnershipMode, ResultLineage};
use crate::standard_library::catalog::{ConstructorSpec, StandardModule, TypeSpec};
use crate::standard_library::contract::{ActionContractSpec, ContractType, PhrasePart, ResultSpec};
use crate::type_system::Ty;

pub(in crate::standard_library::lab) fn module() -> StandardModule {
    let copy = OwnershipMode::Copy;
    let borrow = OwnershipMode::Borrow;
    let take = OwnershipMode::Take;
    let operand = |name: &str, r#type, mode| PhrasePart::operand(name, r#type, mode);
    // Most results are the same material further along. The two that are not
    // say so: a transformation establishes an organism, and each picked colony
    // is an independent transformant.
    let continues = |name: &str, r#type, from: &[&str]| ResultSpec {
        name: name.to_owned(),
        r#type,
        lineage: ResultLineage::Continues {
            from: from.iter().map(|operand| (*operand).to_owned()).collect(),
        },
    };
    let begins = |name: &str, r#type| ResultSpec {
        name: name.to_owned(),
        r#type,
        lineage: ResultLineage::Begins,
    };
    let identified_by = |name: &str, r#type, operands: &[&str]| ResultSpec {
        name: name.to_owned(),
        r#type,
        lineage: ResultLineage::IdentifiedBy {
            operands: operands
                .iter()
                .map(|operand| (*operand).to_owned())
                .collect(),
        },
    };
    let concrete = ContractType::Concrete;
    let named = Ty::named;
    let material = Ty::material;
    let sequence_check_fields = [
        ("material", material(named("Plasmid"))),
        ("evidence", Ty::List(Box::new(named("Evidence")))),
    ];
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
            results: vec![continues("image", concrete(named("Image")), &["plate"])],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.synthesize".to_owned(),
            phrase: vec![
                PhrasePart::word("synthesize"),
                operand("design", concrete(named("Plasmid")), copy),
            ],
            results: vec![identified_by(
                "fragments",
                concrete(Ty::List(Box::new(named("Fragment")))),
                &["design"],
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
            results: vec![begins("construct", concrete(material(named("Plasmid"))))],
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
            results: vec![identified_by(
                "material",
                ContractType::MaterialOf("item".to_owned()),
                &["item"],
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
            results: vec![continues(
                "culture",
                concrete(strain("recovered")),
                &["culture"],
            )],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.dilute".to_owned(),
            phrase: vec![
                PhrasePart::word("dilute"),
                operand("culture", concrete(strain("recovered")), take),
            ],
            results: vec![continues(
                "culture",
                concrete(strain("diluted")),
                &["culture"],
            )],
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
            results: vec![continues(
                "plate",
                concrete(plate("inoculated")),
                &["culture"],
            )],
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
            results: vec![continues(
                "screening",
                concrete(named("Screening")),
                &["candidates"],
            )],
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
            results: vec![continues("culture", concrete(strain("grown")), &["clone"])],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.purify".to_owned(),
            phrase: vec![
                PhrasePart::word("purify"),
                operand("culture", concrete(strain("grown")), take),
            ],
            results: vec![continues(
                "plasmid",
                concrete(material(named("Plasmid"))),
                &["culture"],
            )],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.split".to_owned(),
            phrase: vec![
                PhrasePart::word("split"),
                operand("material", concrete(material(named("Plasmid"))), take),
            ],
            results: vec![
                continues(
                    "retained",
                    ContractType::SameAs("material".to_owned()),
                    &["material"],
                ),
                continues(
                    "aliquot",
                    ContractType::SameAs("material".to_owned()),
                    &["material"],
                ),
            ],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.sequence".to_owned(),
            phrase: vec![
                PhrasePart::word("sequence"),
                operand("aliquot", concrete(material(named("Plasmid"))), take),
            ],
            results: vec![continues(
                "result",
                concrete(named("SequenceCheck")),
                &["aliquot"],
            )],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.quantify".to_owned(),
            phrase: vec![
                PhrasePart::word("quantify"),
                operand("material", concrete(material(named("Plasmid"))), borrow),
            ],
            results: vec![continues(
                "evidence",
                concrete(named("Evidence")),
                &["material"],
            )],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.store".to_owned(),
            phrase: vec![
                PhrasePart::word("store"),
                operand("material", concrete(material(named("Plasmid"))), take),
                PhrasePart::word("at"),
                PhrasePart::quantity("temperature", true, &["C"]),
            ],
            results: vec![continues(
                "material",
                ContractType::SameAs("material".to_owned()),
                &["material"],
            )],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid.dispose".to_owned(),
            phrase: vec![
                PhrasePart::word("dispose"),
                operand("material", ContractType::AnyMaterial, take),
            ],
            results: Vec::new(),
        },
    ];

    let sequence_check = TypeSpec::nominal("SequenceCheck")
        .with_fields(sequence_check_fields.clone())
        .documented("A sequenced plasmid material together with the evidence used to judge it.");
    let sequence_check_case = |name, operation, documentation| {
        ConstructorSpec::new(
            name,
            operation,
            sequence_check_fields.clone(),
            named("SequenceCheck"),
        )
        .documented(documentation)
    };

    StandardModule::new("std.lab.plasmid")
        .with_type_specs([sequence_check])
        .with_constructors([
            sequence_check_case(
                "Exact",
                "std.lab.plasmid.sequence.Exact",
                "A sequence that exactly matches the intended plasmid.",
            ),
            sequence_check_case(
                "Mismatch",
                "std.lab.plasmid.sequence.Mismatch",
                "A sequence that does not match the intended plasmid.",
            ),
            sequence_check_case(
                "Inconclusive",
                "std.lab.plasmid.sequence.Inconclusive",
                "Evidence that is insufficient to judge the plasmid sequence.",
            ),
        ])
        .with_actions(actions)
}
