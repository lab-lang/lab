//! Lower checked Lab modules into target-neutral Design and Workflow intent.

use std::collections::BTreeMap;

use lab_language::{
    CheckedActionArgument, CheckedDeclaration, CheckedExpression, CheckedModule, CheckedStatement,
    ResolvedAction, TypedExpression,
};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WorkflowActionIntent {
    Realize {
        product: String,
        construct: String,
    },
    Provision {
        cells: String,
        item: String,
    },
    Transform {
        culture: String,
        construct: String,
        cells: String,
    },
    Recover {
        culture: String,
        input: String,
        duration_magnitude: String,
        duration_unit: String,
    },
    Dilute {
        culture: String,
        input: String,
    },
    Plate {
        plate: String,
        culture: String,
        selection: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RealizationFlow {
    dependencies: Vec<String>,
    actions: Vec<WorkflowActionIntent>,
}

#[derive(Debug, Error)]
pub enum SourceLoweringError {
    #[error("source module does not declare any build artifacts")]
    EmptyBuild,
    #[error("artifact '{artifact}' is missing workflow input '{field}'")]
    MissingField {
        artifact: String,
        field: &'static str,
    },
    #[error("artifact '{artifact}' workflow input '{field}' has the wrong checked value shape")]
    InvalidField {
        artifact: String,
        field: &'static str,
    },
    #[error("artifact '{artifact}' count '{field}' exceeds u8")]
    CountOverflow {
        artifact: String,
        field: &'static str,
    },
    #[error("artifact '{0}' has no std.bio.build.realize workflow operation")]
    MissingRealization(String),
    #[error("realize workflow for artifact '{0}' has unsupported dependency dataflow")]
    InvalidDependencyFlow(String),
    #[error("workflow realizing artifact '{artifact}' contains unsupported action '{operation}'")]
    UnsupportedWorkflowAction { artifact: String, operation: String },
    #[error("workflow action '{operation}' for artifact '{artifact}' has invalid result bindings")]
    InvalidActionResults { artifact: String, operation: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BuildArtifactIntent {
    pub name: String,
    pub sequence: String,
    pub dependencies: Vec<String>,
    pub recipe: BuildRecipeIntent,
    pub actions: Vec<WorkflowActionIntent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BuildRecipeIntent {
    pub backbone: String,
    pub components: Vec<String>,
    pub restriction_enzyme: String,
    pub assembly_replicates: u8,
    pub transformation_replicates: u8,
    pub plating_replicates: u8,
    pub serial_dilutions: u8,
}

/// Project backend-neutral checked source into explicit Design and Workflow
/// intent. This is the only layer that knows the standard-library operation
/// identities used by the source frontend.
pub(crate) fn lower_build_intent(
    module: &CheckedModule,
) -> Result<Vec<BuildArtifactIntent>, SourceLoweringError> {
    let identities = inventory_identities(module);
    let flows = realization_flows(module, &identities)?;
    let artifacts = module
        .declarations
        .iter()
        .filter_map(|declaration| {
            let CheckedDeclaration::Plasmid {
                name, properties, ..
            } = declaration
            else {
                return None;
            };
            Some(lower_artifact(
                name,
                properties,
                flows.get(name),
                &identities,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if artifacts.is_empty() {
        return Err(SourceLoweringError::EmptyBuild);
    }
    Ok(artifacts)
}

fn lower_artifact(
    name: &str,
    properties: &[lab_language::CheckedProperty],
    flow: Option<&RealizationFlow>,
    identities: &BTreeMap<String, String>,
) -> Result<BuildArtifactIntent, SourceLoweringError> {
    let find = |field: &'static str| {
        properties
            .iter()
            .find(|property| property.name == field)
            .map(|property| &property.value)
            .ok_or_else(|| SourceLoweringError::MissingField {
                artifact: name.to_owned(),
                field,
            })
    };
    let symbol = |field, accepted| checked_symbol(name, field, find(field)?, identities, accepted);
    let symbols =
        |field, accepted| checked_symbols(name, field, find(field)?, identities, accepted);
    let count = |field, default| match properties.iter().find(|property| property.name == field) {
        Some(property) => checked_u8(name, field, &property.value),
        None => Ok(default),
    };
    let flow = flow.ok_or_else(|| SourceLoweringError::MissingRealization(name.to_owned()))?;
    Ok(BuildArtifactIntent {
        name: name.to_owned(),
        sequence: checked_dna(name, find("sequence")?)?,
        dependencies: flow.dependencies.clone(),
        recipe: BuildRecipeIntent {
            backbone: symbol("backbone", &["Backbone"])?,
            components: symbols("components", &["Part", "Plasmid"])?,
            restriction_enzyme: symbol("restriction_enzyme", &["RestrictionEnzyme"])?,
            assembly_replicates: count("assembly_replicates", 1)?,
            transformation_replicates: count("transformation_replicates", 2)?,
            plating_replicates: count("plating_replicates", 2)?,
            serial_dilutions: count("serial_dilutions", 2)?,
        },
        actions: flow.actions.clone(),
    })
}

fn inventory_identities(module: &CheckedModule) -> BTreeMap<String, String> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| {
            let CheckedDeclaration::Binding(binding) = declaration else {
                return None;
            };
            let target = binding.targets.first()?.name.clone();
            let CheckedExpression::Call {
                operation,
                arguments,
            } = &binding.value.value
            else {
                return None;
            };
            if !operation.starts_with("std.bio.inventory.") || arguments.len() != 1 {
                return None;
            }
            let CheckedExpression::String { value } = &arguments[0].value.value else {
                return None;
            };
            Some((target, value.clone()))
        })
        .collect()
}

fn realization_flows(
    module: &CheckedModule,
    identities: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, RealizationFlow>, SourceLoweringError> {
    let mut result = BTreeMap::new();
    for declaration in &module.declarations {
        let CheckedDeclaration::Workflow { body, .. } = declaration else {
            continue;
        };
        let Some(design) = realized_design(body) else {
            continue;
        };
        let bindings = body
            .iter()
            .filter_map(|statement| {
                let CheckedStatement::Binding(binding) = statement else {
                    return None;
                };
                binding
                    .targets
                    .first()
                    .map(|target| (target.name.clone(), &binding.value))
            })
            .collect::<BTreeMap<_, _>>();
        let mut dependencies = None;
        let mut actions = Vec::new();
        for statement in body {
            let CheckedStatement::Effect { results, action } = statement else {
                continue;
            };
            let result_names = || {
                results
                    .iter()
                    .map(|result| result.name.clone())
                    .collect::<Vec<_>>()
            };
            match action.operation.as_str() {
                "std.bio.build.realize" => {
                    let names = result_names();
                    let [product, construct] = names.as_slice() else {
                        return Err(invalid_results(&design, action));
                    };
                    let dependency_binding = action_argument(action, "dependencies")
                        .and_then(reference_name)
                        .and_then(|name| bindings.get(name))
                        .ok_or_else(|| {
                            SourceLoweringError::InvalidDependencyFlow(design.clone())
                        })?;
                    let CheckedExpression::List { elements } = &dependency_binding.value else {
                        return Err(SourceLoweringError::InvalidDependencyFlow(design));
                    };
                    dependencies = Some(
                        elements
                            .iter()
                            .map(|element| {
                                reference_name(element).map(str::to_owned).ok_or_else(|| {
                                    SourceLoweringError::InvalidDependencyFlow(design.clone())
                                })
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                    );
                    actions.push(WorkflowActionIntent::Realize {
                        product: product.clone(),
                        construct: construct.clone(),
                    });
                }
                "std.lab.plasmid_actions.provision" => {
                    let names = result_names();
                    let [cells] = names.as_slice() else {
                        return Err(invalid_results(&design, action));
                    };
                    let item = resolved_reference(action, "strain", identities, &design)?;
                    actions.push(WorkflowActionIntent::Provision {
                        cells: cells.clone(),
                        item,
                    });
                }
                "std.lab.plasmid_actions.transform" => {
                    let names = result_names();
                    let [culture] = names.as_slice() else {
                        return Err(invalid_results(&design, action));
                    };
                    actions.push(WorkflowActionIntent::Transform {
                        culture: culture.clone(),
                        construct: required_reference(action, "construct", &design)?,
                        cells: required_reference(action, "cells", &design)?,
                    });
                }
                "std.lab.plasmid_actions.recover" => {
                    let names = result_names();
                    let [culture] = names.as_slice() else {
                        return Err(invalid_results(&design, action));
                    };
                    let duration = action_argument(action, "duration")
                        .ok_or_else(|| invalid_field(&design, "duration"))?;
                    let CheckedExpression::Quantity { magnitude, unit } = &duration.value else {
                        return Err(invalid_field(&design, "duration"));
                    };
                    actions.push(WorkflowActionIntent::Recover {
                        culture: culture.clone(),
                        input: required_reference(action, "culture", &design)?,
                        duration_magnitude: magnitude.clone(),
                        duration_unit: unit.clone(),
                    });
                }
                "std.lab.plasmid_actions.dilute" => {
                    let names = result_names();
                    let [culture] = names.as_slice() else {
                        return Err(invalid_results(&design, action));
                    };
                    actions.push(WorkflowActionIntent::Dilute {
                        culture: culture.clone(),
                        input: required_reference(action, "culture", &design)?,
                    });
                }
                "std.lab.plasmid_actions.plate" => {
                    let names = result_names();
                    let [plate] = names.as_slice() else {
                        return Err(invalid_results(&design, action));
                    };
                    actions.push(WorkflowActionIntent::Plate {
                        plate: plate.clone(),
                        culture: required_reference(action, "culture", &design)?,
                        selection: resolved_reference(action, "antibiotic", identities, &design)?,
                    });
                }
                operation => {
                    return Err(SourceLoweringError::UnsupportedWorkflowAction {
                        artifact: design,
                        operation: operation.to_owned(),
                    });
                }
            }
        }
        let flow = RealizationFlow {
            dependencies: dependencies
                .ok_or_else(|| SourceLoweringError::InvalidDependencyFlow(design.clone()))?,
            actions,
        };
        if result.insert(design.clone(), flow).is_some() {
            return Err(SourceLoweringError::InvalidDependencyFlow(design));
        }
    }
    Ok(result)
}

fn realized_design(body: &[CheckedStatement]) -> Option<String> {
    body.iter().find_map(|statement| {
        let CheckedStatement::Effect { action, .. } = statement else {
            return None;
        };
        (action.operation == "std.bio.build.realize")
            .then(|| action_argument(action, "design").and_then(reference_name))
            .flatten()
            .map(str::to_owned)
    })
}

fn action_argument<'a>(action: &'a ResolvedAction, name: &str) -> Option<&'a TypedExpression> {
    action
        .arguments
        .iter()
        .find(|argument: &&CheckedActionArgument| argument.name == name)
        .map(|argument| &argument.value)
}

fn required_reference(
    action: &ResolvedAction,
    argument: &'static str,
    artifact: &str,
) -> Result<String, SourceLoweringError> {
    action_argument(action, argument)
        .and_then(reference_name)
        .map(str::to_owned)
        .ok_or_else(|| invalid_field(artifact, argument))
}

fn resolved_reference(
    action: &ResolvedAction,
    argument: &'static str,
    identities: &BTreeMap<String, String>,
    artifact: &str,
) -> Result<String, SourceLoweringError> {
    let name = required_reference(action, argument, artifact)?;
    Ok(identities.get(&name).cloned().unwrap_or(name))
}

fn invalid_results(artifact: &str, action: &ResolvedAction) -> SourceLoweringError {
    SourceLoweringError::InvalidActionResults {
        artifact: artifact.to_owned(),
        operation: action.operation.clone(),
    }
}

fn reference_name(expression: &TypedExpression) -> Option<&str> {
    let CheckedExpression::Reference { path, .. } = &expression.value else {
        return None;
    };
    (path.len() == 1).then(|| path[0].as_str())
}

fn checked_string(
    artifact: &str,
    field: &'static str,
    expression: &TypedExpression,
) -> Result<String, SourceLoweringError> {
    match &expression.value {
        CheckedExpression::String { value } => Ok(value.clone()),
        _ => Err(invalid_field(artifact, field)),
    }
}

fn checked_symbols(
    artifact: &str,
    field: &'static str,
    expression: &TypedExpression,
    identities: &BTreeMap<String, String>,
    accepted: &[&str],
) -> Result<Vec<String>, SourceLoweringError> {
    let CheckedExpression::List { elements } = &expression.value else {
        return Err(invalid_field(artifact, field));
    };
    elements
        .iter()
        .map(|element| checked_symbol(artifact, field, element, identities, accepted))
        .collect()
}

fn checked_symbol(
    artifact: &str,
    field: &'static str,
    expression: &TypedExpression,
    identities: &BTreeMap<String, String>,
    accepted: &[&str],
) -> Result<String, SourceLoweringError> {
    let lab_language::CheckedType::Named { name, arguments } = &expression.r#type else {
        return Err(invalid_field(artifact, field));
    };
    if !arguments.is_empty() || !accepted.contains(&name.as_str()) {
        return Err(invalid_field(artifact, field));
    }
    let CheckedExpression::Reference { path, .. } = &expression.value else {
        return Err(invalid_field(artifact, field));
    };
    if path.len() != 1 {
        return Err(invalid_field(artifact, field));
    }
    Ok(identities
        .get(&path[0])
        .cloned()
        .unwrap_or_else(|| path[0].clone()))
}

fn checked_dna(
    artifact: &str,
    expression: &TypedExpression,
) -> Result<String, SourceLoweringError> {
    let CheckedExpression::Call {
        operation,
        arguments,
    } = &expression.value
    else {
        return Err(invalid_field(artifact, "sequence"));
    };
    if operation != "std.bio.dna" || arguments.len() != 1 {
        return Err(invalid_field(artifact, "sequence"));
    }
    checked_string(artifact, "sequence", &arguments[0].value)
}

fn checked_u8(
    artifact: &str,
    field: &'static str,
    expression: &TypedExpression,
) -> Result<u8, SourceLoweringError> {
    let CheckedExpression::Integer { value } = expression.value else {
        return Err(invalid_field(artifact, field));
    };
    u8::try_from(value).map_err(|_| SourceLoweringError::CountOverflow {
        artifact: artifact.to_owned(),
        field,
    })
}

fn invalid_field(artifact: &str, field: &'static str) -> SourceLoweringError {
    SourceLoweringError::InvalidField {
        artifact: artifact.to_owned(),
        field,
    }
}
