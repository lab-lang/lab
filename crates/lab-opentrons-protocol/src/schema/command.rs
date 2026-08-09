//! Command envelopes and parameters (command schema v8).
//!
//! Every command serializes to `{"commandType": ..., "params": {...}, "key"?}`.
//! The `intent` field exists for HTTP-enqueued commands and protocol files
//! omit it, so this model does too.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::schema::{
    Coordinates, DropTipWellLocation, LabwareLocation, LabwareMovementStrategy, ModuleModel,
    MotorAxis, Mount, MovementAxis, NozzleLayoutConfiguration, PipetteName, StatusBarAnimation,
    TipPresenceState, WellLocation, WellOffset,
};

/// One command in a protocol's `commands` array.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Command {
    #[serde(flatten)]
    pub action: CommandAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

impl From<CommandAction> for Command {
    fn from(action: CommandAction) -> Self {
        Self { action, key: None }
    }
}

/// Every command type in command schema v8, tagged by `commandType` with its
/// parameters under `params`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "commandType", content = "params")]
pub enum CommandAction {
    // Loading.
    #[serde(rename = "loadPipette")]
    LoadPipette(LoadPipetteParams),
    #[serde(rename = "loadLabware")]
    LoadLabware(LoadLabwareParams),
    #[serde(rename = "loadModule")]
    LoadModule(LoadModuleParams),
    #[serde(rename = "loadLiquid")]
    LoadLiquid(LoadLiquidParams),
    #[serde(rename = "reloadLabware")]
    ReloadLabware(ReloadLabwareParams),

    // Pipetting.
    #[serde(rename = "aspirate")]
    Aspirate(AspirateParams),
    #[serde(rename = "aspirateInPlace")]
    AspirateInPlace(AspirateInPlaceParams),
    #[serde(rename = "dispense")]
    Dispense(DispenseParams),
    #[serde(rename = "dispenseInPlace")]
    DispenseInPlace(DispenseInPlaceParams),
    #[serde(rename = "blowout")]
    Blowout(BlowoutParams),
    #[serde(rename = "blowOutInPlace")]
    BlowOutInPlace(BlowOutInPlaceParams),
    #[serde(rename = "pickUpTip")]
    PickUpTip(PickUpTipParams),
    #[serde(rename = "dropTip")]
    DropTip(DropTipParams),
    #[serde(rename = "dropTipInPlace")]
    DropTipInPlace(DropTipInPlaceParams),
    #[serde(rename = "touchTip")]
    TouchTip(TouchTipParams),
    #[serde(rename = "prepareToAspirate")]
    PrepareToAspirate(PrepareToAspirateParams),
    #[serde(rename = "configureForVolume")]
    ConfigureForVolume(ConfigureForVolumeParams),
    #[serde(rename = "configureNozzleLayout")]
    ConfigureNozzleLayout(ConfigureNozzleLayoutParams),
    #[serde(rename = "liquidProbe")]
    LiquidProbe(LiquidProbeParams),
    #[serde(rename = "tryLiquidProbe")]
    TryLiquidProbe(LiquidProbeParams),
    #[serde(rename = "getTipPresence")]
    GetTipPresence(GetTipPresenceParams),
    #[serde(rename = "verifyTipPresence")]
    VerifyTipPresence(VerifyTipPresenceParams),

    // Movement.
    #[serde(rename = "moveToWell")]
    MoveToWell(MoveToWellParams),
    #[serde(rename = "moveToCoordinates")]
    MoveToCoordinates(MoveToCoordinatesParams),
    #[serde(rename = "moveToAddressableArea")]
    MoveToAddressableArea(MoveToAddressableAreaParams),
    #[serde(rename = "moveToAddressableAreaForDropTip")]
    MoveToAddressableAreaForDropTip(MoveToAddressableAreaForDropTipParams),
    #[serde(rename = "moveRelative")]
    MoveRelative(MoveRelativeParams),
    #[serde(rename = "moveLabware")]
    MoveLabware(MoveLabwareParams),
    #[serde(rename = "home")]
    Home(HomeParams),
    #[serde(rename = "retractAxis")]
    RetractAxis(RetractAxisParams),
    #[serde(rename = "savePosition")]
    SavePosition(SavePositionParams),

    // Timing and miscellany.
    #[serde(rename = "waitForDuration")]
    WaitForDuration(WaitForDurationParams),
    #[serde(rename = "waitForResume")]
    WaitForResume(WaitForResumeParams),
    #[serde(rename = "comment")]
    Comment(CommentParams),
    #[serde(rename = "custom")]
    Custom(serde_json::Value),
    #[serde(rename = "setRailLights")]
    SetRailLights(SetRailLightsParams),
    #[serde(rename = "setStatusBar")]
    SetStatusBar(SetStatusBarParams),

    // Temperature module.
    #[serde(rename = "temperatureModule/setTargetTemperature")]
    TemperatureModuleSetTargetTemperature(TemperatureModuleSetTargetTemperatureParams),
    #[serde(rename = "temperatureModule/waitForTemperature")]
    TemperatureModuleWaitForTemperature(TemperatureModuleWaitForTemperatureParams),
    #[serde(rename = "temperatureModule/deactivate")]
    TemperatureModuleDeactivate(ModuleOnlyParams),

    // Thermocycler.
    #[serde(rename = "thermocycler/openLid")]
    ThermocyclerOpenLid(ModuleOnlyParams),
    #[serde(rename = "thermocycler/closeLid")]
    ThermocyclerCloseLid(ModuleOnlyParams),
    #[serde(rename = "thermocycler/deactivateBlock")]
    ThermocyclerDeactivateBlock(ModuleOnlyParams),
    #[serde(rename = "thermocycler/deactivateLid")]
    ThermocyclerDeactivateLid(ModuleOnlyParams),
    #[serde(rename = "thermocycler/setTargetBlockTemperature")]
    ThermocyclerSetTargetBlockTemperature(ThermocyclerSetBlockTemperatureParams),
    #[serde(rename = "thermocycler/waitForBlockTemperature")]
    ThermocyclerWaitForBlockTemperature(ModuleOnlyParams),
    #[serde(rename = "thermocycler/setTargetLidTemperature")]
    ThermocyclerSetTargetLidTemperature(ThermocyclerSetLidTemperatureParams),
    #[serde(rename = "thermocycler/waitForLidTemperature")]
    ThermocyclerWaitForLidTemperature(ModuleOnlyParams),
    #[serde(rename = "thermocycler/runProfile")]
    ThermocyclerRunProfile(ThermocyclerRunProfileParams),

    // Heater-shaker.
    #[serde(rename = "heaterShaker/setTargetTemperature")]
    HeaterShakerSetTargetTemperature(HeaterShakerSetTargetTemperatureParams),
    #[serde(rename = "heaterShaker/waitForTemperature")]
    HeaterShakerWaitForTemperature(HeaterShakerWaitForTemperatureParams),
    #[serde(rename = "heaterShaker/setAndWaitForShakeSpeed")]
    HeaterShakerSetAndWaitForShakeSpeed(HeaterShakerSetShakeSpeedParams),
    #[serde(rename = "heaterShaker/openLabwareLatch")]
    HeaterShakerOpenLabwareLatch(ModuleOnlyParams),
    #[serde(rename = "heaterShaker/closeLabwareLatch")]
    HeaterShakerCloseLabwareLatch(ModuleOnlyParams),
    #[serde(rename = "heaterShaker/deactivateHeater")]
    HeaterShakerDeactivateHeater(ModuleOnlyParams),
    #[serde(rename = "heaterShaker/deactivateShaker")]
    HeaterShakerDeactivateShaker(ModuleOnlyParams),

    // Magnetic module (OT-2 only; the Flex magnetic block is passive).
    #[serde(rename = "magneticModule/engage")]
    MagneticModuleEngage(MagneticModuleEngageParams),
    #[serde(rename = "magneticModule/disengage")]
    MagneticModuleDisengage(ModuleOnlyParams),

    // Absorbance reader.
    #[serde(rename = "absorbanceReader/openLid")]
    AbsorbanceReaderOpenLid(ModuleOnlyParams),
    #[serde(rename = "absorbanceReader/closeLid")]
    AbsorbanceReaderCloseLid(ModuleOnlyParams),
    #[serde(rename = "absorbanceReader/initialize")]
    AbsorbanceReaderInitialize(AbsorbanceReaderMeasureParams),
    #[serde(rename = "absorbanceReader/read")]
    AbsorbanceReaderRead(AbsorbanceReaderMeasureParams),

    // Calibration. These belong to maintenance flows rather than authored
    // protocols, so they carry free-form parameters here and the builder does
    // not construct them.
    #[serde(rename = "calibration/calibrateGripper")]
    CalibrateGripper(serde_json::Value),
    #[serde(rename = "calibration/calibratePipette")]
    CalibratePipette(serde_json::Value),
    #[serde(rename = "calibration/calibrateModule")]
    CalibrateModule(serde_json::Value),
    #[serde(rename = "calibration/moveToMaintenancePosition")]
    MoveToMaintenancePosition(serde_json::Value),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadPipetteParams {
    pub pipette_name: PipetteName,
    pub mount: Mount,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipette_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tip_overlap_not_after_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub liquid_presence_detection: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadLabwareParams {
    pub location: LabwareLocation,
    pub load_name: String,
    pub namespace: String,
    pub version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labware_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadModuleParams {
    pub model: ModuleModel,
    /// The module's deck slot. A thermocycler names its front-most slot:
    /// `B1` on a Flex, `7` on an OT-2.
    pub location: crate::schema::DeckSlotLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadLiquidParams {
    pub liquid_id: String,
    pub labware_id: String,
    pub volume_by_well: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReloadLabwareParams {
    pub labware_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AspirateParams {
    pub pipette_id: String,
    pub labware_id: String,
    pub well_name: String,
    /// µL.
    pub volume: f64,
    /// µL/s; must be greater than zero.
    pub flow_rate: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub well_location: Option<WellLocation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AspirateInPlaceParams {
    pub pipette_id: String,
    pub volume: f64,
    pub flow_rate: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DispenseParams {
    pub pipette_id: String,
    pub labware_id: String,
    pub well_name: String,
    pub volume: f64,
    pub flow_rate: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub well_location: Option<WellLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_out: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DispenseInPlaceParams {
    pub pipette_id: String,
    pub volume: f64,
    pub flow_rate: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_out: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlowoutParams {
    pub pipette_id: String,
    pub labware_id: String,
    pub well_name: String,
    pub flow_rate: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub well_location: Option<WellLocation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlowOutInPlaceParams {
    pub pipette_id: String,
    pub flow_rate: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickUpTipParams {
    pub pipette_id: String,
    pub labware_id: String,
    pub well_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub well_location: Option<WellLocation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DropTipParams {
    pub pipette_id: String,
    pub labware_id: String,
    pub well_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub well_location: Option<DropTipWellLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home_after: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternate_drop_location: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DropTipInPlaceParams {
    pub pipette_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home_after: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TouchTipParams {
    pub pipette_id: String,
    pub labware_id: String,
    pub well_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub well_location: Option<WellLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius: Option<f64>,
    /// mm/s.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareToAspirateParams {
    pub pipette_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureForVolumeParams {
    pub pipette_id: String,
    pub volume: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tip_overlap_not_after_version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureNozzleLayoutParams {
    pub pipette_id: String,
    pub configuration_params: NozzleLayoutConfiguration,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiquidProbeParams {
    pub pipette_id: String,
    pub labware_id: String,
    pub well_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub well_location: Option<WellLocation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTipPresenceParams {
    pub pipette_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyTipPresenceParams {
    pub pipette_id: String,
    pub expected_state: TipPresenceState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveToWellParams {
    pub pipette_id: String,
    pub labware_id: String,
    pub well_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub well_location: Option<WellLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_z_height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_direct: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveToCoordinatesParams {
    pub pipette_id: String,
    pub coordinates: Coordinates,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_z_height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_direct: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveToAddressableAreaParams {
    pub pipette_id: String,
    pub addressable_area_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<WellOffset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_z_height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_direct: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stay_at_highest_possible_z: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveToAddressableAreaForDropTipParams {
    pub pipette_id: String,
    pub addressable_area_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<WellOffset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternate_drop_location: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_tip_configuration: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_z_height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_direct: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveRelativeParams {
    pub pipette_id: String,
    pub axis: MovementAxis,
    /// mm.
    pub distance: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveLabwareParams {
    pub labware_id: String,
    pub new_location: LabwareLocation,
    pub strategy: LabwareMovementStrategy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pick_up_offset: Option<WellOffset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drop_offset: Option<WellOffset>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeParams {
    /// Omitted means every axis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axes: Option<Vec<MotorAxis>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_if_mount_position_ok: Option<Mount>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetractAxisParams {
    pub axis: MotorAxis,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePositionParams {
    pub pipette_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fail_on_not_homed: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitForDurationParams {
    pub seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitForResumeParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentParams {
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetRailLightsParams {
    pub on: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetStatusBarParams {
    pub animation: StatusBarAnimation,
}

/// Parameters shared by every module command that names only its module.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleOnlyParams {
    pub module_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemperatureModuleSetTargetTemperatureParams {
    pub module_id: String,
    pub celsius: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemperatureModuleWaitForTemperatureParams {
    pub module_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub celsius: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermocyclerSetBlockTemperatureParams {
    pub module_id: String,
    pub celsius: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_max_volume_ul: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold_time_seconds: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermocyclerSetLidTemperatureParams {
    pub module_id: String,
    pub celsius: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermocyclerRunProfileParams {
    pub module_id: String,
    pub profile: Vec<RunProfileStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_max_volume_ul: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunProfileStep {
    pub celsius: f64,
    pub hold_seconds: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaterShakerSetTargetTemperatureParams {
    pub module_id: String,
    pub celsius: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaterShakerWaitForTemperatureParams {
    pub module_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub celsius: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaterShakerSetShakeSpeedParams {
    pub module_id: String,
    pub rpm: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MagneticModuleEngageParams {
    pub module_id: String,
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AbsorbanceReaderMeasureParams {
    pub module_id: String,
    /// nm.
    pub sample_wavelength: u32,
}

#[cfg(test)]
mod tests {
    use crate::schema::command::*;

    #[test]
    fn a_command_serializes_to_the_tagged_envelope() {
        let command: Command = CommandAction::Aspirate(AspirateParams {
            pipette_id: "pipette-0".into(),
            labware_id: "labware-1".into(),
            well_name: "A1".into(),
            volume: 20.0,
            flow_rate: 35.0,
            well_location: None,
        })
        .into();
        let json = serde_json::to_value(&command).unwrap();
        assert_eq!(json["commandType"], "aspirate");
        assert_eq!(json["params"]["pipetteId"], "pipette-0");
        assert_eq!(json["params"]["wellName"], "A1");
        assert!(
            json.get("key").is_none(),
            "an unset key is omitted, not null"
        );
    }

    #[test]
    fn slash_named_module_commands_keep_their_wire_spelling() {
        let command: Command = CommandAction::ThermocyclerOpenLid(ModuleOnlyParams {
            module_id: "module-0".into(),
        })
        .into();
        let json = serde_json::to_value(&command).unwrap();
        assert_eq!(json["commandType"], "thermocycler/openLid");
    }

    #[test]
    fn labware_locations_cover_every_wire_shape() {
        use crate::schema::LabwareLocation;
        assert_eq!(
            serde_json::to_value(LabwareLocation::slot("B1")).unwrap(),
            serde_json::json!({"slotName": "B1"})
        );
        assert_eq!(
            serde_json::to_value(LabwareLocation::module("module-0")).unwrap(),
            serde_json::json!({"moduleId": "module-0"})
        );
        assert_eq!(
            serde_json::to_value(LabwareLocation::off_deck()).unwrap(),
            serde_json::json!("offDeck")
        );
        let round_trip: LabwareLocation =
            serde_json::from_value(serde_json::json!("offDeck")).unwrap();
        assert_eq!(round_trip, LabwareLocation::off_deck());
    }
}
