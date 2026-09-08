//! Serial dilution of an explicitly valued liquid input, independent of its preparation.

use super::{
    AspirationStrategy, FluidPathPolicy, MixSettings, PipettingBuilder, TransferSettings,
    TransferTechnique, ValidatedPipettingProgramV1,
};
use crate::procedure::{ProcedureLocalId, Volume};

pub struct SerialDilutionSettings {
    pub replicates: u32,
    pub stages: u32,
    pub input_volume_each: Volume,
    pub diluent_volume: Volume,
    pub diluent_load: Option<Volume>,
    pub retained_diluent: Option<Volume>,
    pub transfer: TransferSettings,
    pub mixing: MixSettings,
}

/// Prefill every destination, then transfer and mix in stage order within each replicate.
/// Each replicate uses an independent continuous fluid path. The caller supplies the
/// actual starting volume; this algorithm makes no assumptions about earlier recipes.
pub fn serial_dilution(
    input: u32,
    diluent: ProcedureLocalId,
    output: ProcedureLocalId,
    settings: SerialDilutionSettings,
) -> Result<ValidatedPipettingProgramV1, String> {
    let positions = settings
        .replicates
        .checked_mul(settings.stages)
        .ok_or("serial-dilution position count overflows")?;
    let mut program = PipettingBuilder::new();
    let samples = program.input(
        "culture-input",
        input,
        settings.replicates,
        settings.input_volume_each,
    )?;
    let medium = program.source(
        "medium-source",
        diluent,
        settings.diluent_load,
        settings.retained_diluent,
    )?;
    let dilutions = program.product("dilution-plate", output, positions)?;
    program.distribute(
        "add-medium",
        medium.position(0)?,
        dilutions.positions(),
        &TransferSettings {
            volume: settings.diluent_volume,
            technique: TransferTechnique {
                aspiration: AspirationStrategy::TrackedLiquidSurface,
                ..Default::default()
            },
        },
        FluidPathPolicy::SharedSourceNoReentry,
    )?;
    for replicate in 0..settings.replicates {
        let mut path = program.continuous_path(
            &format!("series-{replicate:04}"),
            FluidPathPolicy::IsolatedDestinations,
        )?;
        let mut source = samples.position(replicate)?;
        for stage in 0..settings.stages {
            // The checked product above guarantees this position arithmetic fits in u32.
            let destination = dilutions.position(stage * settings.replicates + replicate)?;
            path.transfer(
                &format!("dilute-{replicate:04}-{stage:04}"),
                source,
                destination.clone(),
                &settings.transfer,
            )?;
            path.mix(
                &format!("mix-{replicate:04}-{stage:04}"),
                destination.clone(),
                &settings.mixing,
            )?;
            source = destination;
        }
    }
    program.finish()
}
