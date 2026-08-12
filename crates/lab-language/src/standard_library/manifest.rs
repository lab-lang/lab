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

use serde::Serialize;

use super::catalog::StandardLibrary;
use crate::checked::CheckedType;
use crate::semantics::{ExportKind, ModuleExport, ModuleInterface};

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
        name: String,
        documentation: String,
        produces: String,
        fields: Vec<Field>,
        /// Which combinations of stated properties are complete, as prose.
        declares: Option<String>,
    },
    Type {
        name: String,
        documentation: String,
        parameters: usize,
        roles: Vec<String>,
        fields: Vec<Field>,
    },
    /// A part types can play. It has no values, so it may bound a type
    /// parameter and may never be the type of anything.
    Role { name: String, documentation: String },
    Value {
        name: String,
        documentation: String,
        r#type: String,
    },
    Function {
        name: String,
        documentation: String,
        parameters: Vec<String>,
        result: String,
    },
    Constructor {
        name: String,
        documentation: String,
        fields: Vec<Field>,
        result: String,
    },
    /// A durable effect, and the phrase that performs it. The phrase is the
    /// words and operand names in the order they are written.
    Action {
        name: String,
        documentation: String,
        phrase: Vec<String>,
        results: Vec<Field>,
    },
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
                name: spec.name.to_owned(),
                documentation: spec.documentation.to_owned(),
            }
        } else {
            Export::Type {
                name: spec.name.to_owned(),
                documentation: spec.documentation.to_owned(),
                parameters: spec.parameters,
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
            name: (*name).to_owned(),
            documentation: String::new(),
            r#type: ty.to_string(),
        });
    }
    for function in &module.functions {
        exports.push(Export::Function {
            name: function.name.to_owned(),
            documentation: function.documentation.to_owned(),
            parameters: function
                .parameters
                .iter()
                .map(ToString::to_string)
                .collect(),
            result: function.result.to_string(),
        });
    }
    for constructor in &module.constructors {
        exports.push(Export::Constructor {
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
        exports.push(Export::Action {
            name: action
                .source_name()
                .expect("catalog validation guarantees an action source name")
                .to_owned(),
            documentation: String::new(),
            phrase: action
                .phrase
                .iter()
                .flat_map(super::contract::PhrasePart::parts)
                .map(phrase_word)
                .collect(),
            results: action
                .results
                .iter()
                .map(|result| Field {
                    name: result.name.to_owned(),
                    r#type: format!("{:?}", result.r#type),
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
                name: name.to_owned(),
                documentation,
                produces: schema.produces.display_name(),
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
            name: name.to_owned(),
            documentation,
        }),
        ExportKind::Type => Some(Export::Type {
            name: name.to_owned(),
            documentation,
            parameters: export.parameters.names.len(),
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
            name: name.to_owned(),
            documentation,
            r#type: export
                .r#type
                .as_ref()
                .map_or_else(String::new, CheckedType::display_name),
        }),
        ExportKind::Function | ExportKind::Workflow => {
            let callable = export.callable.as_ref()?;
            Some(Export::Function {
                name: name.to_owned(),
                documentation,
                parameters: callable
                    .inputs
                    .iter()
                    .map(CheckedType::display_name)
                    .collect(),
                result: callable
                    .outputs
                    .first()
                    .map_or_else(String::new, |output| output.r#type.display_name()),
            })
        }
        ExportKind::Action => None,
    }
}
