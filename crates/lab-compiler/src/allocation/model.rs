//! Facility-bound semantic records produced by allocation.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::method::{IntentOperationId, LocalId};
use crate::planning::{
    PlanningMethodYield, PlanningPort, PlanningProcedureParameter, PlanningTaskInput,
    PlanningTaskOutput, SelectedCapabilityParameter, SelectedMaterialBinding,
};
use crate::procedure::ProcedureProgram;
use lab_capability::{
    CapabilityKind, ControlMode, MethodId, ProcedureImplementationId, QualificationLevel,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One complete facility allocation, independent of its encoded LAIR representation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AllocatedProgram {
    pub problem_sha256: String,
    pub inventory_sha256: String,
    pub facility: String,
    pub methods: Vec<AllocatedMethod>,
}

/// One selected Method and its facility-bound Procedure graph.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AllocatedMethod {
    pub choice: LocalId,
    pub source_operation: IntentOperationId,
    pub method: MethodId,
    /// Explicit completion dependencies retained from the selected Method choice.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<LocalId>,
    /// Exact selected-Method input bindings, including cross-choice value edges.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<PlanningPort>,
    /// Exact selected-Method output ports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<PlanningPort>,
    /// The selected Procedure value that realizes each Method output.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub yields: Vec<PlanningMethodYield>,
    pub tasks: Vec<AllocatedProcedureTask>,
}

/// One semantic Procedure node after facility allocation.
///
/// The task remains present even when it is manual and therefore has no adapter invocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AllocatedProcedureTask {
    pub id: LocalId,
    pub operation: lab_capability::OperationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<ProcedureProgram>,
    pub inputs: Vec<PlanningTaskInput>,
    pub outputs: Vec<PlanningTaskOutput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<PlanningProcedureParameter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<SelectedMaterialBinding>,
    pub requirements: Vec<AllocatedRequirementBinding>,
}

/// The exact catalog and optional implementation binding for one semantic requirement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AllocatedRequirementBinding {
    pub id: LocalId,
    pub capability_kind: CapabilityKind,
    pub minimum_qualification: QualificationLevel,
    pub accepted_control_modes: BTreeSet<ControlMode>,
    pub offering: String,
    pub asset: String,
    pub observed_qualification: String,
    pub control_mode: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<SelectedCapabilityParameter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedure_implementation: Option<ProcedureImplementationId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<InvocationAdapter>,
}

/// One exact driver and profile selected for a physical Asset.
///
/// Versioned Procedure implementation identities remain on individual requirement bindings so a
/// single adapter invocation can realize several explicit contracts without splitting the Asset.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct InvocationAdapter {
    pub driver: String,
    pub profile_path: PathBuf,
    pub profile_sha256: String,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub features: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub accepted_run_formats: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub emitted_run_formats: BTreeSet<String>,
}
