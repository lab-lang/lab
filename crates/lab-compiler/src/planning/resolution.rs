use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::planning::model::MaterialLotCandidates;
use crate::planning::{
    ArtifactResolution, BuildAttempt, BuildGraph, BuildInventory, DependencyBuildManifest,
    DependencyBuildStatus, DependencyEdge, DependencyInventorySource, DependencyNode,
    MaterialLotBinding,
};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DependencyGraphError {
    #[error("artifact '{artifact}' depends on undeclared artifact '{dependency}'")]
    UndeclaredDependency {
        artifact: String,
        dependency: String,
    },
    #[error(
        "the manifest declares material '{material}', which this build never uses; \
         a catalogued name that was renamed leaves its old identity here"
    )]
    UnusedInventoryMaterial { material: String },
    #[error(
        "{kind} `{symbol}` has no exact sbol_identity; SBOLInventory matching never uses declaration names or display IDs"
    )]
    MissingDesignIdentity { kind: &'static str, symbol: String },
    #[error(
        "{kind} `{symbol}` realizes SBOL Component `{component}` through several active MaterialLots ({material_lots}); allocation policy must select one"
    )]
    AmbiguousMaterialLot {
        kind: &'static str,
        symbol: String,
        component: String,
        material_lots: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Availability {
    Missing,
    Legacy,
    MaterialLot(MaterialLotBinding),
}

impl Availability {
    fn is_available(&self) -> bool {
        !matches!(self, Self::Missing)
    }

    fn binding(&self) -> Option<&MaterialLotBinding> {
        match self {
            Self::MaterialLot(binding) => Some(binding),
            Self::Missing | Self::Legacy => None,
        }
    }
}

/// Resolve graph waves against inventory without interpreting any biological
/// operation, execution target, or assembly hierarchy.
pub fn resolve_dependency_graph(
    graph: &BuildGraph,
    inventory: &BuildInventory,
) -> Result<DependencyBuildManifest, DependencyGraphError> {
    if let BuildInventory::LegacySymbols(inventory) = inventory {
        // A legacy manifest is authored by hand, so an unused name is likely a
        // typo or stale entry. A semantic catalog may contain any number of
        // unrelated lots and is never subjected to this check.
        let required = graph
            .nodes
            .values()
            .flat_map(|node| node.required_materials.iter().cloned())
            .collect::<BTreeSet<_>>();
        if let Some(material) = inventory
            .available_materials
            .iter()
            .find(|material| !required.contains(*material))
        {
            return Err(DependencyGraphError::UnusedInventoryMaterial {
                material: material.clone(),
            });
        }
    }

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
    let mut existing = BTreeSet::new();
    let mut existing_material_lots = BTreeMap::new();
    for artifact in &names {
        let availability = artifact_availability(inventory, artifact)?;
        if availability.is_available() {
            existing.insert(artifact.clone());
            if let Some(binding) = availability.binding() {
                existing_material_lots.insert(artifact.clone(), binding.clone());
            }
        }
    }
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
    let required_materials = graph
        .nodes
        .iter()
        .filter(|(artifact, _)| !existing.contains(*artifact))
        .flat_map(|(_, node)| node.required_materials.iter().cloned())
        .collect::<BTreeSet<_>>();
    let material_availability = required_materials
        .into_iter()
        .map(|material| {
            let availability = material_availability(inventory, &material)?;
            Ok((material, availability))
        })
        .collect::<Result<BTreeMap<_, _>, DependencyGraphError>>()?;
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
                .filter(|material| !material_availability[*material].is_available())
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
                        .filter(|material| !material_availability[*material].is_available())
                        .cloned()
                        .collect(),
                )
            };
            let material_lot_bindings = node
                .required_materials
                .iter()
                .filter_map(|material| material_availability.get(material))
                .filter_map(Availability::binding)
                .cloned()
                .collect();
            DependencyNode {
                artifact: artifact.clone(),
                dependencies: node.dependencies.iter().cloned().collect(),
                steps: node.steps.clone(),
                inventory_materials: node.required_materials.iter().cloned().collect(),
                material_lot_bindings,
                existing_material_lot: existing_material_lots.get(artifact).cloned(),
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
        schema_version: "lab.dependency-build.v1".into(),
        inventory: inventory_source(inventory),
        status,
        roots,
        nodes,
        edges,
        attempts,
        generated_artifacts: generated.into_iter().map(|(name, _)| name).collect(),
        existing_artifacts: existing.into_iter().collect(),
    })
}

fn inventory_source(inventory: &BuildInventory) -> DependencyInventorySource {
    match inventory {
        BuildInventory::MaterialLots(inventory) => DependencyInventorySource::SbolInventory {
            source_sha256: inventory.source_sha256.clone(),
            facility: inventory.facility.clone(),
        },
        BuildInventory::LegacySymbols(_) => DependencyInventorySource::LegacySymbols,
    }
}

fn artifact_availability(
    inventory: &BuildInventory,
    artifact: &str,
) -> Result<Availability, DependencyGraphError> {
    match inventory {
        BuildInventory::LegacySymbols(inventory) => {
            Ok(if inventory.available_artifacts.contains(artifact) {
                Availability::Legacy
            } else {
                Availability::Missing
            })
        }
        BuildInventory::MaterialLots(inventory) => {
            exact_availability(&inventory.artifacts, "artifact", artifact)
        }
    }
}

fn material_availability(
    inventory: &BuildInventory,
    material: &str,
) -> Result<Availability, DependencyGraphError> {
    match inventory {
        BuildInventory::LegacySymbols(inventory) => {
            Ok(if inventory.available_materials.contains(material) {
                Availability::Legacy
            } else {
                Availability::Missing
            })
        }
        BuildInventory::MaterialLots(inventory) => {
            exact_availability(&inventory.materials, "material", material)
        }
    }
}

fn exact_availability(
    entries: &BTreeMap<String, MaterialLotCandidates>,
    kind: &'static str,
    symbol: &str,
) -> Result<Availability, DependencyGraphError> {
    let Some(candidates) = entries.get(symbol) else {
        return Err(DependencyGraphError::MissingDesignIdentity {
            kind,
            symbol: symbol.to_owned(),
        });
    };
    let MaterialLotCandidates::Identified {
        component,
        material_lots,
    } = candidates
    else {
        return Err(DependencyGraphError::MissingDesignIdentity {
            kind,
            symbol: symbol.to_owned(),
        });
    };
    match material_lots.as_slice() {
        [] => Ok(Availability::Missing),
        [material_lot] => Ok(Availability::MaterialLot(MaterialLotBinding {
            symbol: symbol.to_owned(),
            component: component.clone(),
            material_lot: material_lot.clone(),
        })),
        _ => Err(DependencyGraphError::AmbiguousMaterialLot {
            kind,
            symbol: symbol.to_owned(),
            component: component.clone(),
            material_lots: material_lots.join(", "),
        }),
    }
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
    use crate::planning::BuildGraphNode;
    use crate::planning::model::{MaterialLotBuildInventory, MaterialLotCandidates};
    use crate::planning::resolution::*;

    fn identified(component: &str, material_lots: &[&str]) -> MaterialLotCandidates {
        MaterialLotCandidates::Identified {
            component: component.to_owned(),
            material_lots: material_lots.iter().map(|lot| (*lot).to_owned()).collect(),
        }
    }

    fn semantic_inventory(
        materials: BTreeMap<String, MaterialLotCandidates>,
        artifacts: BTreeMap<String, MaterialLotCandidates>,
    ) -> BuildInventory {
        BuildInventory::MaterialLots(MaterialLotBuildInventory {
            source_sha256: "abc123".to_owned(),
            facility: "https://example.org/facility".to_owned(),
            materials,
            artifacts,
        })
    }

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
        let inventory = BuildInventory::legacy(["source".into()], []);
        let manifest = resolve_dependency_graph(&graph, &inventory).unwrap();
        assert_eq!(manifest.generated_artifacts, ["leaf", "root"]);
        assert_eq!(manifest.status, DependencyBuildStatus::Complete);
    }

    #[test]
    fn freezes_exact_component_and_material_lot_bindings() {
        let graph = BuildGraph {
            nodes: BTreeMap::from([(
                "product".into(),
                BuildGraphNode {
                    required_materials: BTreeSet::from(["input".into()]),
                    ..BuildGraphNode::default()
                },
            )]),
        };
        let inventory = semantic_inventory(
            BTreeMap::from([(
                "input".into(),
                identified(
                    "https://example.org/component/input",
                    &["https://example.org/lot/input-7"],
                ),
            )]),
            BTreeMap::from([(
                "product".into(),
                identified("https://example.org/component/product", &[]),
            )]),
        );

        let manifest = resolve_dependency_graph(&graph, &inventory).unwrap();

        assert_eq!(manifest.schema_version, "lab.dependency-build.v1");
        assert_eq!(
            manifest.inventory,
            DependencyInventorySource::SbolInventory {
                source_sha256: "abc123".to_owned(),
                facility: "https://example.org/facility".to_owned(),
            }
        );
        assert_eq!(
            manifest.nodes[0].material_lot_bindings,
            [MaterialLotBinding {
                symbol: "input".to_owned(),
                component: "https://example.org/component/input".to_owned(),
                material_lot: "https://example.org/lot/input-7".to_owned(),
            }]
        );
        assert_eq!(manifest.nodes[0].resolution, ArtifactResolution::Generated);
    }

    #[test]
    fn refuses_to_allocate_an_ambiguous_material_lot() {
        let graph = BuildGraph {
            nodes: BTreeMap::from([(
                "product".into(),
                BuildGraphNode {
                    required_materials: BTreeSet::from(["input".into()]),
                    ..BuildGraphNode::default()
                },
            )]),
        };
        let inventory = semantic_inventory(
            BTreeMap::from([(
                "input".into(),
                identified(
                    "https://example.org/component/input",
                    &["https://example.org/lot/a", "https://example.org/lot/b"],
                ),
            )]),
            BTreeMap::from([(
                "product".into(),
                identified("https://example.org/component/product", &[]),
            )]),
        );

        let error = resolve_dependency_graph(&graph, &inventory).unwrap_err();

        assert_eq!(
            error,
            DependencyGraphError::AmbiguousMaterialLot {
                kind: "material",
                symbol: "input".to_owned(),
                component: "https://example.org/component/input".to_owned(),
                material_lots: "https://example.org/lot/a, https://example.org/lot/b".to_owned(),
            }
        );
    }

    #[test]
    fn exact_inventory_never_falls_back_to_a_symbol_name() {
        let graph = BuildGraph {
            nodes: BTreeMap::from([(
                "product".into(),
                BuildGraphNode {
                    required_materials: BTreeSet::from(["input".into()]),
                    ..BuildGraphNode::default()
                },
            )]),
        };
        let inventory = semantic_inventory(
            BTreeMap::from([("input".into(), MaterialLotCandidates::Unidentified)]),
            BTreeMap::from([(
                "product".into(),
                identified("https://example.org/component/product", &[]),
            )]),
        );

        assert_eq!(
            resolve_dependency_graph(&graph, &inventory).unwrap_err(),
            DependencyGraphError::MissingDesignIdentity {
                kind: "material",
                symbol: "input".to_owned(),
            }
        );
    }

    #[test]
    fn an_existing_artifact_binds_its_lot_without_resolving_recipe_leaves() {
        let graph = BuildGraph {
            nodes: BTreeMap::from([(
                "product".into(),
                BuildGraphNode {
                    required_materials: BTreeSet::from(["unavailable_recipe_leaf".into()]),
                    ..BuildGraphNode::default()
                },
            )]),
        };
        let inventory = semantic_inventory(
            BTreeMap::new(),
            BTreeMap::from([(
                "product".into(),
                identified(
                    "https://example.org/component/product",
                    &["https://example.org/lot/product"],
                ),
            )]),
        );

        let manifest = resolve_dependency_graph(&graph, &inventory).unwrap();

        assert_eq!(manifest.nodes[0].resolution, ArtifactResolution::Existing);
        assert_eq!(
            manifest.nodes[0].existing_material_lot,
            Some(MaterialLotBinding {
                symbol: "product".to_owned(),
                component: "https://example.org/component/product".to_owned(),
                material_lot: "https://example.org/lot/product".to_owned(),
            })
        );
        assert!(manifest.nodes[0].material_lot_bindings.is_empty());
    }
}
