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
    },
    Provision {
        cells: String,
        item: String,
    },
    Transform {
        strain: String,
        culture: String,
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
    #[error(
        "the opentrons-ot2 target does not know how to build a '{kind}', which artifact \
         '{artifact}' declares"
    )]
    UnsupportedArtifactKind { artifact: String, kind: String },
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
    #[error("artifact '{artifact}' quantity '{field}' expects unit '{expected}', found '{found}'")]
    WrongUnit {
        artifact: String,
        field: &'static str,
        expected: &'static str,
        found: String,
    },
    #[error(
        "artifact '{artifact}' reagents and DNA exceed its {reaction_volume_ul} uL reaction volume"
    )]
    UnbalancedReaction {
        artifact: String,
        reaction_volume_ul: u16,
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

/// One declared artifact together with the workflow that realizes it. The two
/// kinds are separate because they name different materials and produce
/// different laboratory stages, not because a target requires it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BuildArtifactIntent {
    Plasmid(PlasmidArtifactIntent),
    Strain(StrainArtifactIntent),
}

impl BuildArtifactIntent {
    pub fn name(&self) -> &str {
        match self {
            Self::Plasmid(intent) => &intent.name,
            Self::Strain(intent) => &intent.name,
        }
    }

    pub fn dependencies(&self) -> &[String] {
        match self {
            Self::Plasmid(intent) => &intent.dependencies,
            Self::Strain(intent) => &intent.dependencies,
        }
    }

    pub fn actions(&self) -> &[WorkflowActionIntent] {
        match self {
            Self::Plasmid(intent) => &intent.actions,
            Self::Strain(intent) => &intent.actions,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlasmidArtifactIntent {
    pub name: String,
    pub sequence: String,
    pub dependencies: Vec<String>,
    pub recipe: AssemblyRecipeIntent,
    pub actions: Vec<WorkflowActionIntent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssemblyRecipeIntent {
    pub backbone: String,
    pub components: Vec<String>,
    pub restriction_enzyme: String,
    pub assembly_replicates: u8,
    pub chemistry: AssemblyChemistryIntent,
}

/// Golden Gate reaction chemistry. These are scientific choices stated by the
/// design, not properties of the bench that runs it, so they travel with the
/// artifact rather than with a target profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssemblyChemistryIntent {
    pub reaction_volume_ul: u16,
    pub part_volume_ul: u16,
    pub enzyme_volume_ul: u16,
    pub ligase_volume_ul: u16,
    pub buffer_volume_ul: u16,
    pub cycles: u16,
    pub digest_temperature_c: u16,
    pub digest_minutes: u16,
    pub ligate_temperature_c: u16,
    pub ligate_minutes: u16,
}

impl AssemblyChemistryIntent {
    /// Nuclease-free water making the reaction up to its stated volume.
    pub fn water_volume_ul(&self, dna_pieces: usize) -> Option<u16> {
        let reagents = self.buffer_volume_ul + self.ligase_volume_ul + self.enzyme_volume_ul;
        let dna = self.part_volume_ul.checked_mul(dna_pieces as u16)?;
        self.reaction_volume_ul.checked_sub(reagents + dna)
    }
}

/// Heat-shock transformation and plating chemistry stated by a strain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StrainChemistryIntent {
    pub cell_volume_ul: u16,
    pub dna_volume_ul: u16,
    pub recovery_volume_ul: u16,
    pub cold_minutes: u16,
    pub heat_shock_temperature_c: u16,
    pub heat_shock_minutes: u16,
    pub recovery_temperature_c: u16,
    pub recovery_minutes: u16,
    pub medium_volume_ul: u16,
    pub culture_volume_ul: u16,
    pub colony_volume_ul: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StrainArtifactIntent {
    pub name: String,
    pub chassis: String,
    /// Plasmid designs the strain carries.
    pub plasmids: Vec<String>,
    /// Artifacts whose materials the realizing workflow consumes.
    pub dependencies: Vec<String>,
    pub selection: String,
    pub transformation_replicates: u8,
    pub plating_replicates: u8,
    pub serial_dilutions: u8,
    pub chemistry: StrainChemistryIntent,
    pub actions: Vec<WorkflowActionIntent>,
}

/// Project backend-neutral checked source into explicit Design and Workflow
/// intent. This is the only layer that knows the standard-library operation
/// identities used by the source frontend.
///
/// The modules are one program. Inventory identities, artifact declarations,
/// and realization workflows are read across all of them, so a declaration and
/// the workflow that realizes it may live in different modules. The caller
/// supplies them in a deterministic order.
pub(crate) fn lower_build_intent(
    modules: &[&CheckedModule],
) -> Result<Vec<BuildArtifactIntent>, SourceLoweringError> {
    let identities = inventory_identities(modules);
    let stated = inventory_properties(modules);
    let flows = realization_flows(modules, &identities)?;
    let artifacts = declarations(modules)
        .filter_map(|declaration| {
            let CheckedDeclaration::Artifact {
                artifact,
                name,
                properties,
                ..
            } = declaration
            else {
                return None;
            };
            Some(lower_artifact(
                artifact.as_str(),
                name,
                properties,
                flows.get(name),
                &identities,
                &stated,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if artifacts.is_empty() {
        return Err(SourceLoweringError::EmptyBuild);
    }
    Ok(artifacts)
}

fn declarations<'a>(
    modules: &'a [&'a CheckedModule],
) -> impl Iterator<Item = &'a CheckedDeclaration> {
    modules.iter().flat_map(|module| module.declarations.iter())
}

fn lower_artifact(
    kind: &str,
    name: &str,
    properties: &[lab_language::CheckedProperty],
    flow: Option<&RealizationFlow>,
    identities: &BTreeMap<String, String>,
    stated: &BTreeMap<String, Vec<lab_language::CheckedProperty>>,
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
    let owner = |owner_field: &'static str| {
        properties
            .iter()
            .find(|property| property.name == owner_field)
            .and_then(|property| reference_name(&property.value))
            .and_then(|name| stated.get(name))
    };
    // A design's own value wins; otherwise the item it names supplies one.
    let inherited = |owner_field: &'static str, field: &'static str| {
        owner(owner_field)?
            .iter()
            .find(|property| property.name == field)
            .map(|property| property.value.clone())
    };
    let count = |field, default| match properties.iter().find(|property| property.name == field) {
        Some(property) => checked_u8(name, field, &property.value),
        None => Ok(default),
    };
    let quantity = |field: &'static str, unit, default| match properties
        .iter()
        .find(|property| property.name == field)
    {
        Some(property) => checked_quantity(name, field, &property.value, unit),
        None => {
            match inherited("restriction_enzyme", field).or_else(|| inherited("chassis", field)) {
                Some(value) => checked_quantity(name, field, &value, unit),
                None => Ok(default),
            }
        }
    };
    let whole = |field, default| match properties.iter().find(|property| property.name == field) {
        Some(property) => checked_u16(name, field, &property.value),
        None => Ok(default),
    };
    let flow = flow.ok_or_else(|| SourceLoweringError::MissingRealization(name.to_owned()))?;
    match kind {
        "plasmid" => {
            let components = symbols("components", &["Part", "Plasmid"])?;
            let chemistry = AssemblyChemistryIntent {
                reaction_volume_ul: quantity("reaction_volume", "uL", 20)?,
                part_volume_ul: quantity("part_volume", "uL", 2)?,
                enzyme_volume_ul: quantity("enzyme_volume", "uL", 2)?,
                ligase_volume_ul: quantity("ligase_volume", "uL", 4)?,
                buffer_volume_ul: quantity("buffer_volume", "uL", 2)?,
                cycles: whole("assembly_cycles", 75)?,
                digest_temperature_c: quantity("digest_temperature", "C", 37)?,
                digest_minutes: quantity("digest_duration", "min", 2)?,
                ligate_temperature_c: quantity("ligate_temperature", "C", 16)?,
                ligate_minutes: quantity("ligate_duration", "min", 5)?,
            };
            // The backbone joins the reaction alongside every component.
            if chemistry.water_volume_ul(1 + components.len()).is_none() {
                return Err(SourceLoweringError::UnbalancedReaction {
                    artifact: name.to_owned(),
                    reaction_volume_ul: chemistry.reaction_volume_ul,
                });
            }
            Ok(BuildArtifactIntent::Plasmid(PlasmidArtifactIntent {
                name: name.to_owned(),
                sequence: checked_dna(name, find("sequence")?)?,
                dependencies: flow.dependencies.clone(),
                recipe: AssemblyRecipeIntent {
                    backbone: symbol("backbone", &["Backbone"])?,
                    components,
                    restriction_enzyme: symbol("restriction_enzyme", &["RestrictionEnzyme"])?,
                    assembly_replicates: count("assembly_replicates", 1)?,
                    chemistry,
                },
                actions: flow.actions.clone(),
            }))
        }
        "strain" => Ok(BuildArtifactIntent::Strain(StrainArtifactIntent {
            name: name.to_owned(),
            chassis: symbol("chassis", &["Chassis"])?,
            plasmids: symbols("plasmids", &["Plasmid"])?,
            dependencies: flow.dependencies.clone(),
            selection: symbol("selection", &["Antibiotic"])?,
            transformation_replicates: count("transformation_replicates", 2)?,
            plating_replicates: count("plating_replicates", 2)?,
            serial_dilutions: count("serial_dilutions", 2)?,
            chemistry: StrainChemistryIntent {
                cell_volume_ul: quantity("cell_volume", "uL", 20)?,
                dna_volume_ul: quantity("dna_volume", "uL", 2)?,
                recovery_volume_ul: quantity("recovery_volume", "uL", 60)?,
                cold_minutes: quantity("cold_incubation", "min", 30)?,
                heat_shock_temperature_c: quantity("heat_shock_temperature", "C", 42)?,
                heat_shock_minutes: quantity("heat_shock_duration", "min", 1)?,
                recovery_temperature_c: quantity("recovery_temperature", "C", 37)?,
                recovery_minutes: quantity("recovery_duration", "min", 60)?,
                medium_volume_ul: quantity("medium_volume", "uL", 18)?,
                culture_volume_ul: quantity("culture_volume", "uL", 2)?,
                colony_volume_ul: quantity("colony_volume", "uL", 4)?,
            },
            actions: flow.actions.clone(),
        })),
        // This backend builds plasmids and strains. A package may declare other
        // kinds; a target that does not know how to make one says so.
        other => Err(SourceLoweringError::UnsupportedArtifactKind {
            artifact: name.to_owned(),
            kind: other.to_owned(),
        }),
    }
}

/// What each catalogued item states about itself.
///
/// An enzyme's working temperature and a chassis's heat shock belong to the
/// item rather than to every design that names it, so a design that says
/// nothing about them still gets them.
fn inventory_properties(
    modules: &[&CheckedModule],
) -> BTreeMap<String, Vec<lab_language::CheckedProperty>> {
    declarations(modules)
        .filter_map(|declaration| match declaration {
            CheckedDeclaration::Catalog {
                name, properties, ..
            } if !properties.is_empty() => Some((name.clone(), properties.clone())),
            _ => None,
        })
        .collect()
}

/// What each catalogued symbol calls the item a supplier lists.
///
/// A catalog declaration carries both, so this reads two fields rather than
/// recognizing the shape of a synthesized call.
fn inventory_identities(modules: &[&CheckedModule]) -> BTreeMap<String, String> {
    declarations(modules)
        .filter_map(|declaration| match declaration {
            CheckedDeclaration::Catalog { name, identity, .. } => {
                Some((name.clone(), identity.clone()))
            }
            _ => None,
        })
        .collect()
}

fn realization_flows(
    modules: &[&CheckedModule],
    identities: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, RealizationFlow>, SourceLoweringError> {
    let mut result = BTreeMap::new();
    for declaration in declarations(modules) {
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
                    let [product] = names.as_slice() else {
                        return Err(invalid_results(&design, action));
                    };
                    dependencies = Some(dependency_names(
                        action,
                        "dependencies",
                        &bindings,
                        &design,
                    )?);
                    actions.push(WorkflowActionIntent::Realize {
                        product: product.clone(),
                    });
                }
                "std.lab.plasmid.provision" => {
                    let names = result_names();
                    let [cells] = names.as_slice() else {
                        return Err(invalid_results(&design, action));
                    };
                    let item = resolved_reference(action, "item", identities, &design)?;
                    actions.push(WorkflowActionIntent::Provision {
                        cells: cells.clone(),
                        item,
                    });
                }
                "std.lab.plasmid.transform" => {
                    let names = result_names();
                    let [strain, culture] = names.as_slice() else {
                        return Err(invalid_results(&design, action));
                    };
                    dependencies = Some(dependency_names(action, "plasmids", &bindings, &design)?);
                    actions.push(WorkflowActionIntent::Transform {
                        strain: strain.clone(),
                        culture: culture.clone(),
                        cells: required_reference(action, "cells", &design)?,
                    });
                }
                "std.lab.plasmid.recover" => {
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
                "std.lab.plasmid.dilute" => {
                    let names = result_names();
                    let [culture] = names.as_slice() else {
                        return Err(invalid_results(&design, action));
                    };
                    actions.push(WorkflowActionIntent::Dilute {
                        culture: culture.clone(),
                        input: required_reference(action, "culture", &design)?,
                    });
                }
                "std.lab.plasmid.plate" => {
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

/// The artifact a workflow realizes, taken from whichever operation brings the
/// artifact into existence: assembly for a plasmid, transformation for a
/// strain. A workflow that realizes nothing is not a build.
fn realized_design(body: &[CheckedStatement]) -> Option<String> {
    body.iter().find_map(|statement| {
        let CheckedStatement::Effect { action, .. } = statement else {
            return None;
        };
        matches!(
            action.operation.as_str(),
            "std.bio.build.realize" | "std.lab.plasmid.transform"
        )
        .then(|| action_argument(action, "design").and_then(reference_name))
        .flatten()
        .map(str::to_owned)
    })
}

/// The artifacts whose materials a realization consumes. Dependencies are
/// dataflow, so a build order never depends on declaration names or text order.
///
/// The operand is either a name bound to a list, or the empty list the checker
/// supplies when the source leaves the dependency clause out entirely.
fn dependency_names(
    action: &ResolvedAction,
    argument: &'static str,
    bindings: &BTreeMap<String, &TypedExpression>,
    design: &str,
) -> Result<Vec<String>, SourceLoweringError> {
    let operand = action_argument(action, argument)
        .ok_or_else(|| SourceLoweringError::InvalidDependencyFlow(design.to_owned()))?;
    let binding = match reference_name(operand) {
        Some(name) => bindings
            .get(name)
            .copied()
            .ok_or_else(|| SourceLoweringError::InvalidDependencyFlow(design.to_owned()))?,
        None => operand,
    };
    let CheckedExpression::List { elements } = &binding.value else {
        return Err(SourceLoweringError::InvalidDependencyFlow(
            design.to_owned(),
        ));
    };
    elements
        .iter()
        .map(|element| {
            reference_name(element)
                .map(str::to_owned)
                .ok_or_else(|| SourceLoweringError::InvalidDependencyFlow(design.to_owned()))
        })
        .collect()
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

fn checked_u16(
    artifact: &str,
    field: &'static str,
    expression: &TypedExpression,
) -> Result<u16, SourceLoweringError> {
    let CheckedExpression::Integer { value } = expression.value else {
        return Err(invalid_field(artifact, field));
    };
    u16::try_from(value).map_err(|_| SourceLoweringError::CountOverflow {
        artifact: artifact.to_owned(),
        field,
    })
}

/// A chemistry property written as a quantity literal, such as `20 uL`. The
/// unit is checked rather than assumed, so `20 mL` is a diagnostic instead of a
/// thousandfold error on the bench.
fn checked_quantity(
    artifact: &str,
    field: &'static str,
    expression: &TypedExpression,
    expected: &'static str,
) -> Result<u16, SourceLoweringError> {
    let CheckedExpression::Quantity { magnitude, unit } = &expression.value else {
        return Err(invalid_field(artifact, field));
    };
    if unit != expected {
        return Err(SourceLoweringError::WrongUnit {
            artifact: artifact.to_owned(),
            field,
            expected,
            found: unit.clone(),
        });
    }
    magnitude
        .parse::<u16>()
        .map_err(|_| invalid_field(artifact, field))
}

fn invalid_field(artifact: &str, field: &'static str) -> SourceLoweringError {
    SourceLoweringError::InvalidField {
        artifact: artifact.to_owned(),
        field,
    }
}
