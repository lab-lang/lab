//! Stable identities used by built-in Procedure contracts and their derived facility demands.

pub const PIPETTING_PROGRAM_V1: &str =
    "https://www.lab-compiler.org/ns/procedure-contract#PipettingProgramV1";
pub const THERMAL_PROGRAM_V1: &str =
    "https://www.lab-compiler.org/ns/procedure-contract#ThermalProgramV1";

pub const METERED_LIQUID_TRANSFER: &str = "https://sbol.io/ns/capability#MeteredLiquidTransfer";
pub const IN_WELL_MIXING: &str = "https://sbol.io/ns/capability#InWellMixing";
pub const TEMPERATURE_CONTROLLED_STAGING: &str =
    "https://sbol.io/ns/capability#TemperatureControlledStaging";
pub const PROGRAMMED_BLOCK_TEMPERATURE_CONTROL: &str =
    "https://sbol.io/ns/capability#ProgrammedBlockTemperatureControl";
pub const HEATED_LID_TEMPERATURE_CONTROL: &str =
    "https://sbol.io/ns/capability#HeatedLidTemperatureControl";
pub const CONTROLLED_TEMPERATURE_RAMP: &str =
    "https://sbol.io/ns/capability#ControlledTemperatureRamp";
pub const LIQUID_LEVEL_AWARE_ASPIRATION: &str =
    "https://sbol.io/ns/capability#LiquidLevelAwareAspiration";
pub const VESSEL_RELATIVE_LIQUID_ACCESS: &str =
    "https://sbol.io/ns/capability#VesselRelativeLiquidAccess";
pub const AIR_GAP_HANDLING: &str = "https://sbol.io/ns/capability#AirGapHandling";
pub const POST_DISPENSE_BLOWOUT: &str = "https://sbol.io/ns/capability#PostDispenseBlowout";
pub const TOUCH_TIP: &str = "https://sbol.io/ns/capability#TouchTip";

pub const MINIMUM_TRANSFER_VOLUME: &str = "https://sbol.io/ns/capability#MinimumTransferVolume";
pub const MAXIMUM_TRANSFER_VOLUME: &str = "https://sbol.io/ns/capability#MaximumTransferVolume";
pub const MAXIMUM_MIX_VOLUME: &str = "https://sbol.io/ns/capability#MaximumMixVolume";
pub const MINIMUM_TEMPERATURE: &str = "https://sbol.io/ns/capability#MinimumTemperature";
pub const MAXIMUM_TEMPERATURE: &str = "https://sbol.io/ns/capability#MaximumTemperature";
pub const MINIMUM_BLOCK_TEMPERATURE: &str = "https://sbol.io/ns/capability#MinimumBlockTemperature";
pub const MAXIMUM_BLOCK_TEMPERATURE: &str = "https://sbol.io/ns/capability#MaximumBlockTemperature";
pub const MINIMUM_LID_TEMPERATURE: &str = "https://sbol.io/ns/capability#MinimumLidTemperature";
pub const MAXIMUM_LID_TEMPERATURE: &str = "https://sbol.io/ns/capability#MaximumLidTemperature";
pub const MAXIMUM_SAMPLE_COUNT: &str = "https://sbol.io/ns/capability#MaximumSampleCount";
pub const MINIMUM_THERMAL_SAMPLE_VOLUME: &str =
    "https://sbol.io/ns/capability#MinimumThermalSampleVolume";
pub const MAXIMUM_THERMAL_SAMPLE_VOLUME: &str =
    "https://sbol.io/ns/capability#MaximumThermalSampleVolume";
pub const MAXIMUM_RAMP_RATE: &str = "https://sbol.io/ns/capability#MaximumRampRate";
pub const MAXIMUM_AIR_GAP_VOLUME: &str = "https://sbol.io/ns/capability#MaximumAirGapVolume";

pub const MICROLITRE: &str = "http://qudt.org/vocab/unit/MicroL";
pub const DEGREE_CELSIUS: &str = "http://qudt.org/vocab/unit/DEG_C";
pub const SECOND: &str = "http://qudt.org/vocab/unit/SEC";
pub const DEGREE_CELSIUS_PER_SECOND: &str = "http://qudt.org/vocab/unit/DEG_C-PER-SEC";
pub const MILLIMETRE: &str = "http://qudt.org/vocab/unit/MilliM";
