use std::collections::BTreeMap;

use crate::{AcceptanceObligation, OperationKind, PlanValue, ValueKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulationEvent {
    pub sequence: usize,
    pub step_id: String,
    pub operation: OperationKind,
    pub inputs: Vec<String>,
    pub outputs: Vec<PlanValue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulationTrace {
    pub protocol: String,
    pub environment: String,
    pub events: Vec<SimulationEvent>,
    pub final_state: LabState,
    pub acceptance: Vec<AcceptanceObligation>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabState {
    pub values: BTreeMap<String, SimulatedValue>,
    pub elapsed_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulatedValue {
    pub kind: ValueKind,
    pub available: bool,
    pub consumed: bool,
}
