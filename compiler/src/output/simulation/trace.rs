use crate::{AcceptanceObligation, OperationKind, PlanValue};
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
    pub artifact: String,
    pub lab_profile: String,
    pub events: Vec<SimulationEvent>,
    pub acceptance: Vec<AcceptanceObligation>,
}
