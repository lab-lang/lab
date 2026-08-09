//! The faithful wire model of an Opentrons JSON protocol (protocol schema v8).
//!
//! Everything here serializes to exactly the shape the protocol schema
//! accepts; nothing here validates semantics. The checked authoring API in
//! [`crate::v8::builder`] produces these types, and hand-authoring them directly
//! is the escape hatch for anything the builder does not model.

mod command;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use crate::v8::schema::command::{
    AbsorbanceReaderMeasureParams, AspirateInPlaceParams, AspirateParams, BlowOutInPlaceParams,
    BlowoutParams, Command, CommandAction, CommentParams, ConfigureForVolumeParams,
    ConfigureNozzleLayoutParams, DispenseInPlaceParams, DispenseParams, DropTipInPlaceParams,
    DropTipParams, GetTipPresenceParams, HeaterShakerSetShakeSpeedParams,
    HeaterShakerSetTargetTemperatureParams, HeaterShakerWaitForTemperatureParams, HomeParams,
    LiquidProbeParams, LoadLabwareParams, LoadLiquidParams, LoadModuleParams, LoadPipetteParams,
    MagneticModuleEngageParams, ModuleOnlyParams, MoveLabwareParams, MoveRelativeParams,
    MoveToAddressableAreaForDropTipParams, MoveToAddressableAreaParams, MoveToCoordinatesParams,
    MoveToWellParams, PickUpTipParams, PrepareToAspirateParams, ReloadLabwareParams,
    RetractAxisParams, RunProfileStep, SavePositionParams, SetRailLightsParams, SetStatusBarParams,
    TemperatureModuleSetTargetTemperatureParams, TemperatureModuleWaitForTemperatureParams,
    ThermocyclerRunProfileParams, ThermocyclerSetBlockTemperatureParams,
    ThermocyclerSetLidTemperatureParams, TouchTipParams, VerifyTipPresenceParams,
    WaitForDurationParams, WaitForResumeParams,
};

/// The complete protocol document. The protocol schema declares
/// `additionalProperties: false` at the top level, so this struct serializes
/// nothing beyond the fields the schema names and rejects anything extra on
/// the way back in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProtocolDocument {
    #[serde(rename = "$otSharedSchema")]
    pub ot_shared_schema: String,
    pub schema_version: u8,
    pub metadata: Metadata,
    pub robot: Robot,
    pub labware_definition_schema_id: String,
    /// Full labware-schema-2 definitions keyed `{namespace}/{loadName}/{version}`,
    /// embedded verbatim so every `loadLabware` resolves here.
    pub labware_definitions: BTreeMap<String, serde_json::Value>,
    pub command_schema_id: String,
    pub commands: Vec<Command>,
    pub command_annotation_schema_id: String,
    pub command_annotations: Vec<serde_json::Value>,
    pub liquid_schema_id: String,
    pub liquids: BTreeMap<String, Liquid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub designer_application: Option<DesignerApplication>,
}

impl ProtocolDocument {
    /// Serialize with a trailing newline, the form written to protocol files.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self).map(|mut text| {
            text.push('\n');
            text
        })
    }

    /// The `$otSharedSchema` value every v8 protocol declares.
    pub const OT_SHARED_SCHEMA: &'static str = "#/protocol/schemas/8";
    pub const SCHEMA_VERSION: u8 = 8;
    pub const LABWARE_DEFINITION_SCHEMA_ID: &'static str = "opentronsLabwareSchemaV2";
    pub const COMMAND_SCHEMA_ID: &'static str = "opentronsCommandSchemaV8";
    pub const COMMAND_ANNOTATION_SCHEMA_ID: &'static str = "opentronsCommandAnnotationSchemaV1";
    pub const LIQUID_SCHEMA_ID: &'static str = "opentronsLiquidSchemaV1";
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// UNIX timestamp in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subcategory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Robot {
    pub model: RobotModel,
    pub deck_id: String,
}

impl Robot {
    pub fn flex() -> Self {
        Self {
            model: RobotModel::Flex,
            deck_id: "ot3_standard".into(),
        }
    }

    pub fn ot2() -> Self {
        Self {
            model: RobotModel::Ot2,
            deck_id: "ot2_standard".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RobotModel {
    #[serde(rename = "OT-2 Standard")]
    Ot2,
    #[serde(rename = "OT-3 Standard")]
    Flex,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Liquid {
    pub display_name: String,
    pub description: String,
    /// `#rrggbb` or `#rrggbbaa`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_color: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignerApplication {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Instrument mount. `extension` is the Flex gripper.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mount {
    Left,
    Right,
    Extension,
}

/// Every pipette name the command schema accepts, across both robots. The
/// builder narrows this to the vocabulary of the robot it authors for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipetteName {
    #[serde(rename = "p50_single_flex")]
    P50SingleFlex,
    #[serde(rename = "p50_multi_flex")]
    P50MultiFlex,
    #[serde(rename = "p1000_single_flex")]
    P1000SingleFlex,
    #[serde(rename = "p1000_multi_flex")]
    P1000MultiFlex,
    #[serde(rename = "p1000_96")]
    P1000Channel96,
    #[serde(rename = "p20_single_gen2")]
    P20SingleGen2,
    #[serde(rename = "p20_multi_gen2")]
    P20MultiGen2,
    #[serde(rename = "p300_single_gen2")]
    P300SingleGen2,
    #[serde(rename = "p300_multi_gen2")]
    P300MultiGen2,
    #[serde(rename = "p1000_single_gen2")]
    P1000SingleGen2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModuleModel {
    #[serde(rename = "temperatureModuleV1")]
    TemperatureModuleV1,
    #[serde(rename = "temperatureModuleV2")]
    TemperatureModuleV2,
    #[serde(rename = "magneticModuleV1")]
    MagneticModuleV1,
    #[serde(rename = "magneticModuleV2")]
    MagneticModuleV2,
    #[serde(rename = "thermocyclerModuleV1")]
    ThermocyclerModuleV1,
    #[serde(rename = "thermocyclerModuleV2")]
    ThermocyclerModuleV2,
    #[serde(rename = "heaterShakerModuleV1")]
    HeaterShakerModuleV1,
    #[serde(rename = "magneticBlockV1")]
    MagneticBlockV1,
    #[serde(rename = "absorbanceReaderV1")]
    AbsorbanceReaderV1,
}

/// Position within a well that an offset is measured from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WellOrigin {
    Top,
    Bottom,
    Center,
}

/// Millimetre offset from a well origin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WellOffset {
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default)]
    pub z: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WellLocation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<WellOrigin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<WellOffset>,
}

impl WellLocation {
    pub fn origin(origin: WellOrigin) -> Self {
        Self {
            origin: Some(origin),
            offset: None,
        }
    }

    pub fn with_offset(origin: WellOrigin, x: f64, y: f64, z: f64) -> Self {
        Self {
            origin: Some(origin),
            offset: Some(WellOffset { x, y, z }),
        }
    }
}

/// `dropTip` additionally accepts a `default` origin, which is its default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DropTipWellOrigin {
    Top,
    Bottom,
    Center,
    Default,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DropTipWellLocation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<DropTipWellOrigin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<WellOffset>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Coordinates {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Where labware loads or moves to: a deck slot, a module, another labware,
/// an addressable area, or off the deck entirely.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LabwareLocation {
    Slot(DeckSlotLocation),
    Module(ModuleLocation),
    InLabware(OnLabwareLocation),
    AddressableArea(AddressableAreaLocation),
    OffDeck(OffDeckMarker),
}

impl LabwareLocation {
    pub fn slot(slot_name: impl Into<String>) -> Self {
        Self::Slot(DeckSlotLocation {
            slot_name: slot_name.into(),
        })
    }

    pub fn module(module_id: impl Into<String>) -> Self {
        Self::Module(ModuleLocation {
            module_id: module_id.into(),
        })
    }

    pub fn addressable_area(name: impl Into<String>) -> Self {
        Self::AddressableArea(AddressableAreaLocation {
            addressable_area_name: name.into(),
        })
    }

    pub fn off_deck() -> Self {
        Self::OffDeck(OffDeckMarker::OffDeck)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckSlotLocation {
    pub slot_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleLocation {
    pub module_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnLabwareLocation {
    pub labware_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressableAreaLocation {
    pub addressable_area_name: String,
}

/// The literal string `"offDeck"`, as a unit enum so the untagged
/// [`LabwareLocation`] serializes it as a string rather than `null`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OffDeckMarker {
    #[serde(rename = "offDeck")]
    OffDeck,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LabwareMovementStrategy {
    #[serde(rename = "usingGripper")]
    UsingGripper,
    #[serde(rename = "manualMoveWithPause")]
    ManualMoveWithPause,
    #[serde(rename = "manualMoveWithoutPause")]
    ManualMoveWithoutPause,
}

/// Axis vocabulary for `moveRelative`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MovementAxis {
    X,
    Y,
    Z,
}

/// Motor axis vocabulary for `home` and `retractAxis`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotorAxis {
    #[serde(rename = "x")]
    X,
    #[serde(rename = "y")]
    Y,
    #[serde(rename = "leftZ")]
    LeftZ,
    #[serde(rename = "rightZ")]
    RightZ,
    #[serde(rename = "leftPlunger")]
    LeftPlunger,
    #[serde(rename = "rightPlunger")]
    RightPlunger,
    #[serde(rename = "extensionZ")]
    ExtensionZ,
    #[serde(rename = "extensionJaw")]
    ExtensionJaw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TipPresenceState {
    Present,
    Absent,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StatusBarAnimation {
    Idle,
    Confirm,
    Updating,
    Disco,
    Off,
}

/// Nozzle layout for `configureNozzleLayout`, tagged by `style`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "style")]
pub enum NozzleLayoutConfiguration {
    #[serde(rename = "ALL")]
    All,
    #[serde(rename = "SINGLE", rename_all = "camelCase")]
    Single { primary_nozzle: String },
    #[serde(rename = "ROW", rename_all = "camelCase")]
    Row { primary_nozzle: String },
    #[serde(rename = "COLUMN", rename_all = "camelCase")]
    Column { primary_nozzle: String },
    #[serde(rename = "QUADRANT", rename_all = "camelCase")]
    Quadrant {
        primary_nozzle: String,
        front_right_nozzle: String,
        back_left_nozzle: String,
    },
}
