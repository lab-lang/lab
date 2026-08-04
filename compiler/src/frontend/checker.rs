use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use super::action_contracts::{
    ActionContractSpec, ContractType, PhrasePart, standard_action_contract,
};
use super::ast::*;
use super::checked::*;
use super::semantic_error::SemanticError;
use super::source::Span;

pub(crate) fn check_module(module: &Module) -> Result<CheckedModule, SemanticError> {
    Checker::new(module).check(module)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Ty {
    Named(String, Vec<Ty>),
    Union(Vec<Ty>),
    List(Box<Ty>),
    Quantity(String),
    Integer,
    Decimal,
    String,
    Bool,
    None,
    EmptyList,
}

impl Ty {
    pub(super) fn named(name: impl Into<String>) -> Self {
        Self::Named(name.into(), Vec::new())
    }

    pub(super) fn material(inner: Ty) -> Self {
        Self::Named("Material".to_owned(), vec![inner])
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named(name, arguments) if arguments.is_empty() => formatter.write_str(name),
            Self::Named(name, arguments) => {
                write!(formatter, "{name}<")?;
                for (index, argument) in arguments.iter().enumerate() {
                    if index != 0 {
                        formatter.write_str(", ")?;
                    }
                    write!(formatter, "{argument}")?;
                }
                formatter.write_str(">")
            }
            Self::Union(alternatives) => {
                for (index, alternative) in alternatives.iter().enumerate() {
                    if index != 0 {
                        formatter.write_str(" | ")?;
                    }
                    write!(formatter, "{alternative}")?;
                }
                Ok(())
            }
            Self::List(element) => write!(formatter, "List<{element}>"),
            Self::Quantity(unit) => write!(formatter, "Quantity<{unit}>"),
            Self::Integer => formatter.write_str("Integer"),
            Self::Decimal => formatter.write_str("Decimal"),
            Self::String => formatter.write_str("String"),
            Self::Bool => formatter.write_str("Bool"),
            Self::None => formatter.write_str("None"),
            Self::EmptyList => formatter.write_str("List<_>"),
        }
    }
}

#[derive(Clone)]
struct CircuitSignature {
    parameters: Vec<String>,
    bounds: HashMap<String, Ty>,
    inputs: Vec<Ty>,
    output: Ty,
}

#[derive(Clone)]
struct DataSignature {
    kind: DataKind,
    fields: BTreeMap<String, Ty>,
    cases: BTreeMap<String, BTreeMap<String, Ty>>,
}

#[derive(Clone)]
struct WorkflowSignature {
    inputs: Vec<Ty>,
    output: Ty,
}

struct Checker {
    known_types: BTreeSet<String>,
    imports: BTreeSet<String>,
    values: HashMap<String, Ty>,
    circuits: HashMap<String, CircuitSignature>,
    data: HashMap<String, DataSignature>,
    cases: HashMap<String, String>,
    event_types: BTreeSet<String>,
    workflows: HashMap<String, WorkflowSignature>,
}

impl Checker {
    fn new(module: &Module) -> Self {
        let known_types = [
            "Accepted",
            "Antibiotic",
            "Backbone",
            "Bool",
            "CDS",
            "Circuit",
            "Clone",
            "CloneSet",
            "Colonies",
            "ColonyMap",
            "Construct",
            "Culture",
            "DNA",
            "Duration",
            "Evidence",
            "Fragment",
            "Image",
            "Integer",
            "List",
            "Material",
            "None",
            "Part",
            "Plate",
            "Plasmid",
            "Promoter",
            "Protein",
            "Reason",
            "Rejected",
            "RestrictionEnzyme",
            "Screening",
            "Signal",
            "Strain",
            "String",
            "Topology",
            "WorkflowContext",
        ]
        .into_iter()
        .map(str::to_owned)
        .chain(module.items.iter().filter_map(|item| match item {
            Item::Data(declaration) => Some(declaration.name.value.clone()),
            _ => None,
        }))
        .collect();
        Self {
            known_types,
            imports: BTreeSet::new(),
            values: HashMap::new(),
            circuits: HashMap::new(),
            data: HashMap::new(),
            cases: HashMap::new(),
            event_types: BTreeSet::new(),
            workflows: HashMap::new(),
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
                Item::Plasmid(declaration) => {
                    declarations.push(self.check_plasmid(declaration)?);
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

        Ok(CheckedModule {
            imports: self
                .imports
                .iter()
                .map(|module| ResolvedImport {
                    module: module.clone(),
                    provider: "builtin-standard-library".to_owned(),
                })
                .collect(),
            declarations,
        })
    }

    fn resolve_imports(&mut self, module: &Module) -> Result<(), SemanticError> {
        for item in &module.items {
            let Item::Use(import) = item else {
                continue;
            };
            let path = path_text(&import.path);
            match path.as_str() {
                "std.bio.parts" => {
                    self.insert_import(&path, import.span)?;
                    self.insert_value(
                        "pTet",
                        named("Promoter", vec![Ty::named("Tetracycline")]),
                        import.span,
                    )?;
                    self.insert_value(
                        "sfGFP",
                        named("CDS", vec![Ty::named("GreenFluorescentProtein")]),
                        import.span,
                    )?;
                    self.insert_value("B0034", Ty::named("Part"), import.span)?;
                    self.insert_value("B0015", Ty::named("Part"), import.span)?;
                    self.insert_value("BsaI", Ty::named("RestrictionEnzyme"), import.span)?;
                }
                "std.bio.backbones" => {
                    self.insert_import(&path, import.span)?;
                    self.insert_value("p15A_kan", Ty::named("Backbone"), import.span)?;
                }
                "std.lab.plasmid_actions" => {
                    self.insert_import(&path, import.span)?;
                    self.insert_value("competent_ecoli", Ty::named("Strain"), import.span)?;
                    self.insert_value("kanamycin", Ty::named("Antibiotic"), import.span)?;
                }
                _ => {
                    return Err(SemanticError::new(
                        import.span,
                        format!("module '{path}' cannot be resolved"),
                    ));
                }
            }
        }
        for (name, ty) in [
            ("circular", Ty::named("Topology")),
            ("None", Ty::None),
            ("no_colonies", Ty::named("Reason")),
            ("sequence_mismatch", Ty::named("Reason")),
            ("inconclusive_sequence", Ty::named("Reason")),
            ("acceptance_failed", Ty::named("Reason")),
        ] {
            self.values.insert(name.to_owned(), ty);
        }
        Ok(())
    }

    fn insert_import(&mut self, path: &str, span: Span) -> Result<(), SemanticError> {
        if !self.imports.insert(path.to_owned()) {
            return Err(SemanticError::new(
                span,
                format!("module '{path}' is imported more than once"),
            ));
        }
        Ok(())
    }

    fn insert_value(&mut self, name: &str, ty: Ty, span: Span) -> Result<(), SemanticError> {
        if self.values.insert(name.to_owned(), ty).is_some() {
            return Err(SemanticError::new(
                span,
                format!("imported name '{name}' is ambiguous"),
            ));
        }
        Ok(())
    }

    fn collect_declarations(&mut self, module: &Module) -> Result<(), SemanticError> {
        let mut names = BTreeSet::new();
        for item in &module.items {
            let (name, span) = match item {
                Item::Use(_) => continue,
                Item::Circuit(value) => (&value.name.value, value.name.span),
                Item::Plasmid(value) => (&value.name.value, value.name.span),
                Item::Data(value) => (&value.name.value, value.name.span),
                Item::Workflow(value) => (&value.name.value, value.name.span),
                Item::Binding(value) => (&value.names[0].value, value.names[0].span),
            };
            if !names.insert(name.clone()) || self.values.contains_key(name) {
                return Err(SemanticError::new(
                    span,
                    format!("duplicate declaration '{name}'"),
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
                    let output = declaration.output.as_ref().ok_or_else(|| {
                        SemanticError::new(declaration.span, "workflow requires an output type")
                    })?;
                    let output = self.lower_type(output, &BTreeSet::new())?;
                    self.workflows.insert(
                        declaration.name.value.clone(),
                        WorkflowSignature { inputs, output },
                    );
                }
                Item::Use(_) | Item::Plasmid(_) | Item::Binding(_) => {}
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

    fn check_plasmid(
        &self,
        declaration: &PlasmidDecl,
    ) -> Result<CheckedDeclaration, SemanticError> {
        let mut environment = self.values.clone();
        environment.extend([
            ("topology".to_owned(), Ty::named("Topology")),
            ("length".to_owned(), Ty::Quantity("bp".to_owned())),
            ("sequence".to_owned(), Ty::named("DNA")),
            ("concentration".to_owned(), Ty::Quantity("ng/uL".to_owned())),
            ("volume".to_owned(), Ty::Quantity("uL".to_owned())),
            ("design".to_owned(), Ty::named("Plasmid")),
        ]);
        let mut bindings = Vec::new();
        let mut requirements = Vec::new();
        let mut acceptance = Vec::new();
        for member in &declaration.members {
            match member {
                PlasmidMember::Binding(binding) => {
                    bindings.push(self.check_binding(binding, &mut environment)?.0);
                }
                PlasmidMember::Requirement(claim) => {
                    self.require_bool(&claim.predicate, &environment, "require")?;
                    requirements.push(self.lower_checked_expr(
                        &claim.predicate,
                        &environment,
                        None,
                    )?);
                }
                PlasmidMember::Acceptance(claim) => {
                    self.require_bool(&claim.predicate, &environment, "accept")?;
                    acceptance.push(self.lower_checked_expr(
                        &claim.predicate,
                        &environment,
                        None,
                    )?);
                }
                PlasmidMember::Section(section) => {
                    return Err(SemanticError::new(
                        section.span,
                        format!(
                            "plasmid section '{}' has no semantics yet",
                            section.name.value
                        ),
                    ));
                }
            }
        }
        let has_direct_sequence = declaration.members.iter().any(|member| {
            matches!(member, PlasmidMember::Binding(binding) if binding.names[0].value == "sequence")
        });
        if !has_direct_sequence
            && (!environment.contains_key("backbone") || !environment.contains_key("cargo"))
        {
            return Err(SemanticError::new(
                declaration.span,
                "plasmid requires either a sequence binding or backbone and cargo bindings",
            ));
        }
        if let Some(backbone) = environment.get("backbone")
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
        Ok(CheckedDeclaration::Plasmid {
            name: declaration.name.value.clone(),
            bindings,
            requirements,
            acceptance,
        })
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
            &signature.output,
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
                    "workflow '{}' may finish without returning an outcome",
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
            output: to_checked_type(&signature.output),
            state,
            body: checked.statements,
        })
    }

    fn check_block(
        &self,
        statements: &[Stmt],
        starting_environment: &HashMap<String, Ty>,
        output: &Ty,
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
                    let ty = self.infer_expr(&statement.value, &environment)?;
                    if !compatible(&ty, output) {
                        return Err(SemanticError::new(
                            statement.value.span(),
                            format!("workflow returns {ty}, expected {output}"),
                        ));
                    }
                    checked.push(CheckedStatement::Return {
                        value: self.lower_checked_expr(
                            &statement.value,
                            &environment,
                            Some(&ty),
                        )?,
                    });
                    terminates = true;
                }
                Stmt::If(statement) => {
                    self.require_bool(&statement.condition, &environment, "if")?;
                    let then_block =
                        self.check_block(&statement.then_body, &environment, output, state_types)?;
                    let else_block =
                        self.check_block(&statement.else_body, &environment, output, state_types)?;
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
                            self.check_block(&case.body, &environment, output, state_types)?;
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
                        self.check_block(&statement.body, &loop_environment, output, state_types)?;
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
                        self.check_block(&statement.body, &environment, output, state_types)?;
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
        if let Some(contract) = standard_action_contract(operation) {
            if !self.imports.contains("std.lab.plasmid_actions") {
                return Err(SemanticError::new(
                    effect.span,
                    "durable plasmid actions require 'use std.lab.plasmid_actions'",
                ));
            }
            return self.check_standard_action_contract(effect, &words, environment, contract);
        }
        if !self.workflows.contains_key(operation)
            && !self.imports.contains("std.lab.plasmid_actions")
        {
            return Err(SemanticError::new(
                effect.span,
                "durable plasmid actions require 'use std.lab.plasmid_actions'",
            ));
        }
        let lookup = |word: Option<&&str>| -> Result<Ty, SemanticError> {
            let word = word.ok_or_else(|| SemanticError::new(effect.span, "incomplete action"))?;
            self.resolve_action_operand(word, environment, effect.span)
        };
        let results = match operation {
            "capture" => {
                require_phrase(&words, 4, &[(1, "image"), (2, "of")], effect.span)?;
                require_action_type(
                    lookup(words.last())?,
                    Ty::material(Ty::named("Plate")),
                    effect.span,
                    operation,
                )?;
                vec![Ty::named("Image")]
            }
            "synthesize" => {
                require_phrase(&words, 2, &[], effect.span)?;
                require_action_type(
                    lookup(words.get(1))?,
                    Ty::named("Plasmid"),
                    effect.span,
                    operation,
                )?;
                vec![Ty::List(Box::new(Ty::named("Fragment")))]
            }
            "assemble" => {
                require_phrase(&words, 2, &[], effect.span)?;
                require_action_type(
                    lookup(words.get(1))?,
                    Ty::List(Box::new(Ty::named("Fragment"))),
                    effect.span,
                    operation,
                )?;
                vec![Ty::material(Ty::named("Construct"))]
            }
            "provision" => {
                require_phrase(&words, 2, &[], effect.span)?;
                require_action_type(
                    lookup(words.get(1))?,
                    Ty::named("Strain"),
                    effect.span,
                    operation,
                )?;
                vec![Ty::material(Ty::named("Strain"))]
            }
            "transform" => {
                require_phrase(&words, 4, &[(2, "into")], effect.span)?;
                require_action_type(
                    lookup(words.get(1))?,
                    Ty::material(Ty::named("Construct")),
                    effect.span,
                    operation,
                )?;
                require_action_type(
                    lookup(words.last())?,
                    Ty::material(Ty::named("Strain")),
                    effect.span,
                    operation,
                )?;
                vec![Ty::material(Ty::named("Culture"))]
            }
            "recover" => {
                require_phrase(&words, 5, &[(2, "for")], effect.span)?;
                require_unsigned(words[3], "recovery duration", effect.span)?;
                require_one_of(words[4], &["min", "h"], "duration unit", effect.span)?;
                require_action_type(
                    lookup(words.get(1))?,
                    Ty::material(Ty::named("Culture")),
                    effect.span,
                    operation,
                )?;
                vec![Ty::material(Ty::named("Culture"))]
            }
            "plate" => {
                require_phrase(&words, 4, &[(2, "on")], effect.span)?;
                require_action_type(
                    lookup(words.get(1))?,
                    Ty::material(Ty::named("Culture")),
                    effect.span,
                    operation,
                )?;
                require_action_type(
                    lookup(words.last())?,
                    Ty::named("Antibiotic"),
                    effect.span,
                    operation,
                )?;
                vec![Ty::material(Ty::named("Plate"))]
            }
            "pick" => {
                require_phrase(
                    &words,
                    6,
                    &[(2, "isolated"), (3, "colonies"), (4, "from")],
                    effect.span,
                )?;
                require_unsigned(words[1], "colony count", effect.span)?;
                require_action_type(
                    lookup(words.last())?,
                    Ty::material(Ty::named("Plate")),
                    effect.span,
                    operation,
                )?;
                vec![Ty::List(Box::new(Ty::named("Clone")))]
            }
            "screen" => {
                require_phrase(&words, 4, &[(2, "against")], effect.span)?;
                require_action_type(
                    lookup(words.get(1))?,
                    Ty::List(Box::new(Ty::named("Clone"))),
                    effect.span,
                    operation,
                )?;
                require_action_type(
                    lookup(words.last())?,
                    Ty::named("Plasmid"),
                    effect.span,
                    operation,
                )?;
                vec![Ty::named("Screening")]
            }
            "grow" => {
                require_phrase(
                    &words,
                    8,
                    &[(2, "at"), (4, "C"), (5, "for"), (7, "h")],
                    effect.span,
                )?;
                require_signed(words[3], "growth temperature", effect.span)?;
                require_unsigned(words[6], "growth duration", effect.span)?;
                require_action_type(
                    lookup(words.get(1))?,
                    Ty::named("Clone"),
                    effect.span,
                    operation,
                )?;
                vec![Ty::material(Ty::named("Culture"))]
            }
            "purify" => {
                require_phrase(&words, 2, &[], effect.span)?;
                require_action_type(
                    lookup(words.get(1))?,
                    Ty::material(Ty::named("Culture")),
                    effect.span,
                    operation,
                )?;
                vec![Ty::material(Ty::named("Plasmid"))]
            }
            "split" => {
                require_phrase(&words, 2, &[], effect.span)?;
                let input = lookup(words.get(1))?;
                require_action_type(
                    input.clone(),
                    Ty::material(Ty::named("Plasmid")),
                    effect.span,
                    operation,
                )?;
                vec![input.clone(), input]
            }
            "sequence" => {
                require_phrase(&words, 2, &[], effect.span)?;
                require_action_type(
                    lookup(words.get(1))?,
                    Ty::material(Ty::named("Plasmid")),
                    effect.span,
                    operation,
                )?;
                vec![Ty::named("SequenceCheck")]
            }
            "quantify" => {
                require_phrase(&words, 2, &[], effect.span)?;
                require_action_type(
                    lookup(words.get(1))?,
                    Ty::material(Ty::named("Plasmid")),
                    effect.span,
                    operation,
                )?;
                vec![Ty::named("Evidence")]
            }
            "store" => {
                require_phrase(&words, 5, &[(2, "at"), (4, "C")], effect.span)?;
                require_signed(words[3], "storage temperature", effect.span)?;
                let input = lookup(words.get(1))?;
                require_action_type(
                    input.clone(),
                    Ty::material(Ty::named("Plasmid")),
                    effect.span,
                    operation,
                )?;
                vec![input]
            }
            "dispose" => {
                require_phrase(&words, 2, &[], effect.span)?;
                lookup(words.get(1))?;
                Vec::new()
            }
            workflow if self.workflows.contains_key(workflow) => {
                let signature = &self.workflows[workflow];
                if words.len() - 1 != signature.inputs.len() {
                    return Err(SemanticError::new(
                        effect.span,
                        format!(
                            "workflow '{workflow}' expects {} argument(s)",
                            signature.inputs.len()
                        ),
                    ));
                }
                for (word, expected) in words[1..].iter().zip(&signature.inputs) {
                    let actual = self.resolve_action_operand(word, environment, effect.span)?;
                    if !compatible(&actual, expected) {
                        return Err(SemanticError::new(
                            effect.span,
                            format!("workflow '{workflow}' expects {expected}, found {actual}"),
                        ));
                    }
                }
                vec![signature.output.clone()]
            }
            _ => {
                return Err(SemanticError::new(
                    effect.span,
                    format!("unknown durable operation '{operation}'"),
                ));
            }
        };
        let signature = self.workflows.get(operation).ok_or_else(|| {
            SemanticError::new(
                effect.span,
                format!("unknown durable operation '{operation}'"),
            )
        })?;
        let arguments = words[1..]
            .iter()
            .zip(&signature.inputs)
            .enumerate()
            .map(|(index, (word, ty))| {
                Ok(CheckedActionArgument {
                    name: format!("input_{index}"),
                    mode: if is_material_type(ty) {
                        OwnershipMode::Take
                    } else {
                        OwnershipMode::Copy
                    },
                    value: action_reference(word, ty),
                })
            })
            .collect::<Result<Vec<_>, SemanticError>>()?;
        Ok((
            ResolvedAction {
                operation: format!("workflow.{operation}"),
                capability: None,
                arguments,
                results: vec![checked_field("outcome", &signature.output)],
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
            match part {
                PhrasePart::Word(expected) => {
                    if words.get(cursor) != Some(expected) {
                        return Err(SemanticError::new(
                            effect.span,
                            format!("malformed '{}' action phrase", contract.operation),
                        ));
                    }
                    cursor += 1;
                }
                PhrasePart::Operand { name, r#type, mode } => {
                    let word = words.get(cursor).ok_or_else(|| {
                        SemanticError::new(
                            effect.span,
                            format!(
                                "action '{}' is missing operand '{name}'",
                                contract.operation
                            ),
                        )
                    })?;
                    let actual = self.resolve_action_operand(word, environment, effect.span)?;
                    let expected = resolve_contract_type(r#type, &operands, effect.span)?;
                    require_action_type(actual.clone(), expected, effect.span, contract.operation)?;
                    operands.insert((*name).to_owned(), actual.clone());
                    arguments.push(CheckedActionArgument {
                        name: (*name).to_owned(),
                        mode: *mode,
                        value: action_reference(word, &actual),
                    });
                    cursor += 1;
                }
                PhrasePart::Integer { name, signed } => {
                    let word = words.get(cursor).ok_or_else(|| {
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
                    cursor += 1;
                }
                PhrasePart::Quantity {
                    name,
                    signed,
                    units,
                } => {
                    let magnitude = words.get(cursor).ok_or_else(|| {
                        SemanticError::new(
                            effect.span,
                            format!(
                                "action '{}' is missing quantity '{name}'",
                                contract.operation
                            ),
                        )
                    })?;
                    checked_integer_literal(magnitude, *signed, effect.span)?;
                    let unit = words.get(cursor + 1).ok_or_else(|| {
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
                    cursor += 2;
                }
            }
        }
        if cursor != words.len() {
            return Err(SemanticError::new(
                effect.span,
                format!("malformed '{}' action phrase", contract.operation),
            ));
        }
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
                } else {
                    match name.as_str() {
                        "dna" => "std.bio.dna".to_owned(),
                        "detect_colonies" => "std.lab.imaging.detect_colonies".to_owned(),
                        "sites" => "std.bio.sequence.sites".to_owned(),
                        "design.accepts" => "Plasmid.accepts".to_owned(),
                        _ => name,
                    }
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
                } else {
                    format!("std.outcome.{name}")
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
            Expr::List { elements, span } => {
                let Some(first) = elements.first() else {
                    return Ok(Ty::EmptyList);
                };
                let element_type = self.infer_expr(first, environment)?;
                for element in &elements[1..] {
                    let found = self.infer_expr(element, environment)?;
                    if !compatible(&found, &element_type) {
                        return Err(SemanticError::new(
                            *span,
                            format!("list mixes {element_type} and {found}"),
                        ));
                    }
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
                if !satisfies_bound(inferred, bound) {
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
        match name.as_str() {
            "dna" => {
                let arguments = self.require_call_arguments(arguments, 1, environment)?;
                if arguments[0] != Ty::String {
                    return Err(SemanticError::new(
                        span,
                        format!("dna expects String, found {}", arguments[0]),
                    ));
                }
                Ok(Ty::named("DNA"))
            }
            "detect_colonies" => {
                self.require_call_arguments(arguments, 1, environment)?;
                Ok(Ty::named("ColonyMap"))
            }
            "sites" => {
                self.require_call_arguments(arguments, 1, environment)?;
                Ok(Ty::Integer)
            }
            "design.accepts" => {
                self.require_call_arguments(arguments, 1, environment)?;
                Ok(Ty::Bool)
            }
            _ => Err(SemanticError::new(
                span,
                format!("unknown pure operation '{name}'"),
            )),
        }
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
        } else if name == "Accepted" || name == "Rejected" {
            let evidence_type = Ty::Union(
                std::iter::once(Ty::named("Evidence"))
                    .chain(
                        self.data
                            .iter()
                            .filter(|(_, signature)| {
                                matches!(signature.kind, DataKind::Observation | DataKind::Evidence)
                            })
                            .map(|(name, _)| Ty::named(name)),
                    )
                    .collect(),
            );
            let mut expected = BTreeMap::from([
                ("material".to_owned(), Ty::material(Ty::named("Plasmid"))),
                ("evidence".to_owned(), Ty::List(Box::new(evidence_type))),
            ]);
            if name == "Rejected" {
                expected.insert(
                    "material".to_owned(),
                    Ty::Union(vec![Ty::material(Ty::named("Plasmid")), Ty::None]),
                );
                expected.insert("reason".to_owned(), Ty::named("Reason"));
            }
            self.check_constructor_fields(&name, fields, &expected, environment, span)?;
            named(&name, vec![Ty::named("Plasmid")])
        } else {
            return Err(SemanticError::new(
                span,
                format!("unknown constructor '{name}'"),
            ));
        };
        Ok(result)
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
            (Ty::Named(name, _), "isolated") if name == "ColonyMap" => Some(Ty::named("Colonies")),
            (Ty::Named(name, _), "count") if name == "Colonies" => Some(Ty::Integer),
            (Ty::Named(name, _), "clones") if name == "Screening" => Some(Ty::named("CloneSet")),
            (Ty::Named(name, _), "highest_confidence") if name == "CloneSet" => {
                Some(Ty::material(Ty::named("Clone")))
            }
            (Ty::Named(name, _), "elapsed") if name == "WorkflowContext" => {
                Some(Ty::named("Duration"))
            }
            (Ty::Named(name, _), "sequence") if name == "Plasmid" => Some(Ty::named("DNA")),
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
                let expected_arity = match name.as_str() {
                    "Accepted" | "CDS" | "List" | "Material" | "Promoter" | "Rejected" => Some(1),
                    "Circuit" => Some(2),
                    _ => None,
                };
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

struct CheckedBlock {
    statements: Vec<CheckedStatement>,
    environment: HashMap<String, Ty>,
    terminates: bool,
}

fn named(name: impl Into<String>, arguments: Vec<Ty>) -> Ty {
    Ty::Named(name.into(), arguments)
}

fn to_checked_type(ty: &Ty) -> CheckedType {
    match ty {
        Ty::Named(name, arguments) => CheckedType::Named {
            name: name.clone(),
            arguments: arguments.iter().map(to_checked_type).collect(),
        },
        Ty::Union(alternatives) => CheckedType::Union {
            alternatives: alternatives.iter().map(to_checked_type).collect(),
        },
        Ty::List(element) => CheckedType::List {
            element: Box::new(to_checked_type(element)),
        },
        Ty::Quantity(unit) => CheckedType::Quantity { unit: unit.clone() },
        Ty::Integer => CheckedType::Integer,
        Ty::Decimal => CheckedType::Decimal,
        Ty::String => CheckedType::String,
        Ty::Bool => CheckedType::Bool,
        Ty::None => CheckedType::None,
        Ty::EmptyList => CheckedType::List {
            element: Box::new(CheckedType::Named {
                name: "_".to_owned(),
                arguments: Vec::new(),
            }),
        },
    }
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
    }
}

fn action_reference(path: &str, ty: &Ty) -> TypedExpression {
    TypedExpression {
        r#type: to_checked_type(ty),
        value: CheckedExpression::Reference {
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

fn is_material_type(ty: &Ty) -> bool {
    matches!(ty, Ty::Named(name, _) if name == "Material")
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

fn compatible(actual: &Ty, expected: &Ty) -> bool {
    if actual == expected || matches!(actual, Ty::EmptyList) && matches!(expected, Ty::List(_)) {
        return true;
    }
    match expected {
        Ty::Union(alternatives) => alternatives.iter().any(|ty| compatible(actual, ty)),
        Ty::List(expected) => match actual {
            Ty::List(actual) => compatible(actual, expected),
            Ty::EmptyList => true,
            _ => false,
        },
        Ty::Named(expected_name, expected_args) => match actual {
            Ty::Named(actual_name, actual_args) => {
                actual_name == expected_name
                    && actual_args.len() == expected_args.len()
                    && actual_args
                        .iter()
                        .zip(expected_args)
                        .all(|(actual, expected)| compatible(actual, expected))
            }
            _ => false,
        },
        _ => false,
    }
}

fn comparable(left: &Ty, right: &Ty) -> bool {
    compatible(left, right)
        || compatible(right, left)
        || matches!((left, right), (Ty::Quantity(_), Ty::Quantity(_)))
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

fn require_phrase(
    words: &[&str],
    length: usize,
    fixed: &[(usize, &str)],
    span: Span,
) -> Result<(), SemanticError> {
    let operation = words.first().copied().unwrap_or("action");
    if words.len() != length
        || fixed
            .iter()
            .any(|(index, expected)| words.get(*index) != Some(expected))
    {
        return Err(SemanticError::new(
            span,
            format!("malformed '{operation}' action phrase"),
        ));
    }
    Ok(())
}

fn require_unsigned(value: &str, description: &str, span: Span) -> Result<(), SemanticError> {
    value.parse::<u64>().map(|_| ()).map_err(|_| {
        SemanticError::new(
            span,
            format!("{description} must be a non-negative integer"),
        )
    })
}

fn require_signed(value: &str, description: &str, span: Span) -> Result<(), SemanticError> {
    value
        .parse::<i64>()
        .map(|_| ())
        .map_err(|_| SemanticError::new(span, format!("{description} must be an integer")))
}

fn require_one_of(
    value: &str,
    choices: &[&str],
    description: &str,
    span: Span,
) -> Result<(), SemanticError> {
    if choices.contains(&value) {
        Ok(())
    } else {
        Err(SemanticError::new(
            span,
            format!("{description} must be one of {choices:?}, found '{value}'"),
        ))
    }
}

fn satisfies_bound(actual: &Ty, bound: &Ty) -> bool {
    compatible(actual, bound)
        || matches!(
            (actual, bound),
            (
                Ty::Named(actual, arguments),
                Ty::Named(bound, bound_arguments)
            ) if arguments.is_empty()
                && bound_arguments.is_empty()
                && matches!(
                    (actual.as_str(), bound.as_str()),
                    ("Tetracycline", "Signal")
                        | ("GreenFluorescentProtein", "Protein")
                )
        )
}

fn unify(
    template: &Ty,
    actual: &Ty,
    parameters: &[String],
    substitutions: &mut HashMap<String, Ty>,
    span: Span,
) -> Result<(), SemanticError> {
    if let Ty::Named(name, arguments) = template {
        if arguments.is_empty() && parameters.contains(name) {
            if let Some(previous) = substitutions.get(name) {
                if !compatible(actual, previous) {
                    return Err(SemanticError::new(
                        span,
                        format!("type parameter {name} inferred as both {previous} and {actual}"),
                    ));
                }
            } else {
                substitutions.insert(name.clone(), actual.clone());
            }
            return Ok(());
        }
        let Ty::Named(actual_name, actual_arguments) = actual else {
            return Err(SemanticError::new(
                span,
                format!("expected {template}, found {actual}"),
            ));
        };
        if name != actual_name || arguments.len() != actual_arguments.len() {
            return Err(SemanticError::new(
                span,
                format!("expected {template}, found {actual}"),
            ));
        }
        for (template, actual) in arguments.iter().zip(actual_arguments) {
            unify(template, actual, parameters, substitutions, span)?;
        }
        return Ok(());
    }
    if compatible(actual, template) {
        Ok(())
    } else {
        Err(SemanticError::new(
            span,
            format!("expected {template}, found {actual}"),
        ))
    }
}

fn substitute(ty: &Ty, substitutions: &HashMap<String, Ty>) -> Ty {
    match ty {
        Ty::Named(name, arguments) if arguments.is_empty() && substitutions.contains_key(name) => {
            substitutions[name].clone()
        }
        Ty::Named(name, arguments) => Ty::Named(
            name.clone(),
            arguments
                .iter()
                .map(|argument| substitute(argument, substitutions))
                .collect(),
        ),
        Ty::Union(alternatives) => Ty::Union(
            alternatives
                .iter()
                .map(|alternative| substitute(alternative, substitutions))
                .collect(),
        ),
        Ty::List(element) => Ty::List(Box::new(substitute(element, substitutions))),
        other => other.clone(),
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
    use super::super::compile_module;
    use super::*;

    #[test]
    fn compiles_representative_design_module() {
        let module = compile_module(include_str!(
            "../../docs/language/specimens/plasmid-design.lab"
        ))
        .unwrap();
        assert!(
            module
                .declarations
                .iter()
                .any(|declaration| matches!(declaration, CheckedDeclaration::Plasmid { .. }))
        );
    }

    #[test]
    fn compiles_representative_reactive_workflow_module() {
        let module = compile_module(include_str!(
            "../../docs/language/specimens/plasmid-build.lab"
        ))
        .unwrap();
        assert!(module.declarations.iter().any(|declaration| matches!(
            declaration,
            CheckedDeclaration::Workflow { name, .. } if name == "build_plasmid"
        )));
    }

    #[test]
    fn rejects_unknown_modules() {
        let error = compile_module("use mystery.catalog\n").unwrap_err();
        assert!(error.to_string().contains("cannot be resolved"));
    }

    #[test]
    fn checks_durable_action_operand_types() {
        let error = compile_module(
            r#"use std.lab.plasmid_actions

workflow invalid:
  input image: Image
  output Evidence
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

workflow preserve:
  input plasmid: Material<Plasmid>
  output Material<Plasmid>
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
            r#"workflow counter:
  output Integer
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
            r#"workflow invalid:
  output Integer
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
            r#"workflow invalid:
  output Integer
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
