//! Backend-neutral rendering of a dependency-driven build: the dependency
//! report and the stitched human instruction document.

use std::collections::BTreeSet;
use std::fmt::Write;

use crate::planning::{ArtifactResolution, DependencyBuildManifest};

pub(in crate::backend) fn render_full_build_instructions(
    manifest: &DependencyBuildManifest,
    batches: &[(usize, usize, String, String, String)],
) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "# Lab dependency-driven build — human instructions\n"
    )
    .unwrap();
    writeln!(output, "> Generated concept protocol. Review and qualify every run for the actual laboratory before execution. Planning success is not physical-build or acceptance evidence.\n").unwrap();
    writeln!(output, "## Build overview\n").unwrap();
    writeln!(output, "- Planning status: `{:?}`", manifest.status).unwrap();
    writeln!(output, "- Root artifacts: {}", manifest.roots.join(", ")).unwrap();
    writeln!(output, "- Robot runs: {}", batches.len()).unwrap();
    writeln!(
        output,
        "- Existing artifacts reused: {}\n",
        if manifest.existing_artifacts.is_empty() {
            "none".to_owned()
        } else {
            manifest.existing_artifacts.join(", ")
        }
    )
    .unwrap();
    writeln!(output, "Consult `dependency_manifest.json` for the machine-readable plan and `dependency_report.md` for dependency and blocker details. Every artifact in one wave is dependency-independent of the others, so a wave is a single robot run over a single deck. Do not begin a wave until every artifact the previous waves produce has been physically made or retrieved and accepted as a suitable input.\n").unwrap();
    writeln!(output, "This package does not automate DNA recovery or preparation between waves. Before a generated artifact is used downstream, prepare it in the form and concentration required by the later wave and record the corresponding acceptance evidence.\n").unwrap();

    writeln!(output, "## Execution order\n").unwrap();
    if batches.is_empty() {
        writeln!(output, "No robot run is scheduled. The requested roots are either already available or unresolved; inspect the dependency report before proceeding.\n").unwrap();
    } else {
        writeln!(
            output,
            "| Run | Planning wave | Artifacts | Package directory |"
        )
        .unwrap();
        writeln!(output, "| ---: | ---: | --- | --- |").unwrap();
        for (batch, iteration, artifact, directory, _) in batches {
            writeln!(
                output,
                "| {batch:03} | {iteration} | `{artifact}` | `{directory}/` |"
            )
            .unwrap();
        }
        writeln!(output).unwrap();
    }

    for (batch, iteration, artifact, directory, manual) in batches {
        writeln!(output, "## Run {batch:03} — `{artifact}`\n").unwrap();
        writeln!(output, "Planning wave: {iteration}. Robot protocols, the Lab automation manifest, and the standalone run manual are in `{directory}/`.\n").unwrap();
        let inputs = manifest
            .nodes
            .iter()
            .filter(|node| artifact.split(", ").any(|name| node.artifact == name))
            .flat_map(|node| node.dependencies.iter())
            .collect::<BTreeSet<_>>();
        if !inputs.is_empty() {
            writeln!(
                output,
                "Required generated or retrieved artifact inputs: {}.\n",
                inputs
                    .iter()
                    .map(|dependency| format!("`{dependency}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .unwrap();
        }
        let steps = manifest
            .nodes
            .iter()
            .filter(|node| artifact.split(", ").any(|name| node.artifact == name))
            .flat_map(|node| node.steps.iter())
            .collect::<BTreeSet<_>>();
        writeln!(
            output,
            "Requested abstract steps: {}.\n",
            steps
                .iter()
                .map(|step| format!("`{step}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
        if manifest
            .edges
            .iter()
            .any(|edge| artifact.split(", ").any(|name| edge.depends_on == name))
        {
            writeln!(output, "After completing this run, retain, prepare, and verify the material it produced before treating it as an input to a later run.\n").unwrap();
        } else {
            writeln!(output, "After completing this batch, retain and verify the requested root artifact `{artifact}` and record its acceptance evidence.\n").unwrap();
        }
        for (line_index, line) in manual.lines().enumerate() {
            if line_index == 0 && line.starts_with("# ") {
                continue;
            }
            if line.starts_with('#') {
                writeln!(output, "#{line}").unwrap();
            } else {
                writeln!(output, "{line}").unwrap();
            }
        }
        writeln!(output).unwrap();
    }

    output
}

pub(in crate::backend) fn render_report(manifest: &DependencyBuildManifest) -> String {
    let mut output = String::new();
    writeln!(output, "# Dependency-driven build\n").unwrap();
    writeln!(output, "Status: `{:?}`\n", manifest.status).unwrap();
    writeln!(output, "Roots: {}\n", manifest.roots.join(", ")).unwrap();
    writeln!(
        output,
        "| Artifact | Dependencies | Resolution | Iteration |"
    )
    .unwrap();
    writeln!(output, "| --- | --- | --- | ---: |").unwrap();
    for node in &manifest.nodes {
        writeln!(
            output,
            "| {} | {} | {:?} | {} |",
            node.artifact,
            node.dependencies.join(", "),
            node.resolution,
            node.generated_in_iteration
                .map_or_else(|| "-".into(), |value| value.to_string())
        )
        .unwrap();
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
        writeln!(output, "\n## Unresolved inputs\n").unwrap();
        for node in blockers {
            writeln!(
                output,
                "- `{}`: dependencies [{}]; materials [{}]; resolution `{:?}`",
                node.artifact,
                node.missing_dependencies.join(", "),
                node.missing_materials.join(", "),
                node.resolution
            )
            .unwrap();
        }
    }
    writeln!(output, "\n## Execution boundary\n").unwrap();
    writeln!(output, "Each generated artifact is packaged as an independently reviewable assembly, transformation, and plating batch. A product is added to the planning inventory only after its batch is scheduled; physical execution and acceptance evidence remain laboratory responsibilities.").unwrap();
    output
}
