//! Import resolution, declaration-signature collection, and the checks for
//! declaration-shaped items: circuits, data types, and artifacts (including
//! the plasmid/strain shape rules).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::ast::{
    ArtifactDecl, ArtifactKind, ArtifactMember, CircuitDecl, DataDecl, DataKind, FieldDecl, Item,
    Module, TypeExpr, WorkflowOutputs,
};
use crate::checked::{CheckedCase, CheckedDeclaration, CheckedProperty, CheckedSection};
use crate::semantic_error::SemanticError;
use crate::source::Span;
use crate::type_system::{Ty, compatible, to_checked_type};

use super::Checker;
use super::context::{CircuitSignature, DataSignature, WorkflowSignature};

impl Checker {
    pub fn resolve_imports(&mut self, module: &Module) -> Result<(), SemanticError> {
        let preludes = self
            .standard_library
            .prelude_modules()
            .cloned()
            .collect::<Vec<_>>();
        for prelude in preludes {
            self.register_standard_module(prelude, Span::at(0))?;
        }

        for item in &module.items {
            let Item::Use(import) = item else {
                continue;
            };
            let path = super::path_text(&import.path);
            if let Some(standard) = self.standard_library.module(&path).cloned() {
                self.insert_import(standard.path, "builtin-standard-library", import.span)?;
                self.register_standard_module(standard, import.span)?;
            } else if let Some(interface) = self.provided_modules.module(&path).cloned() {
                self.insert_import(&path, "package", import.span)?;
                self.register_module_interface(&interface, import.span)?;
            } else {
                return Err(SemanticError::new(
                    import.span,
                    format!("module '{path}' cannot be resolved"),
                ));
            }
        }
        Ok(())
    }

    pub fn collect_declarations(&mut self, module: &Module) -> Result<(), SemanticError> {
        let mut names = BTreeSet::new();
        for item in &module.items {
            let (name, span) = match item {
                Item::Use(_) => continue,
                Item::Circuit(value) => (&value.name.value, value.name.span),
                Item::Artifact(value) => (&value.name.value, value.name.span),
                Item::Data(value) => (&value.name.value, value.name.span),
                Item::Workflow(value) => (&value.name.value, value.name.span),
                Item::Binding(value) => (&value.names[0].value, value.names[0].span),
            };
            if !names.insert(name.clone()) || self.imported_names.contains_key(name) {
                return Err(SemanticError::new(
                    span,
                    format!("duplicate declaration '{name}'"),
                ));
            }
            if matches!(item, Item::Data(_)) && !self.known_types.insert(name.clone()) {
                return Err(SemanticError::new(
                    span,
                    format!("type '{name}' is already defined"),
                ));
            }
        }

        for item in &module.items {
            match item {
                Item::Circuit(declaration) => {
                    let parameters = declaration
                        .parameters
                        .iter()
                        .map(|parameter| parameter.name.value.clone())
                        .collect::<Vec<_>>();
                    let generic_names = parameters.iter().cloned().collect::<BTreeSet<_>>();
                    let bounds = declaration
                        .parameters
                        .iter()
                        .filter_map(|parameter| {
                            parameter.bound.as_ref().map(|bound| {
                                self.lower_type(bound, &generic_names)
                                    .map(|bound| (parameter.name.value.clone(), bound))
                            })
                        })
                        .collect::<Result<HashMap<_, _>, _>>()?;
                    let inputs = declaration
                        .inputs
                        .iter()
                        .map(|field| self.lower_type(&field.ty, &generic_names))
                        .collect::<Result<Vec<_>, _>>()?;
                    let output = declaration.output.as_ref().ok_or_else(|| {
                        SemanticError::new(declaration.span, "circuit requires an output type")
                    })?;
                    let output = self.lower_type(output, &generic_names)?;
                    self.circuits.insert(
                        declaration.name.value.clone(),
                        CircuitSignature {
                            parameters,
                            bounds,
                            inputs,
                            output,
                        },
                    );
                }
                Item::Data(declaration) => {
                    let generic_names = declaration
                        .parameters
                        .iter()
                        .map(|parameter| parameter.name.value.clone())
                        .collect::<BTreeSet<_>>();
                    let fields = self.collect_fields(&declaration.fields, &generic_names)?;
                    let mut cases = BTreeMap::new();
                    for case in &declaration.cases {
                        if self
                            .cases
                            .insert(case.name.value.clone(), declaration.name.value.clone())
                            .is_some()
                        {
                            return Err(SemanticError::new(
                                case.name.span,
                                format!("duplicate outcome case '{}'", case.name.value),
                            ));
                        }
                        let case_fields = self.collect_fields(&case.fields, &generic_names)?;
                        if cases.insert(case.name.value.clone(), case_fields).is_some() {
                            return Err(SemanticError::new(case.name.span, "duplicate case"));
                        }
                    }
                    self.data.insert(
                        declaration.name.value.clone(),
                        DataSignature {
                            kind: declaration.kind,
                            fields,
                            cases,
                        },
                    );
                    if declaration.kind == DataKind::Event {
                        self.event_types.insert(declaration.name.value.clone());
                    }
                }
                Item::Workflow(declaration) => {
                    let inputs = declaration
                        .inputs
                        .iter()
                        .map(|field| self.lower_type(&field.ty, &BTreeSet::new()))
                        .collect::<Result<Vec<_>, _>>()?;
                    let outputs = self.lower_workflow_outputs(&declaration.outputs)?;
                    self.workflows.insert(
                        declaration.name.value.clone(),
                        WorkflowSignature { inputs, outputs },
                    );
                }
                Item::Artifact(declaration) => {
                    self.values.insert(
                        declaration.name.value.clone(),
                        Ty::named(declaration.kind.type_name()),
                    );
                }
                Item::Use(_) | Item::Binding(_) => {}
            }
        }
        Ok(())
    }

    pub fn collect_fields(
        &self,
        fields: &[FieldDecl],
        generics: &BTreeSet<String>,
    ) -> Result<BTreeMap<String, Ty>, SemanticError> {
        let mut result = BTreeMap::new();
        for field in fields {
            let ty = self.lower_type(&field.ty, generics)?;
            if result.insert(field.name.value.clone(), ty).is_some() {
                return Err(SemanticError::new(
                    field.name.span,
                    format!("duplicate field '{}'", field.name.value),
                ));
            }
        }
        Ok(result)
    }

    pub fn lower_workflow_outputs(
        &self,
        outputs: &WorkflowOutputs,
    ) -> Result<Vec<(String, Ty)>, SemanticError> {
        match outputs {
            WorkflowOutputs::Single { ty } => Ok(vec![(
                "outcome".to_owned(),
                self.lower_type(ty, &BTreeSet::new())?,
            )]),
            WorkflowOutputs::Named { fields } => {
                let mut names = BTreeSet::new();
                fields
                    .iter()
                    .map(|field| {
                        if !names.insert(field.name.value.clone()) {
                            return Err(SemanticError::new(
                                field.name.span,
                                format!("duplicate workflow result '{}'", field.name.value),
                            ));
                        }
                        Ok((
                            field.name.value.clone(),
                            self.lower_type(&field.ty, &BTreeSet::new())?,
                        ))
                    })
                    .collect()
            }
        }
    }

    pub fn check_circuit(
        &self,
        declaration: &CircuitDecl,
    ) -> Result<CheckedDeclaration, SemanticError> {
        let signature = self
            .circuits
            .get(&declaration.name.value)
            .expect("circuit was collected");
        let mut environment = self.values.clone();
        for (field, ty) in declaration.inputs.iter().zip(&signature.inputs) {
            environment.insert(field.name.value.clone(), ty.clone());
        }
        let mut sections = Vec::new();
        for section in &declaration.sections {
            for expression in &section.entries {
                self.infer_expr(expression, &environment)?;
            }
            sections.push(CheckedSection {
                name: section.name.value.clone(),
                entries: section
                    .entries
                    .iter()
                    .map(|expression| self.lower_checked_expr(expression, &environment, None))
                    .collect::<Result<Vec<_>, _>>()?,
            });
        }
        Ok(CheckedDeclaration::Circuit {
            doc: declaration.doc.clone(),
            name: declaration.name.value.clone(),
            parameters: declaration
                .parameters
                .iter()
                .map(|parameter| parameter.name.value.clone())
                .collect(),
            inputs: declaration
                .inputs
                .iter()
                .zip(&signature.inputs)
                .map(|(field, ty)| super::checked_field(&field.name.value, ty))
                .collect(),
            output: to_checked_type(&signature.output),
            sections,
        })
    }

    pub fn checked_data(
        &self,
        declaration: &DataDecl,
    ) -> Result<CheckedDeclaration, SemanticError> {
        let signature = self
            .data
            .get(&declaration.name.value)
            .expect("data was collected");
        Ok(CheckedDeclaration::Data {
            doc: declaration.doc.clone(),
            category: format!("{:?}", declaration.kind).to_ascii_lowercase(),
            name: declaration.name.value.clone(),
            fields: signature
                .fields
                .iter()
                .map(|(name, ty)| super::checked_field(name, ty))
                .collect(),
            cases: signature
                .cases
                .iter()
                .map(|(name, fields)| CheckedCase {
                    name: name.clone(),
                    fields: fields
                        .iter()
                        .map(|(name, ty)| super::checked_field(name, ty))
                        .collect(),
                })
                .collect(),
        })
    }

    pub fn check_artifact(
        &self,
        declaration: &ArtifactDecl,
    ) -> Result<CheckedDeclaration, SemanticError> {
        let kind = declaration.kind;
        let keyword = kind.keyword();
        let mut environment = self.values.clone();
        environment.extend(
            self.standard_types
                .get(kind.type_name())
                .expect("the bundled prelude defines every artifact type")
                .fields
                .iter()
                .map(|(name, ty)| ((*name).to_owned(), ty.clone())),
        );
        let mut properties = Vec::new();
        let mut property_names = BTreeSet::new();
        let mut requirements = Vec::new();
        let mut acceptance = Vec::new();
        for member in &declaration.members {
            match member {
                ArtifactMember::Property(property) => {
                    if !property_names.insert(property.name.value.clone()) {
                        return Err(SemanticError::new(
                            property.name.span,
                            format!("duplicate {keyword} property '{}'", property.name.value),
                        ));
                    }
                    let inferred = self.infer_expr(&property.value, &environment)?;
                    if let Some(expected) = environment.get(&property.name.value)
                        && !compatible(&inferred, expected)
                    {
                        return Err(SemanticError::new(
                            property.value.span(),
                            format!(
                                "{keyword} property '{}' expects {expected}, found {inferred}",
                                property.name.value
                            ),
                        ));
                    }
                    environment.insert(property.name.value.clone(), inferred.clone());
                    properties.push(CheckedProperty {
                        name: property.name.value.clone(),
                        value: self.lower_checked_expr(
                            &property.value,
                            &environment,
                            Some(&inferred),
                        )?,
                    });
                }
                ArtifactMember::Requirement(claim) => {
                    self.require_bool(&claim.predicate, &environment, "require")?;
                    requirements.push(self.lower_checked_expr(
                        &claim.predicate,
                        &environment,
                        None,
                    )?);
                }
                ArtifactMember::Acceptance(claim) => {
                    self.require_bool(&claim.predicate, &environment, "accept")?;
                    acceptance.push(self.lower_checked_expr(
                        &claim.predicate,
                        &environment,
                        None,
                    )?);
                }
                ArtifactMember::Section(section) => {
                    return Err(SemanticError::new(
                        section.span,
                        format!(
                            "{keyword} section '{}' has no semantics yet",
                            section.name.value
                        ),
                    ));
                }
            }
        }
        match kind {
            ArtifactKind::Plasmid => {
                self.check_plasmid_shape(declaration, &property_names, &environment)?
            }
            ArtifactKind::Strain => self.check_strain_shape(declaration, &property_names)?,
        }
        Ok(CheckedDeclaration::Artifact {
            doc: declaration.doc.clone(),
            artifact: kind,
            name: declaration.name.value.clone(),
            properties,
            requirements,
            acceptance,
        })
    }

    /// A plasmid states its sequence directly, or states the backbone and cargo
    /// a sequence can be derived from.
    pub fn check_plasmid_shape(
        &self,
        declaration: &ArtifactDecl,
        property_names: &BTreeSet<String>,
        environment: &HashMap<String, Ty>,
    ) -> Result<(), SemanticError> {
        let has_direct_sequence = property_names.contains("sequence");
        if !has_direct_sequence
            && (!environment.contains_key("backbone") || !environment.contains_key("cargo"))
        {
            return Err(SemanticError::new(
                declaration.span,
                "plasmid requires either a sequence property or backbone and cargo properties",
            ));
        }
        if !has_direct_sequence
            && let Some(backbone) = environment.get("backbone")
            && !compatible(backbone, &Ty::named("Backbone"))
        {
            return Err(SemanticError::new(
                declaration.span,
                format!("plasmid backbone must be Backbone, found {backbone}"),
            ));
        }
        if let Some(cargo) = environment.get("cargo")
            && !matches!(cargo, Ty::Named(name, _) if name == "Circuit")
        {
            return Err(SemanticError::new(
                declaration.span,
                format!("plasmid cargo must be Circuit, found {cargo}"),
            ));
        }
        Ok(())
    }

    /// A strain names the chassis it is built in and the plasmid designs it
    /// carries. Both are properties rather than derived facts, because a strain
    /// carrying no plasmid is a different artifact from one that does.
    pub fn check_strain_shape(
        &self,
        declaration: &ArtifactDecl,
        property_names: &BTreeSet<String>,
    ) -> Result<(), SemanticError> {
        for required in ["chassis", "plasmids"] {
            if !property_names.contains(required) {
                return Err(SemanticError::new(
                    declaration.span,
                    format!("strain requires a '{required}' property"),
                ));
            }
        }
        Ok(())
    }

    pub fn lower_type(
        &self,
        expression: &TypeExpr,
        generics: &BTreeSet<String>,
    ) -> Result<Ty, SemanticError> {
        match expression {
            TypeExpr::Path {
                path,
                arguments,
                span,
            } => {
                let name = super::path_text(path);
                if !self.known_types.contains(&name) && !generics.contains(&name) {
                    return Err(SemanticError::new(*span, format!("unknown type '{name}'")));
                }
                let arguments = arguments
                    .iter()
                    .map(|argument| self.lower_type(argument, generics))
                    .collect::<Result<Vec<_>, _>>()?;
                let expected_arity = self.standard_types.get(&name).map(|spec| spec.parameters);
                if let Some(expected) = expected_arity
                    && arguments.len() != expected
                {
                    return Err(SemanticError::new(
                        *span,
                        format!(
                            "type '{name}' expects {expected} argument(s), found {}",
                            arguments.len()
                        ),
                    ));
                }
                Ok(match name.as_str() {
                    "Integer" => Ty::Integer,
                    "String" => Ty::String,
                    "Bool" => Ty::Bool,
                    "None" => Ty::None,
                    "List" if arguments.len() == 1 => Ty::List(Box::new(arguments[0].clone())),
                    _ => Ty::Named(name, arguments),
                })
            }
            TypeExpr::Union { alternatives, .. } => Ok(Ty::Union(
                alternatives
                    .iter()
                    .map(|alternative| self.lower_type(alternative, generics))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
        }
    }
}
