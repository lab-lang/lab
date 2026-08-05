//! `std.lab.plasmid_actions` durable action contracts.

use crate::checked::OwnershipMode;
use crate::standard_library::catalog::StandardModule;
use crate::standard_library::contract::{ActionContractSpec, ContractType, PhrasePart, ResultSpec};
use crate::type_system::Ty;

pub(in crate::standard_library::lab) fn module() -> StandardModule {
    let copy = OwnershipMode::Copy;
    let borrow = OwnershipMode::Borrow;
    let take = OwnershipMode::Take;
    let operand = |name, r#type, mode| PhrasePart::Operand { name, r#type, mode };
    let result = |name, r#type| ResultSpec { name, r#type };
    let concrete = ContractType::Concrete;
    let named = Ty::named;
    let material = Ty::material;

    let plasmid_actions = vec![
        ActionContractSpec {
            operation: "std.lab.plasmid_actions.capture",
            capability: "plate_imaging",
            phrase: vec![
                PhrasePart::Word("capture"),
                PhrasePart::Word("image"),
                PhrasePart::Word("of"),
                operand("plate", concrete(material(named("Plate"))), borrow),
            ],
            results: vec![result("image", concrete(named("Image")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid_actions.synthesize",
            capability: "dna_synthesis",
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
            operation: "std.lab.plasmid_actions.assemble",
            capability: "dna_assembly",
            phrase: vec![
                PhrasePart::Word("assemble"),
                operand(
                    "fragments",
                    concrete(Ty::List(Box::new(named("Fragment")))),
                    take,
                ),
            ],
            results: vec![result("construct", concrete(material(named("Construct"))))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid_actions.provision",
            capability: "inventory",
            phrase: vec![
                PhrasePart::Word("provision"),
                operand("strain", concrete(named("Strain")), copy),
            ],
            results: vec![result("cells", concrete(material(named("Strain"))))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid_actions.transform",
            capability: "chemical_transformation",
            phrase: vec![
                PhrasePart::Word("transform"),
                operand("construct", concrete(material(named("Construct"))), take),
                PhrasePart::Word("into"),
                operand("cells", concrete(material(named("Strain"))), take),
            ],
            results: vec![result("culture", concrete(material(named("Culture"))))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid_actions.recover",
            capability: "culture_incubation",
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
            operation: "std.lab.plasmid_actions.dilute",
            capability: "liquid_handling",
            phrase: vec![
                PhrasePart::Word("dilute"),
                operand("culture", concrete(material(named("Culture"))), take),
            ],
            results: vec![result("culture", concrete(material(named("Culture"))))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid_actions.plate",
            capability: "antibiotic_selection",
            phrase: vec![
                PhrasePart::Word("plate"),
                operand("culture", concrete(material(named("Culture"))), take),
                PhrasePart::Word("on"),
                operand("antibiotic", concrete(named("Antibiotic")), copy),
            ],
            results: vec![result("plate", concrete(material(named("Plate"))))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid_actions.pick",
            capability: "colony_picking",
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
            results: vec![result(
                "candidates",
                concrete(Ty::List(Box::new(material(named("Clone"))))),
            )],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid_actions.screen",
            capability: "clone_screening",
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
            operation: "std.lab.plasmid_actions.grow",
            capability: "culture_incubation",
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
            operation: "std.lab.plasmid_actions.purify",
            capability: "plasmid_purification",
            phrase: vec![
                PhrasePart::Word("purify"),
                operand("culture", concrete(material(named("Culture"))), take),
            ],
            results: vec![result("plasmid", concrete(material(named("Plasmid"))))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid_actions.split",
            capability: "liquid_handling",
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
            operation: "std.lab.plasmid_actions.sequence",
            capability: "sanger_sequencing",
            phrase: vec![
                PhrasePart::Word("sequence"),
                operand("aliquot", concrete(material(named("Plasmid"))), take),
            ],
            results: vec![result("result", concrete(named("SequenceCheck")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid_actions.quantify",
            capability: "dna_quantification",
            phrase: vec![
                PhrasePart::Word("quantify"),
                operand("material", concrete(material(named("Plasmid"))), borrow),
            ],
            results: vec![result("evidence", concrete(named("Evidence")))],
        },
        ActionContractSpec {
            operation: "std.lab.plasmid_actions.store",
            capability: "cold_storage",
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
            operation: "std.lab.plasmid_actions.dispose",
            capability: "waste_handling",
            phrase: vec![
                PhrasePart::Word("dispose"),
                operand("material", ContractType::AnyMaterial, take),
            ],
            results: Vec::new(),
        },
    ];

    StandardModule::new("std.lab.plasmid_actions").with_actions(plasmid_actions)
}
