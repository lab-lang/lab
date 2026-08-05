//! Projection of Protocol provenance into target-independent build scheduling data.

use std::collections::BTreeSet;

use crate::ProtocolLairProgram;
use crate::planning::{BuildGraph, BuildGraphNode};

use super::trace::analyze_protocol;
use super::{Ot2PlanningError, SUPPORTED_STEPS};

pub(in crate::backend::opentrons_ot2) fn protocol_build_graph(
    protocol: &ProtocolLairProgram,
) -> Result<BuildGraph, Ot2PlanningError> {
    let context = protocol.context();
    let traces = analyze_protocol(protocol, None)?;
    Ok(BuildGraph {
        nodes: traces
            .into_iter()
            .map(|trace| {
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
                required_materials.insert(trace.host(context));
                required_materials.insert(trace.selection(context));
                required_materials.extend(
                    [
                        "T4_DNA_ligase",
                        "T4_DNA_ligase_buffer",
                        "nuclease_free_water",
                        "recovery_medium",
                    ]
                    .into_iter()
                    .map(str::to_owned),
                );
                (
                    trace.artifact(context),
                    BuildGraphNode {
                        dependencies,
                        steps: SUPPORTED_STEPS.into_iter().map(str::to_owned).collect(),
                        required_materials,
                    },
                )
            })
            .collect(),
    })
}
