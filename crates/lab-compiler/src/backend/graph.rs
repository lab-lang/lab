//! Projection of Protocol provenance into target-independent build scheduling data.

use std::collections::BTreeSet;

use crate::ProtocolLairProgram;
use crate::planning::{BuildGraph, BuildGraphNode};

use crate::backend::error::PlanningError;
use crate::backend::trace::analyze_protocol;

/// Laboratory steps each artifact kind contributes to a build graph node.
const ASSEMBLY_STEPS: [&str; 1] = ["assemble"];
const STRAIN_STEPS: [&str; 4] = ["transform", "recover", "dilute", "plate"];

/// Reagents every Golden Gate assembly consumes regardless of its design.
const ASSEMBLY_REAGENTS: [&str; 3] = [
    "T4_DNA_ligase",
    "T4_DNA_ligase_buffer",
    "nuclease_free_water",
];

pub(in crate::backend) fn protocol_build_graph(
    protocol: &ProtocolLairProgram,
) -> Result<BuildGraph, PlanningError> {
    let context = protocol.context();
    let traces = analyze_protocol(protocol, None)?;
    let mut nodes = std::collections::BTreeMap::new();

    for trace in &traces.assemblies {
        let dependencies = trace
            .dependencies(context)
            .into_iter()
            .collect::<BTreeSet<_>>();
        let mut required_materials = trace
            .components(context)
            .iter()
            .filter(|component| !dependencies.contains(*component))
            .cloned()
            .collect::<BTreeSet<_>>();
        required_materials.insert(trace.backbone(context));
        required_materials.insert(trace.restriction_enzyme(context));
        required_materials.extend(ASSEMBLY_REAGENTS.into_iter().map(str::to_owned));
        nodes.insert(
            trace.artifact(context),
            BuildGraphNode {
                dependencies,
                steps: ASSEMBLY_STEPS.into_iter().map(str::to_owned).collect(),
                required_materials,
            },
        );
    }

    // What this build assembles. A strain names the DNA that went into it
    // whether that DNA was made here or fetched off the shelf, and only the
    // former is something to wait for.
    let assembled = nodes.keys().cloned().collect::<BTreeSet<_>>();

    for trace in &traces.strains {
        let named = trace.dependencies(context).into_iter().collect::<Vec<_>>();
        let dependencies = named
            .iter()
            .filter(|plasmid| assembled.contains(*plasmid))
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut required_materials = named
            .iter()
            .chain(trace.plasmids(context).iter())
            .filter(|plasmid| !dependencies.contains(*plasmid))
            .cloned()
            .collect::<BTreeSet<_>>();
        required_materials.insert(trace.host(context));
        required_materials.insert(trace.selection(context));
        required_materials.insert("recovery_medium".to_owned());
        nodes.insert(
            trace.artifact(context),
            BuildGraphNode {
                dependencies,
                steps: STRAIN_STEPS.into_iter().map(str::to_owned).collect(),
                required_materials,
            },
        );
    }

    Ok(BuildGraph { nodes })
}
