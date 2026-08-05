//! Implicitly imported foundational types, values, and pure operations.

use super::catalog::{PureFunctionSpec, StandardModule};
use crate::type_system::Ty;

pub(super) fn modules() -> Vec<StandardModule> {
    let named = Ty::named;
    let types = [
        "Accepted",
        "Antibiotic",
        "Backbone",
        "CDS",
        "Circuit",
        "Clone",
        "CloneSet",
        "Colonies",
        "ColonyMap",
        "Construct",
        "Culture",
        "DNA",
        "Duration",
        "Evidence",
        "Fragment",
        "Image",
        "Material",
        "Part",
        "Plate",
        "Plasmid",
        "Promoter",
        "Protein",
        "Reason",
        "Rejected",
        "RestrictionEnzyme",
        "Screening",
        "Signal",
        "Strain",
        "Topology",
        "WorkflowContext",
    ];
    let values = [
        ("circular", named("Topology")),
        ("None", Ty::None),
        ("no_colonies", named("Reason")),
        ("sequence_mismatch", named("Reason")),
        ("inconclusive_sequence", named("Reason")),
        ("acceptance_failed", named("Reason")),
    ];
    let functions = [
        PureFunctionSpec::new("dna", "std.bio.dna", vec![Ty::String], named("DNA")),
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
        PureFunctionSpec::new(
            "design.accepts",
            "Plasmid.accepts",
            vec![Ty::List(Box::new(named("Evidence")))],
            Ty::Bool,
        ),
    ];

    vec![
        StandardModule::prelude("std.prelude")
            .with_types(types)
            .with_values(values)
            .with_functions(functions),
    ]
}
