//! Import resolution, declaration-signature collection, and the checks for
//! declaration-shaped items: circuits, data types, and artifacts (including
//! the plasmid/strain shape rules).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::ast::{
    ActionDecl, ArtifactDecl, ArtifactMember, CircuitDecl, DataDecl, Expr, FacetDecl, FieldDecl,
    Item, Module, Path, PhraseToken, Provenance, TypeArgument, TypeExpr, Unit, WorkflowOutputs,
    instance_word,
};
use crate::checked::{
    CheckedAcceptance, CheckedActionOperand, CheckedActionResult, CheckedCase, CheckedDeclaration,
    CheckedPhraseToken, CheckedPresence, CheckedProperty, CheckedSection, OwnershipMode,
};
use crate::is_absolute_iri;
use crate::semantic_error::SemanticError;
use crate::source::{Identifier, Span};
use crate::standard_library::{
    ActionContractSpec, ContractType, Lineage as ContractLineage, PhrasePart as ContractPhrasePart,
    ResultSpec as ContractResultSpec,
};
use crate::type_system::{Ty, to_checked_type};

use super::Checker;
use super::context::{
    ArtifactKindSignature, CheckedActionContract, CircuitSignature, DataSignature, FacetSignature,
    FacetState, Generics, SchemaField, WorkflowSignature,
};

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
                    // A narrowing introduces no name either, but the subject it
                    // narrows is an ordinary argument and may mention one.
                    TypeArgument::InState { subject, .. } => collect_mentions(subject, out),
                    TypeArgument::Type(ty) => collect_mentions(ty, out),
                }
            }
        }
        TypeExpr::Union { alternatives, .. } => {
            for alternative in alternatives {
                collect_mentions(alternative, out);
            }
        }
        // A unit is not a name a signature can introduce or refer to.
        TypeExpr::Quantity { .. } => {}
    }
}

/// Where an expression first names one of `names` on its own, if it does.
///
/// A bare name is the only form that reads a property, so a qualified path or a
/// field access on some other subject is not a mention.
fn first_mention<'a>(
    expression: &'a Expr,
    names: &BTreeSet<&str>,
) -> Option<(&'a str, crate::source::Span)> {
    let mention = |expression| first_mention(expression, names);
    match expression {
        Expr::Path(path) => {
            let [segment] = path.segments.as_slice() else {
                return None;
            };
            names
                .contains(segment.value.as_str())
                .then_some((segment.value.as_str(), path.span))
        }
        Expr::Quantity { magnitude, .. } => mention(magnitude),
        Expr::List { elements, .. } => elements.iter().find_map(mention),
        Expr::Call {
            callee, arguments, ..
        } => mention(callee).or_else(|| arguments.iter().find_map(|it| mention(&it.value))),
        Expr::Record { fields, .. } => fields.iter().find_map(|it| mention(&it.value)),
        Expr::Field { subject, .. } => mention(subject),
        Expr::Convert { value, .. } => mention(value),
        Expr::Unary { operand, .. } => mention(operand),
        Expr::Binary { left, right, .. } => mention(left).or_else(|| mention(right)),
        Expr::Integer { .. } | Expr::Decimal { .. } | Expr::String { .. } => None,
    }
}

/// Edit distance between two names, for suggesting the one that was meant.
fn distance(from: &str, to: &str) -> usize {
    let target = to.chars().collect::<Vec<_>>();
    let mut row = (0..=target.len()).collect::<Vec<_>>();
    for (i, source) in from.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, target) in target.iter().enumerate() {
            let cost = usize::from(source != *target);
            let replace = previous + cost;
            previous = row[j + 1];
            row[j + 1] = (row[j] + 1).min(previous + 1).min(replace);
        }
    }
    row[target.len()]
}

/// The candidate a misspelling most likely meant, if one is close enough to be
/// worth suggesting.
///
/// Two slips are worth catching and they need different measures. A mistyped
/// name is a short edit away, where a third of it may differ before the
/// suggestion stops being credible. A name typed short — `digest_temp` for
/// `digest_temperature` — is many edits away but is a prefix of what was meant,
/// which on its own is strong enough evidence.
fn nearest<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let limit = (name.chars().count() / 3).max(1);
    candidates
        .map(|candidate| (distance(name, candidate), candidate))
        .filter(|(distance, candidate)| *distance <= limit || candidate.starts_with(name))
        .min_by_key(|(distance, candidate)| (*distance, candidate.len()))
        .map(|(_, candidate)| candidate)
}

/// `a`, `a and b`, `a, b, and c` — a list that reads as a sentence rather than
/// as compiler output.
fn conjunction(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// The kind a facet classifies, which is a bare name and never a built type.
///
/// A facet says which states a kind's materials may be in. A list or a
/// measurement has no states, and a generic subject would make the states depend
/// on an argument, so the subject is one name.
fn facet_subject(declaration: &FacetDecl) -> Result<&Identifier, SemanticError> {
    match &declaration.subject {
        TypeExpr::Path {
            path, arguments, ..
        } if arguments.is_empty()
            && let [segment] = path.segments.as_slice() =>
        {
            Ok(segment)
        }
        other => Err(
            SemanticError::new(other.span(), "a facet classifies a named kind")
                .help("write 'on' followed by the kind whose materials this facet classifies"),
        ),
    }
}

/// The facet a property name states, if it names one.
///
/// A facet is stated by its own name in snake_case, the way an artifact kind's
/// instances are written with the type's name in snake_case. One convention
/// serves both, so neither has to be learned separately.
fn stated_facet(checker: &Checker, produces: &Ty, name: &str) -> Option<String> {
    let Ty::Named(subject, _) = produces else {
        return None;
    };
    checker
        .type_facets
        .get(subject)?
        .iter()
        .find(|facet| instance_word(facet) == name)
        .cloned()
}

fn facet_state_expected(facet: &str, span: Span) -> SemanticError {
    SemanticError::new(span, format!("'{facet}' is not a state"))
        .help("a facet is stated as one of its states, written as a bare name")
}

fn quoted_conjunction(names: &[&str]) -> String {
    let quoted = names
        .iter()
        .map(|name| format!("'{name}'"))
        .collect::<Vec<_>>();
    conjunction(&quoted.iter().map(String::as_str).collect::<Vec<_>>())
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
                Item::Facet(value) => (&value.name.value, value.name.span),
                Item::Action(value) => (&value.name.value, value.name.span),
                Item::ArtifactKind(value) => (&value.name.value, value.name.span),
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
                if let Some(term) = &declaration.term {
                    let canonical = super::ontology::check_term(term)?;
                    self.role_terms
                        .insert(declaration.name.value.clone(), canonical);
                }
            }
        }

        // Facets resolve before any signature, because a signature may narrow a
        // material to a state and the states have to be known by then. They ask
        // only for the subject's name, so a kind declared further down the file
        // is not yet needed; that the name is a real kind is checked once every
        // declaration has been seen.
        for item in &module.items {
            if let Item::Facet(declaration) = item {
                self.collect_facet(declaration)?;
            }
        }

        // Actions register after facets, because an operand may be narrowed to a
        // facet state and the states must be known by then. A verb the workflow
        // pass then reads is checked against the contract collected here.
        for item in &module.items {
            if let Item::Action(declaration) = item {
                self.collect_action(declaration)?;
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
                // Each is collected in a pass of its own.
                Item::Role(_) | Item::Facet(_) | Item::Action(_) => {}
                Item::ArtifactKind(declaration) => {
                    let produces = self.lower_kind_type(&declaration.produces)?;
                    // A kind's roles classify the type it produces, because
                    // that is the type a workflow names and a bound reads.
                    let produced_name = match &produces {
                        Ty::Named(name, _) => name.clone(),
                        other => {
                            return Err(SemanticError::new(
                                declaration.produces.span(),
                                format!("a kind must produce a named type, found '{other}'"),
                            ));
                        }
                    };
                    for role in &declaration.roles {
                        let name = super::path_text(role);
                        if !self.roles.contains(&name) {
                            return Err(self.not_a_role(&name, role.span));
                        }
                        self.type_roles
                            .entry(produced_name.clone())
                            .or_default()
                            .insert(name);
                    }
                    // A kind that states no schema of its own takes the
                    // fields of the type it names, which is what a supplier's
                    // item fills in. Stating a schema replaces them.
                    let fields = if declaration.fields.is_empty() {
                        self.type_fields(&produces)
                            .into_iter()
                            .map(|(name, ty)| {
                                (
                                    name,
                                    SchemaField {
                                        ty,
                                        optional: false,
                                    },
                                )
                            })
                            .collect()
                    } else {
                        self.collect_schema_fields(&declaration.fields)?
                    };
                    let declares = declaration
                        .declares
                        .as_ref()
                        .map(|predicate| {
                            self.check_presence_predicate(predicate, &fields, &declaration.fields)
                        })
                        .transpose()?;
                    self.artifact_kinds.insert(
                        declaration.name.value.clone(),
                        ArtifactKindSignature {
                            produces,
                            fields,
                            declares,
                        },
                    );
                }
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
                                format!("case '{}' is already declared", case.name.value),
                            )
                            .help(
                                "a case constructor is a module-level name, so no two cases may share one",
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
                    // A journalled occurrence says so by playing `Event`,
                    // which is what `emit` and `when` resolve against.
                    if self
                        .type_roles
                        .get(&declaration.name.value)
                        .is_some_and(|roles| roles.contains("Event"))
                    {
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
                    // The kind says what its instances are, so the value's type
                    // comes from the schema rather than from the word — except
                    // where a generic kind leaves the arguments to the instance.
                    let ty = self
                        .artifact_kinds
                        .get(&declaration.kind.value)
                        .map(|signature| signature.produces.clone())
                        .ok_or_else(|| {
                            self.unknown_artifact_kind(
                                &declaration.kind.value,
                                declaration.kind.span,
                            )
                        })?;
                    let ty = match &declaration.ascribed {
                        Some(ascribed) => {
                            let named = self.lower_type(ascribed, &BTreeSet::new())?;
                            self.check_ascription(&named, &ty, ascribed.span())?;
                            named
                        }
                        None => ty,
                    };
                    // A stated facet narrows the thing itself, so every use of
                    // the name carries the state. Provisioning a chassis
                    // declared competent then yields competent cells without
                    // the action contract knowing that facets exist.
                    let ty = self.narrowed_by_stated_facets(&ty, declaration)?;
                    self.values.insert(declaration.name.value.clone(), ty);
                }
                Item::Use(_) | Item::Binding(_) => {}
            }
        }

        for item in &module.items {
            if let Item::Facet(declaration) = item {
                let subject = facet_subject(declaration)?;
                if !self.known_types.contains(&subject.value)
                    && !self.artifact_kinds.values().any(|kind| {
                        matches!(&kind.produces, Ty::Named(name, _) if name == &subject.value)
                    })
                    && !self.standard_types.contains_key(&subject.value)
                {
                    return Err(SemanticError::new(
                        subject.span,
                        format!("'{}' is not a kind in scope", subject.value),
                    )
                    .help("a facet classifies the materials of a declared kind"));
                }
            }
        }
        Ok(())
    }

    /// Narrow an instance's type by the facet states its declaration states.
    ///
    /// A property whose name is a facet on this kind says which state this thing
    /// is in. It is not a schema field: the kind declares what a thing *may*
    /// state, and a facet declares what it may *be*, so the two namespaces are
    /// checked separately and a facet name is never a missing property.
    fn narrowed_by_stated_facets(
        &self,
        ty: &Ty,
        declaration: &ArtifactDecl,
    ) -> Result<Ty, SemanticError> {
        let Ty::Named(subject, _) = ty else {
            return Ok(ty.clone());
        };
        let Some(facets) = self.type_facets.get(subject) else {
            return Ok(ty.clone());
        };
        let mut narrowed = ty.clone();
        for member in &declaration.members {
            let ArtifactMember::Property(property) = member else {
                continue;
            };
            let Some(facet) = facets
                .iter()
                .find(|facet| instance_word(facet) == property.name.value)
            else {
                continue;
            };
            let signature = self
                .facets
                .get(facet)
                .expect("a facet on this type was collected");
            let state = match &property.value {
                Expr::Path(path) => match path.segments.as_slice() {
                    [segment] => segment,
                    _ => return Err(facet_state_expected(&property.name.value, path.span)),
                },
                other => return Err(facet_state_expected(&property.name.value, other.span())),
            };
            if signature.state(&state.value).is_none() {
                let states = signature
                    .states
                    .iter()
                    .map(|state| state.name.as_str())
                    .collect::<Vec<_>>();
                return Err(SemanticError::new(
                    state.span,
                    format!("facet '{facet}' has no state '{}'", state.value),
                )
                .help(format!("states of '{facet}': {}", states.join(", "))));
            }
            narrowed = Ty::InState(Box::new(narrowed), state.value.clone());
        }
        Ok(narrowed)
    }

    /// The facet states this declaration puts itself in.
    ///
    /// The properties were validated when the type was narrowed, so this reads
    /// the same pairs back rather than checking them twice.
    fn stated_facet_states(
        &self,
        ty: &Ty,
        declaration: &ArtifactDecl,
    ) -> Result<Vec<(String, String)>, SemanticError> {
        let mut stated = Vec::new();
        for member in &declaration.members {
            let ArtifactMember::Property(property) = member else {
                continue;
            };
            let Some(facet) = stated_facet(self, ty, &property.name.value) else {
                continue;
            };
            let Expr::Path(path) = &property.value else {
                continue;
            };
            let [segment] = path.segments.as_slice() else {
                continue;
            };
            stated.push((facet, segment.value.clone()));
        }
        Ok(stated)
    }

    /// Check that some facet of `subject` admits `state`.
    ///
    /// A material is narrowed by naming a state rather than the facet it belongs
    /// to, because at the point of use the state is what a person knows: cells
    /// are competent, a culture is diluted. Which facet said so is the
    /// declaration's business.
    fn check_facet_state(&self, subject: &Ty, state: &Identifier) -> Result<(), SemanticError> {
        let Ty::Named(subject_name, _) = subject else {
            return Err(SemanticError::new(
                state.span,
                format!("'{subject}' has no states, so it cannot be narrowed to one"),
            )
            .help("only a named kind carries facets"));
        };
        let facets = self.type_facets.get(subject_name);
        let admits = facets.is_some_and(|names| {
            names.iter().any(|facet| {
                self.facets
                    .get(facet)
                    .is_some_and(|signature| signature.state(&state.value).is_some())
            })
        });
        if admits {
            return Ok(());
        }
        let known = facets
            .map(|names| {
                let mut states = names
                    .iter()
                    .filter_map(|facet| self.facets.get(facet))
                    .flat_map(|signature| signature.states.iter().map(|state| state.name.as_str()))
                    .collect::<Vec<_>>();
                states.sort_unstable();
                states
            })
            .unwrap_or_default();
        let error = SemanticError::new(
            state.span,
            format!("'{subject_name}' has no state '{}'", state.value),
        );
        Err(if known.is_empty() {
            error.help(format!(
                "no facet classifies '{subject_name}'; declare one with 'facet <Name> on {subject_name}:'"
            ))
        } else {
            error.help(format!("states of '{subject_name}': {}", known.join(", ")))
        })
    }

    /// Validate one action and register its contract so workflows check
    /// against it.
    ///
    /// The phrase's holes are typed by the body, and each hole's type decides
    /// the part it becomes: a measurement is a quantity slot, a whole number an
    /// integer slot, and anything else a material or value operand.
    fn collect_action(&mut self, declaration: &ActionDecl) -> Result<(), SemanticError> {
        let mut binding_types = BTreeMap::new();
        let mut binding_modes = BTreeMap::new();
        for binding in &declaration.bindings {
            let ty = self.lower_type(&binding.ty, &BTreeSet::new())?;
            if binding_types
                .insert(binding.name.value.clone(), ty)
                .is_some()
            {
                return Err(SemanticError::new(
                    binding.name.span,
                    format!("'{}' is typed more than once", binding.name.value),
                ));
            }
            binding_modes.insert(binding.name.value.clone(), binding.mode);
        }

        let hole = |name: &Identifier| -> Result<Ty, SemanticError> {
            binding_types.get(&name.value).cloned().ok_or_else(|| {
                SemanticError::new(
                    name.span,
                    format!(
                        "operand '{}' is named in the phrase but never typed",
                        name.value
                    ),
                )
            })
        };

        let mut phrase = Vec::new();
        let mut phrase_tokens = Vec::new();
        for token in &declaration.phrase {
            match token {
                PhraseToken::Word(word) => {
                    phrase.push(ContractPhrasePart::word(&word.value));
                    phrase_tokens.push(CheckedPhraseToken::Word(word.value.clone()));
                }
                PhraseToken::Hole(name) => {
                    let ty = hole(name)?;
                    phrase.push(self.action_operand_part(name, &ty, binding_modes[&name.value])?);
                    phrase_tokens.push(CheckedPhraseToken::Hole(name.value.clone()));
                }
            }
        }

        let mut operands = Vec::new();
        for token in &declaration.phrase {
            if let PhraseToken::Hole(name) = token {
                let ty = hole(name)?;
                operands.push(CheckedActionOperand {
                    name: name.value.clone(),
                    r#type: to_checked_type(&ty),
                    mode: binding_modes[&name.value].unwrap_or(OwnershipMode::Take),
                });
            }
        }

        let mut results = Vec::new();
        let mut result_specs = Vec::new();
        for name in &declaration.results {
            let ty = hole(name)?;
            results.push(CheckedActionResult {
                name: name.value.clone(),
                r#type: to_checked_type(&ty),
            });
            result_specs.push(ContractResultSpec {
                name: name.value.clone(),
                r#type: ContractType::Concrete(ty),
                lineage: ContractLineage::Continues,
            });
        }

        let operation = format!("{}.{}", self.module_id.as_str(), declaration.name.value);
        let contract = ActionContractSpec {
            operation: operation.clone(),
            phrase,
            results: result_specs,
            inert: Vec::new(),
        };
        contract.validate().map_err(|message| {
            SemanticError::new(
                declaration.name.span,
                format!("malformed action: {message}"),
            )
        })?;
        self.actions
            .insert(declaration.name.value.clone(), contract);
        self.action_contracts.insert(
            declaration.name.value.clone(),
            CheckedActionContract {
                operation,
                phrase: phrase_tokens,
                operands,
                results,
                capability: declaration.capability.value.clone(),
            },
        );
        Ok(())
    }

    /// The phrase part one operand hole becomes, chosen by its type.
    fn action_operand_part(
        &self,
        name: &Identifier,
        ty: &Ty,
        mode: Option<OwnershipMode>,
    ) -> Result<ContractPhrasePart, SemanticError> {
        match ty {
            Ty::Quantity(unit) => Ok(ContractPhrasePart::quantity(&name.value, false, &[unit])),
            Ty::Integer => Ok(ContractPhrasePart::integer(&name.value, false)),
            Ty::Measuring(_) => Err(SemanticError::new(
                name.span,
                format!(
                    "operand '{}' must state one unit, not a dimension",
                    name.value
                ),
            )
            .help("an action operand pins its unit; a field may name a dimension")),
            _ => Ok(ContractPhrasePart::operand(
                &name.value,
                ContractType::Concrete(ty.clone()),
                mode.unwrap_or(OwnershipMode::Take),
            )),
        }
    }

    /// Validate one facet and register it against the type it classifies.
    fn collect_facet(&mut self, declaration: &FacetDecl) -> Result<(), SemanticError> {
        let subject_name = facet_subject(declaration)?.value.clone();
        let subject = Ty::named(subject_name.clone());

        let mut states: Vec<FacetState> = Vec::new();
        for state in &declaration.states {
            if states.iter().any(|seen| seen.name == state.name.value) {
                return Err(SemanticError::new(
                    state.name.span,
                    format!("duplicate state '{}'", state.name.value),
                )
                .help("each state a facet admits is listed once"));
            }
            let mut fields = BTreeMap::new();
            for field in &state.fields {
                let ty = self.lower_type(&field.ty, &BTreeSet::new())?;
                fields.insert(
                    field.name.value.clone(),
                    SchemaField {
                        ty,
                        optional: field.optional,
                    },
                );
            }
            states.push(FacetState {
                doc: state.doc.clone(),
                name: state.name.value.clone(),
                fields,
            });
        }

        let known = |name: &Identifier| states.iter().any(|state| state.name == name.value);
        let mut transitions = Vec::new();
        for transition in &declaration.transitions {
            for endpoint in [&transition.from, &transition.to] {
                if !known(endpoint) {
                    return Err(SemanticError::new(
                        endpoint.span,
                        format!(
                            "facet '{}' has no state '{}'",
                            declaration.name.value, endpoint.value
                        ),
                    )
                    .help(format!(
                        "states in this facet: {}",
                        states
                            .iter()
                            .map(|state| state.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )));
                }
            }
            transitions.push((transition.from.value.clone(), transition.to.value.clone()));
        }

        // The first state is where a material starts, so it needs no transition
        // into it. Any other state nothing reaches cannot be established, and a
        // state no action can establish is a claim the kind cannot honor.
        for state in states.iter().skip(1) {
            if !transitions.iter().any(|(_, to)| to == &state.name) {
                let span = declaration
                    .states
                    .iter()
                    .find(|declared| declared.name.value == state.name)
                    .map(|declared| declared.name.span)
                    .unwrap_or(declaration.name.span);
                return Err(SemanticError::new(
                    span,
                    format!("no transition reaches state '{}'", state.name),
                )
                .help(format!(
                    "a material starts in '{}'; write a transition into '{}' or remove it",
                    states[0].name, state.name
                )));
            }
        }

        self.type_facets
            .entry(subject_name.clone())
            .or_default()
            .insert(declaration.name.value.clone());
        self.facets.insert(
            declaration.name.value.clone(),
            FacetSignature {
                subject,
                states,
                transitions,
            },
        );
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

    /// An artifact kind's schema, where a field carries whether an instance may
    /// omit it. A kind takes no type parameters, so its fields lower against no
    /// generics.
    pub fn collect_schema_fields(
        &self,
        fields: &[FieldDecl],
    ) -> Result<BTreeMap<String, SchemaField>, SemanticError> {
        let mut result = BTreeMap::new();
        for field in fields {
            let ty = self.lower_type(&field.ty, &BTreeSet::new())?;
            let schema = SchemaField {
                ty,
                optional: field.optional,
            };
            if result.insert(field.name.value.clone(), schema).is_some() {
                return Err(SemanticError::new(
                    field.name.span,
                    format!("duplicate field '{}'", field.name.value),
                ));
            }
        }
        Ok(result)
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

    /// A rule cannot read a property this declaration leaves unstated. Saying
    /// that plainly is worth more than the unknown-name error the missing
    /// binding would otherwise produce, because the name is right there in the
    /// schema and the reader would go looking for a typo.
    fn reject_omitted(
        &self,
        predicate: &Expr,
        omitted: &BTreeSet<&str>,
        keyword: &str,
        artifact: &Identifier,
    ) -> Result<(), SemanticError> {
        let Some((name, span)) = first_mention(predicate, omitted) else {
            return Ok(());
        };
        Err(SemanticError::new(
            span,
            format!("{keyword} '{}' does not state '{name}'", artifact.value),
        )
        .related(artifact.span, format!("this {keyword} states no '{name}'"))
        .help(format!(
            "'{name}' is optional, so a rule may read it only where it is stated"
        )))
    }

    /// The fields of a named type, whether it is declared in Lab or built in.
    ///
    /// A generic type's fields are read through its arguments, so a
    /// `Promoter<Tetracycline>` states what a promoter for tetracycline states.
    fn type_fields(&self, ty: &Ty) -> BTreeMap<String, Ty> {
        let Ty::Named(name, arguments) = ty else {
            return BTreeMap::new();
        };
        if let Some(signature) = self.data.get(name) {
            let substitutions = signature
                .parameters
                .iter()
                .cloned()
                .zip(arguments.iter().map(|ty| (ty.clone(), Span::at(0))))
                .collect();
            return signature
                .fields
                .iter()
                .map(|(field, ty)| {
                    (
                        field.clone(),
                        crate::type_system::substitute(ty, &substitutions),
                    )
                })
                .collect();
        }
        self.standard_types
            .get(name)
            .map(|spec| {
                spec.fields
                    .iter()
                    .map(|(field, ty)| ((*field).to_owned(), ty.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The type a kind produces. A generic kind names a bare head, because the
    /// arguments belong to each instance: `artifact Promoter` covers every
    /// promoter, and `buy promoter pTet: Promoter<Tetracycline>` says which.
    fn lower_kind_type(&self, expression: &TypeExpr) -> Result<Ty, SemanticError> {
        if let TypeExpr::Path {
            path, arguments, ..
        } = expression
            && arguments.is_empty()
        {
            let name = super::path_text(path);
            let generic = self
                .standard_types
                .get(&name)
                .map(|spec| spec.parameters)
                .or_else(|| self.data.get(&name).map(|spec| spec.parameters.len()))
                .is_some_and(|parameters| parameters > 0);
            if generic {
                return Ok(Ty::Named(name, Vec::new()));
            }
        }
        self.lower_type(expression, &BTreeSet::new())
    }

    /// An instance may fill in a generic kind's arguments, and may not name
    /// some other type: the word already said what kind of thing this is.
    fn check_ascription(&self, named: &Ty, produces: &Ty, span: Span) -> Result<(), SemanticError> {
        let head = |ty: &Ty| match ty {
            Ty::Named(name, _) => Some(name.clone()),
            _ => None,
        };
        if head(named) == head(produces) {
            return Ok(());
        }
        Err(
            SemanticError::new(span, format!("this names {named}, not {produces}"))
                .help("an instance may fill in its kind's type arguments, not replace its type"),
        )
    }

    /// Evidence is counted in entities, so asking for none is asking for
    /// nothing and is a mistake rather than a way to opt out.
    fn check_replication(
        &self,
        replication: &crate::ast::Replication,
    ) -> Result<(), SemanticError> {
        if replication.count == 0 {
            return Err(SemanticError::new(
                replication.span,
                "a claim cannot be believed on zero biological replicates",
            )
            .help("omit 'across' to accept whatever evidence is offered"));
        }
        Ok(())
    }

    pub fn check_artifact(
        &self,
        declaration: &ArtifactDecl,
    ) -> Result<CheckedDeclaration, SemanticError> {
        let keyword = declaration.kind.value.as_str();
        let signature = self
            .artifact_kinds
            .get(keyword)
            .ok_or_else(|| self.unknown_artifact_kind(keyword, declaration.kind.span))?;
        let produces = match &declaration.ascribed {
            Some(ascribed) => self.lower_type(ascribed, &BTreeSet::new())?,
            None => signature.produces.clone(),
        };
        let mut environment = self.values.clone();
        // `require` and `accept` read the type's own fields; the kind's schema
        // says which properties an author may state and what each holds.
        // Whether the type is declared here or built in, its own fields are
        // what a rule reads and what a bought item fills in.
        let produced_fields = self.type_fields(&produces);
        environment.extend(
            produced_fields
                .iter()
                .map(|(name, ty)| (name.clone(), ty.clone())),
        );
        // An unstated optional property is absent, so it never reaches the
        // environment and `require` reading it is an unknown name rather than a
        // value that silently means nothing. Which properties a declaration
        // states is known before any of them is checked, so a rule may read a
        // property declared below it.
        let stated = declaration
            .members
            .iter()
            .filter_map(|member| match member {
                ArtifactMember::Property(property) => Some(property.name.value.clone()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        environment.extend(
            signature
                .fields
                .iter()
                .map(|(name, field)| (name.clone(), field.ty.clone())),
        );
        // A rule reads the artifact that gets built, so where the produced type
        // carries a field of the same name that field is what the rule means:
        // `accept sequence == design.sequence` compares the realized plasmid
        // against its design, whether or not the declaration stated a sequence.
        // A schema property with no such field is only ever the stated value,
        // so omitting it leaves nothing to read.
        let mut omitted = BTreeSet::new();
        for (name, field) in &signature.fields {
            if !field.optional || stated.contains(name.as_str()) {
                continue;
            }
            match produced_fields.get(name) {
                Some(ty) => {
                    environment.insert(name.clone(), ty.clone());
                }
                None => {
                    environment.remove(name);
                    omitted.insert(name.as_str());
                }
            }
        }
        if declaration.provenance == Provenance::Buy
            && let Some(span) = declaration.members.iter().find_map(|member| match member {
                ArtifactMember::Requirement(claim) | ArtifactMember::Acceptance(claim) => {
                    Some(claim.span)
                }
                ArtifactMember::Replication(replication) => Some(replication.span),
                _ => None,
            })
        {
            return Err(SemanticError::new(
                span,
                format!(
                    "'{}' is bought, so nothing here builds it",
                    declaration.name.value
                ),
            )
            .help("'require', 'accept', and 'across' describe a thing a laboratory makes"));
        }
        // What a material in a state carries is stated alongside the state, so
        // the fields of every state this declaration puts itself in are
        // readable here exactly as the kind's own schema fields are.
        let stated_states = self.stated_facet_states(&produces, declaration)?;
        let mut state_fields: BTreeMap<String, Ty> = BTreeMap::new();
        for (facet, state) in &stated_states {
            let Some(state) = self.facets.get(facet).and_then(|facet| facet.state(state)) else {
                continue;
            };
            for (name, field) in &state.fields {
                state_fields.insert(name.clone(), field.ty.clone());
            }
        }
        for (name, ty) in &state_fields {
            environment.insert(name.clone(), ty.clone());
        }

        let mut properties = Vec::new();
        let mut property_names = BTreeSet::new();
        let mut sbol_identity = None;
        let mut supplier_identity = None;
        let mut requirements = Vec::new();
        let mut acceptance = Vec::new();
        // The declaration's standard is read first so a claim written above it
        // still takes it, and so stating it twice is caught rather than
        // silently resolved by position.
        let mut default_replicates = None;
        for member in &declaration.members {
            let ArtifactMember::Replication(replication) = member else {
                continue;
            };
            if default_replicates.is_some() {
                return Err(SemanticError::new(
                    replication.span,
                    format!(
                        "{keyword} '{}' states its evidence twice",
                        declaration.name.value
                    ),
                )
                .help("a declaration sets one standard; a claim may state its own"));
            }
            self.check_replication(replication)?;
            default_replicates = Some(replication.count);
        }
        for member in &declaration.members {
            match member {
                ArtifactMember::Property(property) => {
                    if !property_names.insert(property.name.value.clone()) {
                        return Err(SemanticError::new(
                            property.name.span,
                            format!("duplicate {keyword} property '{}'", property.name.value),
                        ));
                    }
                    let is_sbol_identity = property.name.value == "sbol_identity";
                    let is_supplier_identity = declaration.provenance == Provenance::Buy
                        && matches!(
                            property.name.value.as_str(),
                            "identity" | "supplier_identity"
                        );
                    if is_sbol_identity || is_supplier_identity {
                        let Expr::String { value, .. } = &property.value else {
                            return Err(SemanticError::new(
                                property.value.span(),
                                format!(
                                    "{} is a String literal",
                                    if is_sbol_identity {
                                        "an SBOL identity"
                                    } else {
                                        "a supplier identity"
                                    }
                                ),
                            ));
                        };
                        if is_sbol_identity {
                            if !is_absolute_iri(value) {
                                return Err(SemanticError::new(
                                    property.value.span(),
                                    format!("SBOL identity '{value}' is not an absolute IRI"),
                                )
                                .help("use an absolute IRI such as 'https://example.org/design'"));
                            }
                            sbol_identity = Some(value.clone());
                        } else {
                            if supplier_identity.is_some() {
                                return Err(SemanticError::new(
                                    property.name.span,
                                    "a bought item states its supplier identity twice",
                                )
                                .help("use 'supplier_identity'; 'identity' is its legacy alias"));
                            }
                            supplier_identity = Some(value.clone());
                        }
                        continue;
                    }
                    // A schema says what a thing may state, so a name it does
                    // not declare is a mistake rather than an extension. SBOL
                    // and supplier identities were consumed above because
                    // they describe the instance, not one artifact kind.
                    // A facet name says which state this thing is in. It narrowed
                    // the value's type when the declaration was collected, and
                    // it is not a schema field, so it is neither checked against
                    // one nor reported as missing from one.
                    if stated_facet(self, &produces, &property.name.value).is_some() {
                        continue;
                    }
                    if !signature.fields.contains_key(&property.name.value)
                        && !state_fields.contains_key(&property.name.value)
                    {
                        let mut error = SemanticError::new(
                            property.name.span,
                            format!("{produces} has no property '{}'", property.name.value),
                        );
                        if let Some(near) = nearest(
                            &property.name.value,
                            signature
                                .fields
                                .keys()
                                .chain(state_fields.keys())
                                .map(String::as_str),
                        ) {
                            error = error.help(format!("did you mean '{near}'?"));
                        }
                        return Err(error);
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
                    self.reject_omitted(&claim.predicate, &omitted, keyword, &declaration.name)?;
                    self.require_bool(&claim.predicate, &environment, "require")?;
                    requirements.push(self.lower_checked_expr(
                        &claim.predicate,
                        &environment,
                        None,
                    )?);
                }
                ArtifactMember::Acceptance(claim) => {
                    self.reject_omitted(&claim.predicate, &omitted, keyword, &declaration.name)?;
                    self.require_bool(&claim.predicate, &environment, "accept")?;
                    // A claim's own standard replaces the declaration's rather
                    // than adding to it, so what a claim is believed on is
                    // written in one place.
                    let replicates = match &claim.replicates {
                        Some(replication) => {
                            self.check_replication(replication)?;
                            Some(replication.count)
                        }
                        None => default_replicates,
                    };
                    acceptance.push(CheckedAcceptance {
                        predicate: self.lower_checked_expr(&claim.predicate, &environment, None)?,
                        replicates,
                    });
                }
                ArtifactMember::Replication(_) => {}
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
        let missing = signature
            .fields
            .iter()
            .filter(|(name, field)| !field.optional && !property_names.contains(name.as_str()))
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            let required = signature
                .fields
                .iter()
                .filter(|(_, field)| !field.optional)
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>();
            return Err(SemanticError::new(
                declaration.name.span,
                format!(
                    "{keyword} '{}' does not state {}",
                    declaration.name.value,
                    quoted_conjunction(&missing)
                ),
            )
            .help(format!("every {keyword} states {}", conjunction(&required))));
        }
        // A state carries what is true of a material in it, so stating the
        // state and leaving those unstated says less than the facet promised.
        // Cells are not competent in the abstract; they are competent to a
        // number, and that number is what a batch is accepted on.
        for (facet, state) in &stated_states {
            let signature = self
                .facets
                .get(facet)
                .expect("a facet on this type was collected");
            let state = signature
                .state(state)
                .expect("the stated state was validated when the type was narrowed");
            let missing = state
                .fields
                .keys()
                .filter(|field| !property_names.contains(field.as_str()))
                .map(String::as_str)
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                return Err(SemanticError::new(
                    declaration.span,
                    format!(
                        "'{}' is {} but does not state {}",
                        declaration.name.value,
                        state.name,
                        quoted_conjunction(&missing)
                    ),
                )
                .help(format!(
                    "'{}' carries {}",
                    state.name,
                    conjunction(&state.fields.keys().map(String::as_str).collect::<Vec<_>>())
                )));
            }
        }
        if let Some(predicate) = &signature.declares
            && !predicate.satisfied_by(&property_names)
        {
            return Err(SemanticError::new(
                declaration.span,
                format!("{keyword} '{}' is incomplete", declaration.name.value),
            )
            .help(format!("a {keyword} states {}", predicate.describe())));
        }
        if declaration.provenance == Provenance::Buy {
            // A supplier lists it, so it is never built and there is nothing to
            // accept it against. Its order identifier defaults to the declared
            // name and stays independent of the biological design IRI.
            return Ok(CheckedDeclaration::Catalog {
                doc: declaration.doc.clone(),
                name: declaration.name.value.clone(),
                // The stated state travels with the exported type, so a module
                // that imports this name sees the same narrowing the declaring
                // module did. Collection already computed it, so reading it back
                // keeps one answer rather than two that could disagree.
                r#type: to_checked_type(
                    self.values
                        .get(&declaration.name.value)
                        .unwrap_or(&produces),
                ),
                sbol_identity,
                supplier_identity: supplier_identity
                    .unwrap_or_else(|| declaration.name.value.clone()),
                properties,
            });
        }
        Ok(CheckedDeclaration::Artifact {
            doc: declaration.doc.clone(),
            artifact: keyword.to_owned(),
            name: declaration.name.value.clone(),
            produces: to_checked_type(&produces),
            sbol_identity,
            properties,
            requirements,
            acceptance,
        })
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
            TypeExpr::Quantity { unit, span } => match unit {
                Unit::Exact(unit) => Ok(Ty::Quantity(unit.clone())),
                Unit::Dimension(dimension) => {
                    if crate::units::Dimension::named(dimension).is_none() {
                        return Err(SemanticError::new(
                            *span,
                            format!("'{dimension}' is not something this compiler measures"),
                        )
                        .help(
                            "measurable things are Mass, Volume, Amount, Length, Duration, \
Temperature, and Count",
                        ));
                    }
                    Ok(Ty::Measuring(dimension.clone()))
                }
            },
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
                        TypeArgument::InState {
                            subject, state, ..
                        } => {
                            let subject = self.lower_type(subject, generics)?;
                            self.check_facet_state(&subject, state)?;
                            Ok(Ty::InState(Box::new(subject), state.value.clone()))
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
                    "Decimal" => Ty::Decimal,
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

    /// A `declares` clause is a predicate over which properties were stated,
    /// not an expression over their values. Keeping its operators to `and`,
    /// `or`, and `not` is what stops it becoming a second expression language.
    fn check_presence_predicate(
        &self,
        predicate: &Expr,
        fields: &BTreeMap<String, SchemaField>,
        declared: &[FieldDecl],
    ) -> Result<CheckedPresence, SemanticError> {
        match predicate {
            Expr::Path(path) => {
                let name = super::path_text(path);
                let Some(field) = fields.get(&name) else {
                    return Err(SemanticError::new(
                        path.span,
                        format!("'{name}' is not a property of this kind"),
                    )
                    .help("a completeness rule names properties the schema declares"));
                };
                // A required field is already stated by every declaration, so
                // naming it here asserts nothing. The two ways of saying a
                // property is needed would then disagree in every case that
                // matters, and the author has to pick one.
                if !field.optional {
                    let mut error = SemanticError::new(
                        path.span,
                        format!("'{name}' is required, so a completeness rule cannot mention it"),
                    );
                    if let Some(declaration) =
                        declared.iter().find(|field| field.name.value == name)
                    {
                        error = error.related(declaration.name.span, "declared required here");
                    }
                    return Err(error
                        .help(format!(
                            "write '{name}?:' if it is one of several ways to be complete"
                        ))
                        .help(format!(
                            "or drop '{name}' from the rule if every declaration must state it"
                        )));
                }
                Ok(CheckedPresence::Property { name })
            }
            Expr::Binary {
                op: op @ (crate::ast::BinaryOp::And | crate::ast::BinaryOp::Or),
                left,
                right,
                ..
            } => {
                let parts = vec![
                    self.check_presence_predicate(left, fields, declared)?,
                    self.check_presence_predicate(right, fields, declared)?,
                ];
                Ok(if matches!(op, crate::ast::BinaryOp::And) {
                    CheckedPresence::All { parts }
                } else {
                    CheckedPresence::Any { parts }
                })
            }
            Expr::Unary {
                op: crate::ast::UnaryOp::Not,
                operand,
                ..
            } => Ok(CheckedPresence::Not {
                part: Box::new(self.check_presence_predicate(operand, fields, declared)?),
            }),
            other => Err(SemanticError::new(
                other.span(),
                "a completeness rule combines property names with 'and', 'or', and 'not'",
            )
            .help("it asks which properties were stated, not what their values are")),
        }
    }

    /// The error for a word no imported package declares.
    fn unknown_artifact_kind(&self, word: &str, span: Span) -> SemanticError {
        let mut known = self.artifact_kinds.keys().cloned().collect::<Vec<_>>();
        known.sort();
        let mut error = SemanticError::new(span, format!("unknown declaration kind '{word}'"));
        if known.is_empty() {
            error = error.help("a package declares one with 'artifact <Type>:'");
        } else {
            error = error.help(format!("kinds in scope: {}", known.join(", ")));
        }
        error
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

#[cfg(test)]
mod tests {
    use super::{conjunction, nearest, quoted_conjunction};

    #[test]
    fn suggests_the_name_a_typo_meant() {
        let fields = ["digest_temperature", "supplier"];
        assert_eq!(
            nearest("digest_temperatue", fields.into_iter()),
            Some("digest_temperature")
        );
    }

    #[test]
    fn suggests_the_name_a_short_spelling_opens() {
        // Many edits away, but a prefix of exactly what was meant.
        let fields = ["digest_temperature", "supplier"];
        assert_eq!(
            nearest("digest_temp", fields.into_iter()),
            Some("digest_temperature")
        );
    }

    #[test]
    fn offers_nothing_for_an_unrelated_name() {
        let fields = ["digest_temperature", "supplier"];
        assert_eq!(nearest("volume", fields.into_iter()), None);
    }

    #[test]
    fn lists_names_as_a_sentence() {
        assert_eq!(conjunction(&["a"]), "a");
        assert_eq!(conjunction(&["a", "b"]), "a and b");
        assert_eq!(conjunction(&["a", "b", "c"]), "a, b, and c");
        assert_eq!(quoted_conjunction(&["a", "b"]), "'a' and 'b'");
    }
}
