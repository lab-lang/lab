use std::fmt;

use serde::{Deserialize, Serialize};

/// Stable identity of a source or provided module within one compilation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModuleId(String);

impl ModuleId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn standalone() -> Self {
        Self::new("standalone")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModuleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Identity of a declaration after name resolution.
///
/// `local` is deterministic within the defining module. Standard-library
/// providers use the exported name; source modules use the declaration name
/// plus its byte offset so future scoped declarations cannot collide.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DefinitionId {
    pub module: ModuleId,
    pub local: String,
}

impl DefinitionId {
    pub fn exported(module: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            module: ModuleId::new(module),
            local: name.into(),
        }
    }

    pub fn source(module: ModuleId, name: &str, offset: usize) -> Self {
        Self {
            module,
            local: format!("{name}@{offset}"),
        }
    }
}
