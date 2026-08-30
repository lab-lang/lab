use lab_capability::{CapabilityKind, PropertyConstraint};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ProcedureLocalId;

/// How every clause in one derived capability formula must be bound.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BindingScope {
    /// Clauses may be implemented by independent facility resources.
    #[default]
    Independent,
    /// Clauses form one operation and must be implemented by one exact Asset or a declared
    /// `fac:partOf` Asset assembly under one adapter invocation.
    AtomicAssetAssembly,
}

/// One semantic facility capability demanded by a canonical Procedure program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityClause {
    pub role: ProcedureLocalId,
    pub capability_kind: CapabilityKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<PropertyConstraint>,
}

/// A conjunction of capability clauses mechanically derived from a Procedure program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityFormula {
    pub binding_scope: BindingScope,
    pub all_of: Vec<CapabilityClause>,
}
