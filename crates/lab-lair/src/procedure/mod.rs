//! Compiler-owned normalization from open Method operations into canonical Procedure contracts.

mod chemical_transformation;
mod golden_gate;
mod plating;
mod recovery;
mod serial_dilution;
mod thermal_cycle;
mod view;

use crate::method::{LocalId, ProcedureValue};
use lab_capability::OperationId;
use lab_procedure::ProcedureProgram;
use thiserror::Error;

pub(crate) const SETUP_GOLDEN_GATE: &str =
    "https://www.lab-compiler.org/ns/procedure#SetupGoldenGateReaction";
pub(crate) const PROVISION_MATERIAL: &str =
    "https://www.lab-compiler.org/ns/procedure#ProvisionMaterial";
pub(crate) const SERIAL_DILUTION: &str =
    "https://www.lab-compiler.org/ns/procedure#SeriallyDiluteCulture";
pub(crate) const CYCLE_GOLDEN_GATE: &str =
    "https://www.lab-compiler.org/ns/procedure#ThermalCycleGoldenGateReaction";
pub(crate) const PREPARE_CHEMICAL_TRANSFORMATION: &str =
    "https://www.lab-compiler.org/ns/procedure#PrepareChemicalTransformation";
pub(crate) const HEAT_SHOCK_TRANSFORMATION: &str =
    "https://www.lab-compiler.org/ns/procedure#HeatShockTransformation";
pub(crate) const ADD_RECOVERY_MEDIUM: &str =
    "https://www.lab-compiler.org/ns/procedure#AddRecoveryMedium";
pub(crate) const INCUBATE_RECOVERY_CULTURE: &str =
    "https://www.lab-compiler.org/ns/procedure#IncubateRecoveryCulture";
pub(crate) const PLATE_DILUTED_CULTURE: &str =
    "https://www.lab-compiler.org/ns/procedure#PlateDilutedCulture";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedProcedureParameter {
    pub(crate) id: LocalId,
    pub(crate) value: ProcedureValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedProcedureMaterial {
    pub(crate) id: LocalId,
    pub(crate) symbol: String,
}

pub(crate) struct ProcedureTaskInstance<'a> {
    pub(crate) id: &'a LocalId,
    pub(crate) operation: &'a OperationId,
    pub(crate) input_count: usize,
    pub(crate) outputs: &'a [LocalId],
    pub(crate) parameters: &'a [ResolvedProcedureParameter],
    pub(crate) materials: &'a [ResolvedProcedureMaterial],
}

pub(crate) fn normalize_task(
    task: &ProcedureTaskInstance<'_>,
) -> Result<Option<ProcedureProgram>, ProcedureNormalizationError> {
    let result = match task.operation.as_str() {
        SETUP_GOLDEN_GATE => Some(golden_gate::normalize(task)),
        SERIAL_DILUTION => Some(serial_dilution::normalize(task)),
        CYCLE_GOLDEN_GATE => Some(thermal_cycle::normalize(task)),
        PREPARE_CHEMICAL_TRANSFORMATION => Some(chemical_transformation::normalize_prepare(task)),
        HEAT_SHOCK_TRANSFORMATION => Some(chemical_transformation::normalize_heat_shock(task)),
        ADD_RECOVERY_MEDIUM => Some(recovery::normalize_add_medium(task)),
        INCUBATE_RECOVERY_CULTURE => Some(recovery::normalize_incubation(task)),
        PLATE_DILUTED_CULTURE => Some(plating::normalize(task)),
        _ => None,
    };
    result
        .transpose()
        .map_err(|message| ProcedureNormalizationError {
            task: task.id.clone(),
            operation: task.operation.clone(),
            message,
        })
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("Procedure task `{task}` operation `{operation}` cannot be normalized: {message}")]
pub(crate) struct ProcedureNormalizationError {
    task: LocalId,
    operation: OperationId,
    message: String,
}
