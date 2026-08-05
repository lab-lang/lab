//! Lower checked Lab modules into the OT-2 backend build IR.

use std::collections::BTreeMap;

use lab_language::{
    CheckedDeclaration, CheckedExpression, CheckedModule, CheckedStatement, TypedExpression,
};
use thiserror::Error;

use super::{Ot2BuildArtifact, Ot2BuildIr, Ot2BuildIrError, Ot2BuildRecipe};

struct RealizationFlow {
    dependencies: Vec<String>,
    steps: Vec<String>,
}

#[derive(Debug, Error)]
pub enum Ot2LoweringError {
    #[error(transparent)]
    InvalidTargetIr(#[from] Ot2BuildIrError),
    #[error("artifact '{artifact}' is missing target input '{field}'")]
    MissingField {
        artifact: String,
        field: &'static str,
    },
    #[error("artifact '{artifact}' target input '{field}' has the wrong checked value shape")]
    InvalidField {
        artifact: String,
        field: &'static str,
    },
    #[error("artifact '{artifact}' target count '{field}' exceeds u8")]
    CountOverflow {
        artifact: String,
        field: &'static str,
    },
    #[error("artifact '{0}' has no std.bio.build.realize workflow operation")]
    MissingRealization(String),
    #[error("realize workflow for artifact '{0}' has unsupported dependency dataflow")]
    InvalidDependencyFlow(String),
}

/// Narrow target lowering from backend-neutral checked module IR.
///
/// Field names are interpreted here, not by the parser, AST, or semantic
/// checker. Other targets can consume the same checked declarations using a
/// different target IR.
pub fn lower_build(module: &CheckedModule) -> Result<Ot2BuildIr, Ot2LoweringError> {
    let dependencies = realization_dependencies(module)?;
    let identities = inventory_identities(module);
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
                dependencies.get(name),
                &identities,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Ot2BuildIr::new(artifacts)?)
}

fn lower_artifact(
    name: &str,
    properties: &[lab_language::CheckedProperty],
    flow: Option<&RealizationFlow>,
    identities: &BTreeMap<String, String>,
) -> Result<Ot2BuildArtifact, Ot2LoweringError> {
    let find = |field: &'static str| {
        properties
            .iter()
            .find(|property| property.name == field)
            .map(|property| &property.value)
            .ok_or_else(|| Ot2LoweringError::MissingField {
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
    Ok(Ot2BuildArtifact {
        name: name.to_owned(),
        sequence: checked_dna(name, find("sequence")?)?,
        dependencies: flow
            .map(|flow| flow.dependencies.clone())
            .ok_or_else(|| Ot2LoweringError::MissingRealization(name.to_owned()))?,
        recipe: Ot2BuildRecipe {
            backbone: symbol("backbone", &["Backbone"])?,
            components: symbols("components", &["Part", "Plasmid"])?,
            steps: flow
                .map(|flow| flow.steps.clone())
                .ok_or_else(|| Ot2LoweringError::MissingRealization(name.to_owned()))?,
            restriction_enzyme: symbol("restriction_enzyme", &["RestrictionEnzyme"])?,
            host: symbol("host", &["Strain"])?,
            selection: symbol("selection", &["Antibiotic"])?,
            assembly_replicates: count("assembly_replicates", 1)?,
            transformation_replicates: count("transformation_replicates", 2)?,
            plating_replicates: count("plating_replicates", 2)?,
            serial_dilutions: count("serial_dilutions", 2)?,
        },
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

fn realization_dependencies(
    module: &CheckedModule,
) -> Result<BTreeMap<String, RealizationFlow>, Ot2LoweringError> {
    let mut result = BTreeMap::new();
    for declaration in &module.declarations {
        let CheckedDeclaration::Workflow { body, .. } = declaration else {
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
        for statement in body {
            let CheckedStatement::Effect { action, .. } = statement else {
                continue;
            };
            if action.operation != "std.bio.build.realize" {
                continue;
            }
            let design = action
                .arguments
                .iter()
                .find(|argument| argument.name == "design")
                .and_then(|argument| reference_name(&argument.value))
                .ok_or_else(|| Ot2LoweringError::InvalidDependencyFlow("<unknown>".into()))?;
            let dependency_binding = action
                .arguments
                .iter()
                .find(|argument| argument.name == "dependencies")
                .and_then(|argument| reference_name(&argument.value))
                .and_then(|name| bindings.get(name))
                .ok_or_else(|| Ot2LoweringError::InvalidDependencyFlow(design.to_owned()))?;
            let CheckedExpression::List { elements } = &dependency_binding.value else {
                return Err(Ot2LoweringError::InvalidDependencyFlow(design.to_owned()));
            };
            let dependencies = elements
                .iter()
                .map(|element| {
                    reference_name(element)
                        .map(str::to_owned)
                        .ok_or_else(|| Ot2LoweringError::InvalidDependencyFlow(design.to_owned()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let steps = body
                .iter()
                .filter_map(|statement| {
                    let CheckedStatement::Effect { action, .. } = statement else {
                        return None;
                    };
                    match action.operation.as_str() {
                        "std.bio.build.realize" => Some("assemble"),
                        "std.lab.plasmid_actions.transform" => Some("transform"),
                        "std.lab.plasmid_actions.recover" => Some("recover"),
                        "std.lab.plasmid_actions.dilute" => Some("dilute"),
                        "std.lab.plasmid_actions.plate" => Some("plate"),
                        _ => None,
                    }
                })
                .map(str::to_owned)
                .collect();
            if result
                .insert(
                    design.to_owned(),
                    RealizationFlow {
                        dependencies,
                        steps,
                    },
                )
                .is_some()
            {
                return Err(Ot2LoweringError::InvalidDependencyFlow(design.to_owned()));
            }
        }
    }
    Ok(result)
}

fn reference_name(expression: &TypedExpression) -> Option<&str> {
    let CheckedExpression::Reference { path } = &expression.value else {
        return None;
    };
    (path.len() == 1).then(|| path[0].as_str())
}

fn checked_string(
    artifact: &str,
    field: &'static str,
    expression: &TypedExpression,
) -> Result<String, Ot2LoweringError> {
    match &expression.value {
        CheckedExpression::String { value } => Ok(value.clone()),
        _ => Err(invalid(artifact, field)),
    }
}

fn checked_symbols(
    artifact: &str,
    field: &'static str,
    expression: &TypedExpression,
    identities: &BTreeMap<String, String>,
    accepted: &[&str],
) -> Result<Vec<String>, Ot2LoweringError> {
    let CheckedExpression::List { elements } = &expression.value else {
        return Err(invalid(artifact, field));
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
) -> Result<String, Ot2LoweringError> {
    let lab_language::CheckedType::Named { name, arguments } = &expression.r#type else {
        return Err(invalid(artifact, field));
    };
    if !arguments.is_empty() || !accepted.contains(&name.as_str()) {
        return Err(invalid(artifact, field));
    }
    let CheckedExpression::Reference { path } = &expression.value else {
        return Err(invalid(artifact, field));
    };
    if path.len() != 1 {
        return Err(invalid(artifact, field));
    }
    Ok(identities
        .get(&path[0])
        .cloned()
        .unwrap_or_else(|| path[0].clone()))
}

fn checked_dna(artifact: &str, expression: &TypedExpression) -> Result<String, Ot2LoweringError> {
    let CheckedExpression::Call {
        operation,
        arguments,
    } = &expression.value
    else {
        return Err(invalid(artifact, "sequence"));
    };
    if operation != "std.bio.dna" || arguments.len() != 1 {
        return Err(invalid(artifact, "sequence"));
    }
    checked_string(artifact, "sequence", &arguments[0].value)
}

fn checked_u8(
    artifact: &str,
    field: &'static str,
    expression: &TypedExpression,
) -> Result<u8, Ot2LoweringError> {
    let CheckedExpression::Integer { value } = expression.value else {
        return Err(invalid(artifact, field));
    };
    u8::try_from(value).map_err(|_| Ot2LoweringError::CountOverflow {
        artifact: artifact.to_owned(),
        field,
    })
}

fn invalid(artifact: &str, field: &'static str) -> Ot2LoweringError {
    Ot2LoweringError::InvalidField {
        artifact: artifact.to_owned(),
        field,
    }
}
