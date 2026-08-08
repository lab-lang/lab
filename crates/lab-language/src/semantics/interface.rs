use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{DefinitionId, ModuleId};
use crate::checked::{CheckedField, CheckedType};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportKind {
    Type,
    Value,
    Function,
    Action,
    Constructor,
    Workflow,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleExport {
    pub definition: DefinitionId,
    pub kind: ExportKind,
    pub r#type: Option<CheckedType>,
    pub callable: Option<CallableSignature>,
    pub fields: BTreeMap<String, CheckedType>,
    pub documentation: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallableSignature {
    pub inputs: Vec<CheckedType>,
    pub outputs: Vec<CheckedField>,
}

/// Checked public surface supplied to import resolution independently of
/// module bodies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleInterface {
    pub module: ModuleId,
    pub documentation: String,
    pub exports: BTreeMap<String, ModuleExport>,
}

#[derive(Clone, Debug, Default)]
pub struct SemanticEnvironment {
    modules: BTreeMap<String, ModuleInterface>,
}

impl SemanticEnvironment {
    pub fn new(interfaces: impl IntoIterator<Item = ModuleInterface>) -> Self {
        Self {
            modules: interfaces
                .into_iter()
                .map(|interface| (interface.module.to_string(), interface))
                .collect(),
        }
    }

    pub fn module(&self, name: &str) -> Option<&ModuleInterface> {
        self.modules.get(name)
    }

    pub fn insert(&mut self, visible_name: impl Into<String>, interface: ModuleInterface) {
        self.modules.insert(visible_name.into(), interface);
    }

    pub fn extend(&mut self, other: &Self) {
        self.modules.extend(other.modules.clone());
    }
}

impl ModuleInterface {
    pub fn empty(module: ModuleId) -> Self {
        Self {
            module,
            documentation: String::new(),
            exports: BTreeMap::new(),
        }
    }
}
