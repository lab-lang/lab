//! Semantic state owned by one checker invocation.
//!
//! Keeping catalogs and symbol tables here makes their lifetime and mutation
//! boundary explicit. Checking algorithms operate on this state but do not
//! own ad-hoc global maps of their own.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::ast::{DataKind, Path};
use crate::checked::CheckedType;
use crate::semantic_error::SemanticError;
use crate::semantics::{DefinitionId, ExportKind, ModuleId, ModuleInterface, SemanticEnvironment};
use crate::source::Span;
use crate::standard_library::{
    ActionContractSpec, ConstructorSpec, PureFunctionSpec, StandardLibrary, StandardModule,
    TypeSpec,
};
use crate::type_system::{Ty, from_checked_type};

#[derive(Clone)]
pub(super) struct CircuitSignature {
    pub parameters: Vec<String>,
    pub bounds: HashMap<String, Ty>,
    pub inputs: Vec<Ty>,
    pub output: Ty,
}

#[derive(Clone)]
pub(super) struct DataSignature {
    pub kind: DataKind,
    pub fields: BTreeMap<String, Ty>,
    pub cases: BTreeMap<String, BTreeMap<String, Ty>>,
}

#[derive(Clone)]
pub(super) struct WorkflowSignature {
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
    pub circuits: HashMap<String, CircuitSignature>,
    pub data: HashMap<String, DataSignature>,
    pub cases: HashMap<String, String>,
    pub event_types: BTreeSet<String>,
    pub workflows: HashMap<String, WorkflowSignature>,
}

impl SemanticContext {
    pub fn new(module_id: ModuleId, provided_modules: SemanticEnvironment) -> Self {
        Self {
            module_id,
            provided_modules,
            standard_library: StandardLibrary::bundled(),
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
            circuits: HashMap::new(),
            data: HashMap::new(),
            cases: HashMap::new(),
            event_types: BTreeSet::new(),
            workflows: HashMap::new(),
        }
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
            if export.kind != ExportKind::Type {
                continue;
            }
            if !self.known_types.insert(name.clone()) {
                return Err(SemanticError::new(
                    span,
                    format!("imported type '{name}' is ambiguous"),
                ));
            }
            self.data.insert(
                name.clone(),
                DataSignature {
                    kind: DataKind::Record,
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
                ExportKind::Type => {}
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
                                parameters: Vec::new(),
                                bounds: HashMap::new(),
                                inputs: signature.inputs.iter().map(from_checked_type).collect(),
                                output: from_checked_type(&output.r#type),
                            },
                        );
                    }
                }
                ExportKind::Workflow | ExportKind::Action => {
                    self.insert_imported_name(interface.module.as_str(), name, span)?;
                    if let Some(signature) = &export.callable {
                        self.workflows.insert(
                            name.clone(),
                            WorkflowSignature {
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
