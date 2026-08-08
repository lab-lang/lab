//! Import resolution, declaration-signature collection, and the checks for
//! declaration-shaped items: circuits, data types, and artifacts (including
//! the plasmid/strain shape rules).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::ast::{
    ArtifactDecl, ArtifactKind, ArtifactMember, CircuitDecl, DataDecl, DataKind, FieldDecl, Item,
    Module, Path, TypeArgument, TypeExpr, WorkflowOutputs,
};
use crate::checked::{CheckedCase, CheckedDeclaration, CheckedProperty, CheckedSection};
use crate::semantic_error::SemanticError;
use crate::source::{Identifier, Span};
use crate::type_system::{Ty, to_checked_type};

use super::Checker;
use super::context::{CircuitSignature, DataSignature, Generics, WorkflowSignature};

/// Where a type parameter's name appears in a signature, in source order.
enum Mention<'a> {
    Bind {
        name: &'a Identifier,
        role: &'a Path,
        span: Span,
    },
    Use {
        name: &'a str,
        span: Span,
    },
}

/// Walk a type in source order, recording every place a bare name is used and
/// every place an argument introduces one.
fn collect_mentions<'a>(ty: &'a TypeExpr, out: &mut Vec<Mention<'a>>) {
    match ty {
        TypeExpr::Path {
            path,
            arguments,
            span,
        } => {
            if arguments.is_empty()
                && let [segment] = path.segments.as_slice()
            {
                out.push(Mention::Use {
                    name: &segment.value,
                    span: *span,
                });
            }
            for argument in arguments {
                match argument {
                    TypeArgument::Binding { name, role, span } => {
                        out.push(Mention::Bind {
                            name,
                            role,
                            span: *span,
                        });
                    }
                    // A forgotten argument introduces no name and refers to
                    // none, so it takes no part in signature scoping.
                    TypeArgument::Any { .. } => {}
                    TypeArgument::Type(ty) => collect_mentions(ty, out),
                }
            }
        }
        TypeExpr::Union { alternatives, .. } => {
            for alternative in alternatives {
                collect_mentions(alternative, out);
            }
        }
    }
}

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
            } else if let Some(interface) = self.standard_library.authored_module(&path).cloned() {
                // A standard module written in Lab resolves through the same
                // checked interface a package module does.
                self.insert_import(&path, "builtin-standard-library", import.span)?;
                self.register_module_interface(&interface, import.span)?;
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
        let mut names: BTreeMap<String, Span> = BTreeMap::new();
        for item in &module.items {
            let (name, span) = match item {
                Item::Use(_) => continue,
                Item::Role(value) => (&value.name.value, value.name.span),
                Item::Circuit(value) => (&value.name.value, value.name.span),
                Item::Artifact(value) => (&value.name.value, value.name.span),
                Item::Data(value) => (&value.name.value, value.name.span),
                Item::Workflow(value) => (&value.name.value, value.name.span),
                Item::Binding(value) => (&value.names[0].value, value.names[0].span),
            };
            // A name taken twice in one file and a name taken from an import are
            // different mistakes with different fixes, so they are different
            // diagnostics.
            if let Some(first) = names.get(name) {
                return Err(
                    SemanticError::new(span, format!("duplicate declaration '{name}'"))
                        .related(*first, format!("'{name}' is already declared here")),
                );
            }
            if let Some(provider) = self.imported_names.get(name) {
                return Err(
                    SemanticError::new(span, format!("'{name}' is already imported"))
                        .help(format!(
                            "'{name}' comes from '{provider}'; rename this declaration or drop the import"
                        )),
                );
            }
            names.insert(name.clone(), span);
            // Roles and types share one namespace, and both are registered
            // before anything is lowered, so a declaration may name a role or
            // type declared further down the file.
            if matches!(item, Item::Data(_) | Item::Role(_))
                && !self.known_types.insert(name.clone())
            {
                // A name declared twice in this file was caught above, so
                // reaching here means the collision is with an import or the
                // standard library.
                let existing = if self.roles.contains(name) {
                    "role"
                } else {
                    "type"
                };
                return Err(
                    SemanticError::new(span, format!("'{name}' is already a {existing}")).help(
                        format!(
                            "the {existing} '{name}' comes from an import or the standard library; choose another name"
                        ),
                    ),
                );
            }
            if let Item::Role(declaration) = item {
                self.roles.insert(declaration.name.value.clone());
            }
        }

        for item in &module.items {
            match item {
                Item::Circuit(declaration) => {
                    let generics = self.collect_generics(
                        declaration
                            .inputs
                            .iter()
                            .map(|field| &field.ty)
                            .chain([&declaration.output]),
                    )?;
                    let generic_names = generics.names();
                    let inputs = declaration
                        .inputs
                        .iter()
                        .map(|field| self.lower_type(&field.ty, &generic_names))
                        .collect::<Result<Vec<_>, _>>()?;
                    let output = self.lower_type(&declaration.output, &generic_names)?;
                    self.circuits.insert(
                        declaration.name.value.clone(),
                        CircuitSignature {
                            generics,
                            inputs,
                            output,
                        },
                    );
                }
                Item::Role(_) => {}
                Item::Data(declaration) => {
                    let generic_names = declaration
                        .parameters
                        .iter()
                        .map(|parameter| parameter.name.value.clone())
                        .collect::<BTreeSet<_>>();
                    // A data type's parameters carry bounds of their own, which
                    // `lower_type` enforces wherever the type is used.
                    let bounds = declaration
                        .parameters
                        .iter()
                        .filter_map(|parameter| {
                            parameter.bound.as_ref().map(|bound| {
                                self.lower_bound(bound, &generic_names)
                                    .map(|bound| (parameter.name.value.clone(), bound))
                            })
                        })
                        .collect::<Result<HashMap<_, _>, _>>()?;
                    for role in &declaration.roles {
                        let name = super::path_text(role);
                        if !self.roles.contains(&name) {
                            return Err(self.not_a_role(&name, role.span));
                        }
                        self.type_roles
                            .entry(declaration.name.value.clone())
                            .or_default()
                            .insert(name);
                    }
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
                            parameters: declaration
                                .parameters
                                .iter()
                                .map(|parameter| parameter.name.value.clone())
                                .collect(),
                            bounds,
                            fields,
                            cases,
                        },
                    );
                    if declaration.kind == DataKind::Event {
                        self.event_types.insert(declaration.name.value.clone());
                    }
                }
                Item::Workflow(declaration) => {
                    let generics = self.collect_generics(
                        declaration
                            .inputs
                            .iter()
                            .map(|field| &field.ty)
                            .chain(declaration.outputs.types()),
                    )?;
                    let generic_names = generics.names();
                    let inputs = declaration
                        .inputs
                        .iter()
                        .map(|field| self.lower_type(&field.ty, &generic_names))
                        .collect::<Result<Vec<_>, _>>()?;
                    let outputs =
                        self.lower_workflow_outputs(&declaration.outputs, &generic_names)?;
                    self.workflows.insert(
                        declaration.name.value.clone(),
                        WorkflowSignature {
                            generics,
                            inputs,
                            outputs,
                        },
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

    /// Harvest the type parameters a signature introduces.
    ///
    /// A parameter is introduced where it is first needed rather than in a
    /// header, so reading order and binding order are the same. That is what
    /// makes the form readable, and it is why using a name before the argument
    /// that introduces it is an error rather than something a second pass
    /// quietly resolves.
    pub fn collect_generics<'a>(
        &self,
        signature: impl IntoIterator<Item = &'a TypeExpr>,
    ) -> Result<Generics, SemanticError> {
        let mut mentions = Vec::new();
        for ty in signature {
            collect_mentions(ty, &mut mentions);
        }

        let introduced = mentions
            .iter()
            .filter_map(|mention| match mention {
                Mention::Bind { name, .. } => Some(name.value.clone()),
                Mention::Use { .. } => None,
            })
            .collect::<BTreeSet<_>>();

        let mut generics = Generics::default();
        let mut bound: BTreeMap<String, Span> = BTreeMap::new();
        for mention in &mentions {
            match mention {
                Mention::Bind { name, role, span } => {
                    if let Some(first) = bound.get(&name.value) {
                        return Err(SemanticError::new(
                            *span,
                            format!("'{}' is already introduced", name.value),
                        )
                        .related(*first, format!("'{}' is introduced here", name.value))
                        .help(format!(
                            "write '{}' alone to mean the same one, or pick another name for a different one",
                            name.value
                        )));
                    }
                    let role_name = super::path_text(role);
                    if !self.roles.contains(&role_name) {
                        return Err(self.not_a_role(&role_name, role.span));
                    }
                    bound.insert(name.value.clone(), *span);
                    generics.parameters.push(name.value.clone());
                    generics
                        .bounds
                        .insert(name.value.clone(), Ty::named(role_name));
                }
                Mention::Use { name, span } => {
                    if introduced.contains(*name) && !bound.contains_key(*name) {
                        let (later, role) = mentions
                            .iter()
                            .find_map(|mention| match mention {
                                Mention::Bind {
                                    name: bound_name,
                                    role,
                                    span,
                                } if bound_name.value == *name => {
                                    Some((*span, super::path_text(role)))
                                }
                                _ => None,
                            })
                            .expect("the name is introduced somewhere");
                        return Err(SemanticError::new(
                            *span,
                            format!("'{name}' is used before it is introduced"),
                        )
                        .related(later, format!("'{name}' is introduced here"))
                        .help(format!(
                            "move ': {role}' to the first place '{name}' appears"
                        )));
                    }
                }
            }
        }
        Ok(generics)
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
        generics: &BTreeSet<String>,
    ) -> Result<Vec<(String, Ty)>, SemanticError> {
        match outputs {
            WorkflowOutputs::Single { ty } => {
                Ok(vec![("outcome".to_owned(), self.lower_type(ty, generics)?)])
            }
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
                            self.lower_type(&field.ty, generics)?,
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
            parameters: signature.generics.parameters.clone(),
            bounds: signature
                .generics
                .bounds
                .iter()
                .map(|(name, bound)| (name.clone(), to_checked_type(bound)))
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
            parameters: signature.parameters.clone(),
            bounds: signature
                .bounds
                .iter()
                .map(|(name, bound)| (name.clone(), to_checked_type(bound)))
                .collect(),
            roles: self
                .type_roles
                .get(&declaration.name.value)
                .map(|roles| roles.iter().cloned().collect())
                .unwrap_or_default(),
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
                        && !self.compatible(&inferred, expected)
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
            && !self.compatible(backbone, &Ty::named("Backbone"))
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

    /// Lower a type parameter's bound, which may name a role.
    ///
    /// This is the one position where a role belongs: it constrains what a
    /// parameter may stand for. Everywhere else a role is not a type.
    pub fn lower_bound(
        &self,
        expression: &TypeExpr,
        generics: &BTreeSet<String>,
    ) -> Result<Ty, SemanticError> {
        if let TypeExpr::Path {
            path,
            arguments,
            span,
        } = expression
        {
            let name = super::path_text(path);
            if self.roles.contains(&name) {
                if !arguments.is_empty() {
                    return Err(SemanticError::new(
                        *span,
                        format!("role '{name}' does not take type arguments"),
                    ));
                }
                return Ok(Ty::named(name));
            }
        }
        self.lower_type(expression, generics)
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
                    let mut error = SemanticError::new(*span, format!("unknown type '{name}'"));
                    // In a signature, an unknown name is usually a parameter
                    // spelled differently from the one that was introduced.
                    if !generics.is_empty() {
                        error = error.help(format!(
                            "this signature introduces {}",
                            generics
                                .iter()
                                .map(|name| format!("'{name}'"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                    return Err(error);
                }
                if self.roles.contains(&name) {
                    return Err(self.role_is_not_a_type(&name, *span));
                }
                let arguments = arguments
                    .iter()
                    .map(|argument| match argument {
                        TypeArgument::Type(ty) => self.lower_type(ty, generics),
                        // A binding only means something where a signature
                        // harvested it; anywhere else there is nothing for the
                        // caller to choose.
                        TypeArgument::Binding { name, span, .. } => {
                            if generics.contains(&name.value) {
                                Ok(Ty::named(name.value.clone()))
                            } else {
                                Err(SemanticError::new(
                                    *span,
                                    format!(
                                        "'{}' cannot be introduced here",
                                        name.value
                                    ),
                                )
                                .help(
                                    "a type parameter is introduced by a circuit or workflow parameter",
                                ))
                            }
                        }
                        TypeArgument::Any { role, span } => {
                            let role_name = super::path_text(role);
                            if !self.roles.contains(&role_name) {
                                return Err(self.not_a_role(&role_name, *span));
                            }
                            Ok(Ty::Any(role_name))
                        }
                    })
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
                // A declared type's own parameters are checked the same way a
                // circuit's are: a bound that is not enforced where the type is
                // used is not a bound.
                if let Some(signature) = self.data.get(&name)
                    && !signature.parameters.is_empty()
                {
                    if arguments.len() != signature.parameters.len() {
                        return Err(SemanticError::new(
                            *span,
                            format!(
                                "type '{name}' expects {} argument(s), found {}",
                                signature.parameters.len(),
                                arguments.len()
                            ),
                        ));
                    }
                    for (parameter, argument) in signature.parameters.iter().zip(&arguments) {
                        let Some(bound) = signature.bounds.get(parameter) else {
                            continue;
                        };
                        if !self.satisfies_bound(argument, bound) {
                            return Err(self.unsatisfied_bound(
                                argument,
                                bound,
                                &format!("type '{name}' requires its '{parameter}'"),
                                *span,
                            ));
                        }
                    }
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

    /// The error a reader meets when they write a category where one specific
    /// thing belongs. It states both ways forward, because which one is right
    /// depends on something the compiler cannot know: whether the caller's
    /// choice still matters further down.
    /// The error for an argument that does not satisfy a bound.
    ///
    /// Naming what *would* satisfy it is the difference between a rule the
    /// reader must already know and one they can learn from being stopped.
    pub fn unsatisfied_bound(
        &self,
        actual: &Ty,
        bound: &Ty,
        requirement: &str,
        span: Span,
    ) -> SemanticError {
        let role = bound.to_string();
        let members = self.role_members(&role);
        let mut error =
            SemanticError::new(span, format!("'{actual}' does not play the role {role}"))
                .help(format!("{requirement} to play {role}"));
        if !members.is_empty() {
            error = error.help(format!("types that play {role}: {}", members.join(", ")));
        }
        error.help(format!(
            "declare '{actual} is {role}' if it plays that part"
        ))
    }

    /// The error for `is` naming something that is not a role.
    fn not_a_role(&self, name: &str, span: Span) -> SemanticError {
        let mut error = SemanticError::new(span, format!("'{name}' is not a role"));
        error = if self.known_types.contains(name) {
            error
                .help(format!(
                    "'{name}' is a type; a type cannot be played, only named"
                ))
                .help(format!(
                    "declare 'role {name}' if you meant a category of types"
                ))
        } else {
            error.help(format!("declare it first with 'role {name}'"))
        };
        error
    }

    fn role_is_not_a_type(&self, name: &str, span: Span) -> SemanticError {
        let members = self.role_members(name);
        let error = SemanticError::new(span, format!("'{name}' is a role, not a type")).help(
            format!("name it, and everything using that name must agree: <T: {name}>"),
        );
        if members.is_empty() {
            return error.help(format!("or name a specific type that plays {name}"));
        }
        error.help(format!(
            "or name a type that plays {name}: {}",
            members.join(", ")
        ))
    }
}
