//! Built-in Procedure-program construction algorithms.
//!
//! This module owns implementations only. The public builder registry assigns every algorithm its
//! stable identity and contract; no Method operation name is inspected here.

mod chemical_transformation;
mod golden_gate;
mod plating;
mod recovery;
mod serial_dilution;
mod thermal_cycle;
mod view;

use crate::procedure::{ProcedureProgram, ProcedureProgramBuildContext};

pub(crate) fn build_golden_gate(
    context: &ProcedureProgramBuildContext<'_>,
) -> Result<ProcedureProgram, String> {
    golden_gate::normalize(context)
}

pub(crate) fn build_serial_dilution(
    context: &ProcedureProgramBuildContext<'_>,
) -> Result<ProcedureProgram, String> {
    serial_dilution::normalize(context)
}

pub(crate) fn build_golden_gate_cycle(
    context: &ProcedureProgramBuildContext<'_>,
) -> Result<ProcedureProgram, String> {
    thermal_cycle::normalize(context)
}

pub(crate) fn build_chemical_transformation_preparation(
    context: &ProcedureProgramBuildContext<'_>,
) -> Result<ProcedureProgram, String> {
    chemical_transformation::normalize_prepare(context)
}

pub(crate) fn build_chemical_transformation_heat_shock(
    context: &ProcedureProgramBuildContext<'_>,
) -> Result<ProcedureProgram, String> {
    chemical_transformation::normalize_heat_shock(context)
}

pub(crate) fn build_recovery_medium_addition(
    context: &ProcedureProgramBuildContext<'_>,
) -> Result<ProcedureProgram, String> {
    recovery::normalize_add_medium(context)
}

pub(crate) fn build_recovery_incubation(
    context: &ProcedureProgramBuildContext<'_>,
) -> Result<ProcedureProgram, String> {
    recovery::normalize_incubation(context)
}

pub(crate) fn build_selective_plating(
    context: &ProcedureProgramBuildContext<'_>,
) -> Result<ProcedureProgram, String> {
    plating::normalize(context)
}
