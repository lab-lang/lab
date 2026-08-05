//! Typed contracts for the built-in laboratory action package.
//!
//! Phrase syntax, operand types, ownership modes, results, and capabilities
//! live together so the semantic checker does not assign meaning by verb.

use super::checked::OwnershipMode;
use super::checker::Ty;

#[derive(Clone)]
pub(super) enum ContractType {
    Concrete(Ty),
    SameAs(&'static str),
    AnyMaterial,
}

#[derive(Clone)]
pub(super) enum PhrasePart {
    Word(&'static str),
    Operand {
        name: &'static str,
        r#type: ContractType,
        mode: OwnershipMode,
    },
    Integer {
        name: &'static str,
        signed: bool,
    },
    Quantity {
        name: &'static str,
        signed: bool,
        units: &'static [&'static str],
    },
}

pub(super) struct ResultSpec {
    pub name: &'static str,
    pub r#type: ContractType,
}

pub(super) struct ActionContractSpec {
    pub operation: &'static str,
    pub capability: &'static str,
    pub phrase: Vec<PhrasePart>,
    pub results: Vec<ResultSpec>,
}

pub(super) fn standard_action_contract(operation: &str) -> Option<ActionContractSpec> {
    let copy = OwnershipMode::Copy;
    let borrow = OwnershipMode::Borrow;
    let take = OwnershipMode::Take;
    let operand = |name, r#type, mode| PhrasePart::Operand { name, r#type, mode };
    let result = |name, r#type| ResultSpec { name, r#type };
    let concrete = ContractType::Concrete;
    let named = Ty::named;
    let material = Ty::material;

    Some(match operation {
        "realize" => ActionContractSpec {
            operation: "std.bio.build.realize",
            capability: "artifact_realization",
            phrase: vec![
                PhrasePart::Word("realize"),
                operand("design", concrete(named("Plasmid")), copy),
                PhrasePart::Word("from"),
                operand(
                    "dependencies",
                    concrete(Ty::List(Box::new(material(named("Plasmid"))))),
                    take,
                ),
            ],
            results: vec![
                result("product", concrete(material(named("Plasmid")))),
                result("construct", concrete(material(named("Construct")))),
            ],
        },
        "capture" => ActionContractSpec {
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
        "synthesize" => ActionContractSpec {
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
        "assemble" => ActionContractSpec {
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
        "provision" => ActionContractSpec {
            operation: "std.lab.plasmid_actions.provision",
            capability: "inventory",
            phrase: vec![
                PhrasePart::Word("provision"),
                operand("strain", concrete(named("Strain")), copy),
            ],
            results: vec![result("cells", concrete(material(named("Strain"))))],
        },
        "transform" => ActionContractSpec {
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
        "recover" => ActionContractSpec {
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
        "dilute" => ActionContractSpec {
            operation: "std.lab.plasmid_actions.dilute",
            capability: "liquid_handling",
            phrase: vec![
                PhrasePart::Word("dilute"),
                operand("culture", concrete(material(named("Culture"))), take),
            ],
            results: vec![result("culture", concrete(material(named("Culture"))))],
        },
        "plate" => ActionContractSpec {
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
        "pick" => ActionContractSpec {
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
        "screen" => ActionContractSpec {
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
        "grow" => ActionContractSpec {
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
        "purify" => ActionContractSpec {
            operation: "std.lab.plasmid_actions.purify",
            capability: "plasmid_purification",
            phrase: vec![
                PhrasePart::Word("purify"),
                operand("culture", concrete(material(named("Culture"))), take),
            ],
            results: vec![result("plasmid", concrete(material(named("Plasmid"))))],
        },
        "split" => ActionContractSpec {
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
        "sequence" => ActionContractSpec {
            operation: "std.lab.plasmid_actions.sequence",
            capability: "sanger_sequencing",
            phrase: vec![
                PhrasePart::Word("sequence"),
                operand("aliquot", concrete(material(named("Plasmid"))), take),
            ],
            results: vec![result("result", concrete(named("SequenceCheck")))],
        },
        "quantify" => ActionContractSpec {
            operation: "std.lab.plasmid_actions.quantify",
            capability: "dna_quantification",
            phrase: vec![
                PhrasePart::Word("quantify"),
                operand("material", concrete(material(named("Plasmid"))), borrow),
            ],
            results: vec![result("evidence", concrete(named("Evidence")))],
        },
        "store" => ActionContractSpec {
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
        "dispose" => ActionContractSpec {
            operation: "std.lab.plasmid_actions.dispose",
            capability: "waste_handling",
            phrase: vec![
                PhrasePart::Word("dispose"),
                operand("material", ContractType::AnyMaterial, take),
            ],
            results: Vec::new(),
        },
        _ => return None,
    })
}
