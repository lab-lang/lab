//! Compiler-owned normalization from open Method operations into canonical Procedure contracts.

mod golden_gate;

use lab_capability::OperationId;
use lab_method::{LocalId, ProcedureValue};
use lab_procedure::ProcedureProgram;
use thiserror::Error;

pub(crate) const SETUP_GOLDEN_GATE: &str =
    "https://www.lab-compiler.org/ns/procedure#SetupGoldenGateReaction";

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
    pub(crate) outputs: &'a [LocalId],
    pub(crate) parameters: &'a [ResolvedProcedureParameter],
    pub(crate) materials: &'a [ResolvedProcedureMaterial],
}

pub(crate) fn normalize_task(
    task: &ProcedureTaskInstance<'_>,
) -> Result<Option<ProcedureProgram>, ProcedureNormalizationError> {
    let result = match task.operation.as_str() {
        SETUP_GOLDEN_GATE => Some(golden_gate::normalize(task)),
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
