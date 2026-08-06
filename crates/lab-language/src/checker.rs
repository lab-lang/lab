use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::{Deref, DerefMut};

mod context;
mod interface;

use context::{CircuitSignature, DataSignature, SemanticContext, WorkflowSignature};
use interface::build_interface;

use crate::ast::*;
use crate::checked::*;
use crate::semantic_error::SemanticError;
use crate::semantics::{DefinitionId, ModuleId, SemanticEnvironment};
use crate::source::Span;
use crate::standard_library::{ActionContractSpec, ConstructorSpec, ContractType, PhrasePart};
use crate::type_system::{
    Ty, common_type, comparable, compatible, substitute, to_checked_type, unify,
};

pub(crate) fn check_module(
    module_id: ModuleId,
    environment: &SemanticEnvironment,
    module: &Module,
) -> Result<CheckedModule, SemanticError> {
    Checker::new(module_id, environment.clone()).check(module)
}

struct Checker {
    context: SemanticContext,
}

impl Deref for Checker {
    type Target = SemanticContext;

    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl DerefMut for Checker {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.context
    }
}

impl Checker {
    fn new(module_id: ModuleId, provided_modules: SemanticEnvironment) -> Self {
        Self {
            context: SemanticContext::new(module_id, provided_modules),
        }
    }

    fn check(mut self, module: &Module) -> Result<CheckedModule, SemanticError> {
        self.resolve_imports(module)?;
        self.collect_declarations(module)?;

        let mut declarations = Vec::new();
        for item in &module.items {
            match item {
                Item::Use(_) => {}
                Item::Circuit(declaration) => {
                    declarations.push(self.check_circuit(declaration)?);
                }
                Item::Artifact(declaration) => {
                    declarations.push(self.check_artifact(declaration)?);
                }
                Item::Data(declaration) => {
                    declarations.push(self.checked_data(declaration)?);
                }
                Item::Workflow(declaration) => {
                    declarations.push(self.check_workflow(declaration)?);
                }
                Item::Binding(binding) => {
                    let (checked, inferred) =
                        self.check_binding(binding, &mut self.values.clone())?;
                    self.values.insert(binding.names[0].value.clone(), inferred);
                    declarations.push(CheckedDeclaration::Binding(checked));
                }
            }
        }

        let interface = build_interface(&self.module_id, &declarations);
        Ok(CheckedModule {
            schema_version: PORTABLE_MODULE_SCHEMA_VERSION.to_owned(),
            module: self.module_id.clone(),
            interface,
            imports: self
                .imports
                .iter()
                .map(|module| ResolvedImport {
                    module: module.clone(),
                    provider: self
                        .import_providers
                        .get(module)
                        .cloned()
                        .unwrap_or_else(|| "builtin-standard-library".to_owned()),
                })
                .collect(),
            declarations,
        })
    }

    fn resolve_imports(&mut self, module: &Module) -> Result<(), SemanticError> {
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
            let path = path_text(&import.path);
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

    fn collect_declarations(&mut self, module: &Module) -> Result<(), SemanticError> {
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

    fn collect_fields(
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

    fn lower_workflow_outputs(
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

    fn check_circuit(
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
                .map(|(field, ty)| checked_field(&field.name.value, ty))
                .collect(),
            output: to_checked_type(&signature.output),
            sections,
        })
    }

    fn checked_data(&self, declaration: &DataDecl) -> Result<CheckedDeclaration, SemanticError> {
        let signature = self
            .data
            .get(&declaration.name.value)
            .expect("data was collected");
        Ok(CheckedDeclaration::Data {
            category: format!("{:?}", declaration.kind).to_ascii_lowercase(),
            name: declaration.name.value.clone(),
            fields: signature
                .fields
                .iter()
                .map(|(name, ty)| checked_field(name, ty))
                .collect(),
            cases: signature
                .cases
                .iter()
                .map(|(name, fields)| CheckedCase {
                    name: name.clone(),
                    fields: fields
                        .iter()
                        .map(|(name, ty)| checked_field(name, ty))
                        .collect(),
                })
                .collect(),
        })
    }

    fn check_artifact(
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
            artifact: kind,
            name: declaration.name.value.clone(),
            properties,
            requirements,
            acceptance,
        })
    }

    /// A plasmid states its sequence directly, or states the backbone and cargo
    /// a sequence can be derived from.
    fn check_plasmid_shape(
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
    fn check_strain_shape(
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

    fn check_workflow(
        &self,
        declaration: &WorkflowDecl,
    ) -> Result<CheckedDeclaration, SemanticError> {
        let signature = self
            .workflows
            .get(&declaration.name.value)
            .expect("workflow was collected");
        let mut environment = self.values.clone();
        environment.insert("workflow".to_owned(), Ty::named("WorkflowContext"));
        for (field, ty) in declaration.inputs.iter().zip(&signature.inputs) {
            environment.insert(field.name.value.clone(), ty.clone());
        }
        let mut state = Vec::new();
        let mut state_types = HashMap::new();
        let mut body_start = 0;
        for statement in &declaration.body {
            let Stmt::State(declaration) = statement else {
                break;
            };
            let ty = self.lower_type(&declaration.ty, &BTreeSet::new())?;
            let initial_ty = self.infer_expr(&declaration.initial, &environment)?;
            if !compatible(&initial_ty, &ty) {
                return Err(SemanticError::new(
                    declaration.initial.span(),
                    format!("state has type {initial_ty}, but declaration requires {ty}"),
                ));
            }
            if environment
                .insert(declaration.name.value.clone(), ty.clone())
                .is_some()
            {
                return Err(SemanticError::new(
                    declaration.name.span,
                    format!("duplicate state name '{}'", declaration.name.value),
                ));
            }
            state_types.insert(declaration.name.value.clone(), ty.clone());
            state.push(CheckedState {
                name: declaration.name.value.clone(),
                r#type: to_checked_type(&ty),
                initial: self.lower_checked_expr(&declaration.initial, &environment, Some(&ty))?,
            });
            body_start += 1;
        }
        let checked = self.check_block(
            &declaration.body[body_start..],
            &environment,
            &signature.outputs,
            &state_types,
        )?;
        if !checked.terminates
            && !declaration
                .body
                .iter()
                .any(|statement| matches!(statement, Stmt::When(_)))
        {
            return Err(SemanticError::new(
                declaration.span,
                format!(
                    "workflow '{}' may finish without returning its declared results",
                    declaration.name.value
                ),
            ));
        }
        Ok(CheckedDeclaration::Workflow {
            name: declaration.name.value.clone(),
            inputs: declaration
                .inputs
                .iter()
                .zip(&signature.inputs)
                .map(|(field, ty)| checked_field(&field.name.value, ty))
                .collect(),
            outputs: signature
                .outputs
                .iter()
                .map(|(name, ty)| checked_field(name, ty))
                .collect(),
            state,
            body: checked.statements,
        })
    }

    fn check_block(
        &self,
        statements: &[Stmt],
        starting_environment: &HashMap<String, Ty>,
        outputs: &[(String, Ty)],
        state_types: &HashMap<String, Ty>,
    ) -> Result<CheckedBlock, SemanticError> {
        let mut environment = starting_environment.clone();
        let mut checked = Vec::new();
        let mut terminates = false;
        for statement in statements {
            if terminates {
                return Err(SemanticError::new(
                    statement.span(),
                    "unreachable statement",
                ));
            }
            match statement {
                Stmt::State(state) => {
                    return Err(SemanticError::new(
                        state.span,
                        "state declarations must appear before workflow statements",
                    ));
                }
                Stmt::Binding(binding) => {
                    let is_state_update = state_types.contains_key(&binding.names[0].value);
                    if environment.contains_key(&binding.names[0].value) && !is_state_update {
                        return Err(SemanticError::new(
                            binding.span,
                            format!(
                                "cannot reassign '{}'; declare durable workflow memory with 'state'",
                                binding.names[0].value
                            ),
                        ));
                    }
                    let (binding_ir, ty) = self.check_binding(binding, &mut environment)?;
                    if let Some(state_type) = state_types.get(&binding.names[0].value) {
                        if !compatible(&ty, state_type) {
                            return Err(SemanticError::new(
                                binding.span,
                                format!("state update expects {state_type}, found {ty}"),
                            ));
                        }
                        checked.push(CheckedStatement::StateUpdate {
                            state: binding.names[0].value.clone(),
                            value: binding_ir.value,
                        });
                    } else {
                        checked.push(CheckedStatement::Binding(binding_ir));
                    }
                }
                Stmt::Effect(effect) => {
                    let (action, result_types) = self.check_effect(effect, &environment)?;
                    if effect.names.len() != result_types.len() {
                        return Err(SemanticError::new(
                            effect.span,
                            format!(
                                "operation '{}' returns {} value(s), but {} name(s) were provided",
                                action.operation,
                                result_types.len(),
                                effect.names.len()
                            ),
                        ));
                    }
                    let results = effect
                        .names
                        .iter()
                        .zip(result_types)
                        .map(|(name, ty)| {
                            environment.insert(name.value.clone(), ty.clone());
                            checked_field(&name.value, &ty)
                        })
                        .collect();
                    checked.push(CheckedStatement::Effect { results, action });
                }
                Stmt::Return(statement) => {
                    if statement.values.len() != outputs.len() {
                        return Err(SemanticError::new(
                            statement.span,
                            format!(
                                "workflow returns {} value(s), expected {}",
                                statement.values.len(),
                                outputs.len()
                            ),
                        ));
                    }
                    let values = statement
                        .values
                        .iter()
                        .zip(outputs)
                        .map(|(value, (name, expected))| {
                            let actual = self.infer_expr(value, &environment)?;
                            if !compatible(&actual, expected) {
                                return Err(SemanticError::new(
                                    value.span(),
                                    format!(
                                        "workflow result '{name}' has type {actual}, expected {expected}"
                                    ),
                                ));
                            }
                            Ok(CheckedFieldValue {
                                name: name.clone(),
                                value: self.lower_checked_expr(
                                    value,
                                    &environment,
                                    Some(&actual),
                                )?,
                            })
                        })
                        .collect::<Result<Vec<_>, SemanticError>>()?;
                    checked.push(CheckedStatement::Return { values });
                    terminates = true;
                }
                Stmt::If(statement) => {
                    self.require_bool(&statement.condition, &environment, "if")?;
                    let then_block =
                        self.check_block(&statement.then_body, &environment, outputs, state_types)?;
                    let else_block =
                        self.check_block(&statement.else_body, &environment, outputs, state_types)?;
                    if !statement.else_body.is_empty()
                        && then_block.terminates
                        && else_block.terminates
                    {
                        terminates = true;
                    }
                    checked.push(CheckedStatement::If {
                        condition: self.lower_checked_expr(
                            &statement.condition,
                            &environment,
                            Some(&Ty::Bool),
                        )?,
                        body: then_block.statements,
                        else_body: else_block.statements,
                    });
                }
                Stmt::Match(statement) => {
                    let matched_type = self.infer_expr(&statement.value, &environment)?;
                    let mut cases = Vec::new();
                    let mut seen_patterns = BTreeSet::new();
                    let mut has_binding_pattern = false;
                    let mut continuing_environments = Vec::new();
                    for case in &statement.cases {
                        self.check_pattern(&case.pattern, &matched_type)?;
                        match &case.pattern {
                            Pattern::Name(_) => has_binding_pattern = true,
                            Pattern::Constructor { path, .. } => {
                                let name = path_text(path);
                                if !seen_patterns.insert(name.clone()) {
                                    return Err(SemanticError::new(
                                        path.span,
                                        format!("duplicate match case '{name}'"),
                                    ));
                                }
                            }
                        }
                        if let Some(guard) = &case.guard {
                            self.require_bool(guard, &environment, "case guard")?;
                        }
                        let block =
                            self.check_block(&case.body, &environment, outputs, state_types)?;
                        if !block.terminates {
                            continuing_environments.push(block.environment.clone());
                        }
                        cases.push(CheckedMatchCase {
                            pattern: self.checked_pattern(&case.pattern),
                            body: block.statements,
                            terminates: block.terminates,
                        });
                    }
                    if !has_binding_pattern
                        && let Ty::Named(name, _) = &matched_type
                        && let Some(signature) = self.data.get(name)
                        && !signature.cases.is_empty()
                    {
                        let missing = signature
                            .cases
                            .keys()
                            .filter(|name| !seen_patterns.contains(*name))
                            .cloned()
                            .collect::<Vec<_>>();
                        if !missing.is_empty() {
                            return Err(SemanticError::new(
                                statement.span,
                                format!("non-exhaustive match; missing cases {missing:?}"),
                            ));
                        }
                    }
                    if continuing_environments.is_empty() {
                        terminates = true;
                    } else {
                        promote_common_bindings(
                            &mut environment,
                            starting_environment,
                            &continuing_environments,
                        );
                    }
                    checked.push(CheckedStatement::Match {
                        value: self.lower_checked_expr(
                            &statement.value,
                            &environment,
                            Some(&matched_type),
                        )?,
                        cases,
                    });
                }
                Stmt::For(statement) => {
                    let iterable = self.infer_expr(&statement.iterable, &environment)?;
                    let element = match iterable {
                        Ty::List(element) => *element,
                        other => {
                            return Err(SemanticError::new(
                                statement.iterable.span(),
                                format!("for expects a collection, found {other}"),
                            ));
                        }
                    };
                    let mut loop_environment = environment.clone();
                    loop_environment.insert(statement.binding.value.clone(), element.clone());
                    let body =
                        self.check_block(&statement.body, &loop_environment, outputs, state_types)?;
                    checked.push(CheckedStatement::For {
                        binding: checked_field(&statement.binding.value, &element),
                        iterable: self.lower_checked_expr(
                            &statement.iterable,
                            &environment,
                            None,
                        )?,
                        body: body.statements,
                    });
                }
                Stmt::When(statement) => {
                    let trigger = match &statement.trigger {
                        Trigger::Every(expression) => {
                            self.require_duration(expression, &environment)?;
                            CheckedTrigger::Every {
                                duration: self.lower_checked_expr(
                                    expression,
                                    &environment,
                                    None,
                                )?,
                            }
                        }
                        Trigger::After(expression) => {
                            self.require_duration(expression, &environment)?;
                            CheckedTrigger::After {
                                duration: self.lower_checked_expr(
                                    expression,
                                    &environment,
                                    None,
                                )?,
                            }
                        }
                        Trigger::Event(expression) => CheckedTrigger::Event {
                            expression: self.lower_checked_expr(expression, &environment, None)?,
                        },
                    };
                    let body =
                        self.check_block(&statement.body, &environment, outputs, state_types)?;
                    checked.push(CheckedStatement::When {
                        trigger,
                        body: body.statements,
                    });
                }
                Stmt::Emit(statement) => {
                    let ty = self.infer_expr(&statement.event, &environment)?;
                    let is_event =
                        matches!(&ty, Ty::Named(name, _) if self.event_types.contains(name));
                    if !is_event {
                        return Err(SemanticError::new(
                            statement.event.span(),
                            format!("emit expects an event value, found {ty}"),
                        ));
                    }
                    checked.push(CheckedStatement::Emit {
                        event: self.lower_checked_expr(
                            &statement.event,
                            &environment,
                            Some(&ty),
                        )?,
                    });
                }
            }
        }
        Ok(CheckedBlock {
            statements: checked,
            environment,
            terminates,
        })
    }

    fn check_binding(
        &self,
        binding: &BindingStmt,
        environment: &mut HashMap<String, Ty>,
    ) -> Result<(CheckedBinding, Ty), SemanticError> {
        let inferred = self.infer_expr(&binding.value, environment)?;
        let ty = if let Some(annotation) = &binding.annotation {
            let declared = self.lower_type(annotation, &BTreeSet::new())?;
            if !compatible(&inferred, &declared) {
                return Err(SemanticError::new(
                    binding.value.span(),
                    format!("binding has type {inferred}, but annotation requires {declared}"),
                ));
            }
            declared
        } else {
            inferred
        };
        if let Some(previous) = environment.get(&binding.names[0].value)
            && !compatible(&ty, previous)
        {
            return Err(SemanticError::new(
                binding.span,
                format!(
                    "cannot replace binding '{}' of type {previous} with {ty}",
                    binding.names[0].value
                ),
            ));
        }
        environment.insert(binding.names[0].value.clone(), ty.clone());
        Ok((
            CheckedBinding {
                targets: binding
                    .names
                    .iter()
                    .map(|name| checked_field(&name.value, &ty))
                    .collect(),
                value: self.lower_checked_expr(&binding.value, environment, Some(&ty))?,
            },
            ty,
        ))
    }

    fn check_effect(
        &self,
        effect: &EffectStmt,
        environment: &HashMap<String, Ty>,
    ) -> Result<(ResolvedAction, Vec<Ty>), SemanticError> {
        let words = effect.action.split_whitespace().collect::<Vec<_>>();
        let operation = words
            .first()
            .copied()
            .ok_or_else(|| SemanticError::new(effect.span, "empty effect action"))?;
        if let Some(contract) = self.actions.get(operation).cloned() {
            return self.check_standard_action_contract(effect, &words, environment, contract);
        }
        let providers = self.standard_library.action_providers(operation);
        if let [required_module] = providers.as_slice() {
            return Err(SemanticError::new(
                effect.span,
                format!("operation '{operation}' requires 'use {required_module}'"),
            ));
        }
        let signature = self.workflows.get(operation).ok_or_else(|| {
            SemanticError::new(
                effect.span,
                format!("unknown durable operation '{operation}'"),
            )
        })?;
        if words.len() - 1 != signature.inputs.len() {
            return Err(SemanticError::new(
                effect.span,
                format!(
                    "workflow '{operation}' expects {} argument(s)",
                    signature.inputs.len()
                ),
            ));
        }
        for (word, expected) in words[1..].iter().zip(&signature.inputs) {
            let actual = self.resolve_action_operand(word, environment, effect.span)?;
            if !compatible(&actual, expected) {
                return Err(SemanticError::new(
                    effect.span,
                    format!("workflow '{operation}' expects {expected}, found {actual}"),
                ));
            }
        }
        let results = signature.outputs.iter().map(|(_, ty)| ty.clone()).collect();
        let arguments = words[1..]
            .iter()
            .zip(&signature.inputs)
            .enumerate()
            .map(|(index, (word, ty))| {
                Ok(CheckedActionArgument {
                    name: format!("input_{index}"),
                    mode: if self.type_contains_material(ty, &mut BTreeSet::new()) {
                        OwnershipMode::Take
                    } else {
                        OwnershipMode::Copy
                    },
                    value: action_reference(self.definition_for_action_word(word), word, ty),
                })
            })
            .collect::<Result<Vec<_>, SemanticError>>()?;
        Ok((
            ResolvedAction {
                operation: format!("workflow.{operation}"),
                capability: None,
                arguments,
                results: signature
                    .outputs
                    .iter()
                    .map(|(name, ty)| checked_field(name, ty))
                    .collect(),
            },
            results,
        ))
    }

    fn check_standard_action_contract(
        &self,
        effect: &EffectStmt,
        words: &[&str],
        environment: &HashMap<String, Ty>,
        contract: ActionContractSpec,
    ) -> Result<(ResolvedAction, Vec<Ty>), SemanticError> {
        let mut cursor = 0;
        let mut operands = HashMap::new();
        let mut arguments = Vec::new();
        for part in &contract.phrase {
            if let PhrasePart::Optional(clause) = part {
                // A clause is present exactly when the word introducing it is,
                // so an omitted one is not mistaken for a malformed phrase.
                let introducer = match clause.first() {
                    Some(PhrasePart::Word(word)) => *word,
                    _ => unreachable!("contract validation requires a leading word"),
                };
                if words.get(cursor) == Some(&introducer) {
                    for nested in clause {
                        self.match_phrase_part(
                            nested,
                            effect,
                            words,
                            environment,
                            &contract,
                            &mut cursor,
                            &mut operands,
                            &mut arguments,
                        )?;
                    }
                } else {
                    for nested in clause {
                        let PhrasePart::Operand { name, r#type, mode } = nested else {
                            continue;
                        };
                        let ty = resolve_contract_type(r#type, &operands, effect.span)?;
                        operands.insert((*name).to_owned(), ty.clone());
                        arguments.push(CheckedActionArgument {
                            name: (*name).to_owned(),
                            mode: *mode,
                            value: TypedExpression {
                                r#type: to_checked_type(&ty),
                                value: CheckedExpression::List {
                                    elements: Vec::new(),
                                },
                            },
                        });
                    }
                }
                continue;
            }
            self.match_phrase_part(
                part,
                effect,
                words,
                environment,
                &contract,
                &mut cursor,
                &mut operands,
                &mut arguments,
            )?;
        }
        if cursor != words.len() {
            return Err(SemanticError::new(
                effect.span,
                format!("malformed '{}' action phrase", contract.operation),
            ));
        }
        Self::finish_action_contract(effect, contract, operands, arguments)
    }

    #[allow(clippy::too_many_arguments)]
    fn match_phrase_part(
        &self,
        part: &PhrasePart,
        effect: &EffectStmt,
        words: &[&str],
        environment: &HashMap<String, Ty>,
        contract: &ActionContractSpec,
        cursor: &mut usize,
        operands: &mut HashMap<String, Ty>,
        arguments: &mut Vec<CheckedActionArgument>,
    ) -> Result<(), SemanticError> {
        match part {
            PhrasePart::Optional(_) => {
                unreachable!("contract validation rejects a nested optional clause")
            }
            PhrasePart::Word(expected) => {
                if words.get(*cursor) != Some(expected) {
                    return Err(SemanticError::new(
                        effect.span,
                        format!("malformed '{}' action phrase", contract.operation),
                    ));
                }
                *cursor += 1;
            }
            PhrasePart::Operand { name, r#type, mode } => {
                let word = words.get(*cursor).ok_or_else(|| {
                    SemanticError::new(
                        effect.span,
                        format!(
                            "action '{}' is missing operand '{name}'",
                            contract.operation
                        ),
                    )
                })?;
                let actual = self.resolve_action_operand(word, environment, effect.span)?;
                if matches!(r#type, ContractType::AnyMaterial) {
                    if !matches!(&actual, Ty::Named(name, arguments) if name == "Material" && arguments.len() == 1)
                    {
                        return Err(SemanticError::new(
                            effect.span,
                            format!(
                                "operation '{}' expects physical Material<T>, found {actual}",
                                contract.operation
                            ),
                        ));
                    }
                } else {
                    let expected = resolve_contract_type(r#type, operands, effect.span)?;
                    require_action_type(actual.clone(), expected, effect.span, contract.operation)?;
                }
                operands.insert((*name).to_owned(), actual.clone());
                arguments.push(CheckedActionArgument {
                    name: (*name).to_owned(),
                    mode: *mode,
                    value: action_reference(self.definition_for_action_word(word), word, &actual),
                });
                *cursor += 1;
            }
            PhrasePart::Integer { name, signed } => {
                let word = words.get(*cursor).ok_or_else(|| {
                    SemanticError::new(
                        effect.span,
                        format!(
                            "action '{}' is missing integer '{name}'",
                            contract.operation
                        ),
                    )
                })?;
                let value = checked_integer_literal(word, *signed, effect.span)?;
                arguments.push(CheckedActionArgument {
                    name: (*name).to_owned(),
                    mode: OwnershipMode::Copy,
                    value,
                });
                *cursor += 1;
            }
            PhrasePart::Quantity {
                name,
                signed,
                units,
            } => {
                let magnitude = words.get(*cursor).ok_or_else(|| {
                    SemanticError::new(
                        effect.span,
                        format!(
                            "action '{}' is missing quantity '{name}'",
                            contract.operation
                        ),
                    )
                })?;
                checked_integer_literal(magnitude, *signed, effect.span)?;
                let unit = words.get(*cursor + 1).ok_or_else(|| {
                    SemanticError::new(
                        effect.span,
                        format!(
                            "action '{}' is missing a unit for '{name}'",
                            contract.operation
                        ),
                    )
                })?;
                if !units.contains(unit) {
                    return Err(SemanticError::new(
                        effect.span,
                        format!(
                            "action '{}' expects unit {units:?} for '{name}', found '{unit}'",
                            contract.operation
                        ),
                    ));
                }
                arguments.push(CheckedActionArgument {
                    name: (*name).to_owned(),
                    mode: OwnershipMode::Copy,
                    value: TypedExpression {
                        r#type: CheckedType::Quantity {
                            unit: (*unit).to_owned(),
                        },
                        value: CheckedExpression::Quantity {
                            magnitude: (*magnitude).to_owned(),
                            unit: (*unit).to_owned(),
                        },
                    },
                });
                *cursor += 2;
            }
        }
        Ok(())
    }

    fn finish_action_contract(
        effect: &EffectStmt,
        contract: ActionContractSpec,
        operands: HashMap<String, Ty>,
        arguments: Vec<CheckedActionArgument>,
    ) -> Result<(ResolvedAction, Vec<Ty>), SemanticError> {
        let result_contracts = contract
            .results
            .iter()
            .map(|result| {
                let ty = resolve_contract_type(&result.r#type, &operands, effect.span)?;
                Ok((checked_field(result.name, &ty), ty))
            })
            .collect::<Result<Vec<_>, SemanticError>>()?;
        let results = result_contracts
            .iter()
            .map(|(_, ty)| ty.clone())
            .collect::<Vec<_>>();
        let checked_results = result_contracts
            .into_iter()
            .map(|(field, _)| field)
            .collect::<Vec<_>>();
        Ok((
            ResolvedAction {
                operation: contract.operation.to_owned(),
                capability: Some(contract.capability.to_owned()),
                arguments,
                results: checked_results,
            },
            results,
        ))
    }

    fn resolve_action_operand(
        &self,
        word: &str,
        environment: &HashMap<String, Ty>,
        span: Span,
    ) -> Result<Ty, SemanticError> {
        let segments = word.split('.').collect::<Vec<_>>();
        let mut ty = environment
            .get(segments[0])
            .or_else(|| self.values.get(segments[0]))
            .cloned()
            .ok_or_else(|| SemanticError::new(span, format!("unknown action operand '{word}'")))?;
        for field in &segments[1..] {
            ty = self.field_type(&ty, field, span)?;
        }
        Ok(ty)
    }

    fn type_contains_material(&self, ty: &Ty, visiting: &mut BTreeSet<String>) -> bool {
        match ty {
            Ty::Named(name, _) if name == "Material" => true,
            Ty::List(element) => self.type_contains_material(element, visiting),
            Ty::Union(alternatives) => alternatives
                .iter()
                .any(|alternative| self.type_contains_material(alternative, visiting)),
            Ty::Named(name, _) if name == "Screening" => true,
            Ty::Named(name, arguments) => {
                if !visiting.insert(name.clone()) {
                    return false;
                }
                let contains = self.data.get(name).is_some_and(|signature| {
                    signature
                        .fields
                        .values()
                        .chain(signature.cases.values().flat_map(|fields| fields.values()))
                        .any(|field| self.type_contains_material(field, visiting))
                }) || arguments
                    .iter()
                    .any(|argument| self.type_contains_material(argument, visiting));
                visiting.remove(name);
                contains
            }
            _ => false,
        }
    }

    fn lower_checked_expr(
        &self,
        expression: &Expr,
        environment: &HashMap<String, Ty>,
        expected: Option<&Ty>,
    ) -> Result<TypedExpression, SemanticError> {
        let inferred = self.infer_expr(expression, environment)?;
        let ty = expected.unwrap_or(&inferred);
        let value = match expression {
            Expr::Path(path) => CheckedExpression::Reference {
                definition: self.definition_for_path(path),
                path: path
                    .segments
                    .iter()
                    .map(|segment| segment.value.clone())
                    .collect(),
            },
            Expr::Integer { value, .. } => CheckedExpression::Integer { value: *value },
            Expr::Decimal { text, .. } => CheckedExpression::Decimal { text: text.clone() },
            Expr::String { value, .. } => CheckedExpression::String {
                value: value.clone(),
            },
            Expr::Quantity {
                magnitude, unit, ..
            } => CheckedExpression::Quantity {
                magnitude: numeric_text(magnitude)?,
                unit: unit.clone(),
            },
            Expr::List { elements, .. } => CheckedExpression::List {
                elements: elements
                    .iter()
                    .map(|element| self.lower_checked_expr(element, environment, None))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            Expr::Call {
                callee, arguments, ..
            } => {
                let Expr::Path(path) = callee.as_ref() else {
                    return Err(SemanticError::new(
                        callee.span(),
                        "checked call target is not a resolved path",
                    ));
                };
                let name = path_text(path);
                let operation = if self.circuits.contains_key(&name) {
                    format!("circuit.{name}")
                } else if let Some(function) = self.pure_functions.get(&name) {
                    function.operation.to_owned()
                } else {
                    name
                };
                CheckedExpression::Call {
                    operation,
                    arguments: arguments
                        .iter()
                        .map(|argument| {
                            Ok(CheckedArgument {
                                name: argument.name.as_ref().map(|name| name.value.clone()),
                                value: self.lower_checked_expr(
                                    &argument.value,
                                    environment,
                                    None,
                                )?,
                            })
                        })
                        .collect::<Result<Vec<_>, SemanticError>>()?,
                }
            }
            Expr::Record {
                constructor,
                fields,
                ..
            } => {
                let name = path_text(constructor);
                let constructor = if let Some(parent) = self.cases.get(&name) {
                    format!("outcome.{parent}.{name}")
                } else if self.data.contains_key(&name) {
                    format!("data.{name}")
                } else if let Some(spec) = self.constructors.get(&name) {
                    spec.operation.to_owned()
                } else {
                    name
                };
                CheckedExpression::Construct {
                    constructor,
                    fields: fields
                        .iter()
                        .map(|field| {
                            Ok(CheckedFieldValue {
                                name: field.name.value.clone(),
                                value: self.lower_checked_expr(&field.value, environment, None)?,
                            })
                        })
                        .collect::<Result<Vec<_>, SemanticError>>()?,
                }
            }
            Expr::Field { subject, field, .. } => CheckedExpression::Field {
                subject: Box::new(self.lower_checked_expr(subject, environment, None)?),
                field: field.value.clone(),
            },
            Expr::Unary { op, operand, .. } => CheckedExpression::Unary {
                operator: match op {
                    UnaryOp::Negate => "negate",
                    UnaryOp::Not => "not",
                }
                .to_owned(),
                operand: Box::new(self.lower_checked_expr(operand, environment, None)?),
            },
            Expr::Binary {
                op, left, right, ..
            } => CheckedExpression::Binary {
                operator: binary_operator_name(*op).to_owned(),
                left: Box::new(self.lower_checked_expr(left, environment, None)?),
                right: Box::new(self.lower_checked_expr(right, environment, None)?),
            },
        };
        Ok(TypedExpression {
            r#type: to_checked_type(ty),
            value,
        })
    }

    fn infer_expr(
        &self,
        expression: &Expr,
        environment: &HashMap<String, Ty>,
    ) -> Result<Ty, SemanticError> {
        match expression {
            Expr::Path(path) => self.resolve_path(path, environment),
            Expr::Integer { .. } => Ok(Ty::Integer),
            Expr::Decimal { .. } => Ok(Ty::Decimal),
            Expr::String { .. } => Ok(Ty::String),
            Expr::Quantity { unit, .. } => Ok(Ty::Quantity(unit.clone())),
            Expr::List { elements, .. } => {
                let Some(first) = elements.first() else {
                    return Ok(Ty::EmptyList);
                };
                let mut element_type = self.infer_expr(first, environment)?;
                for element in &elements[1..] {
                    let found = self.infer_expr(element, environment)?;
                    element_type = common_type(element_type, found);
                }
                Ok(Ty::List(Box::new(element_type)))
            }
            Expr::Call {
                callee,
                arguments,
                span,
            } => self.infer_call(callee, arguments, *span, environment),
            Expr::Record {
                constructor,
                fields,
                span,
            } => self.infer_record(constructor, fields, *span, environment),
            Expr::Field {
                subject,
                field,
                span,
            } => {
                let subject = self.infer_expr(subject, environment)?;
                self.field_type(&subject, &field.value, *span)
            }
            Expr::Unary { op, operand, span } => {
                let operand = self.infer_expr(operand, environment)?;
                match op {
                    UnaryOp::Not if operand == Ty::Bool => Ok(Ty::Bool),
                    UnaryOp::Negate
                        if matches!(operand, Ty::Integer | Ty::Decimal | Ty::Quantity(_)) =>
                    {
                        Ok(operand)
                    }
                    _ => Err(SemanticError::new(
                        *span,
                        format!("invalid unary operand {operand}"),
                    )),
                }
            }
            Expr::Binary {
                op,
                left,
                right,
                span,
            } => {
                let left = self.infer_expr(left, environment)?;
                let right = self.infer_expr(right, environment)?;
                match op {
                    BinaryOp::Or | BinaryOp::And => {
                        if left == Ty::Bool && right == Ty::Bool {
                            Ok(Ty::Bool)
                        } else {
                            Err(SemanticError::new(
                                *span,
                                "boolean operator requires Bool operands",
                            ))
                        }
                    }
                    BinaryOp::Equal | BinaryOp::NotEqual => Ok(Ty::Bool),
                    BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual => {
                        if comparable(&left, &right) {
                            Ok(Ty::Bool)
                        } else {
                            Err(SemanticError::new(
                                *span,
                                format!("cannot compare {left} with {right}"),
                            ))
                        }
                    }
                    BinaryOp::Add => match (&left, &right) {
                        (Ty::List(left), Ty::List(right)) if compatible(left, right) => {
                            Ok(Ty::List(left.clone()))
                        }
                        (Ty::List(left), Ty::EmptyList) => Ok(Ty::List(left.clone())),
                        (Ty::EmptyList, Ty::List(right)) => Ok(Ty::List(right.clone())),
                        _ if comparable(&left, &right) => Ok(left),
                        _ => Err(SemanticError::new(
                            *span,
                            format!("cannot add {left} and {right}"),
                        )),
                    },
                    BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide => {
                        if comparable(&left, &right) {
                            Ok(left)
                        } else {
                            Err(SemanticError::new(
                                *span,
                                "incompatible arithmetic operands",
                            ))
                        }
                    }
                    BinaryOp::Range => Ok(Ty::List(Box::new(left))),
                }
            }
        }
    }

    fn infer_call(
        &self,
        callee: &Expr,
        arguments: &[Argument],
        span: Span,
        environment: &HashMap<String, Ty>,
    ) -> Result<Ty, SemanticError> {
        let Expr::Path(path) = callee else {
            return Err(SemanticError::new(
                span,
                "call target must resolve to a named operation",
            ));
        };
        let name = path_text(path);
        if let Some(signature) = self.circuits.get(&name) {
            if arguments.len() != signature.inputs.len() {
                return Err(SemanticError::new(
                    span,
                    format!(
                        "circuit '{name}' expects {} arguments",
                        signature.inputs.len()
                    ),
                ));
            }
            let mut substitutions = HashMap::new();
            for (argument, parameter) in arguments.iter().zip(&signature.inputs) {
                let actual = self.infer_expr(&argument.value, environment)?;
                unify(
                    parameter,
                    &actual,
                    &signature.parameters,
                    &mut substitutions,
                    span,
                )?;
            }
            for (parameter, bound) in &signature.bounds {
                let inferred = substitutions.get(parameter).ok_or_else(|| {
                    SemanticError::new(
                        span,
                        format!("could not infer circuit type parameter '{parameter}'"),
                    )
                })?;
                if !self.satisfies_bound(inferred, bound) {
                    return Err(SemanticError::new(
                        span,
                        format!(
                            "circuit type parameter '{parameter}' requires {bound}, found {inferred}"
                        ),
                    ));
                }
            }
            return Ok(substitute(&signature.output, &substitutions));
        }
        if let Some(function) = self.pure_functions.get(&name) {
            let actual =
                self.require_call_arguments(arguments, function.parameters.len(), environment)?;
            for (actual, expected) in actual.iter().zip(&function.parameters) {
                if !compatible(actual, expected) {
                    return Err(SemanticError::new(
                        span,
                        format!(
                            "operation '{}' expects {expected}, found {actual}",
                            function.operation
                        ),
                    ));
                }
            }
            return Ok(function.result.clone());
        }
        let providers = self.standard_library.function_providers(&name);
        if let [required_module] = providers.as_slice() {
            return Err(SemanticError::new(
                span,
                format!("operation '{name}' requires 'use {required_module}'"),
            ));
        }
        Err(SemanticError::new(
            span,
            format!("unknown pure operation '{name}'"),
        ))
    }

    fn require_call_arguments(
        &self,
        arguments: &[Argument],
        expected: usize,
        environment: &HashMap<String, Ty>,
    ) -> Result<Vec<Ty>, SemanticError> {
        if arguments.len() != expected {
            let span = arguments
                .first()
                .map_or(Span::at(0), |argument| argument.span);
            return Err(SemanticError::new(
                span,
                format!("expected {expected} argument(s), found {}", arguments.len()),
            ));
        }
        arguments
            .iter()
            .map(|argument| self.infer_expr(&argument.value, environment))
            .collect()
    }

    fn infer_record(
        &self,
        constructor: &Path,
        fields: &[FieldValue],
        span: Span,
        environment: &HashMap<String, Ty>,
    ) -> Result<Ty, SemanticError> {
        let name = path_text(constructor);
        let result = if let Some(parent) = self.cases.get(&name) {
            let signature = &self.data[parent];
            let mut expected = signature.fields.clone();
            expected.extend(signature.cases[&name].clone());
            self.check_constructor_fields(&name, fields, &expected, environment, span)?;
            Ty::named(parent)
        } else if let Some(signature) = self.data.get(&name) {
            self.check_constructor_fields(&name, fields, &signature.fields, environment, span)?;
            Ty::named(&name)
        } else if let Some(constructor) = self.constructors.get(&name) {
            let expected = self.constructor_fields(constructor);
            self.check_constructor_fields(&name, fields, &expected, environment, span)?;
            constructor.result.clone()
        } else {
            return Err(SemanticError::new(
                span,
                format!("unknown constructor '{name}'"),
            ));
        };
        Ok(result)
    }

    fn constructor_fields(&self, constructor: &ConstructorSpec) -> BTreeMap<String, Ty> {
        constructor
            .fields
            .iter()
            .map(|(name, ty)| {
                let ty = if matches!(ty, Ty::List(element) if **element == Ty::named("Evidence")) {
                    let evidence = std::iter::once(Ty::named("Evidence"))
                        .chain(
                            self.data
                                .iter()
                                .filter(|(_, signature)| {
                                    matches!(
                                        signature.kind,
                                        DataKind::Observation | DataKind::Evidence
                                    )
                                })
                                .map(|(name, _)| Ty::named(name)),
                        )
                        .collect();
                    Ty::List(Box::new(Ty::Union(evidence)))
                } else {
                    ty.clone()
                };
                ((*name).to_owned(), ty)
            })
            .collect()
    }

    fn check_constructor_fields(
        &self,
        constructor: &str,
        fields: &[FieldValue],
        expected: &BTreeMap<String, Ty>,
        environment: &HashMap<String, Ty>,
        span: Span,
    ) -> Result<(), SemanticError> {
        let mut provided = BTreeSet::new();
        for field in fields {
            let Some(expected_type) = expected.get(&field.name.value) else {
                return Err(SemanticError::new(
                    field.name.span,
                    format!(
                        "constructor '{constructor}' has no field '{}'",
                        field.name.value
                    ),
                ));
            };
            if !provided.insert(field.name.value.clone()) {
                return Err(SemanticError::new(
                    field.name.span,
                    "duplicate constructor field",
                ));
            }
            let actual = self.infer_expr(&field.value, environment)?;
            if !compatible(&actual, expected_type) {
                return Err(SemanticError::new(
                    field.value.span(),
                    format!(
                        "field '{}.{}' expects {expected_type}, found {actual}",
                        constructor, field.name.value
                    ),
                ));
            }
        }
        let missing = expected
            .keys()
            .filter(|name| !provided.contains(*name))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(SemanticError::new(
                span,
                format!("constructor '{constructor}' is missing fields {missing:?}"),
            ));
        }
        Ok(())
    }

    fn resolve_path(
        &self,
        path: &Path,
        environment: &HashMap<String, Ty>,
    ) -> Result<Ty, SemanticError> {
        let first = &path.segments[0];
        let mut ty = environment
            .get(&first.value)
            .or_else(|| self.values.get(&first.value))
            .cloned()
            .ok_or_else(|| {
                SemanticError::new(first.span, format!("unknown name '{}'", first.value))
            })?;
        for field in &path.segments[1..] {
            ty = self.field_type(&ty, &field.value, field.span)?;
        }
        Ok(ty)
    }

    fn field_type(&self, subject: &Ty, field: &str, span: Span) -> Result<Ty, SemanticError> {
        let result = match (subject, field) {
            (Ty::Named(name, _), field) if self.data.contains_key(name) => {
                self.data[name].fields.get(field).cloned().or_else(|| {
                    self.data[name]
                        .cases
                        .values()
                        .find_map(|fields| fields.get(field).cloned())
                })
            }
            (Ty::Named(name, _), field) => self
                .standard_types
                .get(name)
                .and_then(|spec| spec.fields.get(field))
                .cloned(),
            _ => None,
        };
        result.ok_or_else(|| {
            SemanticError::new(span, format!("type {subject} has no field '{field}'"))
        })
    }

    fn check_pattern(&self, pattern: &Pattern, matched: &Ty) -> Result<(), SemanticError> {
        let Pattern::Constructor { path, .. } = pattern else {
            return Ok(());
        };
        let case = path_text(path);
        let expected_parent = self.cases.get(&case).ok_or_else(|| {
            SemanticError::new(path.span, format!("unknown outcome case '{case}'"))
        })?;
        if !compatible(&Ty::named(expected_parent), matched) {
            return Err(SemanticError::new(
                path.span,
                format!("case '{case}' does not belong to {matched}"),
            ));
        }
        Ok(())
    }

    fn checked_pattern(&self, pattern: &Pattern) -> CheckedPattern {
        match pattern {
            Pattern::Name(name) => CheckedPattern::Binding {
                name: name.value.clone(),
            },
            Pattern::Constructor { path, fields, .. } => {
                let name = path_text(path);
                let constructor = self
                    .cases
                    .get(&name)
                    .map_or_else(|| name.clone(), |parent| format!("outcome.{parent}.{name}"));
                CheckedPattern::Constructor {
                    constructor,
                    fields: fields
                        .iter()
                        .map(|field| CheckedPatternField {
                            field: field.field.value.clone(),
                            binding: field.binding.value.clone(),
                        })
                        .collect(),
                }
            }
        }
    }

    fn require_bool(
        &self,
        expression: &Expr,
        environment: &HashMap<String, Ty>,
        context: &str,
    ) -> Result<(), SemanticError> {
        let ty = self.infer_expr(expression, environment)?;
        if ty != Ty::Bool {
            return Err(SemanticError::new(
                expression.span(),
                format!("{context} expects Bool, found {ty}"),
            ));
        }
        Ok(())
    }

    fn require_duration(
        &self,
        expression: &Expr,
        environment: &HashMap<String, Ty>,
    ) -> Result<(), SemanticError> {
        let ty = self.infer_expr(expression, environment)?;
        match ty {
            Ty::Quantity(ref unit) if matches!(unit.as_str(), "min" | "h") => Ok(()),
            _ => Err(SemanticError::new(
                expression.span(),
                format!("timer expects a duration, found {ty}"),
            )),
        }
    }

    fn lower_type(
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
                let name = path_text(path);
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

    fn satisfies_bound(&self, actual: &Ty, bound: &Ty) -> bool {
        if compatible(actual, bound) {
            return true;
        }
        let (Ty::Named(actual, actual_arguments), Ty::Named(bound, bound_arguments)) =
            (actual, bound)
        else {
            return false;
        };
        actual_arguments.is_empty()
            && bound_arguments.is_empty()
            && self
                .standard_types
                .get(actual)
                .is_some_and(|spec| spec.implements.contains(&bound.as_str()))
    }
}

struct CheckedBlock {
    statements: Vec<CheckedStatement>,
    environment: HashMap<String, Ty>,
    terminates: bool,
}

fn checked_field(name: &str, ty: &Ty) -> CheckedField {
    CheckedField {
        name: name.to_owned(),
        r#type: to_checked_type(ty),
    }
}

fn resolve_contract_type(
    r#type: &ContractType,
    operands: &HashMap<String, Ty>,
    span: Span,
) -> Result<Ty, SemanticError> {
    match r#type {
        ContractType::Concrete(ty) => Ok(ty.clone()),
        ContractType::SameAs(name) => operands.get(*name).cloned().ok_or_else(|| {
            SemanticError::new(
                span,
                format!("action contract references unknown operand '{name}'"),
            )
        }),
        ContractType::AnyMaterial => Err(SemanticError::new(
            span,
            "action result cannot use unconstrained AnyMaterial",
        )),
    }
}

fn action_reference(definition: DefinitionId, path: &str, ty: &Ty) -> TypedExpression {
    TypedExpression {
        r#type: to_checked_type(ty),
        value: CheckedExpression::Reference {
            definition,
            path: path.split('.').map(str::to_owned).collect(),
        },
    }
}

fn checked_integer_literal(
    text: &str,
    signed: bool,
    span: Span,
) -> Result<TypedExpression, SemanticError> {
    if signed {
        let value = text.parse::<i64>().map_err(|_| {
            SemanticError::new(span, format!("expected an integer, found '{text}'"))
        })?;
        if value < 0 {
            return Ok(TypedExpression {
                r#type: CheckedType::Integer,
                value: CheckedExpression::Unary {
                    operator: "negate".to_owned(),
                    operand: Box::new(TypedExpression {
                        r#type: CheckedType::Integer,
                        value: CheckedExpression::Integer {
                            value: value.unsigned_abs(),
                        },
                    }),
                },
            });
        }
        return Ok(TypedExpression {
            r#type: CheckedType::Integer,
            value: CheckedExpression::Integer {
                value: value as u64,
            },
        });
    }
    let value = text.parse::<u64>().map_err(|_| {
        SemanticError::new(
            span,
            format!("expected a non-negative integer, found '{text}'"),
        )
    })?;
    Ok(TypedExpression {
        r#type: CheckedType::Integer,
        value: CheckedExpression::Integer { value },
    })
}

fn path_text(path: &Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.value.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

fn numeric_text(expression: &Expr) -> Result<String, SemanticError> {
    match expression {
        Expr::Integer { value, .. } => Ok(value.to_string()),
        Expr::Decimal { text, .. } => Ok(text.clone()),
        Expr::Unary {
            op: UnaryOp::Negate,
            operand,
            ..
        } => Ok(format!("-{}", numeric_text(operand)?)),
        _ => Err(SemanticError::new(
            expression.span(),
            "quantity magnitude must be numeric",
        )),
    }
}

fn binary_operator_name(operator: BinaryOp) -> &'static str {
    match operator {
        BinaryOp::Or => "or",
        BinaryOp::And => "and",
        BinaryOp::Equal => "equal",
        BinaryOp::NotEqual => "not_equal",
        BinaryOp::Less => "less",
        BinaryOp::LessEqual => "less_equal",
        BinaryOp::Greater => "greater",
        BinaryOp::GreaterEqual => "greater_equal",
        BinaryOp::Range => "range",
        BinaryOp::Add => "add",
        BinaryOp::Subtract => "subtract",
        BinaryOp::Multiply => "multiply",
        BinaryOp::Divide => "divide",
    }
}

fn require_action_type(
    actual: Ty,
    expected: Ty,
    span: Span,
    operation: &str,
) -> Result<(), SemanticError> {
    if compatible(&actual, &expected) {
        Ok(())
    } else {
        Err(SemanticError::new(
            span,
            format!("operation '{operation}' expects {expected}, found {actual}"),
        ))
    }
}

fn promote_common_bindings(
    target: &mut HashMap<String, Ty>,
    original: &HashMap<String, Ty>,
    continuing: &[HashMap<String, Ty>],
) {
    let Some(first) = continuing.first() else {
        return;
    };
    for (name, ty) in first {
        if original.contains_key(name) {
            continue;
        }
        if continuing
            .iter()
            .skip(1)
            .all(|environment| environment.get(name) == Some(ty))
        {
            target.insert(name.clone(), ty.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::checker::*;
    use crate::standard_library::{PureFunctionSpec, StandardModule};
    use crate::{compile_module, compile_module_in_environment, compile_module_with_id};

    #[test]
    fn compiles_representative_design_module() {
        let module = compile_module(include_str!(
            "../../../docs/language/specimens/plasmid-design.lab"
        ))
        .unwrap();
        assert!(module.declarations.iter().any(|declaration| matches!(
            declaration,
            CheckedDeclaration::Artifact {
                artifact: ArtifactKind::Plasmid,
                ..
            }
        )));
    }

    #[test]
    fn emits_stable_module_interfaces_and_resolved_definition_ids() {
        let module = compile_module_with_id(
            ModuleId::new("reporter.design"),
            "use std.bio.parts\n\nreporter = sfGFP\n",
        )
        .unwrap();
        assert_eq!(module.module.as_str(), "reporter.design");
        assert_eq!(
            module.interface.exports["reporter"].definition,
            DefinitionId::exported("reporter.design", "reporter")
        );
        let CheckedDeclaration::Binding(binding) = &module.declarations[0] else {
            panic!("expected binding")
        };
        let CheckedExpression::Reference { definition, .. } = &binding.value.value else {
            panic!("expected resolved reference")
        };
        assert_eq!(
            definition,
            &DefinitionId::exported("std.bio.parts", "sfGFP")
        );
    }

    #[test]
    fn compiles_representative_reactive_workflow_module() {
        let module = compile_module(include_str!(
            "../../../docs/language/specimens/plasmid-build.lab"
        ))
        .unwrap();
        assert!(module.declarations.iter().any(|declaration| matches!(
            declaration,
            CheckedDeclaration::Workflow { name, .. } if name == "build_plasmid"
        )));
    }

    #[test]
    fn compiles_opentrons_examples_with_symbolic_inventory_names() {
        for source in [
            include_str!("../../../examples/opentrons-build/reporter-library.lab"),
            include_str!("../../../examples/opentrons-build/full-build.lab"),
        ] {
            let module = compile_module(source).unwrap();
            assert!(module.declarations.iter().any(|declaration| matches!(
                declaration,
                CheckedDeclaration::Artifact {
                    artifact: ArtifactKind::Plasmid,
                    ..
                }
            )));
            assert!(
                module
                    .declarations
                    .iter()
                    .any(|declaration| matches!(declaration, CheckedDeclaration::Workflow { .. }))
            );
        }

        let module = compile_module(include_str!(
            "../../../examples/opentrons-build/full-build.lab"
        ))
        .unwrap();
        let components = module
            .declarations
            .iter()
            .find_map(|declaration| {
                let CheckedDeclaration::Artifact {
                    name, properties, ..
                } = declaration
                else {
                    return None;
                };
                (name == "reporter_region").then(|| {
                    properties
                        .iter()
                        .find(|property| property.name == "components")
                        .unwrap()
                })
            })
            .unwrap();
        assert_eq!(
            components.value.r#type.display_name(),
            "List<Plasmid | Part>"
        );
        let CheckedExpression::List { elements } = &components.value.value else {
            panic!("components must remain a structured checked list");
        };
        assert!(
            elements
                .iter()
                .all(|element| matches!(&element.value, CheckedExpression::Reference { .. }))
        );
    }

    #[test]
    fn checks_named_workflow_results_and_multi_result_calls() {
        let module = compile_module(
            r#"workflow preserve(
  product: Material<Plasmid>,
  plate: Material<Plate>,
) -> (
  product: Material<Plasmid>,
  plate: Material<Plate>,
):
  return product, plate

workflow delegate(
  product: Material<Plasmid>,
  plate: Material<Plate>,
) -> (
  product: Material<Plasmid>,
  plate: Material<Plate>,
):
  preserved_product, preserved_plate <- preserve product plate
  return preserved_product, preserved_plate
"#,
        )
        .unwrap();

        let CheckedDeclaration::Workflow { outputs, .. } = &module.declarations[0] else {
            panic!("expected workflow")
        };
        assert_eq!(outputs[0].name, "product");
        assert_eq!(outputs[1].name, "plate");
        let callable = module.interface.exports["preserve"]
            .callable
            .as_ref()
            .unwrap();
        assert_eq!(callable.outputs, *outputs);

        let CheckedDeclaration::Workflow { body, .. } = &module.declarations[1] else {
            panic!("expected workflow")
        };
        let CheckedStatement::Effect { action, .. } = &body[0] else {
            panic!("expected workflow call")
        };
        assert_eq!(action.operation, "workflow.preserve");
        assert_eq!(action.results[0].name, "product");
        assert_eq!(action.results[1].name, "plate");
    }

    #[test]
    fn rejects_named_workflow_return_arity_and_type_mismatches() {
        let duplicate = compile_module(
            r#"workflow invalid() -> (
  value: Integer,
  value: String,
):
  return 1, "one"
"#,
        )
        .unwrap_err();
        assert!(
            duplicate
                .to_string()
                .contains("duplicate workflow result 'value'"),
            "{duplicate}"
        );

        let arity = compile_module(
            r#"workflow invalid(product: Material<Plasmid>) -> (
  product: Material<Plasmid>,
  count: Integer,
):
  return product
"#,
        )
        .unwrap_err();
        assert!(
            arity
                .to_string()
                .contains("workflow returns 1 value(s), expected 2"),
            "{arity}"
        );

        let r#type = compile_module(
            r#"workflow invalid(product: Material<Plasmid>) -> (
  product: Material<Plasmid>,
  count: Integer,
):
  return product, "many"
"#,
        )
        .unwrap_err();
        assert!(
            r#type
                .to_string()
                .contains("workflow result 'count' has type String, expected Integer"),
            "{type}"
        );
    }

    #[test]
    fn imports_named_workflow_result_interfaces() {
        let provider = compile_module_with_id(
            ModuleId::new("tools"),
            r#"workflow pair(left: Integer, right: String) -> (
  left: Integer,
  right: String,
):
  return left, right
"#,
        )
        .unwrap();
        let environment = SemanticEnvironment::new([provider.interface]);
        let consumer = compile_module_in_environment(
            ModuleId::new("consumer"),
            r#"use tools

workflow forward(left: Integer, right: String) -> (
  left: Integer,
  right: String,
):
  forwarded_left, forwarded_right <- pair left right
  return forwarded_left, forwarded_right
"#,
            &environment,
        )
        .unwrap();

        let CheckedDeclaration::Workflow { body, .. } = &consumer.declarations[0] else {
            panic!("expected workflow")
        };
        let CheckedStatement::Effect { action, .. } = &body[0] else {
            panic!("expected imported workflow call")
        };
        assert_eq!(action.results[0].name, "left");
        assert_eq!(action.results[1].name, "right");
    }

    #[test]
    fn inventory_constructors_require_their_standard_module() {
        let error = compile_module("J23101 = part(\"J23101\")\n").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("requires 'use std.bio.inventory'")
        );
    }

    #[test]
    fn imported_standard_modules_reject_ambiguous_exports() {
        let mut checker = Checker::new(ModuleId::standalone(), SemanticEnvironment::default());
        checker
            .register_standard_module(
                StandardModule::new("std.first").with_values([("shared", Ty::String)]),
                Span::at(0),
            )
            .unwrap();
        let error = checker
            .register_standard_module(
                StandardModule::new("std.second").with_functions([PureFunctionSpec::new(
                    "shared",
                    "std.second.shared",
                    Vec::new(),
                    Ty::String,
                )]),
                Span::at(0),
            )
            .unwrap_err();

        let message = error.to_string();
        assert!(message.contains("std.first"));
        assert!(message.contains("std.second"));
        assert!(message.contains("shared"));
    }

    #[test]
    fn rejects_unknown_modules() {
        let error = compile_module("use mystery.catalog\n").unwrap_err();
        assert!(error.to_string().contains("cannot be resolved"));
    }

    const OPTIONAL_CLAUSE: &str = r#"
use std.bio.build
use std.bio.inventory

J23101 = part("J23101")
pSB1C3 = backbone("pSB1C3")
BsaI = restriction_enzyme("BsaI")

plasmid p_gfp:
  sequence: dna("ACGT")
  backbone: pSB1C3
  components: [J23101]
  restriction_enzyme: BsaI
  require topology == circular
  accept sequence == design.sequence

workflow assemble() -> Material<Plasmid>:
  product <- realize p_gfp
  return product
"#;

    #[test]
    fn an_omitted_optional_clause_binds_its_operand_to_the_empty_list() {
        let module = compile_module(OPTIONAL_CLAUSE).unwrap();
        let CheckedDeclaration::Workflow { body, .. } = module
            .declarations
            .iter()
            .find(|declaration| matches!(declaration, CheckedDeclaration::Workflow { .. }))
            .unwrap()
        else {
            unreachable!()
        };
        let CheckedStatement::Effect { action, .. } = &body[0] else {
            panic!("expected the realization effect");
        };
        let dependencies = action
            .arguments
            .iter()
            .find(|argument| argument.name == "dependencies")
            .expect("an omitted clause still supplies its operand");
        assert!(
            matches!(
                &dependencies.value.value,
                CheckedExpression::List { elements } if elements.is_empty()
            ),
            "{:?}",
            dependencies.value.value
        );
    }

    #[test]
    fn stating_an_optional_clause_is_equivalent_to_omitting_it() {
        let stated = OPTIONAL_CLAUSE.replace(
            "  product <- realize p_gfp\n",
            "  dependencies = []\n  product <- realize p_gfp from dependencies\n",
        );
        let omitted = compile_module(OPTIONAL_CLAUSE).unwrap();
        let stated = compile_module(&stated).unwrap();

        let arguments = |module: &CheckedModule| {
            let CheckedDeclaration::Workflow { body, .. } = module
                .declarations
                .iter()
                .find(|declaration| matches!(declaration, CheckedDeclaration::Workflow { .. }))
                .unwrap()
            else {
                unreachable!()
            };
            body.iter()
                .find_map(|statement| match statement {
                    CheckedStatement::Effect { action, .. } => Some(action.arguments.clone()),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(arguments(&omitted).len(), arguments(&stated).len());
    }

    #[test]
    fn a_partly_written_optional_clause_is_still_malformed() {
        let error = compile_module(&OPTIONAL_CLAUSE.replace(
            "  product <- realize p_gfp\n",
            "  product <- realize p_gfp from\n",
        ))
        .expect_err("naming the clause commits to its operand");
        assert!(error.to_string().contains("dependencies"), "{error}");
    }

    #[test]
    fn an_unknown_trailing_word_is_not_mistaken_for_an_omitted_clause() {
        let error = compile_module(&OPTIONAL_CLAUSE.replace(
            "  product <- realize p_gfp\n",
            "  product <- realize p_gfp onto bench\n",
        ))
        .expect_err("a phrase the contract does not describe stays an error");
        assert!(error.to_string().contains("malformed"), "{error}");
    }

    #[test]
    fn checks_durable_action_operand_types() {
        let error = compile_module(
            r#"use std.lab.plasmid_actions

workflow invalid(image: Image) -> Evidence:
  evidence <- quantify image
  return evidence
"#,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("expects Material<Plasmid>, found Image")
        );
    }

    #[test]
    fn lowers_action_capability_ownership_and_result_contract() {
        let module = compile_module(
            r#"use std.lab.plasmid_actions

workflow preserve(plasmid: Material<Plasmid>) -> Material<Plasmid>:
  plasmid <- store plasmid at -20 C
  return plasmid
"#,
        )
        .unwrap();

        let CheckedDeclaration::Workflow { body, .. } = &module.declarations[0] else {
            panic!("expected workflow")
        };
        let CheckedStatement::Effect { action, .. } = &body[0] else {
            panic!("expected effect")
        };
        assert_eq!(action.operation, "std.lab.plasmid_actions.store");
        assert_eq!(action.capability.as_deref(), Some("cold_storage"));
        assert_eq!(action.arguments[0].mode, OwnershipMode::Take);
        assert_eq!(action.results[0].name, "material");
        assert_eq!(action.results[0].r#type.display_name(), "Material<Plasmid>");
    }

    #[test]
    fn lowers_explicit_state_and_state_updates() {
        let module = compile_module(
            r#"workflow counter() -> Integer:
  state count: Integer = 0
  count = count + 1
  return count
"#,
        )
        .unwrap();

        let CheckedDeclaration::Workflow { state, body, .. } = &module.declarations[0] else {
            panic!("expected workflow")
        };
        assert_eq!(state.len(), 1);
        assert_eq!(state[0].name, "count");
        assert!(matches!(
            &body[0],
            CheckedStatement::StateUpdate { state, .. } if state == "count"
        ));
    }

    #[test]
    fn rejects_reassigning_an_ordinary_binding() {
        let error = compile_module(
            r#"workflow invalid() -> Integer:
  count = 0
  count = count + 1
  return count
"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("cannot reassign 'count'"));
        assert!(error.to_string().contains("with 'state'"));
    }

    #[test]
    fn rejects_state_after_executable_statements() {
        let error = compile_module(
            r#"workflow invalid() -> Integer:
  count = 0
  state remembered: Integer = count
  return remembered
"#,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("state declarations must appear before workflow statements")
        );
    }
}
