use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use super::{
    ArtifactResolution, BuildAttempt, BuildGraph, BuildInventory, DependencyBuildManifest,
    DependencyBuildStatus, DependencyEdge, DependencyNode,
};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DependencyGraphError {
    #[error("artifact '{artifact}' depends on undeclared artifact '{dependency}'")]
    UndeclaredDependency {
        artifact: String,
        dependency: String,
    },
}

/// Resolve graph waves against inventory without interpreting any biological
/// operation, execution target, or assembly hierarchy.
pub fn resolve_dependency_graph(
    graph: &BuildGraph,
    inventory: &BuildInventory,
) -> Result<DependencyBuildManifest, DependencyGraphError> {
    let names = graph.nodes.keys().cloned().collect::<BTreeSet<_>>();
    let mut referenced = BTreeSet::new();
    let mut edges = Vec::new();
    for (artifact, node) in &graph.nodes {
        for dependency in &node.dependencies {
            if !names.contains(dependency) {
                return Err(DependencyGraphError::UndeclaredDependency {
                    artifact: artifact.clone(),
                    dependency: dependency.clone(),
                });
            }
            referenced.insert(dependency.clone());
            edges.push(DependencyEdge {
                artifact: artifact.clone(),
                depends_on: dependency.clone(),
            });
        }
    }
    edges.sort_by(|left, right| {
        (&left.artifact, &left.depends_on).cmp(&(&right.artifact, &right.depends_on))
    });

    let mut roots = names.difference(&referenced).cloned().collect::<Vec<_>>();
    if roots.is_empty() {
        roots.extend(names.iter().cloned());
    }
    let existing = names
        .intersection(&inventory.available_artifacts)
        .cloned()
        .collect::<BTreeSet<_>>();
    let unresolved_graph = graph
        .nodes
        .iter()
        .filter(|(artifact, _)| !existing.contains(*artifact))
        .map(|(artifact, node)| {
            (
                artifact.clone(),
                node.dependencies
                    .iter()
                    .filter(|dependency| !existing.contains(*dependency))
                    .cloned()
                    .collect(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let cyclic = cyclic_nodes(&unresolved_graph);
    let mut available = existing.clone();
    let mut pending = names
        .difference(&available)
        .filter(|name| !cyclic.contains(*name))
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut attempts = cyclic
        .iter()
        .map(|artifact| BuildAttempt {
            iteration: 0,
            artifact: artifact.clone(),
            outcome: ArtifactResolution::Cyclic,
            missing_dependencies: graph.nodes[artifact].dependencies.iter().cloned().collect(),
            missing_materials: Vec::new(),
        })
        .collect::<Vec<_>>();
    let mut generated_at = BTreeMap::<String, usize>::new();

    for iteration in 1..=names.len().saturating_add(1) {
        if pending.is_empty() {
            break;
        }
        let available_at_start = available.clone();
        let mut ready = Vec::new();
        for artifact in &pending {
            let node = &graph.nodes[artifact];
            let missing_dependencies = node
                .dependencies
                .iter()
                .filter(|dependency| !available_at_start.contains(*dependency))
                .cloned()
                .collect::<Vec<_>>();
            let missing_materials = node
                .required_materials
                .iter()
                .filter(|material| !inventory.available_materials.contains(*material))
                .cloned()
                .collect::<Vec<_>>();
            let outcome = if missing_dependencies.is_empty() && missing_materials.is_empty() {
                ready.push(artifact.clone());
                ArtifactResolution::Generated
            } else {
                ArtifactResolution::Blocked
            };
            attempts.push(BuildAttempt {
                iteration,
                artifact: artifact.clone(),
                outcome,
                missing_dependencies,
                missing_materials,
            });
        }
        if ready.is_empty() {
            break;
        }
        for artifact in ready {
            pending.remove(&artifact);
            available.insert(artifact.clone());
            generated_at.insert(artifact, iteration);
        }
    }

    let nodes = names
        .iter()
        .map(|artifact| {
            let node = &graph.nodes[artifact];
            let resolution = if existing.contains(artifact) {
                ArtifactResolution::Existing
            } else if generated_at.contains_key(artifact) {
                ArtifactResolution::Generated
            } else if cyclic.contains(artifact) {
                ArtifactResolution::Cyclic
            } else {
                ArtifactResolution::Blocked
            };
            let (missing_dependencies, missing_materials) = if matches!(
                resolution,
                ArtifactResolution::Existing | ArtifactResolution::Generated
            ) {
                (Vec::new(), Vec::new())
            } else {
                (
                    node.dependencies
                        .iter()
                        .filter(|dependency| !available.contains(*dependency))
                        .cloned()
                        .collect(),
                    node.required_materials
                        .iter()
                        .filter(|material| !inventory.available_materials.contains(*material))
                        .cloned()
                        .collect(),
                )
            };
            DependencyNode {
                artifact: artifact.clone(),
                dependencies: node.dependencies.iter().cloned().collect(),
                steps: node.steps.clone(),
                inventory_materials: node.required_materials.iter().cloned().collect(),
                resolution,
                generated_in_iteration: generated_at.get(artifact).copied(),
                missing_dependencies,
                missing_materials,
            }
        })
        .collect::<Vec<_>>();
    let status = if roots.iter().all(|root| available.contains(root)) {
        DependencyBuildStatus::Complete
    } else {
        DependencyBuildStatus::Partial
    };
    let mut generated = generated_at.into_iter().collect::<Vec<_>>();
    generated.sort_by(
        |(left_name, left_iteration), (right_name, right_iteration)| {
            (left_iteration, left_name).cmp(&(right_iteration, right_name))
        },
    );

    Ok(DependencyBuildManifest {
        schema_version: "lab.dependency-build.v0".into(),
        status,
        roots,
        nodes,
        edges,
        attempts,
        generated_artifacts: generated.into_iter().map(|(name, _)| name).collect(),
        existing_artifacts: existing.into_iter().collect(),
    })
}

fn cyclic_nodes(graph: &BTreeMap<String, BTreeSet<String>>) -> BTreeSet<String> {
    fn visit(
        node: &str,
        graph: &BTreeMap<String, BTreeSet<String>>,
        visited: &mut BTreeSet<String>,
        stack: &mut Vec<String>,
        active: &mut BTreeSet<String>,
        cyclic: &mut BTreeSet<String>,
    ) {
        if visited.contains(node) {
            return;
        }
        active.insert(node.to_owned());
        stack.push(node.to_owned());
        for dependency in &graph[node] {
            if active.contains(dependency) {
                if let Some(offset) = stack.iter().position(|entry| entry == dependency) {
                    cyclic.extend(stack[offset..].iter().cloned());
                }
            } else {
                visit(dependency, graph, visited, stack, active, cyclic);
            }
        }
        stack.pop();
        active.remove(node);
        visited.insert(node.to_owned());
    }

    let mut visited = BTreeSet::new();
    let mut stack = Vec::new();
    let mut active = BTreeSet::new();
    let mut cyclic = BTreeSet::new();
    for node in graph.keys() {
        visit(
            node,
            graph,
            &mut visited,
            &mut stack,
            &mut active,
            &mut cyclic,
        );
    }
    cyclic
}

#[cfg(test)]
mod tests {
    use super::super::BuildGraphNode;
    use super::*;

    #[test]
    fn schedules_graph_waves_without_target_knowledge() {
        let graph = BuildGraph {
            nodes: BTreeMap::from([
                (
                    "leaf".into(),
                    BuildGraphNode {
                        required_materials: BTreeSet::from(["source".into()]),
                        ..BuildGraphNode::default()
                    },
                ),
                (
                    "root".into(),
                    BuildGraphNode {
                        dependencies: BTreeSet::from(["leaf".into()]),
                        ..BuildGraphNode::default()
                    },
                ),
            ]),
        };
        let inventory = BuildInventory {
            available_materials: BTreeSet::from(["source".into()]),
            available_artifacts: BTreeSet::new(),
        };
        let manifest = resolve_dependency_graph(&graph, &inventory).unwrap();
        assert_eq!(manifest.generated_artifacts, ["leaf", "root"]);
        assert_eq!(manifest.status, DependencyBuildStatus::Complete);
    }
}
