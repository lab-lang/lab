use std::collections::BTreeSet;

use lab_capability::{
    AbsoluteIri, CapabilityKind, ControlMode, MethodId, OperationId, PropertyConstraint,
    QualificationLevel,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{IntentOperationId, LocalId};

/// The semantic type of one method or Procedure port.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PortType {
    /// Reusable biological design information retained in the Design dialect.
    Design,
    /// Physical matter in an open, method-defined state.
    Material { state: AbsoluteIri },
    /// Non-physical information or evidence of an open semantic kind.
    Data { data_kind: AbsoluteIri },
}

/// One named input accepted by every candidate refining the same Intent operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MethodInput {
    pub name: LocalId,
    pub port_type: PortType,
}

/// A reference to a Procedure value available at a task or method output.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValueReference {
    Input { input: LocalId },
    TaskOutput { task: LocalId, output: LocalId },
}

/// One typed result produced by a Procedure task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TaskOutput {
    pub name: LocalId,
    pub port_type: PortType,
}

/// One result yielded from a complete method candidate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MethodOutput {
    pub name: LocalId,
    pub source: ValueReference,
}

/// One semantic capability requirement owned by its enclosing Procedure task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityRequirementDefinition {
    pub id: LocalId,
    pub capability_kind: CapabilityKind,
    pub minimum_qualification: QualificationLevel,
    pub accepted_control_modes: BTreeSet<ControlMode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<PropertyConstraint>,
}

/// One task in topological order within a facility-independent Procedure graph.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProcedureTaskDefinition {
    pub id: LocalId,
    pub operation: OperationId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<ValueReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<TaskOutput>,
    pub requirements: Vec<CapabilityRequirementDefinition>,
}

/// A portable method definition that refines one Intent operation without selecting a facility.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MethodDefinition {
    pub id: MethodId,
    pub refines: IntentOperationId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<MethodInput>,
    pub tasks: Vec<ProcedureTaskDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<MethodOutput>,
}

/// The common typed boundary all methods refining one Intent operation must implement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodSignature {
    pub inputs: Vec<MethodInput>,
    pub outputs: Vec<TaskOutput>,
}
