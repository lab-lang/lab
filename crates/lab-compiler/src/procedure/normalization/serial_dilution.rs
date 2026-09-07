use crate::procedure::Volume;
use crate::procedure::context::procedure_id;
use crate::procedure::pipetting::{
    MixSettings, SerialDilutionSettings, TransferSettings, serial_dilution,
};
use crate::procedure::{ProcedureProgram, ProcedureProgramBuildContext};

const MICROLITRE: &str = "http://qudt.org/vocab/unit/MicroL";

pub(super) fn normalize(
    task: &ProcedureProgramBuildContext<'_>,
) -> Result<ProcedureProgram, String> {
    task.require_io(1, 1)?;
    task.require_material_roles(&["medium"])?;
    let medium = task.one_material("medium")?;
    let program = serial_dilution(
        0,
        procedure_id(medium.id.as_str())?,
        task.output(0)?,
        SerialDilutionSettings {
            replicates: task.integer_parameter("replicates", None)?,
            stages: task.integer_parameter("serial_dilutions", None)?,
            input_volume_each: volume(recovered_culture_volume(task)?)?,
            diluent_volume: task.volume_parameter("medium_volume_ul")?,
            diluent_load: Some(task.volume_parameter("medium_source_volume_ul")?),
            retained_diluent: Some(task.volume_parameter("medium_dead_volume_ul")?),
            transfer: TransferSettings {
                volume: task.volume_parameter("culture_volume_ul")?,
                technique: Default::default(),
            },
            mixing: MixSettings {
                cycles: task.integer_parameter("mix_cycles", None)?,
                volume: task.volume_parameter("mix_volume_ul")?,
                technique: Default::default(),
            },
        },
    )?;
    Ok(ProcedureProgram::from_pipetting(&program))
}

fn volume(microlitres: u32) -> Result<Volume, String> {
    Volume::parse_microlitres(microlitres.to_string()).map_err(|error| error.to_string())
}

/// The dilution input is the transformed volume plus the recovery medium added by the preceding
/// Method. This domain calculation belongs to the Procedure builder rather than generic source
/// lowering.
fn recovered_culture_volume(view: &ProcedureProgramBuildContext<'_>) -> Result<u32, String> {
    let cells = view.integer_parameter("cell_volume_ul", Some(MICROLITRE))?;
    let dna_each = view.integer_parameter("dna_volume_ul", Some(MICROLITRE))?;
    let dna_count = view.integer_parameter("dna_count", None)?;
    let recovery = view.integer_parameter("recovery_volume_ul", Some(MICROLITRE))?;
    dna_each
        .checked_mul(dna_count)
        .and_then(|dna| cells.checked_add(dna))
        .and_then(|transformed| transformed.checked_add(recovery))
        .ok_or_else(|| "recovered culture volume arithmetic overflows".to_owned())
}

pub(super) fn registrations() -> Vec<crate::procedure::ProcedureProgramBuilderRegistration> {
    use crate::procedure::vocabulary::*;
    vec![crate::procedure::builder::registration(
        SERIAL_DILUTION_BUILDER_V1,
        PIPETTING_PROGRAM_V1,
        normalize,
    )]
}
