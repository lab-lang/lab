//! Immutable catalog and validation for bundled standard-library modules.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::standard_library::contract::ActionContractSpec;
use crate::type_system::Ty;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeSpec {
    pub name: &'static str,
    pub parameters: usize,
    pub fields: BTreeMap<&'static str, Ty>,
    pub implements: Vec<&'static str>,
    pub documentation: &'static str,
}

impl TypeSpec {
    pub(crate) fn nominal(name: &'static str) -> Self {
        Self {
            name,
            parameters: 0,
            fields: BTreeMap::new(),
            implements: Vec::new(),
            documentation: "",
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

#[derive(Clone, Debug)]
pub(crate) struct StandardLibrary {
    modules: BTreeMap<&'static str, StandardModule>,
}

impl StandardLibrary {
    pub(crate) fn bundled() -> Self {
        let modules = crate::standard_library::prelude::modules()
            .into_iter()
            .chain(crate::standard_library::bio::modules())
            .chain(crate::standard_library::lab::modules());
        Self::from_modules(modules).expect("bundled standard-library catalog must be valid")
    }

    fn from_modules(
        modules: impl IntoIterator<Item = StandardModule>,
    ) -> Result<Self, CatalogError> {
        let mut indexed = BTreeMap::new();
        let mut operations = BTreeMap::<&str, &str>::new();
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
                .chain(module.actions.iter().map(|action| action.operation))
            {
                if let Some(previous) = operations.insert(operation, module.path) {
                    return Err(CatalogError::DuplicateOperation {
                        operation: operation.to_owned(),
                        first_module: previous.to_owned(),
                        second_module: module.path.to_owned(),
                    });
                }
            }
            indexed.insert(module.path, module);
        }
        Ok(Self { modules: indexed })
    }

    pub(crate) fn module(&self, path: &str) -> Option<&StandardModule> {
        self.modules.get(path)
    }

    pub(crate) fn prelude_modules(&self) -> impl Iterator<Item = &StandardModule> {
        self.modules.values().filter(|module| module.prelude)
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
        for module in self.modules.values() {
            output.push_str(&format!("## `{}`\n\n", module.path));
            if !module.documentation.is_empty() {
                output.push_str(module.documentation);
                output.push_str("\n\n");
            }
            for spec in &module.types {
                output.push_str(&format!("- type `{}`", spec.name));
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
                    output.push_str(&format!("- action `{name}` [{}]\n", action.capability));
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

        let inventory = library.module("std.bio.inventory").unwrap();
        assert!(
            inventory
                .functions
                .iter()
                .any(|function| function.name == "backbone")
        );
        assert!(library.module("std.bio.build").is_some());
        let plasmid_actions = library.module("std.lab.plasmid_actions").unwrap();
        assert!(plasmid_actions.actions.iter().any(|action| {
            action.source_name() == Some("transform")
                && action.operation == "std.lab.plasmid_actions.transform"
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
            operation: "std.test.broken",
            capability: "testing",
            phrase: vec![PhrasePart::Operand {
                name: "input",
                r#type: ContractType::Concrete(Ty::String),
                mode: crate::OwnershipMode::Copy,
            }],
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
}
