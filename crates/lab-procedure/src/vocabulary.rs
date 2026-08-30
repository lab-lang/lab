//! Stable identities used by built-in Procedure contracts and their derived facility demands.

pub const PIPETTING_PROGRAM_V1: &str =
    "https://www.lab-compiler.org/ns/procedure-contract#PipettingProgramV1";

pub const METERED_LIQUID_TRANSFER: &str = "https://sbol.io/ns/capability#MeteredLiquidTransfer";
pub const IN_WELL_MIXING: &str = "https://sbol.io/ns/capability#InWellMixing";
pub const TEMPERATURE_CONTROLLED_STAGING: &str =
    "https://sbol.io/ns/capability#TemperatureControlledStaging";

pub const MINIMUM_TRANSFER_VOLUME: &str = "https://sbol.io/ns/capability#MinimumTransferVolume";
pub const MAXIMUM_TRANSFER_VOLUME: &str = "https://sbol.io/ns/capability#MaximumTransferVolume";
pub const MAXIMUM_MIX_VOLUME: &str = "https://sbol.io/ns/capability#MaximumMixVolume";
pub const MINIMUM_TEMPERATURE: &str = "https://sbol.io/ns/capability#MinimumTemperature";
pub const MAXIMUM_TEMPERATURE: &str = "https://sbol.io/ns/capability#MaximumTemperature";

pub const MICROLITRE: &str = "http://qudt.org/vocab/unit/MicroL";
pub const DEGREE_CELSIUS: &str = "http://qudt.org/vocab/unit/DEG_C";
