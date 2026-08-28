//! Flex-specific parameter and batch-capacity validation.

use crate::backend::AdapterConstraintError;
use crate::backend::trace::{AssemblyTrace, StrainTrace};

use crate::backend::opentrons::flex::BACKEND;
use crate::backend::opentrons::flex::plan::FlexPlanningError;

pub(super) fn validate_assembly_constraints(
    trace: &AssemblyTrace,
    context: &pliron::context::Context,
) -> Result<(), FlexPlanningError> {
    let artifact = trace.artifact(context);
    require_range(
        &artifact,
        "assembly_replicates",
        trace.assembly_replicates(context),
        u8::MAX,
    )?;
    let reaction_volume_ul = trace.chemistry(context, "reaction_volume_ul");
    let dna_pieces = (1 + trace.components(context).len()) as u16;
    let required_ul = trace.chemistry(context, "buffer_volume_ul")
        + trace.chemistry(context, "ligase_volume_ul")
        + trace.chemistry(context, "enzyme_volume_ul")
        + trace.chemistry(context, "part_volume_ul") * dna_pieces;
    if required_ul > reaction_volume_ul {
        return Err(AdapterConstraintError::CapacityExceeded {
            adapter: BACKEND.into(),
            operation: "assembly".into(),
            subject: artifact,
            resource: "reaction_volume".into(),
            required: u64::from(required_ul),
            capacity: u64::from(reaction_volume_ul),
            unit: "uL".into(),
        }
        .into());
    }
    Ok(())
}

pub(super) fn validate_strain_constraints(
    trace: &StrainTrace,
    context: &pliron::context::Context,
) -> Result<(), FlexPlanningError> {
    let artifact = trace.artifact(context);
    for (parameter, value, maximum) in [
        (
            "transformation_replicates",
            trace.transformation_replicates(context),
            u8::MAX,
        ),
        ("plating_replicates", trace.plating_replicates(context), 8),
        ("serial_dilutions", trace.serial_dilutions(context), 2),
    ] {
        require_range(&artifact, parameter, value, maximum)?;
    }
    Ok(())
}

fn require_range(
    artifact: &str,
    parameter: &'static str,
    value: u8,
    maximum: u8,
) -> Result<(), FlexPlanningError> {
    if !(1..=maximum).contains(&value) {
        return Err(AdapterConstraintError::ParameterOutOfRange {
            adapter: BACKEND.into(),
            subject: artifact.to_owned(),
            parameter: parameter.into(),
            minimum: 1,
            maximum: u64::from(maximum),
            found: u64::from(value),
        }
        .into());
    }
    Ok(())
}

pub(super) fn validate_uniform_batch_settings(
    traces: &[StrainTrace],
    context: &pliron::context::Context,
) -> Result<(), FlexPlanningError> {
    let Some(first) = traces.first() else {
        return Ok(());
    };
    let expected = (
        first.transformation_replicates(context),
        first.plating_replicates(context),
        first.serial_dilutions(context),
    );
    if traces.iter().skip(1).any(|trace| {
        (
            trace.transformation_replicates(context),
            trace.plating_replicates(context),
            trace.serial_dilutions(context),
        ) != expected
    }) {
        Err(AdapterConstraintError::NonUniformParameters {
            adapter: BACKEND.into(),
            subject: "automation_batch".into(),
            parameters: vec![
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

pub(super) fn plate_capacity_error(
    stage: &'static str,
    resource: &'static str,
    required: usize,
    capacity: usize,
) -> FlexPlanningError {
    AdapterConstraintError::CapacityExceeded {
        adapter: BACKEND.into(),
        operation: stage.into(),
        subject: "automation_batch".into(),
        resource: resource.into(),
        required: required as u64,
        capacity: capacity as u64,
        unit: "wells".into(),
    }
    .into()
}

pub(super) fn require_tip_capacity(
    stage: &'static str,
    pipette: &'static str,
    required: usize,
    capacity: usize,
) -> Result<(), FlexPlanningError> {
    if required > capacity {
        Err(AdapterConstraintError::CapacityExceeded {
            adapter: BACKEND.into(),
            operation: stage.into(),
            subject: "automation_batch".into(),
            resource: format!("{pipette}_tip_rack"),
            required: required as u64,
            capacity: capacity as u64,
            unit: "tips".into(),
        }
        .into())
    } else {
        Ok(())
    }
}
