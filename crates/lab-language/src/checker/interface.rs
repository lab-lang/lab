//! Construction of checked public module surfaces.

use std::collections::BTreeMap;

use crate::checked::{CheckedDeclaration, CheckedType};
use crate::semantics::{
    ActionSurface, CallableSignature, DefinitionId, ExportKind, FacetSurface, ModuleExport,
    ModuleId, ModuleInterface, TypeParameters,
};

pub(super) fn build_interface(
    module_id: &ModuleId,
    doc: Option<&str>,
    declarations: &[CheckedDeclaration],
) -> ModuleInterface {
    let mut interface = ModuleInterface::empty(module_id.clone());
    interface.documentation = doc.unwrap_or_default().to_owned();
    let insert = |interface: &mut ModuleInterface,
                  name: &str,
                  kind: ExportKind,
                  ty: Option<&CheckedType>,
                  callable: Option<CallableSignature>,
                  fields: BTreeMap<String, CheckedType>,
                  documentation: &Option<String>| {
        interface.exports.insert(
            name.to_owned(),
            ModuleExport {
                definition: DefinitionId::exported(module_id.as_str(), name),
                kind,
                r#type: ty.cloned(),
                callable,
                fields,
                roles: Vec::new(),
                term: None,
                schema: None,
                facet: None,
                action: None,
                parameters: TypeParameters::default(),
                documentation: documentation.clone().unwrap_or_default(),
            },
        );
    };

    /// Attach type parameters to an export that was just inserted. Types and
    /// callables carry them in the same place, so an importer reads them the
    /// same way whichever it is looking at.
    fn generic(
        interface: &mut ModuleInterface,
        name: &str,
        names: &[String],
        bounds: &BTreeMap<String, CheckedType>,
    ) {
        if names.is_empty() && bounds.is_empty() {
            return;
        }
        interface
            .exports
            .get_mut(name)
            .expect("the export was just inserted")
            .parameters = TypeParameters {
            names: names.to_vec(),
            bounds: bounds.clone(),
        };
    }

    for declaration in declarations {
        match declaration {
            CheckedDeclaration::Catalog {
                doc, name, r#type, ..
            } => insert(
                &mut interface,
                name,
                ExportKind::Value,
                Some(r#type),
                None,
                BTreeMap::new(),
                doc,
            ),
            CheckedDeclaration::Action {
                doc,
                name,
                operation,
                phrase,
                operands,
                results,
                capability,
            } => {
                insert(
                    &mut interface,
                    name,
                    ExportKind::Action,
                    None,
                    None,
                    BTreeMap::new(),
                    doc,
                );
                interface
                    .exports
                    .get_mut(name)
                    .expect("the action export was just inserted")
                    .action = Some(ActionSurface {
                    operation: operation.clone(),
                    phrase: phrase.clone(),
                    operands: operands.clone(),
                    results: results.clone(),
                    capability: capability.clone(),
                });
            }
            CheckedDeclaration::Facet {
                doc,
                name,
                subject,
                states,
                transitions,
            } => {
                insert(
                    &mut interface,
                    name,
                    ExportKind::Facet,
                    None,
                    None,
                    BTreeMap::new(),
                    doc,
                );
                interface
                    .exports
                    .get_mut(name)
                    .expect("the facet export was just inserted")
                    .facet = Some(FacetSurface {
                    subject: subject.clone(),
                    states: states.clone(),
                    transitions: transitions.clone(),
                });
            }
            CheckedDeclaration::Role { doc, name, term } => {
                insert(
                    &mut interface,
                    name,
                    ExportKind::Role,
                    None,
                    None,
                    BTreeMap::new(),
                    doc,
                );
                interface
                    .exports
                    .get_mut(name)
                    .expect("the role export was just inserted")
                    .term = term.clone();
            }
            CheckedDeclaration::Circuit {
                doc,
                name,
                parameters,
                bounds,
                inputs,
                output,
                ..
            } => {
                insert(
                    &mut interface,
                    name,
                    ExportKind::Function,
                    Some(output),
                    Some(CallableSignature {
                        inputs: inputs.iter().map(|field| field.r#type.clone()).collect(),
                        outputs: vec![crate::checked::CheckedField {
                            name: "output".to_owned(),
                            r#type: output.clone(),
                        }],
                    }),
                    BTreeMap::new(),
                    doc,
                );
                generic(&mut interface, name, parameters, bounds);
            }
            CheckedDeclaration::ArtifactKind {
                doc,
                name,
                produces,
                roles,
                fields,
                declares,
            } => {
                insert(
                    &mut interface,
                    name,
                    ExportKind::ArtifactKind,
                    None,
                    None,
                    BTreeMap::new(),
                    doc,
                );
                let export = interface
                    .exports
                    .get_mut(name)
                    .expect("the kind export was just inserted");
                export.roles = roles.clone();
                export.schema = Some(crate::semantics::ArtifactSchema {
                    produces: produces.clone(),
                    fields: fields.clone(),
                    declares: declares.clone(),
                });
            }
            CheckedDeclaration::Artifact {
                doc,
                produces,
                name,
                ..
            } => {
                let ty = produces.clone();
                insert(
                    &mut interface,
                    name,
                    ExportKind::Value,
                    Some(&ty),
                    None,
                    BTreeMap::new(),
                    doc,
                );
            }
            CheckedDeclaration::Data {
                doc,
                name,
                parameters,
                bounds,
                roles,
                fields,
                cases,
                ..
            } => {
                let ty = CheckedType::Named {
                    name: name.clone(),
                    arguments: Vec::new(),
                };
                let base_fields = fields
                    .iter()
                    .map(|field| (field.name.clone(), field.r#type.clone()))
                    .collect::<BTreeMap<_, _>>();
                insert(
                    &mut interface,
                    name,
                    ExportKind::Type,
                    Some(&ty),
                    None,
                    base_fields.clone(),
                    doc,
                );
                // Roles and parameters belong to the type itself, not to its
                // constructors.
                interface
                    .exports
                    .get_mut(name)
                    .expect("the type export was just inserted")
                    .roles = roles.clone();
                generic(&mut interface, name, parameters, bounds);
                for case in cases {
                    let mut fields = base_fields.clone();
                    fields.extend(
                        case.fields
                            .iter()
                            .map(|field| (field.name.clone(), field.r#type.clone())),
                    );
                    insert(
                        &mut interface,
                        &case.name,
                        ExportKind::Constructor,
                        Some(&ty),
                        None,
                        fields,
                        doc,
                    );
                }
            }
            CheckedDeclaration::Workflow {
                doc,
                name,
                parameters,
                bounds,
                inputs,
                outputs,
                ..
            } => {
                insert(
                    &mut interface,
                    name,
                    ExportKind::Workflow,
                    outputs
                        .first()
                        .filter(|_| outputs.len() == 1)
                        .map(|field| &field.r#type),
                    Some(CallableSignature {
                        inputs: inputs.iter().map(|field| field.r#type.clone()).collect(),
                        outputs: outputs.clone(),
                    }),
                    BTreeMap::new(),
                    doc,
                );
                generic(&mut interface, name, parameters, bounds);
            }
            CheckedDeclaration::Binding(binding) => {
                for target in &binding.targets {
                    insert(
                        &mut interface,
                        &target.name,
                        ExportKind::Value,
                        Some(&target.r#type),
                        None,
                        BTreeMap::new(),
                        &binding.doc,
                    );
                }
            }
        }
    }
    interface
}
