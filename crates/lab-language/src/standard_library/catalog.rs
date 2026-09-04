//! Immutable catalog and validation for bundled standard-library modules.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

use thiserror::Error;

use crate::semantics::{ExportKind, ModuleId, ModuleInterface};

use crate::standard_library::contract::ActionContractSpec;
use crate::type_system::Ty;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeSpec {
    pub name: &'static str,
    pub parameters: usize,
    pub fields: BTreeMap<&'static str, Ty>,
    pub implements: Vec<&'static str>,
    /// Whether this name classifies types rather than describing values. A role
    /// may bound a type parameter and may not be the type of anything.
    pub role: bool,
    /// Whether this role is one the compiler enforces a rule for.
    pub law: bool,
    pub documentation: &'static str,
}

impl TypeSpec {
    pub(crate) fn nominal(name: &'static str) -> Self {
        Self {
            name,
            parameters: 0,
            fields: BTreeMap::new(),
            implements: Vec::new(),
            role: false,
            law: false,
            documentation: "",
        }
    }

    /// A part types can play, such as `Signal`.
    pub(crate) fn role(name: &'static str) -> Self {
        Self {
            role: true,
            ..Self::nominal(name)
        }
    }

    /// A role the compiler enforces a rule for, such as `Event`, which is what
    /// `emit` and `when` resolve against. There is no source form for declaring
    /// one: a law no checker reads would mean nothing.
    pub(crate) fn law(name: &'static str) -> Self {
        Self {
            law: true,
            ..Self::role(name)
        }
    }

    pub(crate) fn parameters(mut self, parameters: usize) -> Self {
        self.parameters = parameters;
        self
    }

    pub(crate) fn with_fields(
        mut self,
        fields: impl IntoIterator<Item = (&'static str, Ty)>,
    ) -> Self {
        self.fields.extend(fields);
        self
    }

    pub(crate) fn implements(mut self, contracts: impl IntoIterator<Item = &'static str>) -> Self {
        self.implements.extend(contracts);
        self
    }

    pub(crate) fn documented(mut self, documentation: &'static str) -> Self {
        self.documentation = documentation;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PureFunctionSpec {
    pub name: &'static str,
    pub operation: &'static str,
    pub parameters: Vec<Ty>,
    pub result: Ty,
    pub documentation: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConstructorSpec {
    pub name: &'static str,
    pub operation: &'static str,
    pub fields: BTreeMap<&'static str, Ty>,
    pub result: Ty,
    pub documentation: &'static str,
}

impl ConstructorSpec {
    pub(crate) fn new(
        name: &'static str,
        operation: &'static str,
        fields: impl IntoIterator<Item = (&'static str, Ty)>,
        result: Ty,
    ) -> Self {
        Self {
            name,
            operation,
            fields: fields.into_iter().collect(),
            result,
            documentation: "",
        }
    }

    pub(crate) fn documented(mut self, documentation: &'static str) -> Self {
        self.documentation = documentation;
        self
    }
}

impl PureFunctionSpec {
    pub(crate) fn new(
        name: &'static str,
        operation: &'static str,
        parameters: Vec<Ty>,
        result: Ty,
    ) -> Self {
        Self {
            name,
            operation,
            parameters,
            result,
            documentation: "",
        }
    }

    pub(crate) fn documented(mut self, documentation: &'static str) -> Self {
        self.documentation = documentation;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StandardModule {
    pub path: &'static str,
    pub prelude: bool,
    pub documentation: &'static str,
    pub types: Vec<TypeSpec>,
    pub values: Vec<(&'static str, Ty)>,
    pub functions: Vec<PureFunctionSpec>,
    pub constructors: Vec<ConstructorSpec>,
    pub actions: Vec<ActionContractSpec>,
}

impl StandardModule {
    pub(crate) fn new(path: &'static str) -> Self {
        Self {
            path,
            prelude: false,
            documentation: "",
            types: Vec::new(),
            values: Vec::new(),
            functions: Vec::new(),
            constructors: Vec::new(),
            actions: Vec::new(),
        }
    }

    pub(crate) fn prelude(path: &'static str) -> Self {
        Self {
            prelude: true,
            ..Self::new(path)
        }
    }

    pub(crate) fn with_type_specs(mut self, types: impl IntoIterator<Item = TypeSpec>) -> Self {
        self.types.extend(types);
        self
    }

    pub(crate) fn documented(mut self, documentation: &'static str) -> Self {
        self.documentation = documentation;
        self
    }

    pub(crate) fn with_values(
        mut self,
        values: impl IntoIterator<Item = (&'static str, Ty)>,
    ) -> Self {
        self.values.extend(values);
        self
    }

    pub(crate) fn with_functions(
        mut self,
        functions: impl IntoIterator<Item = PureFunctionSpec>,
    ) -> Self {
        self.functions.extend(functions);
        self
    }

    pub(crate) fn with_constructors(
        mut self,
        constructors: impl IntoIterator<Item = ConstructorSpec>,
    ) -> Self {
        self.constructors.extend(constructors);
        self
    }

    pub(crate) fn with_actions(
        mut self,
        actions: impl IntoIterator<Item = ActionContractSpec>,
    ) -> Self {
        self.actions.extend(actions);
        self
    }

    fn validate(&self) -> Result<(), CatalogError> {
        if !valid_module_path(self.path) {
            return Err(CatalogError::InvalidModulePath(self.path.to_owned()));
        }

        let mut type_names = BTreeSet::new();
        for spec in &self.types {
            let name = spec.name;
            if !valid_export_name(name) {
                return Err(CatalogError::InvalidExport {
                    module: self.path.to_owned(),
                    name: name.to_owned(),
                });
            }
            if !type_names.insert(name) {
                return Err(CatalogError::DuplicateType {
                    module: self.path.to_owned(),
                    name: name.to_owned(),
                });
            }
            if spec.implements.iter().any(|contract| contract.is_empty()) {
                return Err(CatalogError::InvalidType {
                    module: self.path.to_owned(),
                    name: name.to_owned(),
                    message: "implemented type names cannot be empty".to_owned(),
                });
            }
        }

        let mut exports = BTreeSet::new();
        for name in self
            .values
            .iter()
            .map(|(name, _)| *name)
            .chain(self.functions.iter().map(|function| function.name))
            .chain(self.constructors.iter().map(|constructor| constructor.name))
            .chain(
                self.actions
                    .iter()
                    .filter_map(ActionContractSpec::source_name),
            )
        {
            if !valid_export_name(name) {
                return Err(CatalogError::InvalidExport {
                    module: self.path.to_owned(),
                    name: name.to_owned(),
                });
            }
            if !exports.insert(name) {
                return Err(CatalogError::DuplicateExport {
                    module: self.path.to_owned(),
                    name: name.to_owned(),
                });
            }
        }

        for function in &self.functions {
            if function.operation.is_empty() {
                return Err(CatalogError::InvalidFunction {
                    module: self.path.to_owned(),
                    name: function.name.to_owned(),
                    message: "operation identity cannot be empty".to_owned(),
                });
            }
        }

        for constructor in &self.constructors {
            if constructor.operation.is_empty() {
                return Err(CatalogError::InvalidConstructor {
                    module: self.path.to_owned(),
                    name: constructor.name.to_owned(),
                    message: "operation identity cannot be empty".to_owned(),
                });
            }
        }

        for action in &self.actions {
            action
                .validate()
                .map_err(|message| CatalogError::InvalidAction {
                    module: self.path.to_owned(),
                    operation: action.operation.to_owned(),
                    message,
                })?;
        }
        Ok(())
    }
}

/// Standard modules written in Lab, in the order they compile. Each may import
/// the ones before it and nothing after, so the bootstrap is a straight line
/// rather than a graph to resolve.
const AUTHORED_SOURCES: &[(&str, &str)] = &[
    ("std.bio.ontology", include_str!("authored/ontology.lab")),
    ("std.bio.designs", include_str!("authored/designs.lab")),
    ("std.bio.parts", include_str!("authored/parts.lab")),
    ("std.bio.backbones", include_str!("authored/backbones.lab")),
    ("std.bio.reporters", include_str!("authored/reporters.lab")),
    (
        "std.bio.golden_gate",
        include_str!("authored/golden_gate.lab"),
    ),
    (
        "std.lab.competence",
        include_str!("authored/competence.lab"),
    ),
];

static AUTHORED: OnceLock<Arc<BTreeMap<&'static str, ModuleInterface>>> = OnceLock::new();

/// The modules a Lab-written standard module imports, in the order it writes
/// them. A checked interface records what a module exports rather than what it
/// depends on, so this reads the imports back from the source it compiled.
pub(crate) fn authored_imports(path: &str) -> Vec<String> {
    let Some((_, source)) = AUTHORED_SOURCES.iter().find(|(name, _)| *name == path) else {
        return Vec::new();
    };
    let module = crate::parse_module(source).expect("bundled module must parse");
    module
        .items
        .iter()
        .filter_map(|item| match item {
            crate::ast::Item::Use(import) => Some(
                import
                    .path
                    .segments
                    .iter()
                    .map(|segment| segment.value.as_str())
                    .collect::<Vec<_>>()
                    .join("."),
            ),
            _ => None,
        })
        .collect()
}

/// The compiled interfaces of the Lab-written standard modules.
pub(crate) fn authored_interfaces() -> Arc<BTreeMap<&'static str, ModuleInterface>> {
    authored_modules()
}

/// Compile the Lab-written standard modules once for the life of the process.
///
/// A checker is built for every module compiled, so doing this eagerly on each
/// one would recompile the standard library per keystroke in an editor.
fn authored_modules() -> Arc<BTreeMap<&'static str, ModuleInterface>> {
    AUTHORED
        .get_or_init(|| {
            let mut compiled: BTreeMap<&'static str, ModuleInterface> = BTreeMap::new();
            for (path, source) in AUTHORED_SOURCES {
                let library = StandardLibrary::native(Arc::new(compiled.clone()));
                let module =
                    crate::compile_module_with_library(ModuleId::new(*path), source, library)
                        .unwrap_or_else(|error| {
                            panic!("bundled module '{path}' must compile: {error}")
                        });
                compiled.insert(path, module.interface);
            }
            Arc::new(compiled)
        })
        .clone()
}

#[derive(Clone, Debug)]
pub(crate) struct StandardLibrary {
    modules: BTreeMap<&'static str, StandardModule>,
    /// Standard modules written in Lab rather than in Rust. They arrive as
    /// checked interfaces, so an importer resolves them exactly as it resolves
    /// a module from a package.
    authored: Arc<BTreeMap<&'static str, ModuleInterface>>,
}

impl StandardLibrary {
    pub(crate) fn bundled() -> Self {
        Self::native(authored_modules())
    }

    /// The Rust-defined modules, together with whichever Lab-defined ones are
    /// ready. Compiling a Lab-defined module uses this with only its
    /// predecessors, which is what keeps the bootstrap from recursing.
    fn native(authored: Arc<BTreeMap<&'static str, ModuleInterface>>) -> Self {
        let modules = crate::standard_library::prelude::modules()
            .into_iter()
            .chain(crate::standard_library::bio::modules())
            .chain(crate::standard_library::lab::modules());
        let mut library =
            Self::from_modules(modules).expect("bundled standard-library catalog must be valid");
        library.authored = authored;
        library
    }

    fn from_modules(
        modules: impl IntoIterator<Item = StandardModule>,
    ) -> Result<Self, CatalogError> {
        let mut indexed = BTreeMap::new();
        let mut operations = BTreeMap::<String, &str>::new();
        for module in modules {
            module.validate()?;
            if indexed.contains_key(module.path) {
                return Err(CatalogError::DuplicateModule(module.path.to_owned()));
            }
            for operation in module
                .functions
                .iter()
                .map(|function| function.operation)
                .chain(
                    module
                        .constructors
                        .iter()
                        .map(|constructor| constructor.operation),
                )
                .chain(
                    module
                        .actions
                        .iter()
                        .map(|action| action.operation.as_str()),
                )
                .map(str::to_owned)
            {
                if let Some(previous) = operations.insert(operation.clone(), module.path) {
                    return Err(CatalogError::DuplicateOperation {
                        operation,
                        first_module: previous.to_owned(),
                        second_module: module.path.to_owned(),
                    });
                }
            }
            indexed.insert(module.path, module);
        }
        Ok(Self {
            modules: indexed,
            authored: Arc::new(BTreeMap::new()),
        })
    }

    pub(crate) fn module(&self, path: &str) -> Option<&StandardModule> {
        self.modules.get(path)
    }

    /// The checked interface of a standard module written in Lab.
    pub(crate) fn authored_module(&self, path: &str) -> Option<&ModuleInterface> {
        self.authored.get(path)
    }

    pub(crate) fn prelude_modules(&self) -> impl Iterator<Item = &StandardModule> {
        self.modules.values().filter(|module| module.prelude)
    }

    /// Every module defined in Rust.
    pub(crate) fn native_modules(&self) -> impl Iterator<Item = &StandardModule> {
        self.modules.values()
    }

    /// Every module written in Lab, with the interface it compiled to.
    pub(crate) fn authored_interfaces(
        &self,
    ) -> impl Iterator<Item = (&&'static str, &ModuleInterface)> {
        self.authored.iter()
    }

    /// Every durable action contract, across modules. The lineage analysis
    /// reads results from these rather than from any one module.
    pub(crate) fn action_specs(&self) -> impl Iterator<Item = &ActionContractSpec> {
        self.modules
            .values()
            .flat_map(|module| module.actions.iter())
    }

    pub(crate) fn action_providers(&self, name: &str) -> Vec<&'static str> {
        self.modules
            .values()
            .filter(|module| {
                module
                    .actions
                    .iter()
                    .any(|action| action.source_name() == Some(name))
            })
            .map(|module| module.path)
            .collect()
    }

    pub(crate) fn function_providers(&self, name: &str) -> Vec<&'static str> {
        self.modules
            .values()
            .filter(|module| {
                module
                    .functions
                    .iter()
                    .any(|function| function.name == name)
            })
            .map(|module| module.path)
            .collect()
    }

    pub(crate) fn render_markdown(&self) -> String {
        let mut output = String::from("# Lab standard library\n\n");
        for (path, interface) in self.authored.iter() {
            output.push_str(&format!("## `{path}`\n\nWritten in Lab.\n\n"));
            if !interface.documentation.is_empty() {
                output.push_str(&interface.documentation);
                output.push_str("\n\n");
            }
            for (name, export) in &interface.exports {
                let kind = match export.kind {
                    ExportKind::Role => "role",
                    ExportKind::Type => "type",
                    ExportKind::Workflow => "workflow",
                    ExportKind::Function => "circuit",
                    _ => "value",
                };
                output.push_str(&format!("- {kind} `{name}`"));
                if !export.roles.is_empty() {
                    output.push_str(&format!(" is {}", export.roles.join(", ")));
                }
                if !export.documentation.is_empty() {
                    output.push_str(&format!(": {}", export.documentation));
                }
                output.push('\n');
            }
            output.push('\n');
        }
        for module in self.modules.values() {
            output.push_str(&format!("## `{}`\n\n", module.path));
            if !module.documentation.is_empty() {
                output.push_str(module.documentation);
                output.push_str("\n\n");
            }
            for spec in &module.types {
                // A law is a role the compiler enforces a rule for, and there is
                // no source form for declaring one — so the reference is where
                // that privilege has to be visible.
                let kind = match (spec.law, spec.role) {
                    (true, _) => "law",
                    (_, true) => "role",
                    _ => "type",
                };
                output.push_str(&format!("- {kind} `{}`", spec.name));
                if spec.parameters != 0 {
                    output.push_str(&format!(" ({} type parameter(s))", spec.parameters));
                }
                if !spec.implements.is_empty() {
                    output.push_str(&format!(" implements {}", spec.implements.join(", ")));
                }
                if !spec.documentation.is_empty() {
                    output.push_str(&format!(": {}", spec.documentation));
                }
                output.push('\n');
            }
            for (name, ty) in &module.values {
                output.push_str(&format!("- value `{name}: {ty}`\n"));
            }
            for function in &module.functions {
                output.push_str(&format!("- function `{}`", function.name));
                if !function.documentation.is_empty() {
                    output.push_str(&format!(": {}", function.documentation));
                }
                output.push('\n');
            }
            for constructor in &module.constructors {
                output.push_str(&format!("- constructor `{}`", constructor.name));
                if !constructor.documentation.is_empty() {
                    output.push_str(&format!(": {}", constructor.documentation));
                }
                output.push('\n');
            }
            for action in &module.actions {
                if let Some(name) = action.source_name() {
                    output.push_str(&format!("- action `{name}` [{}]\n", action.operation));
                }
            }
            output.push('\n');
        }
        output
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
enum CatalogError {
    #[error("invalid standard-library module path '{0}'")]
    InvalidModulePath(String),
    #[error("standard-library module '{0}' is registered more than once")]
    DuplicateModule(String),
    #[error("type '{name}' is exported more than once by '{module}'")]
    DuplicateType { module: String, name: String },
    #[error("invalid type '{name}' in '{module}': {message}")]
    InvalidType {
        module: String,
        name: String,
        message: String,
    },
    #[error("name '{name}' is exported more than once by '{module}'")]
    DuplicateExport { module: String, name: String },
    #[error("invalid exported name '{name}' in '{module}'")]
    InvalidExport { module: String, name: String },
    #[error("invalid pure function '{name}' in '{module}': {message}")]
    InvalidFunction {
        module: String,
        name: String,
        message: String,
    },
    #[error("invalid constructor '{name}' in '{module}': {message}")]
    InvalidConstructor {
        module: String,
        name: String,
        message: String,
    },
    #[error("operation '{operation}' is registered by both '{first_module}' and '{second_module}'")]
    DuplicateOperation {
        operation: String,
        first_module: String,
        second_module: String,
    },
    #[error("invalid action '{operation}' in '{module}': {message}")]
    InvalidAction {
        module: String,
        operation: String,
        message: String,
    },
}

fn valid_module_path(path: &str) -> bool {
    let mut segments = path.split('.');
    segments.next() == Some("std")
        && segments.clone().next().is_some()
        && segments.all(|segment| {
            let mut characters = segment.chars();
            characters
                .next()
                .is_some_and(|character| character.is_ascii_alphabetic())
                && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
        })
}

fn valid_export_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('.').all(|segment| {
            let mut characters = segment.chars();
            characters
                .next()
                .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
                && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
        })
}

#[cfg(test)]
mod tests {
    use crate::standard_library::catalog::*;
    use crate::standard_library::contract::{ContractType, PhrasePart};

    #[test]
    fn bundled_catalog_has_expected_namespaces_and_prelude() {
        let library = StandardLibrary::bundled();
        let prelude = library.module("std.prelude").unwrap();
        assert!(prelude.types.iter().any(|spec| spec.name == "Plasmid"));
        assert!(
            prelude
                .functions
                .iter()
                .any(|function| { function.name == "dna" && function.operation == "std.bio.dna" })
        );

        // Catalogs are written in Lab; what stays in Rust is the half with
        // no source declaration form.
        assert!(library.module("std.bio.parts").is_none());
        assert!(library.authored_module("std.bio.parts").is_some());
        assert!(library.module("std.bio.build").is_some());
        let plasmid = library.module("std.lab.plasmid").unwrap();
        assert!(plasmid.actions.iter().any(|action| {
            action.source_name() == Some("transform")
                && action.operation == "std.lab.plasmid.transform"
        }));
        assert_eq!(library.prelude_modules().count(), 1);
    }

    #[test]
    fn rejects_duplicate_modules_and_exports() {
        let duplicate_modules = StandardLibrary::from_modules([
            StandardModule::new("std.test"),
            StandardModule::new("std.test"),
        ]);
        assert!(matches!(
            duplicate_modules,
            Err(CatalogError::DuplicateModule(module)) if module == "std.test"
        ));

        let duplicate_exports = StandardLibrary::from_modules([StandardModule::new("std.test")
            .with_values([("thing", Ty::String)])
            .with_functions([PureFunctionSpec::new(
                "thing",
                "std.test.thing",
                Vec::new(),
                Ty::String,
            )])]);
        assert!(matches!(
            duplicate_exports,
            Err(CatalogError::DuplicateExport { module, name })
                if module == "std.test" && name == "thing"
        ));
    }

    #[test]
    fn rejects_malformed_action_contracts_during_registration() {
        let malformed = ActionContractSpec {
            operation: "std.test.broken".to_owned(),
            phrase: vec![PhrasePart::operand(
                "input",
                ContractType::Concrete(Ty::String),
                crate::OwnershipMode::Copy,
            )],
            inert: Vec::new(),
            results: Vec::new(),
        };
        let result = StandardLibrary::from_modules([
            StandardModule::new("std.test").with_actions([malformed])
        ]);
        assert!(matches!(result, Err(CatalogError::InvalidAction { .. })));
    }

    #[test]
    fn rejects_functions_without_stable_operation_identities() {
        let result = StandardLibrary::from_modules([StandardModule::new("std.test")
            .with_functions([PureFunctionSpec::new("broken", "", Vec::new(), Ty::String)])]);
        assert!(matches!(result, Err(CatalogError::InvalidFunction { .. })));
    }

    #[test]
    fn renders_reference_docs_from_the_semantic_catalog() {
        let docs = StandardLibrary::bundled().render_markdown();
        assert!(docs.contains("## `std.prelude`"));
        assert!(docs.contains("type `Plasmid`"));
        assert!(docs.contains("function `dna`"));
        assert!(docs.contains("action `transform`"));
    }

    /// A module written in Lab documents itself from its own source, so its
    /// reference entry comes from the same text a reader sees in the file.
    #[test]
    fn renders_reference_docs_for_modules_written_in_lab() {
        let docs = StandardLibrary::bundled().render_markdown();
        assert!(docs.contains("## `std.bio.reporters`"), "{docs}");
        assert!(docs.contains("Written in Lab."), "{docs}");
        assert!(docs.contains("role `Reporter`"), "{docs}");
        assert!(
            docs.contains("type `Fluorescence` is Reporter"),
            "membership is part of the published surface: {docs}"
        );
        assert!(
            docs.contains("read by a plate reader"),
            "the declaration's own documentation travels: {docs}"
        );
    }

    #[test]
    fn a_module_written_in_lab_exports_its_roles_and_their_members() {
        let library = StandardLibrary::bundled();
        let interface = library
            .authored_module("std.bio.reporters")
            .expect("the bundled Lab modules compiled");

        assert_eq!(interface.exports["Reporter"].kind, ExportKind::Role);
        assert_eq!(interface.exports["Fluorescence"].kind, ExportKind::Type);
        assert_eq!(interface.exports["Fluorescence"].roles, ["Reporter"]);
        assert!(
            interface
                .documentation
                .starts_with("Reporters and the readouts"),
            "{}",
            interface.documentation
        );
    }

    /// The bootstrap must not depend on being called from a particular place:
    /// compiling any module builds a checker, which builds the library.
    #[test]
    fn a_user_module_may_import_a_standard_module_written_in_lab() {
        let module = crate::compile_module(
            "use std.bio.reporters\n\nrole Inducer\n\nworkflow read(\n  design: Circuit<any Inducer, Fluorescence>,\n) -> Circuit<any Inducer, Fluorescence>:\n  return design\n",
        )
        .expect("a Lab-written standard module resolves like any other import");
        assert_eq!(module.imports[0].module, "std.bio.reporters");
        assert_eq!(module.imports[0].provider, "builtin-standard-library");
    }
}
