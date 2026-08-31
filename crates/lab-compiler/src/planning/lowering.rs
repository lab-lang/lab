//! Exact record of the adapter lowerings derived from one facility allocation.

use std::collections::BTreeSet;
use std::path::PathBuf;

use lab_capability::ProcedureImplementationId;
use serde::{Deserialize, Serialize};

pub const FACILITY_LOWERING_SCHEMA_VERSION: &str = "lab.facility-lowering.v2";

/// Device artifacts emitted only after capability requirements have been allocated to a facility.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FacilityLoweringManifest {
    pub schema_version: String,
    pub inventory_sha256: String,
    pub facility: String,
    pub routes: Vec<FacilityLoweringRoute>,
}

/// One exact Asset and adapter implementation selected by allocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FacilityLoweringRoute {
    pub id: String,
    pub asset: String,
    pub driver: String,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub procedure_implementations: BTreeSet<ProcedureImplementationId>,
    pub profile_path: PathBuf,
    pub profile_sha256: String,
    pub requirements: Vec<FacilityLoweredRequirement>,
    pub output: PathBuf,
    pub artifacts: Vec<FacilityLoweredArtifact>,
}

/// A semantic requirement whose allocated route caused this adapter lowering to exist.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FacilityLoweredRequirement {
    pub requirement_instance: String,
    pub capability_kind: String,
    pub offering: String,
}

/// One immutable artifact emitted by an allocated adapter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FacilityLoweredArtifact {
    pub path: PathBuf,
    pub media_type: String,
    pub sha256: String,
    pub role: FacilityLoweredArtifactRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FacilityLoweredArtifactRole {
    AutomationProtocol,
    OperatorDocument,
    Support,
}
