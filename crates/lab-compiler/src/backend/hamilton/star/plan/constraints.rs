//! STAR-specific parameter and batch-capacity validation. The chemistry
//! rules match the Flex backend's — the science is the same — plus the
//! vessel-volume checks a deck without modules needs: everything a well
//! accumulates must fit the labware planning placed it in.

use crate::backend::AdapterConstraintError;
use crate::backend::trace::{AssemblyTrace, StrainTrace};

use crate::backend::hamilton::star::BACKEND;
use crate::backend::hamilton::star::plan::error::StarPlanningError;

pub(super) fn validate_assembly_constraints(
    trace: &AssemblyTrace,
    context: &pliron::context::Context,
    reaction_well_capacity_ul: f64,
) -> Result<(), StarPlanningError> {
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
    if f64::from(reaction_volume_ul) > reaction_well_capacity_ul {
        return Err(AdapterConstraintError::CapacityExceeded {
            adapter: BACKEND.into(),
            operation: "assembly".into(),
            subject: artifact,
            resource: "reaction_plate_well".into(),
            required: u64::from(reaction_volume_ul),
            capacity: reaction_well_capacity_ul as u64,
            unit: "uL".into(),
        }
        .into());
    }
    Ok(())
}

pub(super) fn validate_strain_constraints(
    trace: &StrainTrace,
    context: &pliron::context::Context,
    reaction_well_capacity_ul: f64,
) -> Result<(), StarPlanningError> {
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
    // A culture well accumulates cells, one DNA volume per carried
    // plasmid, and the recovery medium — with no thermocycler lid to
    // squeeze under, the well itself is the ceiling.
    let plasmids = trace.plasmids(context).len() as u16;
    let culture_ul = trace.chemistry(context, "cell_volume_ul")
        + trace.chemistry(context, "dna_volume_ul") * plasmids
        + trace.chemistry(context, "recovery_volume_ul");
    if f64::from(culture_ul) > reaction_well_capacity_ul {
        return Err(AdapterConstraintError::CapacityExceeded {
            adapter: BACKEND.into(),
            operation: "transformation".into(),
            subject: artifact,
            resource: "reaction_plate_well".into(),
            required: u64::from(culture_ul),
            capacity: reaction_well_capacity_ul as u64,
            unit: "uL".into(),
        }
        .into());
    }
    Ok(())
}

fn require_range(
    artifact: &str,
    parameter: &'static str,
    value: u8,
    maximum: u8,
) -> Result<(), StarPlanningError> {
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
) -> Result<(), StarPlanningError> {
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
) -> StarPlanningError {
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
