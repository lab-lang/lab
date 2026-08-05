//! Bundled standard-library catalog exposed to the generic module checker.
//!
//! Domain vocabulary is registered here rather than encoded in parser or AST
//! cases. A future package loader can provide the same catalog interface from
//! separately compiled Lab packages.

use super::action_contracts::{ActionContractSpec, standard_action_contract};
use super::checker::Ty;

pub(super) struct StandardModule {
    pub path: &'static str,
    pub values: Vec<(&'static str, Ty)>,
}

pub(super) struct PureFunctionSpec {
    pub operation: &'static str,
    pub parameters: Vec<Ty>,
    pub result: Ty,
}

pub(super) fn resolve_module(path: &str) -> Option<StandardModule> {
    let named = Ty::named;
    Some(match path {
        "std.bio.parts" => StandardModule {
            path: "std.bio.parts",
            values: vec![
                (
                    "pTet",
                    Ty::Named("Promoter".into(), vec![named("Tetracycline")]),
                ),
                (
                    "sfGFP",
                    Ty::Named("CDS".into(), vec![named("GreenFluorescentProtein")]),
                ),
                ("B0034", named("Part")),
                ("B0015", named("Part")),
                ("BsaI", named("RestrictionEnzyme")),
            ],
        },
        "std.bio.backbones" => StandardModule {
            path: "std.bio.backbones",
            values: vec![("p15A_kan", named("Backbone"))],
        },
        "std.lab.plasmid_actions" => StandardModule {
            path: "std.lab.plasmid_actions",
            values: Vec::new(),
        },
        "std.bio.build" => StandardModule {
            path: "std.bio.build",
            values: Vec::new(),
        },
        "std.bio.inventory" => StandardModule {
            path: "std.bio.inventory",
            values: Vec::new(),
        },
        _ => return None,
    })
}

pub(super) fn resolve_action(operation: &str) -> Option<ActionContractSpec> {
    standard_action_contract(operation)
}

pub(super) fn resolve_pure_function(name: &str) -> Option<PureFunctionSpec> {
    let named = Ty::named;
    let (operation, result) = match name {
        "part" => ("std.bio.inventory.part", named("Part")),
        "backbone" => ("std.bio.inventory.backbone", named("Backbone")),
        "restriction_enzyme" => (
            "std.bio.inventory.restriction_enzyme",
            named("RestrictionEnzyme"),
        ),
        "strain" => ("std.bio.inventory.strain", named("Strain")),
        "antibiotic" => ("std.bio.inventory.antibiotic", named("Antibiotic")),
        _ => return None,
    };
    Some(PureFunctionSpec {
        operation,
        parameters: vec![Ty::String],
        result,
    })
}
