//! Semantic state owned by one checker invocation.
//!
//! Keeping catalogs and symbol tables here makes their lifetime and mutation
//! boundary explicit. Checking algorithms operate on this state but do not
//! own ad-hoc global maps of their own.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::ast::Path;
use crate::checked::{CheckedActionOperand, CheckedActionResult, CheckedPhraseToken, CheckedType};
use crate::semantic_error::SemanticError;
use crate::semantics::{
    ActionSurface, DefinitionId, ExportKind, ModuleId, ModuleInterface, SemanticEnvironment,
};
use crate::source::Span;
use crate::standard_library::{
    ActionContractSpec, ConstructorSpec, ContractType, Lineage, PhrasePart, PureFunctionSpec,
    ResultSpec, StandardLibrary, StandardModule, TypeSpec,
};
use crate::type_system::{Ty, from_checked_type};

/// The type parameters a callable introduces, in the order they appear, with
/// whatever each is bounded by.
#[derive(Clone, Default)]
pub(super) struct Generics {
    pub parameters: Vec<String>,
    pub bounds: HashMap<String, Ty>,
}

impl Generics {
    pub fn names(&self) -> BTreeSet<String> {
        self.parameters.iter().cloned().collect()
    }
}

#[derive(Clone)]
pub(super) struct CircuitSignature {
    pub generics: Generics,
    pub inputs: Vec<Ty>,
    pub output: Ty,
}

#[derive(Clone)]
pub(super) struct DataSignature {
    /// Type parameters in declaration order. Empty for an imported type, whose
    /// parameters the module interface does not yet carry, which leaves its
    /// arguments unchecked exactly as before.
    pub parameters: Vec<String>,
    pub bounds: HashMap<String, Ty>,
    pub fields: BTreeMap<String, Ty>,
    pub cases: BTreeMap<String, BTreeMap<String, Ty>>,
}

/// A property an artifact kind declares, and whether an instance may omit it.
#[derive(Clone)]
pub(super) struct SchemaField {
    pub ty: Ty,
    pub optional: bool,
}

/// What a package's artifact word means: the type its instances have, the
/// properties they may state, and which combinations are complete.
#[derive(Clone)]
pub(super) struct ArtifactKindSignature {
    pub produces: Ty,
    pub fields: BTreeMap<String, SchemaField>,
    pub declares: Option<crate::checked::CheckedPresence>,
}

/// Rebuild the contract a workflow checks against from an imported action's
/// surface.
///
/// A hole becomes a quantity slot for a measurement, an integer slot for a
/// whole number, and an operand otherwise, exactly as the declaring module
/// decided; the surface carries enough to make the same choice.
fn action_contract_from_surface(surface: &ActionSurface) -> ActionContractSpec {
    let operand = |name: &str| {
        surface
            .operands
            .iter()
            .find(|operand| operand.name == name)
            .expect("an action surface names every operand its phrase holds")
    };
    let phrase = surface
        .phrase
        .iter()
        .map(|token| match token {
            CheckedPhraseToken::Word(word) => PhrasePart::word(word),
            CheckedPhraseToken::Hole(name) => {
                let operand = operand(name);
                match from_checked_type(&operand.r#type) {
                    Ty::Quantity(unit) => PhrasePart::quantity(name, false, &[unit.as_str()]),
                    Ty::Integer => PhrasePart::integer(name, false),
                    ty => PhrasePart::operand(name, ContractType::Concrete(ty), operand.mode),
                }
            }
        })
        .collect();
    let results = surface
        .results
        .iter()
        .map(|result| ResultSpec {
            name: result.name.clone(),
            r#type: ContractType::Concrete(from_checked_type(&result.r#type)),
            lineage: Lineage::Continues,
        })
        .collect();
    ActionContractSpec {
        operation: surface.operation.clone(),
        phrase,
        results,
        inert: Vec::new(),
    }
}

/// The full contract of an action declared in source.
#[derive(Clone)]
pub(super) struct CheckedActionContract {
    pub operation: String,
    pub phrase: Vec<CheckedPhraseToken>,
    pub operands: Vec<CheckedActionOperand>,
    pub results: Vec<CheckedActionResult>,
    pub capability: String,
}

/// A facet in scope: the type it classifies, its states, and the state changes
/// it admits.
#[derive(Clone)]
pub(super) struct FacetSignature {
    pub subject: Ty,
    /// The states in declaration order, so the first stays identifiable as the
    /// state a newly established material is in.
    pub states: Vec<FacetState>,
    pub transitions: Vec<(String, String)>,
}

impl FacetSignature {
    /// The state this facet admits under that name, if it admits one.
    pub fn state(&self, name: &str) -> Option<&FacetState> {
        self.states.iter().find(|state| state.name == name)
    }
}

#[derive(Clone)]
pub(super) struct FacetState {
    pub doc: Option<String>,
    pub name: String,
    pub fields: BTreeMap<String, SchemaField>,
}

#[derive(Clone)]
pub(super) struct WorkflowSignature {
    pub generics: Generics,
    pub inputs: Vec<Ty>,
    pub outputs: Vec<(String, Ty)>,
}

pub(super) struct SemanticContext {
    pub module_id: ModuleId,
    pub provided_modules: SemanticEnvironment,
    pub standard_library: StandardLibrary,
    pub known_types: BTreeSet<String>,
    pub standard_types: HashMap<String, TypeSpec>,
    pub imports: BTreeSet<String>,
    pub import_providers: HashMap<String, String>,
    pub imported_names: HashMap<String, String>,
    pub values: HashMap<String, Ty>,
    pub pure_functions: HashMap<String, PureFunctionSpec>,
    pub constructors: HashMap<String, ConstructorSpec>,
    pub actions: HashMap<String, ActionContractSpec>,
    /// The full contract of each action declared in source, by verb.
    ///
    /// The action map above is what a workflow checks against, shared with the
    /// standard library. This carries the extra a source declaration states and
    /// exports: the phrase, the operands and results with their types, and the
    /// capability, so the compiler can derive a method to run the verb.
    pub action_contracts: HashMap<String, CheckedActionContract>,
    pub circuits: HashMap<String, CircuitSignature>,
    pub data: HashMap<String, DataSignature>,
    pub cases: HashMap<String, String>,
    pub event_types: BTreeSet<String>,
    pub workflows: HashMap<String, WorkflowSignature>,
    pub artifact_kinds: HashMap<String, ArtifactKindSignature>,
    /// Every role in scope. A role shares the type namespace so the two cannot
    /// collide, but it is not a type: it may only bound a type parameter.
    pub roles: BTreeSet<String>,
    /// The roles each type plays. Membership declared in source and membership
    /// built into the standard library land here together, so a bound is
    /// satisfied the same way whichever it came from.
    pub type_roles: HashMap<String, BTreeSet<String>>,
    /// The ontology term each role stands for, for the roles that name one.
    ///
    /// A role grounded this way is what lets a type be resolved to the terms an
    /// SBOL document states about it. A role with no term classifies types and
    /// says nothing about any ontology.
    pub role_terms: HashMap<String, String>,
    /// Every facet in scope, by facet name.
    pub facets: HashMap<String, FacetSignature>,
    /// The facets classifying each subject type, by the type's name.
    ///
    /// A material type constrains a state without naming the facet it belongs
    /// to, so resolving `Material<Chassis is competent>` starts from the
    /// subject and asks which of its facets admits that state.
    pub type_facets: HashMap<String, BTreeSet<String>>,
}

impl SemanticContext {
    pub fn new(module_id: ModuleId, provided_modules: SemanticEnvironment) -> Self {
        Self::with_library(module_id, provided_modules, StandardLibrary::bundled())
    }

    pub fn with_library(
        module_id: ModuleId,
        provided_modules: SemanticEnvironment,
        standard_library: StandardLibrary,
    ) -> Self {
        Self {
            module_id,
            provided_modules,
            standard_library,
            known_types: ["Bool", "Decimal", "Integer", "None", "String"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            standard_types: HashMap::new(),
            imports: BTreeSet::new(),
            import_providers: HashMap::new(),
            imported_names: HashMap::new(),
            values: HashMap::new(),
            pure_functions: HashMap::new(),
            constructors: HashMap::new(),
            actions: HashMap::new(),
            action_contracts: HashMap::new(),
            circuits: HashMap::new(),
            data: HashMap::new(),
            cases: HashMap::new(),
            event_types: BTreeSet::new(),
            workflows: HashMap::new(),
            artifact_kinds: HashMap::new(),
            roles: BTreeSet::new(),
            type_roles: HashMap::new(),
            role_terms: HashMap::new(),
            facets: HashMap::new(),
            type_facets: HashMap::new(),
        }
    }

    /// Whether a value of type `actual` may be used where `expected` is
    /// required. Deciding this needs the role table, because a forgotten type
    /// argument is satisfied by playing a role.
    pub fn compatible(&self, actual: &Ty, expected: &Ty) -> bool {
        crate::type_system::compatible(&self.type_roles, actual, expected)
    }

    pub fn common_type(&self, left: Ty, right: Ty) -> Ty {
        crate::type_system::common_type(&self.type_roles, left, right)
    }

    pub fn comparable(&self, left: &Ty, right: &Ty) -> bool {
        crate::type_system::comparable(&self.type_roles, left, right)
    }

    pub fn unify(
        &self,
        template: &Ty,
        actual: &Ty,
        parameters: &[String],
        substitutions: &mut crate::type_system::Substitutions,
        span: Span,
    ) -> Result<(), SemanticError> {
        crate::type_system::unify(
            &self.type_roles,
            template,
            actual,
            parameters,
            substitutions,
            span,
        )
    }

    pub fn add_role(&mut self, ty: &str, role: &str) {
        self.type_roles
            .entry(ty.to_owned())
            .or_default()
            .insert(role.to_owned());
    }

    /// Every type known to play a role, for naming the alternatives when a
    /// bound is not satisfied.
    pub fn role_members(&self, role: &str) -> Vec<&str> {
        let mut members = self
            .type_roles
            .iter()
            .filter(|(_, roles)| roles.contains(role))
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>();
        members.sort_unstable();
        members
    }

    pub fn definition_for_path(&self, path: &Path) -> DefinitionId {
        let name = path
            .segments
            .first()
            .map(|segment| segment.value.as_str())
            .unwrap_or_default();
        self.definition_for_name(name)
    }

    pub fn definition_for_action_word(&self, word: &str) -> DefinitionId {
        self.definition_for_name(word.split('.').next().unwrap_or(word))
    }

    fn definition_for_name(&self, name: &str) -> DefinitionId {
        let module = self
            .imported_names
            .get(name)
            .map(String::as_str)
            .unwrap_or_else(|| self.module_id.as_str());
        DefinitionId::exported(module, name)
    }

    pub fn register_standard_module(
        &mut self,
        module: StandardModule,
        span: Span,
    ) -> Result<(), SemanticError> {
        for spec in module.types {
            let name = spec.name;
            if !self.known_types.insert(name.to_owned()) {
                return Err(SemanticError::new(
                    span,
                    format!("imported type '{name}' is ambiguous"),
                ));
            }
            if spec.role {
                self.roles.insert(name.to_owned());
            }
            for role in &spec.implements {
                self.add_role(name, role);
            }
            self.standard_types.insert(name.to_owned(), spec);
        }
        for (name, ty) in module.values {
            self.insert_imported_name(module.path, name, span)?;
            self.values.insert(name.to_owned(), ty);
        }
        for function in module.functions {
            self.insert_imported_name(module.path, function.name, span)?;
            self.pure_functions
                .insert(function.name.to_owned(), function);
        }
        for constructor in module.constructors {
            self.insert_imported_name(module.path, constructor.name, span)?;
            self.constructors
                .insert(constructor.name.to_owned(), constructor);
        }
        for action in module.actions {
            let name = action
                .source_name()
                .expect("catalog validation guarantees an action source name");
            self.insert_imported_name(module.path, name, span)?;
            self.actions.insert(name.to_owned(), action);
        }
        Ok(())
    }

    pub fn register_module_interface(
        &mut self,
        interface: &ModuleInterface,
        span: Span,
    ) -> Result<(), SemanticError> {
        for (name, export) in &interface.exports {
            if !matches!(export.kind, ExportKind::Type | ExportKind::Role) {
                continue;
            }
            if !self.known_types.insert(name.clone()) {
                return Err(SemanticError::new(
                    span,
                    format!("imported type '{name}' is ambiguous"),
                ));
            }
            if export.kind == ExportKind::Role {
                self.roles.insert(name.clone());
                if let Some(term) = &export.term {
                    self.role_terms.insert(name.clone(), term.clone());
                }
                continue;
            }
            for role in &export.roles {
                self.add_role(name, role);
            }
            self.data.insert(
                name.clone(),
                DataSignature {
                    parameters: export.parameters.names.clone(),
                    bounds: export
                        .parameters
                        .bounds
                        .iter()
                        .map(|(name, bound)| (name.clone(), from_checked_type(bound)))
                        .collect(),
                    fields: export
                        .fields
                        .iter()
                        .map(|(name, ty)| (name.clone(), from_checked_type(ty)))
                        .collect(),
                    cases: BTreeMap::new(),
                },
            );
        }

        for (name, export) in &interface.exports {
            match export.kind {
                // Types and roles are registered above, before anything that
                // could refer to them.
                ExportKind::Type | ExportKind::Role => {}
                // A facet means nothing without its states, so the two travel
                // together the way a kind travels with its schema.
                ExportKind::Facet => {
                    if let Some(surface) = &export.facet {
                        let subject = from_checked_type(&surface.subject);
                        if let Ty::Named(subject_name, _) = &subject {
                            self.type_facets
                                .entry(subject_name.clone())
                                .or_default()
                                .insert(name.clone());
                        }
                        self.facets.insert(
                            name.clone(),
                            FacetSignature {
                                subject,
                                states: surface
                                    .states
                                    .iter()
                                    .map(|state| FacetState {
                                        doc: state.doc.clone(),
                                        name: state.name.clone(),
                                        fields: state
                                            .fields
                                            .iter()
                                            .map(|field| {
                                                (
                                                    field.name.clone(),
                                                    SchemaField {
                                                        ty: from_checked_type(&field.r#type),
                                                        optional: field.optional,
                                                    },
                                                )
                                            })
                                            .collect(),
                                    })
                                    .collect(),
                                transitions: surface
                                    .transitions
                                    .iter()
                                    .map(|transition| {
                                        (transition.from.clone(), transition.to.clone())
                                    })
                                    .collect(),
                            },
                        );
                    }
                }
                // A word a package supplies means nothing without its schema,
                // so the two travel together.
                ExportKind::ArtifactKind => {
                    if let Some(schema) = &export.schema {
                        // A kind's roles classify the type it produces, so an
                        // importer sees the same membership the declaring
                        // module did and grounds the type the same way.
                        if let CheckedType::Named { name: produced, .. } = &schema.produces {
                            for role in &export.roles {
                                self.add_role(produced, role);
                            }
                        }
                        let fields = schema.fields.iter().map(|field| {
                            (
                                field.name.clone(),
                                SchemaField {
                                    ty: from_checked_type(&field.r#type),
                                    optional: field.optional,
                                },
                            )
                        });
                        // Several packages describe one kind: what a plasmid is
                        // comes from one, and what a method needs to build one
                        // from another. Importing both gives a schema carrying
                        // everything either says, and a rule from whichever
                        // states it.
                        match self.artifact_kinds.get_mut(name) {
                            Some(existing) => {
                                existing.fields.extend(fields);
                                existing.declares = existing
                                    .declares
                                    .clone()
                                    .or_else(|| schema.declares.clone());
                            }
                            None => {
                                self.artifact_kinds.insert(
                                    name.clone(),
                                    ArtifactKindSignature {
                                        produces: from_checked_type(&schema.produces),
                                        fields: fields.collect(),
                                        declares: schema.declares.clone(),
                                    },
                                );
                            }
                        }
                    }
                }
                ExportKind::Value => {
                    self.insert_imported_name(interface.module.as_str(), name, span)?;
                    if let Some(ty) = &export.r#type {
                        self.values.insert(name.clone(), from_checked_type(ty));
                    }
                }
                ExportKind::Function => {
                    self.insert_imported_name(interface.module.as_str(), name, span)?;
                    if let Some(signature) = &export.callable {
                        let [output] = signature.outputs.as_slice() else {
                            return Err(SemanticError::new(
                                span,
                                format!("function '{name}' must declare exactly one result"),
                            ));
                        };
                        self.circuits.insert(
                            name.clone(),
                            CircuitSignature {
                                generics: Generics {
                                    parameters: export.parameters.names.clone(),
                                    bounds: export
                                        .parameters
                                        .bounds
                                        .iter()
                                        .map(|(name, bound)| {
                                            (name.clone(), from_checked_type(bound))
                                        })
                                        .collect(),
                                },
                                inputs: signature.inputs.iter().map(from_checked_type).collect(),
                                output: from_checked_type(&output.r#type),
                            },
                        );
                    }
                }
                ExportKind::Action => {
                    self.insert_imported_name(interface.module.as_str(), name, span)?;
                    if let Some(surface) = &export.action {
                        self.actions
                            .insert(name.clone(), action_contract_from_surface(surface));
                    }
                }
                ExportKind::Workflow => {
                    self.insert_imported_name(interface.module.as_str(), name, span)?;
                    if let Some(signature) = &export.callable {
                        self.workflows.insert(
                            name.clone(),
                            WorkflowSignature {
                                generics: Generics {
                                    parameters: export.parameters.names.clone(),
                                    bounds: export
                                        .parameters
                                        .bounds
                                        .iter()
                                        .map(|(name, bound)| {
                                            (name.clone(), from_checked_type(bound))
                                        })
                                        .collect(),
                                },
                                inputs: signature.inputs.iter().map(from_checked_type).collect(),
                                outputs: signature
                                    .outputs
                                    .iter()
                                    .map(|field| {
                                        (field.name.clone(), from_checked_type(&field.r#type))
                                    })
                                    .collect(),
                            },
                        );
                    }
                }
                ExportKind::Constructor => {
                    self.insert_imported_name(interface.module.as_str(), name, span)?;
                    let Some(CheckedType::Named { name: parent, .. }) = &export.r#type else {
                        continue;
                    };
                    self.cases.insert(name.clone(), parent.clone());
                    if let Some(signature) = self.data.get_mut(parent) {
                        let base = signature.fields.keys().cloned().collect::<BTreeSet<_>>();
                        let fields = export
                            .fields
                            .iter()
                            .filter(|(name, _)| !base.contains(*name))
                            .map(|(name, ty)| (name.clone(), from_checked_type(ty)))
                            .collect();
                        signature.cases.insert(name.clone(), fields);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn insert_imported_name(
        &mut self,
        module: &str,
        name: &str,
        span: Span,
    ) -> Result<(), SemanticError> {
        if let Some(previous) = self
            .imported_names
            .insert(name.to_owned(), module.to_owned())
        {
            return Err(SemanticError::new(
                span,
                format!("imported name '{name}' is ambiguous between '{previous}' and '{module}'"),
            ));
        }
        Ok(())
    }

    pub fn insert_import(
        &mut self,
        path: &str,
        provider: &str,
        span: Span,
    ) -> Result<(), SemanticError> {
        if !self.imports.insert(path.to_owned()) {
            return Err(SemanticError::new(
                span,
                format!("module '{path}' is imported more than once"),
            ));
        }
        self.import_providers
            .insert(path.to_owned(), provider.to_owned());
        Ok(())
    }
}
