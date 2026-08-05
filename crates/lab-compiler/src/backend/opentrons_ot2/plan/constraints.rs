//! OT-2-specific parameter and batch-capacity validation.

use crate::backend::TargetConstraintError;

use super::trace::ProtocolTrace;
use super::{Ot2PlanningError, REACTION_VOLUME_UL, TARGET};

const PLATE_CAPACITY: usize = 96;
const TIP_RACK_CAPACITY: usize = 96;

pub(super) fn validate_target_constraints(
    trace: &ProtocolTrace,
    context: &pliron::context::Context,
) -> Result<(), Ot2PlanningError> {
    let artifact = trace.artifact(context);
    for (parameter, value, maximum) in [
        (
            "assembly_replicates",
            trace.assembly_replicates(context),
            u8::MAX,
        ),
        (
            "transformation_replicates",
            trace.transformation_replicates(context),
            u8::MAX,
        ),
        ("plating_replicates", trace.plating_replicates(context), 8),
        ("serial_dilutions", trace.serial_dilutions(context), 2),
    ] {
        if !(1..=maximum).contains(&value) {
            return Err(TargetConstraintError::ParameterOutOfRange {
                target: TARGET.into(),
                subject: artifact.clone(),
                parameter: parameter.into(),
                minimum: 1,
                maximum: u64::from(maximum),
                found: u64::from(value),
            }
            .into());
        }
    }
    let required_ul = (1 + trace.components(context).len()) as u16 * 2 + 8;
    if required_ul > REACTION_VOLUME_UL {
        return Err(TargetConstraintError::CapacityExceeded {
            target: TARGET.into(),
            operation: "assembly".into(),
            subject: artifact,
            resource: "reaction_volume".into(),
            required: u64::from(required_ul),
            capacity: u64::from(REACTION_VOLUME_UL),
            unit: "uL".into(),
        }
        .into());
    }
    Ok(())
}

pub(super) fn validate_uniform_batch_settings(
    traces: &[ProtocolTrace],
    context: &pliron::context::Context,
) -> Result<(), Ot2PlanningError> {
    let Some(first) = traces.first() else {
        return Ok(());
    };
    let expected = (
        first.assembly_replicates(context),
        first.transformation_replicates(context),
        first.plating_replicates(context),
        first.serial_dilutions(context),
    );
    if traces.iter().skip(1).any(|trace| {
        (
            trace.assembly_replicates(context),
            trace.transformation_replicates(context),
            trace.plating_replicates(context),
            trace.serial_dilutions(context),
        ) != expected
    }) {
        Err(TargetConstraintError::NonUniformParameters {
            target: TARGET.into(),
            subject: "automation_batch".into(),
            parameters: vec![
                "assembly_replicates".into(),
                "transformation_replicates".into(),
                "plating_replicates".into(),
                "serial_dilutions".into(),
            ],
        }
        .into())
    } else {
        Ok(())
    }
}

pub(super) fn require_plate_capacity(
    stage: &'static str,
    required: usize,
) -> Result<(), Ot2PlanningError> {
    if required > PLATE_CAPACITY {
        Err(TargetConstraintError::CapacityExceeded {
            target: TARGET.into(),
            operation: stage.into(),
            subject: "automation_batch".into(),
            resource: "destination_plate".into(),
            required: required as u64,
            capacity: PLATE_CAPACITY as u64,
            unit: "wells".into(),
        }
        .into())
    } else {
        Ok(())
    }
}

pub(super) fn require_tip_capacity(
    stage: &'static str,
    pipette: &'static str,
    required: usize,
) -> Result<(), Ot2PlanningError> {
    if required > TIP_RACK_CAPACITY {
        Err(TargetConstraintError::CapacityExceeded {
            target: TARGET.into(),
            operation: stage.into(),
            subject: "automation_batch".into(),
            resource: format!("{pipette}_tip_rack"),
            required: required as u64,
            capacity: TIP_RACK_CAPACITY as u64,
            unit: "tips".into(),
        }
        .into())
    } else {
        Ok(())
    }
}
