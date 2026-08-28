//! Backend-neutral rendering of a dependency-driven build: the dependency
//! report and the stitched human instruction document.

use std::collections::BTreeSet;

use crate::backend::document::{Block, Column, Doc, DocMeta, code, text};
use crate::planning::{ArtifactResolution, DependencyBuildManifest, DependencyInventorySource};

/// One robot run in execution order: run index, planning iteration, artifact
/// label, package directory, and the run's own manual content, spliced into
/// this document one heading level down.
pub(in crate::backend) type InstructionBatch = (usize, usize, String, String, Vec<Block>);

/// Sections that hold for every run in the build, rendered once above the
/// runs rather than repeated under each one.
pub(in crate::backend) fn render_full_build_instructions(
    meta: DocMeta,
    manifest: &DependencyBuildManifest,
    bench: Vec<Block>,
    batches: &[InstructionBatch],
    boundary: Vec<Block>,
) -> Doc {
    let mut doc = Doc::new(meta);
    doc.notice([text(
        "Generated concept protocol. Review and qualify every run for the actual laboratory before execution. Planning success is not physical-build or acceptance evidence.",
    )]);

    let profile_name = doc.meta.adapter_profile.clone();
    doc.heading(1, [text("How this package fits together")]);
    doc.para([
        text("The Lab toolchain compiled this package from the project's "),
        code(".lab"),
        text(" sources against the "),
        code(profile_name),
        text(" allocated facility adapter. Every artifact volume, well address, and deck position in this document was planned at compile time, and the robot files, the machine-readable manifests, and this document are all projections of the same execution plan, so they cannot disagree with one another. Nothing here is meant to be edited by hand: to change what a run does, change the sources, facility inventory, or exact Asset-to-adapter configuration and run "),
        code("lab plan"),
        text(" again."),
    ]);
    doc.bullets([
        vec![
            code("dependency_manifest.json"),
            text(": the machine-readable dependency graph, wave schedule, and blockers."),
        ],
        vec![
            code("dependency_report.pdf"),
            text(": the human dependency summary. Consult it before starting if the planning status below is not complete."),
        ],
        vec![
            code("wave-NNN/"),
            text(
                ": one directory per robot session, holding that wave's robot files and a standalone copy of its run manual.",
            ),
        ],
        vec![
            text("This document: every wave's manual stitched into execution order, with the between-wave handling that no robot performs."),
        ],
    ]);
    doc.para_text(
        "Work through the runs in the order listed below, and treat the boundary between waves as a hard stop: a later wave assumes every artifact from the earlier waves physically exists, has been prepared in the required form, and has passed acceptance.",
    );

    doc.heading(1, [text("Build overview")]);
    doc.bullets([
        vec![
            text("Planning status: "),
            code(format!("{:?}", manifest.status)),
        ],
        vec![text(format!(
            "Root artifacts: {}",
            manifest.roots.join(", ")
        ))],
        vec![text(format!("Robot runs: {}", batches.len()))],
        vec![text(format!(
            "Existing artifacts reused: {}",
            if manifest.existing_artifacts.is_empty() {
                "none".to_owned()
            } else {
                manifest.existing_artifacts.join(", ")
            }
        ))],
    ]);
    doc.para([
        text("Consult "),
        code("dependency_manifest.json"),
        text(" for the machine-readable plan and "),
        code("dependency_report.pdf"),
        text(" for dependency and blocker details. Every artifact in one wave is dependency-independent of the others, so a wave is a single robot run over a single deck. Do not begin a wave until every artifact the previous waves produce has been physically made or retrieved and accepted as a suitable input."),
    ]);
    doc.para_text(
        "This package does not automate DNA recovery or preparation between waves. Before a generated artifact is used downstream, prepare it in the form and concentration required by the later wave and record the corresponding acceptance evidence.",
    );

    doc.heading(1, [text("Execution order")]);
    if batches.is_empty() {
        doc.para_text(
            "No robot run is scheduled. The requested roots are either already available or unresolved; inspect the dependency report before proceeding.",
        );
    } else {
        doc.table(
            [
                Column::right("Run"),
                Column::right("Planning wave"),
                Column::left("Artifacts"),
                Column::left("Package directory"),
            ],
            batches
                .iter()
                .map(|(batch, iteration, artifact, directory, _)| {
                    vec![
                        vec![text(format!("{batch:03}"))],
                        vec![text(iteration.to_string())],
                        vec![code(artifact)],
                        vec![code(format!("{directory}/"))],
                    ]
                }),
        );
    }

    // The bench holds for every run in this build, so its sections are
    // rendered once here rather than repeated under each wave.
    doc.blocks.extend(bench);

    for (batch, iteration, artifact, directory, manual) in batches {
        doc.labeled_heading(1, format!("Run {batch:03}"), [code(artifact)]);
        doc.para([
            text(format!(
                "Planning wave: {iteration}. Robot protocols, the Lab automation manifest, and the standalone run manual are in "
            )),
            code(format!("{directory}/")),
            text("."),
        ]);
        let inputs = manifest
            .nodes
            .iter()
            .filter(|node| artifact.split(", ").any(|name| node.artifact == name))
            .flat_map(|node| node.dependencies.iter())
            .collect::<BTreeSet<_>>();
        if !inputs.is_empty() {
            let mut content = vec![text("Required generated or retrieved artifact inputs: ")];
            for (index, dependency) in inputs.iter().enumerate() {
                if index > 0 {
                    content.push(text(", "));
                }
                content.push(code(dependency.as_str()));
            }
            content.push(text("."));
            doc.para(content);
        }
        let steps = manifest
            .nodes
            .iter()
            .filter(|node| artifact.split(", ").any(|name| node.artifact == name))
            .flat_map(|node| node.steps.iter())
            .collect::<BTreeSet<_>>();
        let mut content = vec![text("Requested abstract steps: ")];
        for (index, step) in steps.iter().enumerate() {
            if index > 0 {
                content.push(text(", "));
            }
            content.push(code(step.as_str()));
        }
        content.push(text("."));
        doc.para(content);
        if manifest
            .edges
            .iter()
            .any(|edge| artifact.split(", ").any(|name| edge.depends_on == name))
        {
            doc.para_text(
                "After completing this run, retain, prepare, and verify the material it produced before treating it as an input to a later run.",
            );
        } else {
            doc.para([
                text("After completing this batch, retain and verify the requested root artifact "),
                code(artifact),
                text(" and record its acceptance evidence."),
            ]);
        }
        doc.extend_nested(manual.iter().cloned(), 1);
    }

    doc.blocks.extend(boundary);
    doc
}

pub(in crate::backend) fn render_report(meta: DocMeta, manifest: &DependencyBuildManifest) -> Doc {
    let mut doc = Doc::new(meta);
    doc.para([text("Status: "), code(format!("{:?}", manifest.status))]);
    doc.para_text(format!("Roots: {}", manifest.roots.join(", ")));
    match &manifest.inventory {
        DependencyInventorySource::SbolInventory {
            source_sha256,
            facility,
        } => {
            doc.para([text("Facility: "), code(facility)]);
            doc.para([text("Inventory source SHA-256: "), code(source_sha256)]);
        }
        DependencyInventorySource::LegacySymbols => {
            doc.para_text("Inventory source: legacy symbolic manifest arrays.");
        }
    }
    doc.table(
        [
            Column::left("Artifact"),
            Column::left("Dependencies"),
            Column::left("Resolution"),
            Column::right("Iteration"),
        ],
        manifest.nodes.iter().map(|node| {
            vec![
                vec![text(node.artifact.as_str())],
                vec![text(node.dependencies.join(", "))],
                vec![text(format!("{:?}", node.resolution))],
                vec![text(
                    node.generated_in_iteration
                        .map_or_else(|| "-".into(), |value| value.to_string()),
                )],
            ]
        }),
    );
    let lot_rows = manifest
        .nodes
        .iter()
        .flat_map(|node| {
            node.existing_material_lot
                .iter()
                .map(|binding| {
                    vec![
                        vec![text(node.artifact.as_str())],
                        vec![text("existing artifact")],
                        vec![code(binding.component.as_str())],
                        vec![code(binding.material_lot.as_str())],
                    ]
                })
                .chain(node.material_lot_bindings.iter().map(|binding| {
                    vec![
                        vec![text(node.artifact.as_str())],
                        vec![code(binding.symbol.as_str())],
                        vec![code(binding.component.as_str())],
                        vec![code(binding.material_lot.as_str())],
                    ]
                }))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    if !lot_rows.is_empty() {
        doc.heading(1, [text("Material lot bindings")]);
        doc.table(
            [
                Column::left("Artifact"),
                Column::left("Use"),
                Column::left("SBOL Component"),
                Column::left("MaterialLot"),
            ],
            lot_rows,
        );
    }
    let blockers = manifest
        .nodes
        .iter()
        .filter(|node| {
            matches!(
                node.resolution,
                ArtifactResolution::Blocked | ArtifactResolution::Cyclic
            )
        })
        .collect::<Vec<_>>();
    if !blockers.is_empty() {
        doc.heading(1, [text("Unresolved inputs")]);
        doc.bullets(blockers.iter().map(|node| {
            vec![
                code(node.artifact.as_str()),
                text(format!(
                    ": dependencies [{}]; materials [{}]; resolution ",
                    node.missing_dependencies.join(", "),
                    node.missing_materials.join(", "),
                )),
                code(format!("{:?}", node.resolution)),
            ]
        }));
    }
    doc.heading(1, [text("Execution boundary")]);
    doc.para_text(
        "Each generated artifact is packaged as an independently reviewable assembly, transformation, and plating batch. A product is added to the planning inventory only after its batch is scheduled; physical execution and acceptance evidence remain laboratory responsibilities.",
    );
    doc
}
