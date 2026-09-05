//! A machine-readable description of the bundled standard library.
//!
//! Tooling outside the compiler needs to know which words each standard module
//! supplies: an editor to complete them, a host-language SDK to mirror them.
//! This is derived from the same catalog name resolution reads, so a module
//! cannot offer one vocabulary to a Lab program and another to a tool.
//!
//! Types appear in their display form rather than as structured type IR. A
//! consumer of this manifest is presenting names to a person, and the structure
//! it would need to do more than that is the checker's own.

use std::collections::BTreeMap;

use serde::Serialize;

use super::catalog::StandardLibrary;
use crate::checked::CheckedType;
use crate::semantics::{DefinitionId, ExportKind, ModuleExport, ModuleInterface};

/// Every bundled standard module, in path order.
#[derive(Clone, Debug, Serialize)]
pub struct Library {
    pub modules: Vec<Module>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Module {
    pub path: String,
    /// Whether every module imports this one without saying so.
    pub prelude: bool,
    pub documentation: String,
    /// The modules this one imports, in the order it writes them.
    ///
    /// A schema is contributed to by several modules, so importing one word
    /// can require importing the module that declared what the word extends.
    /// A consumer cannot work that out from the exports alone.
    pub imports: Vec<String>,
    pub exports: Vec<Export>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Export {
    /// A kind of artifact, named by the word its instances are written with.
    /// `produces` is the type those instances have, which is the name a
    /// workflow writes and the name a schema's rules read fields from.
    ArtifactKind {
        definition: DefinitionId,
        name: String,
        documentation: String,
        produces: String,
        roles: Vec<String>,
        fields: Vec<Field>,
        /// Which combinations of stated properties are complete, as prose.
        declares: Option<String>,
    },
    Type {
        definition: DefinitionId,
        name: String,
        documentation: String,
        parameters: Vec<TypeParameter>,
        roles: Vec<String>,
        fields: Vec<Field>,
    },
    /// A part types can play. It has no values, so it may bound a type
    /// parameter and may never be the type of anything.
    Role {
        definition: DefinitionId,
        name: String,
        documentation: String,
    },
    /// How a kind's materials are classified by the state they are in.
    Facet {
        definition: DefinitionId,
        name: String,
        documentation: String,
        subject: String,
        states: Vec<String>,
    },
    Value {
        definition: DefinitionId,
        name: String,
        documentation: String,
        r#type: String,
    },
    Function {
        definition: DefinitionId,
        name: String,
        documentation: String,
        parameters: Vec<TypeParameter>,
        inputs: Vec<String>,
        result: String,
    },
    Constructor {
        definition: DefinitionId,
        name: String,
        documentation: String,
        fields: Vec<Field>,
        result: String,
    },
    /// A durable effect, and the phrase that performs it. The phrase is the
    /// words and operand names in the order they are written.
    ///
    /// A clause a caller may leave out is written into the phrase in its
    /// place and repeated in `optional`, because a reader of the phrase alone
    /// cannot tell that `realize <design> from <dependencies>` performs
    /// without the `from`.
    Action {
        definition: DefinitionId,
        name: String,
        documentation: String,
        parameters: Vec<TypeParameter>,
        operation: String,
        phrase: Vec<String>,
        operands: Vec<Field>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        optional: Vec<Vec<String>>,
        results: Vec<Field>,
    },
    /// A durable workflow exported by a Lab-authored module. Unlike a pure
    /// function, a workflow has named inputs and may return several named
    /// results, all of which a generated SDK signature must preserve.
    Workflow {
        definition: DefinitionId,
        name: String,
        documentation: String,
        parameters: Vec<TypeParameter>,
        inputs: Vec<Field>,
        results: Vec<Field>,
    },
}

/// One generic parameter in declaration order, including its optional role or
/// type bound exactly as the checked public interface exposes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TypeParameter {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bound: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Field {
    pub name: String,
    pub r#type: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
}

/// Describe the bundled standard library.
pub(crate) fn library() -> Library {
    let bundled = StandardLibrary::bundled();
    let mut modules: Vec<Module> = bundled
        .native_modules()
        .map(native_module)
        .chain(bundled.authored_interfaces().map(authored_module))
        .collect();
    modules.sort_by(|left, right| left.path.cmp(&right.path));
    Library { modules }
}

fn native_module(module: &super::catalog::StandardModule) -> Module {
    let mut exports = Vec::new();
    for spec in &module.types {
        exports.push(if spec.role {
            Export::Role {
                definition: DefinitionId::exported(module.path, spec.name),
                name: spec.name.to_owned(),
                documentation: spec.documentation.to_owned(),
            }
        } else {
            Export::Type {
                definition: DefinitionId::exported(module.path, spec.name),
                name: spec.name.to_owned(),
                documentation: spec.documentation.to_owned(),
                parameters: spec
                    .parameters
                    .iter()
                    .map(|parameter| TypeParameter {
                        name: parameter.name.to_owned(),
                        bound: parameter.bound.as_ref().map(ToString::to_string),
                    })
                    .collect(),
                roles: spec
                    .implements
                    .iter()
                    .map(|role| (*role).to_owned())
                    .collect(),
                fields: spec
                    .fields
                    .iter()
                    .map(|(name, ty)| Field {
                        name: (*name).to_owned(),
                        r#type: ty.to_string(),
                        optional: false,
                    })
                    .collect(),
            }
        });
    }
    for (name, ty) in &module.values {
        exports.push(Export::Value {
            definition: DefinitionId::exported(module.path, *name),
            name: (*name).to_owned(),
            documentation: String::new(),
            r#type: ty.to_string(),
        });
    }
    for function in &module.functions {
        exports.push(Export::Function {
            definition: DefinitionId::exported(module.path, function.name),
            name: function.name.to_owned(),
            documentation: function.documentation.to_owned(),
            parameters: Vec::new(),
            inputs: function
                .parameters
                .iter()
                .map(ToString::to_string)
                .collect(),
            result: function.result.to_string(),
        });
    }
    for constructor in &module.constructors {
        exports.push(Export::Constructor {
            definition: DefinitionId::exported(module.path, constructor.name),
            name: constructor.name.to_owned(),
            documentation: constructor.documentation.to_owned(),
            fields: constructor
                .fields
                .iter()
                .map(|(name, ty)| Field {
                    name: (*name).to_owned(),
                    r#type: ty.to_string(),
                    optional: false,
                })
                .collect(),
            result: constructor.result.to_string(),
        });
    }
    for action in &module.actions {
        let mut parameters = Vec::new();
        let operands = action
            .phrase
            .iter()
            .flat_map(super::contract::PhrasePart::parts)
            .filter_map(|part| action_operand(part, &mut parameters))
            .collect::<Vec<_>>();
        let operand_types = operands
            .iter()
            .map(|operand| (operand.name.clone(), operand.r#type.clone()))
            .collect::<BTreeMap<_, _>>();
        exports.push(Export::Action {
            definition: DefinitionId::exported(
                module.path,
                action
                    .source_name()
                    .expect("catalog validation guarantees an action source name"),
            ),
            name: action
                .source_name()
                .expect("catalog validation guarantees an action source name")
                .to_owned(),
            documentation: String::new(),
            parameters,
            operation: action.operation.clone(),
            phrase: action
                .phrase
                .iter()
                .flat_map(super::contract::PhrasePart::parts)
                .map(phrase_word)
                .collect(),
            operands,
            optional: action
                .phrase
                .iter()
                .filter_map(|part| match part {
                    super::contract::PhrasePart::Optional(clause) => Some(
                        clause
                            .iter()
                            .flat_map(super::contract::PhrasePart::parts)
                            .map(phrase_word)
                            .collect(),
                    ),
                    _ => None,
                })
                .collect(),
            results: action
                .results
                .iter()
                .map(|result| Field {
                    name: result.name.to_owned(),
                    r#type: resolved_contract_type_name(&result.r#type, &operand_types),
                    optional: false,
                })
                .collect(),
        });
    }
    Module {
        path: module.path.to_owned(),
        prelude: module.prelude,
        documentation: module.documentation.to_owned(),
        imports: Vec::new(),
        exports,
    }
}

fn resolved_contract_type_name(
    r#type: &super::contract::ContractType,
    operands: &BTreeMap<String, String>,
) -> String {
    use super::contract::ContractType;

    match r#type {
        ContractType::SameAs(name) => operands
            .get(name.as_str())
            .cloned()
            .unwrap_or_else(|| "object".to_owned()),
        ContractType::MaterialOf(name) => operands
            .get(name.as_str())
            .map(|operand| {
                operand
                    .strip_prefix("Material<")
                    .and_then(|operand| operand.strip_suffix('>'))
                    .map_or_else(
                        || format!("Material<{operand}>"),
                        |subject| format!("Material<{subject}>"),
                    )
            })
            .unwrap_or_else(|| "Material<object>".to_owned()),
        other => contract_type_name(other),
    }
}

fn action_operand(
    part: &super::contract::PhrasePart,
    parameters: &mut Vec<TypeParameter>,
) -> Option<Field> {
    use super::contract::PhrasePart;

    let (name, r#type) = match part {
        PhrasePart::Word(_) | PhrasePart::Optional(_) => return None,
        PhrasePart::Operand { name, r#type, .. } => {
            let r#type = match r#type {
                super::contract::ContractType::AnyValue => {
                    fresh_contract_type_parameter(parameters)
                }
                super::contract::ContractType::AnyMaterial => {
                    format!("Material<{}>", fresh_contract_type_parameter(parameters))
                }
                other => contract_type_name(other),
            };
            (name, r#type)
        }
        PhrasePart::Integer { name, .. } => (name, "Integer".to_owned()),
        PhrasePart::Quantity { name, units, .. } => {
            let unit = if units.len() == 1 {
                units[0].clone()
            } else {
                units.join(" | ")
            };
            (name, format!("Quantity<{unit}>"))
        }
    };
    Some(Field {
        name: name.clone(),
        r#type,
        optional: false,
    })
}

fn fresh_contract_type_parameter(parameters: &mut Vec<TypeParameter>) -> String {
    let name = if parameters.is_empty() {
        "T".to_owned()
    } else {
        format!("T{}", parameters.len() + 1)
    };
    parameters.push(TypeParameter {
        name: name.clone(),
        bound: None,
    });
    name
}

fn contract_type_name(r#type: &super::contract::ContractType) -> String {
    use super::contract::ContractType;

    match r#type {
        ContractType::Concrete(ty) => ty.to_string(),
        ContractType::SameAs(name) => format!("same as {name}"),
        ContractType::AnyMaterial => "Material<any Value>".to_owned(),
        ContractType::AnyValue => "any Value".to_owned(),
        ContractType::MaterialOf(name) => format!("Material<same as {name}>"),
    }
}

fn phrase_word(part: &super::contract::PhrasePart) -> String {
    match part {
        super::contract::PhrasePart::Word(word) => (*word).to_owned(),
        super::contract::PhrasePart::Operand { name, .. }
        | super::contract::PhrasePart::Integer { name, .. }
        | super::contract::PhrasePart::Quantity { name, .. } => format!("<{name}>"),
        super::contract::PhrasePart::Optional(_) => "...".to_owned(),
    }
}

fn authored_module((path, interface): (&&'static str, &ModuleInterface)) -> Module {
    Module {
        path: (*path).to_owned(),
        prelude: false,
        documentation: interface.documentation.clone(),
        imports: super::catalog::authored_imports(path),
        exports: interface
            .exports
            .iter()
            .filter_map(|(name, export)| authored_export(name, export))
            .collect(),
    }
}

fn authored_export(name: &str, export: &ModuleExport) -> Option<Export> {
    let documentation = export.documentation.clone();
    match export.kind {
        ExportKind::ArtifactKind => {
            let schema = export.schema.as_ref()?;
            Some(Export::ArtifactKind {
                definition: export.definition.clone(),
                name: name.to_owned(),
                documentation,
                produces: schema.produces.display_name(),
                roles: export.roles.clone(),
                fields: schema
                    .fields
                    .iter()
                    .map(|field| Field {
                        name: field.name.clone(),
                        r#type: field.r#type.display_name(),
                        optional: field.optional,
                    })
                    .collect(),
                declares: schema.declares.as_ref().map(|rule| rule.describe()),
            })
        }
        ExportKind::Role => Some(Export::Role {
            definition: export.definition.clone(),
            name: name.to_owned(),
            documentation,
        }),
        ExportKind::Type => Some(Export::Type {
            definition: export.definition.clone(),
            name: name.to_owned(),
            documentation,
            parameters: interface_type_parameters(&export.parameters),
            roles: export.roles.clone(),
            fields: export
                .fields
                .iter()
                .map(|(field, ty)| Field {
                    name: field.clone(),
                    r#type: ty.display_name(),
                    optional: false,
                })
                .collect(),
        }),
        ExportKind::Value | ExportKind::Constructor => Some(Export::Value {
            definition: export.definition.clone(),
            name: name.to_owned(),
            documentation,
            r#type: export
                .r#type
                .as_ref()
                .map_or_else(String::new, CheckedType::display_name),
        }),
        ExportKind::Function => {
            let callable = export.callable.as_ref()?;
            Some(Export::Function {
                definition: export.definition.clone(),
                name: name.to_owned(),
                documentation,
                parameters: interface_type_parameters(&export.parameters),
                inputs: callable
                    .inputs
                    .iter()
                    .map(|field| field.r#type.display_name())
                    .collect(),
                result: callable
                    .outputs
                    .first()
                    .map_or_else(String::new, |output| output.r#type.display_name()),
            })
        }
        ExportKind::Facet => {
            let surface = export.facet.as_ref()?;
            Some(Export::Facet {
                definition: export.definition.clone(),
                name: name.to_owned(),
                documentation,
                subject: surface.subject.display_name(),
                states: surface
                    .states
                    .iter()
                    .map(|state| state.name.clone())
                    .collect(),
            })
        }
        ExportKind::Action => {
            let surface = export.action.as_ref()?;
            Some(Export::Action {
                definition: export.definition.clone(),
                name: name.to_owned(),
                documentation,
                parameters: interface_type_parameters(&export.parameters),
                operation: surface.operation.clone(),
                phrase: surface
                    .phrase
                    .iter()
                    .map(|token| match token {
                        crate::checked::CheckedPhraseToken::Word(word) => word.clone(),
                        crate::checked::CheckedPhraseToken::Hole(operand) => {
                            format!("<{operand}>")
                        }
                    })
                    .collect(),
                operands: surface
                    .operands
                    .iter()
                    .map(|operand| Field {
                        name: operand.name.clone(),
                        r#type: operand.r#type.display_name(),
                        optional: false,
                    })
                    .collect(),
                optional: Vec::new(),
                results: surface
                    .results
                    .iter()
                    .map(|result| Field {
                        name: result.name.clone(),
                        r#type: result.r#type.display_name(),
                        optional: false,
                    })
                    .collect(),
            })
        }
        ExportKind::Workflow => {
            let callable = export.callable.as_ref()?;
            Some(Export::Workflow {
                definition: export.definition.clone(),
                name: name.to_owned(),
                documentation,
                parameters: interface_type_parameters(&export.parameters),
                inputs: callable
                    .inputs
                    .iter()
                    .map(|field| Field {
                        name: field.name.clone(),
                        r#type: field.r#type.display_name(),
                        optional: false,
                    })
                    .collect(),
                results: callable
                    .outputs
                    .iter()
                    .map(|field| Field {
                        name: field.name.clone(),
                        r#type: field.r#type.display_name(),
                        optional: false,
                    })
                    .collect(),
            })
        }
    }
}

fn interface_type_parameters(parameters: &crate::semantics::TypeParameters) -> Vec<TypeParameter> {
    parameters
        .names
        .iter()
        .map(|name| TypeParameter {
            name: name.clone(),
            bound: parameters.bounds.get(name).map(CheckedType::display_name),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ModuleId, compile_module_with_id};

    fn parameters(export: Export) -> Vec<TypeParameter> {
        match export {
            Export::Type { parameters, .. } | Export::Workflow { parameters, .. } => parameters,
            _ => panic!("expected a generic type or workflow export"),
        }
    }

    #[test]
    fn authored_type_and_workflow_parameters_keep_declaration_order_and_bounds() {
        let compiled = compile_module_with_id(
            ModuleId::new("generic.manifest"),
            r#"
role FirstRole
role SecondRole

record Pair<Right: SecondRole, Left: FirstRole>

workflow preserve(
  value: Circuit<Right: SecondRole, Left: FirstRole>,
) -> Circuit<Right, Left>:
  return value
"#,
        )
        .unwrap();
        let expected = vec![
            TypeParameter {
                name: "Right".to_owned(),
                bound: Some("SecondRole".to_owned()),
            },
            TypeParameter {
                name: "Left".to_owned(),
                bound: Some("FirstRole".to_owned()),
            },
        ];

        for name in ["Pair", "preserve"] {
            let export = compiled.interface.exports.get(name).unwrap();
            assert_eq!(parameters(authored_export(name, export).unwrap()), expected);
        }
    }
}
