//! Implicitly imported foundational types, values, and pure operations.

use crate::standard_library::catalog::{
    ConstructorSpec, PureFunctionSpec, StandardModule, TypeSpec,
};
use crate::type_system::Ty;

pub(in crate::standard_library) fn modules() -> Vec<StandardModule> {
    let named = Ty::named;
    let types = [
        TypeSpec::nominal("Accepted").parameters(1),
        TypeSpec::nominal("Antibiotic"),
        TypeSpec::nominal("Backbone"),
        TypeSpec::nominal("Buffer")
            .implements(["Solution"])
            .documented("A salt solution cells are washed and resuspended in."),
        TypeSpec::nominal("CDS").parameters(1),
        TypeSpec::nominal("Chassis").documented("A host organism that carries engineered DNA."),
        TypeSpec::nominal("Circuit").parameters(2),
        TypeSpec::nominal("CloneSet").with_fields([(
            "highest_confidence",
            Ty::material(Ty::InState(
                Box::new(named("Strain")),
                "isolated".to_owned(),
            )),
        )]),
        TypeSpec::nominal("Colonies").with_fields([("count", Ty::Integer)]),
        TypeSpec::nominal("ColonyMap").with_fields([("isolated", named("Colonies"))]),
        TypeSpec::nominal("DNA"),
        TypeSpec::nominal("Duration"),
        TypeSpec::nominal("Evidence").implements(["Evidential"]),
        TypeSpec::law("Evidential")
            .documented("Information that may be offered in support of a claim."),
        TypeSpec::law("Event").documented("An occurrence the durable workflow journal records."),
        TypeSpec::nominal("Fragment"),
        TypeSpec::nominal("Image"),
        TypeSpec::nominal("List").parameters(1),
        TypeSpec::nominal("Material").parameters(1),
        TypeSpec::nominal("Medium")
            .implements(["Solution"])
            .documented("What an organism is grown in or on."),
        TypeSpec::nominal("Part"),
        TypeSpec::nominal("Plasmid")
            .with_fields([
                ("topology", named("Topology")),
                ("length", Ty::Quantity("bp".to_owned())),
                ("sequence", named("DNA")),
                ("concentration", Ty::Quantity("ng/uL".to_owned())),
                ("volume", Ty::Quantity("uL".to_owned())),
                ("design", named("Plasmid")),
            ])
            .documented("A backend-neutral plasmid design."),
        TypeSpec::nominal("Promoter").parameters(1),
        TypeSpec::role("Protein").documented("A gene product a coding sequence expresses."),
        TypeSpec::nominal("Reason"),
        TypeSpec::nominal("Regulation")
            .documented("Which way a promoter answers the signal it responds to."),
        TypeSpec::nominal("Rejected").parameters(1),
        TypeSpec::nominal("RestrictionEnzyme"),
        TypeSpec::nominal("Screening").with_fields([("clones", named("CloneSet"))]),
        TypeSpec::role("Signal").documented("A molecule or condition a circuit responds to."),
        TypeSpec::role("Solution")
            .documented("A poured solution: a buffer or a medium a verb pours the same way."),
        TypeSpec::nominal("Strain")
            .with_fields([
                ("chassis", named("Chassis")),
                ("plasmids", Ty::List(Box::new(named("Plasmid")))),
                ("selection", named("Antibiotic")),
            ])
            .documented("A chassis carrying a defined set of plasmid designs."),
        TypeSpec::nominal("Topology"),
        TypeSpec::nominal("WorkflowContext").with_fields([("elapsed", named("Duration"))]),
    ];
    let values = [
        ("circular", named("Topology")),
        // A promoter that answers its signal by expressing more is induced by
        // it; one that answers by expressing less is repressed. Which way a
        // promoter runs is what separates an inverter from a buffer, so it is
        // stated rather than inferred from the numbers a datasheet happens to
        // carry.
        ("induced", named("Regulation")),
        ("repressed", named("Regulation")),
        ("None", Ty::None),
        ("no_colonies", named("Reason")),
        ("sequence_mismatch", named("Reason")),
        ("inconclusive_sequence", named("Reason")),
        ("acceptance_failed", named("Reason")),
    ];
    let functions = [
        PureFunctionSpec::new("dna", "std.bio.dna", vec![Ty::String], named("DNA"))
            .documented("Construct a DNA value from a nucleotide sequence."),
        PureFunctionSpec::new(
            "detect_colonies",
            "std.lab.imaging.detect_colonies",
            vec![named("Image")],
            named("ColonyMap"),
        ),
        PureFunctionSpec::new(
            "sites",
            "std.bio.sequence.sites",
            vec![named("RestrictionEnzyme")],
            Ty::Integer,
        ),
        // A judgement about a design given evidence, rather than something the
        // design does, so the design is an argument like any other.
        PureFunctionSpec::new(
            "accepts",
            "Plasmid.accepts",
            vec![named("Plasmid"), Ty::List(Box::new(named("Evidence")))],
            Ty::Bool,
        )
        .documented("Whether a design's acceptance criteria are met by this evidence."),
    ];
    let evidence = Ty::List(Box::new(named("Evidence")));
    let constructors = [
        ConstructorSpec::new(
            "Accepted",
            "std.outcome.Accepted",
            [
                ("material", Ty::material(named("Plasmid"))),
                ("evidence", evidence.clone()),
            ],
            Ty::Named("Accepted".to_owned(), vec![named("Plasmid")]),
        )
        .documented("An accepted material paired with its supporting evidence."),
        ConstructorSpec::new(
            "Rejected",
            "std.outcome.Rejected",
            [
                (
                    "material",
                    Ty::Union(vec![Ty::material(named("Plasmid")), Ty::None]),
                ),
                ("evidence", evidence),
                ("reason", named("Reason")),
            ],
            Ty::Named("Rejected".to_owned(), vec![named("Plasmid")]),
        )
        .documented("A rejected material with evidence and a machine-readable reason."),
    ];

    vec![
        StandardModule::prelude("std.prelude")
            .documented("Foundational types and operations available to every Lab module.")
            .with_type_specs(types)
            .with_values(values)
            .with_functions(functions)
            .with_constructors(constructors),
    ]
}
