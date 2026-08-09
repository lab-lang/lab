//! Checked authoring API for Opentrons Flex JSON protocols.
//!
//! [`FlexProtocolBuilder`] owns a deck registry, per-pipette tip and volume
//! state, per-module lid, latch, and temperature-target state, and the
//! embedded labware definition table. Load methods return typed handles and
//! command methods accept only handles, so a reference to something that was
//! never loaded cannot be expressed. Every remaining semantic rule the
//! protocol engine enforces during analysis is checked when the command is
//! authored, and each check documents the engine error it prevents.
//!
//! The builder is Flex-specific: slots, pipettes, and modules use Flex
//! vocabulary, so an OT-2 instrument in a Flex protocol is unrepresentable.
//! OT-2 protocols are authored through [`crate::v8::schema`] directly.

use std::collections::BTreeMap;
use std::marker::PhantomData;

use thiserror::Error;

use crate::labware::{LabwareDefinition, standard_definition};
use crate::v8::schema::{
    self, Command, CommandAction, DropTipInPlaceParams, LabwareLocation, LabwareMovementStrategy,
    Liquid, LoadLabwareParams, LoadLiquidParams, LoadModuleParams, LoadPipetteParams, Metadata,
    ModuleModel, ModuleOnlyParams, MoveLabwareParams, MoveToAddressableAreaForDropTipParams,
    PipetteName, ProtocolDocument, RunProfileStep, WellLocation,
};

/// A deck slot an Opentrons Flex pipette can address. Row A is the rear of
/// the deck and column 1 the left. Staging slots A4–D4 are deliberately
/// absent: only the gripper and the 96-channel reach them, and pipetting to
/// one is rejected by the engine (`LocationNotAccessibleByPipetteError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FlexSlot {
    A1,
    A2,
    A3,
    B1,
    B2,
    B3,
    C1,
    C2,
    C3,
    D1,
    D2,
    D3,
}

impl FlexSlot {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A1 => "A1",
            Self::A2 => "A2",
            Self::A3 => "A3",
            Self::B1 => "B1",
            Self::B2 => "B2",
            Self::B3 => "B3",
            Self::C1 => "C1",
            Self::C2 => "C2",
            Self::C3 => "C3",
            Self::D1 => "D1",
            Self::D2 => "D2",
            Self::D3 => "D3",
        }
    }

    /// Parse a slot name such as `"C2"`.
    pub fn parse(name: &str) -> Option<Self> {
        [
            Self::A1,
            Self::A2,
            Self::A3,
            Self::B1,
            Self::B2,
            Self::B3,
            Self::C1,
            Self::C2,
            Self::C3,
            Self::D1,
            Self::D2,
            Self::D3,
        ]
        .into_iter()
        .find(|slot| slot.as_str() == name)
    }

    pub fn column(self) -> u8 {
        self.as_str().as_bytes()[1] - b'0'
    }

    fn row(self) -> u8 {
        self.as_str().as_bytes()[0]
    }

    /// Slots immediately east and west, for the shaking heater-shaker rule.
    fn horizontal_neighbors(self) -> Vec<FlexSlot> {
        let row = self.row();
        let column = self.column();
        [column.checked_sub(1), column.checked_add(1)]
            .into_iter()
            .flatten()
            .filter_map(|column| Self::parse(&format!("{}{column}", row as char)))
            .collect()
    }
}

impl std::fmt::Display for FlexSlot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The Flex trash bin, one of the movable-trash addressable areas. Trash bins
/// install only in columns 1 and 3; the factory configuration is A3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrashArea {
    MovableTrashA1,
    MovableTrashB1,
    MovableTrashC1,
    MovableTrashD1,
    MovableTrashA3,
    MovableTrashB3,
    MovableTrashC3,
    MovableTrashD3,
}

impl TrashArea {
    pub fn area_name(self) -> &'static str {
        match self {
            Self::MovableTrashA1 => "movableTrashA1",
            Self::MovableTrashB1 => "movableTrashB1",
            Self::MovableTrashC1 => "movableTrashC1",
            Self::MovableTrashD1 => "movableTrashD1",
            Self::MovableTrashA3 => "movableTrashA3",
            Self::MovableTrashB3 => "movableTrashB3",
            Self::MovableTrashC3 => "movableTrashC3",
            Self::MovableTrashD3 => "movableTrashD3",
        }
    }

    pub fn slot(self) -> FlexSlot {
        match self {
            Self::MovableTrashA1 => FlexSlot::A1,
            Self::MovableTrashB1 => FlexSlot::B1,
            Self::MovableTrashC1 => FlexSlot::C1,
            Self::MovableTrashD1 => FlexSlot::D1,
            Self::MovableTrashA3 => FlexSlot::A3,
            Self::MovableTrashB3 => FlexSlot::B3,
            Self::MovableTrashC3 => FlexSlot::C3,
            Self::MovableTrashD3 => FlexSlot::D3,
        }
    }

    /// Parse an area name such as `"movableTrashA3"`.
    pub fn parse(name: &str) -> Option<Self> {
        [
            Self::MovableTrashA1,
            Self::MovableTrashB1,
            Self::MovableTrashC1,
            Self::MovableTrashD1,
            Self::MovableTrashA3,
            Self::MovableTrashB3,
            Self::MovableTrashC3,
            Self::MovableTrashD3,
        ]
        .into_iter()
        .find(|area| area.area_name() == name)
    }
}

/// A pipette the Flex accepts. This vocabulary is disjoint from the OT-2's,
/// so loading an OT-2 instrument into a Flex protocol does not typecheck.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlexPipetteName {
    P50Single,
    P50Multi,
    P1000Single,
    P1000Multi,
    P1000Channel96,
}

impl FlexPipetteName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::P50Single => "p50_single_flex",
            Self::P50Multi => "p50_multi_flex",
            Self::P1000Single => "p1000_single_flex",
            Self::P1000Multi => "p1000_multi_flex",
            Self::P1000Channel96 => "p1000_96",
        }
    }

    /// Parse a wire name such as `"p50_single_flex"`.
    pub fn parse(name: &str) -> Option<Self> {
        [
            Self::P50Single,
            Self::P50Multi,
            Self::P1000Single,
            Self::P1000Multi,
            Self::P1000Channel96,
        ]
        .into_iter()
        .find(|pipette| pipette.as_str() == name)
    }

    fn schema_name(self) -> PipetteName {
        match self {
            Self::P50Single => PipetteName::P50SingleFlex,
            Self::P50Multi => PipetteName::P50MultiFlex,
            Self::P1000Single => PipetteName::P1000SingleFlex,
            Self::P1000Multi => PipetteName::P1000MultiFlex,
            Self::P1000Channel96 => PipetteName::P1000Channel96,
        }
    }

    /// The pipette's maximum volume in µL. The working volume is further
    /// capped by the attached tip's capacity.
    pub fn max_volume_ul(self) -> f64 {
        match self {
            Self::P50Single | Self::P50Multi => 50.0,
            Self::P1000Single | Self::P1000Multi | Self::P1000Channel96 => 1000.0,
        }
    }

    /// A conservative default aspirate/dispense flow rate in µL/s.
    pub fn default_flow_rate_ul_s(self) -> f64 {
        match self {
            Self::P50Single | Self::P50Multi => 35.0,
            Self::P1000Single | Self::P1000Multi | Self::P1000Channel96 => 160.0,
        }
    }
}

/// A mount a pipette loads on. The gripper's `extension` mount is not a
/// pipette mount, so it is absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipetteMount {
    Left,
    Right,
}

impl PipetteMount {
    fn schema_mount(self) -> schema::Mount {
        match self {
            Self::Left => schema::Mount::Left,
            Self::Right => schema::Mount::Right,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

mod sealed {
    pub trait Sealed {}
}

/// A module kind the Flex accepts, implemented by the marker types this
/// module exports. Module handles are parameterized by kind, so a
/// thermocycler command against a temperature module does not typecheck
/// (`WrongModuleTypeError`).
pub trait FlexModule: sealed::Sealed {
    #[doc(hidden)]
    const MODEL: ModuleModel;
    #[doc(hidden)]
    const DISPLAY: &'static str;
}

macro_rules! flex_module {
    ($name:ident, $model:expr, $display:literal) => {
        #[doc = concat!("Marker type for the ", $display, ".")]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $name;
        impl sealed::Sealed for $name {}
        impl FlexModule for $name {
            const MODEL: ModuleModel = $model;
            const DISPLAY: &'static str = $display;
        }
    };
}

flex_module!(
    Thermocycler,
    ModuleModel::ThermocyclerModuleV2,
    "Thermocycler Module GEN2"
);
flex_module!(
    TemperatureModule,
    ModuleModel::TemperatureModuleV2,
    "Temperature Module GEN2"
);
flex_module!(
    HeaterShaker,
    ModuleModel::HeaterShakerModuleV1,
    "Heater-Shaker Module GEN1"
);
flex_module!(
    MagneticBlock,
    ModuleModel::MagneticBlockV1,
    "Magnetic Block GEN1"
);
flex_module!(
    AbsorbanceReader,
    ModuleModel::AbsorbanceReaderV1,
    "Absorbance Plate Reader Module GEN1"
);

/// Handle to a loaded pipette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipetteId(usize);

/// Handle to loaded labware.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LabwareId(usize);

/// Handle to a declared liquid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiquidId(usize);

/// Handle to a loaded module, parameterized by its kind.
pub struct ModuleId<M: FlexModule>(usize, PhantomData<M>);

impl<M: FlexModule> Clone for ModuleId<M> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<M: FlexModule> Copy for ModuleId<M> {}
impl<M: FlexModule> std::fmt::Debug for ModuleId<M> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "ModuleId<{}>({})", M::DISPLAY, self.0)
    }
}
impl<M: FlexModule> PartialEq for ModuleId<M> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<M: FlexModule> Eq for ModuleId<M> {}

/// Everything that can go wrong while authoring a protocol. Each variant
/// corresponds to a rejection the protocol engine would produce during
/// analysis; the check here reports it at construction time instead.
#[derive(Debug, Error, PartialEq)]
pub enum ProtocolError {
    #[error("mount '{mount}' already carries '{existing}', and a mount holds one pipette")]
    MountOccupied { mount: String, existing: String },
    #[error("the 96-channel pipette loads on the left mount only")]
    NinetySixChannelNeedsLeftMount,
    #[error("deck slot {slot} already holds {occupant}, and a slot holds one item")]
    SlotOccupied { slot: String, occupant: String },
    #[error(
        "the thermocycler installs across slots A1 and B1 and is loaded as 'B1', not '{found}'"
    )]
    ThermocyclerSlot { found: String },
    #[error("the {module} installs in {requirement}, not slot {slot}")]
    ModuleSlotInvalid {
        module: String,
        requirement: &'static str,
        slot: String,
    },
    #[error(
        "no labware definition named '{load_name}' is embedded in this crate; embed a custom definition or use one of the standard load names"
    )]
    UnknownLabware { load_name: String },
    #[error("well '{well}' does not exist on '{labware}', which is a {well_count}-well labware")]
    WellDoesNotExist {
        labware: String,
        well: String,
        well_count: usize,
    },
    #[error("'{labware}' is not a tip rack, so tips cannot be picked up from it")]
    NotATipRack { labware: String },
    #[error("'{labware}' is a tip rack, so liquid cannot be handled in it")]
    IsATipRack { labware: String },
    #[error("pipette '{pipette}' has no tip attached, which this operation requires")]
    TipNotAttached { pipette: String },
    #[error(
        "pipette '{pipette}' already carries a tip, which must be dropped before another pick-up"
    )]
    TipAlreadyAttached { pipette: String },
    #[error(
        "tip rack '{labware}' well {well} was already picked from, and a tip well holds one tip"
    )]
    TipAlreadyUsed { labware: String, well: String },
    #[error(
        "aspirating {requested} uL into pipette '{pipette}' holding {held} uL exceeds its {working} uL working volume (the lesser of pipette maximum and tip capacity)"
    )]
    OverAspiration {
        pipette: String,
        requested: f64,
        held: f64,
        working: f64,
    },
    #[error("dispensing {requested} uL from pipette '{pipette}' holding only {held} uL")]
    OverDispense {
        pipette: String,
        requested: f64,
        held: f64,
    },
    #[error("a flow rate must be greater than zero, found {found} uL/s")]
    NonPositiveFlowRate { found: f64 },
    #[error("a volume must not be negative, found {found} uL")]
    NegativeVolume { found: f64 },
    #[error(
        "volume {volume} uL is outside pipette '{pipette}''s configurable range of 0 to {maximum} uL"
    )]
    VolumeOutOfRange {
        pipette: String,
        volume: f64,
        maximum: f64,
    },
    #[error("module '{module}' already carries labware '{labware}'")]
    ModuleOccupied { module: String, labware: String },
    #[error(
        "labware '{labware}' sits on the thermocycler, whose lid must be opened before this operation"
    )]
    ThermocyclerLidClosed { labware: String },
    #[error(
        "labware '{labware}' sits on the heater-shaker, whose latch must be closed before pipetting to it"
    )]
    HeaterShakerLatchOpenForPipetting { labware: String },
    #[error(
        "labware '{labware}' sits on the heater-shaker, whose latch must be opened before moving it"
    )]
    HeaterShakerLatchClosedForMove { labware: String },
    #[error("the heater-shaker is shaking, so labware on or beside it cannot be pipetted")]
    HeaterShakerShaking,
    #[error("the heater-shaker's labware latch must be closed before shaking")]
    LatchOpenWhileShaking,
    #[error("the heater-shaker's labware latch cannot open while it is shaking")]
    LatchCannotOpenWhileShaking,
    #[error("shake speed {rpm} rpm is outside the heater-shaker's range of 200 to 3000 rpm")]
    ShakeSpeedOutOfRange { rpm: f64 },
    #[error("{celsius} C is outside the {device}'s range of {minimum} to {maximum} C")]
    TemperatureOutOfRange {
        device: &'static str,
        celsius: f64,
        minimum: f64,
        maximum: f64,
    },
    #[error("the {device} has no target temperature set, so there is nothing to wait for")]
    NoTargetTemperature { device: &'static str },
    #[error("a thermocycler profile must contain at least one step")]
    EmptyProfile,
    #[error(
        "loading {volume} uL into '{labware}' well {well} exceeds the well's {capacity} uL capacity"
    )]
    LiquidVolumeExceedsWell {
        labware: String,
        well: String,
        volume: f64,
        capacity: f64,
    },
    #[error("a display color is '#rrggbb' or '#rrggbbaa', found '{found}'")]
    InvalidDisplayColor { found: String },
    #[error("labware '{labware}' was disposed into the waste chute and cannot be used again")]
    LabwareDisposed { labware: String },
    #[error("slot D3 holds {occupant}, so the waste chute cannot be installed there")]
    WasteChuteBlocked { occupant: String },
    #[error(transparent)]
    Labware(#[from] crate::labware::LabwareDefinitionError),
}

#[derive(Clone, Debug)]
enum Occupant {
    Labware(usize),
    Module(usize),
    Trash,
    WasteChute,
}

#[derive(Clone, Debug)]
enum Placement {
    Slot(FlexSlot),
    OnModule(usize),
    Disposed,
}

#[derive(Clone, Debug)]
struct PipetteState {
    id: String,
    name: FlexPipetteName,
    tip_volume_ul: Option<f64>,
    held_ul: f64,
}

impl PipetteState {
    fn working_volume_ul(&self, tip_volume_ul: f64) -> f64 {
        self.name.max_volume_ul().min(tip_volume_ul)
    }
}

#[derive(Clone, Debug)]
struct LabwareState {
    id: String,
    definition: LabwareDefinition,
    placement: Placement,
    used_tip_wells: std::collections::BTreeSet<String>,
}

#[derive(Clone, Debug)]
struct ModuleState {
    id: String,
    model: ModuleModel,
    slot: FlexSlot,
    labware: Option<usize>,
    lid_open: bool,
    latch_open: bool,
    shaking: bool,
    block_target_c: Option<f64>,
    lid_target_c: Option<f64>,
    temperature_target_c: Option<f64>,
}

/// Checked authoring of one Flex protocol. See the module documentation for
/// the layering; see [`Self::build`] for the emitted document.
#[derive(Clone, Debug)]
pub struct FlexProtocolBuilder {
    metadata: Metadata,
    commands: Vec<Command>,
    liquids: Vec<Liquid>,
    pipettes: Vec<PipetteState>,
    labware: Vec<LabwareState>,
    modules: Vec<ModuleState>,
    occupied: BTreeMap<FlexSlot, Occupant>,
    mounts: BTreeMap<&'static str, usize>,
    trash: TrashArea,
}

impl FlexProtocolBuilder {
    /// Start a protocol with the trash bin in the factory position (A3).
    pub fn new(metadata: Metadata) -> Self {
        Self::with_trash(metadata, TrashArea::MovableTrashA3)
    }

    /// Start a protocol with the trash bin in a stated position. The trash
    /// occupies its deck slot, so nothing else can load there.
    pub fn with_trash(metadata: Metadata, trash: TrashArea) -> Self {
        let mut occupied = BTreeMap::new();
        occupied.insert(trash.slot(), Occupant::Trash);
        Self {
            metadata,
            commands: Vec::new(),
            liquids: Vec::new(),
            pipettes: Vec::new(),
            labware: Vec::new(),
            modules: Vec::new(),
            occupied,
            mounts: BTreeMap::new(),
            trash,
        }
    }

    pub fn trash(&self) -> TrashArea {
        self.trash
    }

    /// Load a pipette. Prevents double-loading a mount and a 96-channel on
    /// the right mount.
    pub fn load_pipette(
        &mut self,
        name: FlexPipetteName,
        mount: PipetteMount,
    ) -> Result<PipetteId, ProtocolError> {
        if name == FlexPipetteName::P1000Channel96 && mount != PipetteMount::Left {
            return Err(ProtocolError::NinetySixChannelNeedsLeftMount);
        }
        if let Some(existing) = self.mounts.get(mount.as_str()) {
            return Err(ProtocolError::MountOccupied {
                mount: mount.as_str().into(),
                existing: self.pipettes[*existing].name.as_str().into(),
            });
        }
        let index = self.pipettes.len();
        let id = format!("pipette-{index}");
        self.mounts.insert(mount.as_str(), index);
        self.pipettes.push(PipetteState {
            id: id.clone(),
            name,
            tip_volume_ul: None,
            held_ul: 0.0,
        });
        self.push(CommandAction::LoadPipette(LoadPipetteParams {
            pipette_name: name.schema_name(),
            mount: mount.schema_mount(),
            pipette_id: Some(id),
            tip_overlap_not_after_version: None,
            liquid_presence_detection: None,
        }));
        Ok(PipetteId(index))
    }

    /// Load a module into a deck slot. Positional rules (`AreaNotInDeckConfigurationError`,
    /// `IncompatibleAddressableAreaError`): the thermocycler installs across
    /// A1+B1 and is addressed as B1; temperature module and heater-shaker
    /// install in columns 1 and 3; the absorbance reader installs in column 3;
    /// the magnetic block installs anywhere.
    pub fn load_module<M: FlexModule>(
        &mut self,
        slot: FlexSlot,
    ) -> Result<ModuleId<M>, ProtocolError> {
        match M::MODEL {
            ModuleModel::ThermocyclerModuleV2 => {
                if slot != FlexSlot::B1 {
                    return Err(ProtocolError::ThermocyclerSlot {
                        found: slot.to_string(),
                    });
                }
                self.require_free(FlexSlot::A1)?;
                self.require_free(FlexSlot::B1)?;
            }
            ModuleModel::TemperatureModuleV2 | ModuleModel::HeaterShakerModuleV1 => {
                if slot.column() == 2 {
                    return Err(ProtocolError::ModuleSlotInvalid {
                        module: M::DISPLAY.into(),
                        requirement: "column 1 or 3",
                        slot: slot.to_string(),
                    });
                }
                self.require_free(slot)?;
            }
            ModuleModel::AbsorbanceReaderV1 => {
                if slot.column() != 3 {
                    return Err(ProtocolError::ModuleSlotInvalid {
                        module: M::DISPLAY.into(),
                        requirement: "column 3",
                        slot: slot.to_string(),
                    });
                }
                self.require_free(slot)?;
            }
            ModuleModel::MagneticBlockV1 => self.require_free(slot)?,
            // The Flex builder loads only Flex module models; the marker
            // types above name no others.
            _ => unreachable!("Flex module markers name only Flex module models"),
        }
        let index = self.modules.len();
        let id = format!("module-{index}");
        self.occupied.insert(slot, Occupant::Module(index));
        if M::MODEL == ModuleModel::ThermocyclerModuleV2 {
            self.occupied.insert(FlexSlot::A1, Occupant::Module(index));
        }
        self.modules.push(ModuleState {
            id: id.clone(),
            model: M::MODEL,
            slot,
            labware: None,
            lid_open: false,
            latch_open: false,
            shaking: false,
            block_target_c: None,
            lid_target_c: None,
            temperature_target_c: None,
        });
        self.push(CommandAction::LoadModule(LoadModuleParams {
            model: M::MODEL,
            location: schema::DeckSlotLocation {
                slot_name: slot.as_str().into(),
            },
            module_id: Some(id),
        }));
        Ok(ModuleId(index, PhantomData))
    }

    /// Load standard labware into a deck slot (`LocationIsOccupiedError`).
    pub fn load_labware(
        &mut self,
        load_name: &str,
        slot: FlexSlot,
    ) -> Result<LabwareId, ProtocolError> {
        let definition = standard_definition(load_name)
            .ok_or_else(|| ProtocolError::UnknownLabware {
                load_name: load_name.into(),
            })?
            .clone();
        self.load_labware_with_definition(definition, slot)
    }

    /// Load labware from a caller-supplied definition into a deck slot.
    pub fn load_labware_with_definition(
        &mut self,
        definition: LabwareDefinition,
        slot: FlexSlot,
    ) -> Result<LabwareId, ProtocolError> {
        self.require_free(slot)?;
        let index = self.insert_labware(definition, Placement::Slot(slot));
        self.occupied.insert(slot, Occupant::Labware(index));
        let labware = &self.labware[index];
        self.push(CommandAction::LoadLabware(LoadLabwareParams {
            location: LabwareLocation::slot(slot.as_str()),
            load_name: labware.definition.load_name().into(),
            namespace: labware.definition.namespace().into(),
            version: labware.definition.version(),
            labware_id: Some(labware.id.clone()),
            display_name: None,
        }));
        Ok(LabwareId(index))
    }

    /// Load standard labware onto a module (`LocationIsOccupiedError` when the
    /// module already carries labware).
    pub fn load_labware_on_module<M: FlexModule>(
        &mut self,
        load_name: &str,
        module: ModuleId<M>,
    ) -> Result<LabwareId, ProtocolError> {
        let definition = standard_definition(load_name)
            .ok_or_else(|| ProtocolError::UnknownLabware {
                load_name: load_name.into(),
            })?
            .clone();
        if let Some(existing) = self.modules[module.0].labware {
            return Err(ProtocolError::ModuleOccupied {
                module: self.modules[module.0].id.clone(),
                labware: self.labware[existing].id.clone(),
            });
        }
        let index = self.insert_labware(definition, Placement::OnModule(module.0));
        self.modules[module.0].labware = Some(index);
        let module_id = self.modules[module.0].id.clone();
        let labware = &self.labware[index];
        self.push(CommandAction::LoadLabware(LoadLabwareParams {
            location: LabwareLocation::module(module_id),
            load_name: labware.definition.load_name().into(),
            namespace: labware.definition.namespace().into(),
            version: labware.definition.version(),
            labware_id: Some(labware.id.clone()),
            display_name: None,
        }));
        Ok(LabwareId(index))
    }

    /// Declare a liquid for `loadLiquid` and the run app's deck map.
    pub fn define_liquid(
        &mut self,
        display_name: &str,
        description: &str,
        display_color: Option<&str>,
    ) -> Result<LiquidId, ProtocolError> {
        if let Some(color) = display_color
            && !is_display_color(color)
        {
            return Err(ProtocolError::InvalidDisplayColor {
                found: color.into(),
            });
        }
        let index = self.liquids.len();
        self.liquids.push(Liquid {
            display_name: display_name.into(),
            description: description.into(),
            display_color: display_color.map(str::to_owned),
        });
        Ok(LiquidId(index))
    }

    /// Place a declared liquid into wells. Rejects unknown wells, tip racks,
    /// and volumes beyond a well's stated capacity.
    pub fn load_liquid(
        &mut self,
        liquid: LiquidId,
        labware: LabwareId,
        volume_by_well: &[(&str, f64)],
    ) -> Result<(), ProtocolError> {
        let state = &self.labware[labware.0];
        if state.definition.is_tip_rack() {
            return Err(ProtocolError::IsATipRack {
                labware: state.id.clone(),
            });
        }
        let mut volumes = BTreeMap::new();
        for (well, volume) in volume_by_well {
            let capacity = self.require_well(labware, well)?;
            if *volume < 0.0 {
                return Err(ProtocolError::NegativeVolume { found: *volume });
            }
            if *volume > capacity {
                return Err(ProtocolError::LiquidVolumeExceedsWell {
                    labware: self.labware[labware.0].id.clone(),
                    well: (*well).to_owned(),
                    volume: *volume,
                    capacity,
                });
            }
            volumes.insert((*well).to_owned(), *volume);
        }
        self.push(CommandAction::LoadLiquid(LoadLiquidParams {
            liquid_id: format!("liquid-{}", liquid.0),
            labware_id: self.labware[labware.0].id.clone(),
            volume_by_well: volumes,
        }));
        Ok(())
    }

    /// Pick up a tip. Prevents `LabwareIsNotTipRackError`, `TipAttachedError`,
    /// `WellDoesNotExistError`, and re-picking an emptied tip well.
    pub fn pick_up_tip(
        &mut self,
        pipette: PipetteId,
        rack: LabwareId,
        well: &str,
    ) -> Result<(), ProtocolError> {
        let rack_state = &self.labware[rack.0];
        if !rack_state.definition.is_tip_rack() {
            return Err(ProtocolError::NotATipRack {
                labware: rack_state.id.clone(),
            });
        }
        self.require_on_deck(rack)?;
        let tip_volume = self.require_well(rack, well)?;
        if self.pipettes[pipette.0].tip_volume_ul.is_some() {
            return Err(ProtocolError::TipAlreadyAttached {
                pipette: self.pipettes[pipette.0].id.clone(),
            });
        }
        if !self.labware[rack.0].used_tip_wells.insert(well.to_owned()) {
            return Err(ProtocolError::TipAlreadyUsed {
                labware: self.labware[rack.0].id.clone(),
                well: well.to_owned(),
            });
        }
        self.pipettes[pipette.0].tip_volume_ul = Some(tip_volume);
        self.pipettes[pipette.0].held_ul = 0.0;
        let params = schema::PickUpTipParams {
            pipette_id: self.pipettes[pipette.0].id.clone(),
            labware_id: self.labware[rack.0].id.clone(),
            well_name: well.to_owned(),
            well_location: None,
        };
        self.push(CommandAction::PickUpTip(params));
        Ok(())
    }

    /// Drop the attached tip into the movable trash bin: a
    /// `moveToAddressableAreaForDropTip` followed by `dropTipInPlace`, the
    /// Flex idiom for a robot with no trash labware.
    pub fn drop_tip_into_trash(&mut self, pipette: PipetteId) -> Result<(), ProtocolError> {
        self.require_tip(pipette)?;
        let pipette_id = self.pipettes[pipette.0].id.clone();
        self.push(CommandAction::MoveToAddressableAreaForDropTip(
            MoveToAddressableAreaForDropTipParams {
                pipette_id: pipette_id.clone(),
                addressable_area_name: self.trash.area_name().into(),
                offset: None,
                alternate_drop_location: None,
                ignore_tip_configuration: None,
                minimum_z_height: None,
                force_direct: None,
                speed: None,
            },
        ));
        self.push(CommandAction::DropTipInPlace(DropTipInPlaceParams {
            pipette_id,
            home_after: None,
        }));
        self.pipettes[pipette.0].tip_volume_ul = None;
        self.pipettes[pipette.0].held_ul = 0.0;
        Ok(())
    }

    /// Return the attached tip to a tip-rack well.
    pub fn drop_tip(
        &mut self,
        pipette: PipetteId,
        rack: LabwareId,
        well: &str,
    ) -> Result<(), ProtocolError> {
        self.require_tip(pipette)?;
        self.require_well(rack, well)?;
        let params = schema::DropTipParams {
            pipette_id: self.pipettes[pipette.0].id.clone(),
            labware_id: self.labware[rack.0].id.clone(),
            well_name: well.to_owned(),
            well_location: None,
            home_after: None,
            alternate_drop_location: None,
        };
        self.push(CommandAction::DropTip(params));
        self.pipettes[pipette.0].tip_volume_ul = None;
        self.pipettes[pipette.0].held_ul = 0.0;
        Ok(())
    }

    /// Aspirate. Prevents `TipNotAttachedError`, `LabwareIsTipRackError`,
    /// `WellDoesNotExistError`, over-aspiration beyond the working volume,
    /// `ThermocyclerNotOpenError`, and heater-shaker access violations.
    pub fn aspirate(
        &mut self,
        pipette: PipetteId,
        labware: LabwareId,
        well: &str,
        volume_ul: f64,
        flow_rate_ul_s: f64,
        well_location: Option<WellLocation>,
    ) -> Result<(), ProtocolError> {
        self.require_liquid_access(pipette, labware, well, flow_rate_ul_s)?;
        if volume_ul < 0.0 {
            return Err(ProtocolError::NegativeVolume { found: volume_ul });
        }
        let state = &self.pipettes[pipette.0];
        let working = state.working_volume_ul(
            state
                .tip_volume_ul
                .expect("require_liquid_access verified a tip is attached"),
        );
        if state.held_ul + volume_ul > working {
            return Err(ProtocolError::OverAspiration {
                pipette: state.id.clone(),
                requested: volume_ul,
                held: state.held_ul,
                working,
            });
        }
        self.pipettes[pipette.0].held_ul += volume_ul;
        let params = schema::AspirateParams {
            pipette_id: self.pipettes[pipette.0].id.clone(),
            labware_id: self.labware[labware.0].id.clone(),
            well_name: well.to_owned(),
            volume: volume_ul,
            flow_rate: flow_rate_ul_s,
            well_location,
        };
        self.push(CommandAction::Aspirate(params));
        Ok(())
    }

    /// Dispense. Additionally prevents `InvalidDispenseVolumeError`:
    /// dispensing more than the pipette currently holds.
    pub fn dispense(
        &mut self,
        pipette: PipetteId,
        labware: LabwareId,
        well: &str,
        volume_ul: f64,
        flow_rate_ul_s: f64,
        well_location: Option<WellLocation>,
    ) -> Result<(), ProtocolError> {
        self.require_liquid_access(pipette, labware, well, flow_rate_ul_s)?;
        if volume_ul < 0.0 {
            return Err(ProtocolError::NegativeVolume { found: volume_ul });
        }
        let state = &self.pipettes[pipette.0];
        if volume_ul > state.held_ul {
            return Err(ProtocolError::OverDispense {
                pipette: state.id.clone(),
                requested: volume_ul,
                held: state.held_ul,
            });
        }
        self.pipettes[pipette.0].held_ul -= volume_ul;
        let params = schema::DispenseParams {
            pipette_id: self.pipettes[pipette.0].id.clone(),
            labware_id: self.labware[labware.0].id.clone(),
            well_name: well.to_owned(),
            volume: volume_ul,
            flow_rate: flow_rate_ul_s,
            well_location,
            push_out: None,
        };
        self.push(CommandAction::Dispense(params));
        Ok(())
    }

    /// Blow out into a well, emptying the pipette.
    pub fn blowout(
        &mut self,
        pipette: PipetteId,
        labware: LabwareId,
        well: &str,
        flow_rate_ul_s: f64,
        well_location: Option<WellLocation>,
    ) -> Result<(), ProtocolError> {
        self.require_liquid_access(pipette, labware, well, flow_rate_ul_s)?;
        self.pipettes[pipette.0].held_ul = 0.0;
        let params = schema::BlowoutParams {
            pipette_id: self.pipettes[pipette.0].id.clone(),
            labware_id: self.labware[labware.0].id.clone(),
            well_name: well.to_owned(),
            flow_rate: flow_rate_ul_s,
            well_location,
        };
        self.push(CommandAction::Blowout(params));
        Ok(())
    }

    /// Mix in place: `repetitions` aspirate/dispense pairs of `volume_ul`.
    pub fn mix(
        &mut self,
        pipette: PipetteId,
        labware: LabwareId,
        well: &str,
        repetitions: u32,
        volume_ul: f64,
        flow_rate_ul_s: f64,
    ) -> Result<(), ProtocolError> {
        for _ in 0..repetitions {
            self.aspirate(pipette, labware, well, volume_ul, flow_rate_ul_s, None)?;
            self.dispense(pipette, labware, well, volume_ul, flow_rate_ul_s, None)?;
        }
        Ok(())
    }

    /// Touch the tip to the well sides. Prevents `TipNotAttachedError` and
    /// `LabwareIsTipRackError`.
    pub fn touch_tip(
        &mut self,
        pipette: PipetteId,
        labware: LabwareId,
        well: &str,
        radius: Option<f64>,
        speed_mm_s: Option<f64>,
    ) -> Result<(), ProtocolError> {
        self.require_tip(pipette)?;
        if self.labware[labware.0].definition.is_tip_rack() {
            return Err(ProtocolError::IsATipRack {
                labware: self.labware[labware.0].id.clone(),
            });
        }
        self.require_well(labware, well)?;
        self.require_reachable(labware)?;
        let params = schema::TouchTipParams {
            pipette_id: self.pipettes[pipette.0].id.clone(),
            labware_id: self.labware[labware.0].id.clone(),
            well_name: well.to_owned(),
            well_location: None,
            radius,
            speed: speed_mm_s,
        };
        self.push(CommandAction::TouchTip(params));
        Ok(())
    }

    /// Configure a pipette for a smaller working volume. Prevents configuring
    /// outside `[0, maximum]`.
    pub fn configure_for_volume(
        &mut self,
        pipette: PipetteId,
        volume_ul: f64,
    ) -> Result<(), ProtocolError> {
        let state = &self.pipettes[pipette.0];
        let maximum = state.name.max_volume_ul();
        if !(0.0..=maximum).contains(&volume_ul) {
            return Err(ProtocolError::VolumeOutOfRange {
                pipette: state.id.clone(),
                volume: volume_ul,
                maximum,
            });
        }
        let params = schema::ConfigureForVolumeParams {
            pipette_id: state.id.clone(),
            volume: volume_ul,
            tip_overlap_not_after_version: None,
        };
        self.push(CommandAction::ConfigureForVolume(params));
        Ok(())
    }

    /// Move labware with the Flex gripper. Prevents moving through a closed
    /// thermocycler lid or a closed heater-shaker latch, and dropping onto an
    /// occupied location.
    pub fn move_labware_to_slot(
        &mut self,
        labware: LabwareId,
        slot: FlexSlot,
    ) -> Result<(), ProtocolError> {
        self.require_movable(labware)?;
        self.require_free(slot)?;
        self.vacate(labware);
        self.labware[labware.0].placement = Placement::Slot(slot);
        self.occupied.insert(slot, Occupant::Labware(labware.0));
        let params = MoveLabwareParams {
            labware_id: self.labware[labware.0].id.clone(),
            new_location: LabwareLocation::slot(slot.as_str()),
            strategy: LabwareMovementStrategy::UsingGripper,
            pick_up_offset: None,
            drop_offset: None,
        };
        self.push(CommandAction::MoveLabware(params));
        Ok(())
    }

    /// Move labware onto a module with the Flex gripper.
    pub fn move_labware_to_module<M: FlexModule>(
        &mut self,
        labware: LabwareId,
        module: ModuleId<M>,
    ) -> Result<(), ProtocolError> {
        self.require_movable(labware)?;
        let target = &self.modules[module.0];
        if let Some(existing) = target.labware {
            return Err(ProtocolError::ModuleOccupied {
                module: target.id.clone(),
                labware: self.labware[existing].id.clone(),
            });
        }
        if target.model == ModuleModel::ThermocyclerModuleV2 && !target.lid_open {
            return Err(ProtocolError::ThermocyclerLidClosed {
                labware: self.labware[labware.0].id.clone(),
            });
        }
        if target.model == ModuleModel::HeaterShakerModuleV1 && !target.latch_open {
            return Err(ProtocolError::HeaterShakerLatchClosedForMove {
                labware: self.labware[labware.0].id.clone(),
            });
        }
        self.vacate(labware);
        self.labware[labware.0].placement = Placement::OnModule(module.0);
        self.modules[module.0].labware = Some(labware.0);
        let params = MoveLabwareParams {
            labware_id: self.labware[labware.0].id.clone(),
            new_location: LabwareLocation::module(self.modules[module.0].id.clone()),
            strategy: LabwareMovementStrategy::UsingGripper,
            pick_up_offset: None,
            drop_offset: None,
        };
        self.push(CommandAction::MoveLabware(params));
        Ok(())
    }

    /// Dispose labware into the waste chute (cutout D3) with the gripper.
    /// The chute and slot D3 labware are mutually exclusive.
    pub fn move_labware_to_waste_chute(&mut self, labware: LabwareId) -> Result<(), ProtocolError> {
        self.require_movable(labware)?;
        if let Some(occupant) = self.occupied.get(&FlexSlot::D3) {
            return Err(ProtocolError::WasteChuteBlocked {
                occupant: self.describe_occupant(occupant),
            });
        }
        self.vacate(labware);
        self.labware[labware.0].placement = Placement::Disposed;
        self.occupied.insert(FlexSlot::D3, Occupant::WasteChute);
        let params = MoveLabwareParams {
            labware_id: self.labware[labware.0].id.clone(),
            new_location: LabwareLocation::addressable_area("gripperWasteChute"),
            strategy: LabwareMovementStrategy::UsingGripper,
            pick_up_offset: None,
            drop_offset: None,
        };
        self.push(CommandAction::MoveLabware(params));
        Ok(())
    }

    // Thermocycler. Commands exist only on `ModuleId<Thermocycler>`, which is
    // what prevents `WrongModuleTypeError`.

    pub fn thermocycler_open_lid(&mut self, module: ModuleId<Thermocycler>) {
        self.modules[module.0].lid_open = true;
        let params = self.module_only(module.0);
        self.push(CommandAction::ThermocyclerOpenLid(params));
    }

    pub fn thermocycler_close_lid(&mut self, module: ModuleId<Thermocycler>) {
        self.modules[module.0].lid_open = false;
        let params = self.module_only(module.0);
        self.push(CommandAction::ThermocyclerCloseLid(params));
    }

    /// Set the block target. Prevents `InvalidTargetTemperatureError`: the
    /// block operates between 0 and 99 C.
    pub fn thermocycler_set_block_temperature(
        &mut self,
        module: ModuleId<Thermocycler>,
        celsius: f64,
        hold_seconds: Option<f64>,
        block_max_volume_ul: Option<f64>,
    ) -> Result<(), ProtocolError> {
        require_temperature("thermocycler block", celsius, 0.0, 99.0)?;
        self.modules[module.0].block_target_c = Some(celsius);
        let params = schema::ThermocyclerSetBlockTemperatureParams {
            module_id: self.modules[module.0].id.clone(),
            celsius,
            block_max_volume_ul,
            hold_time_seconds: hold_seconds,
        };
        self.push(CommandAction::ThermocyclerSetTargetBlockTemperature(params));
        Ok(())
    }

    /// Wait for the block target. Prevents `NoTargetTemperatureSetError`.
    pub fn thermocycler_wait_for_block_temperature(
        &mut self,
        module: ModuleId<Thermocycler>,
    ) -> Result<(), ProtocolError> {
        if self.modules[module.0].block_target_c.is_none() {
            return Err(ProtocolError::NoTargetTemperature {
                device: "thermocycler block",
            });
        }
        let params = self.module_only(module.0);
        self.push(CommandAction::ThermocyclerWaitForBlockTemperature(params));
        Ok(())
    }

    /// Set the lid target. Prevents `InvalidTargetTemperatureError`: the lid
    /// operates between 37 and 110 C.
    pub fn thermocycler_set_lid_temperature(
        &mut self,
        module: ModuleId<Thermocycler>,
        celsius: f64,
    ) -> Result<(), ProtocolError> {
        require_temperature("thermocycler lid", celsius, 37.0, 110.0)?;
        self.modules[module.0].lid_target_c = Some(celsius);
        let params = schema::ThermocyclerSetLidTemperatureParams {
            module_id: self.modules[module.0].id.clone(),
            celsius,
        };
        self.push(CommandAction::ThermocyclerSetTargetLidTemperature(params));
        Ok(())
    }

    /// Wait for the lid target. Prevents `NoTargetTemperatureSetError`.
    pub fn thermocycler_wait_for_lid_temperature(
        &mut self,
        module: ModuleId<Thermocycler>,
    ) -> Result<(), ProtocolError> {
        if self.modules[module.0].lid_target_c.is_none() {
            return Err(ProtocolError::NoTargetTemperature {
                device: "thermocycler lid",
            });
        }
        let params = self.module_only(module.0);
        self.push(CommandAction::ThermocyclerWaitForLidTemperature(params));
        Ok(())
    }

    pub fn thermocycler_deactivate_block(&mut self, module: ModuleId<Thermocycler>) {
        self.modules[module.0].block_target_c = None;
        let params = self.module_only(module.0);
        self.push(CommandAction::ThermocyclerDeactivateBlock(params));
    }

    pub fn thermocycler_deactivate_lid(&mut self, module: ModuleId<Thermocycler>) {
        self.modules[module.0].lid_target_c = None;
        let params = self.module_only(module.0);
        self.push(CommandAction::ThermocyclerDeactivateLid(params));
    }

    /// Run a thermal profile. Every step must sit in the block's 0–99 C
    /// range, and an empty profile is rejected rather than sent to the robot.
    pub fn thermocycler_run_profile(
        &mut self,
        module: ModuleId<Thermocycler>,
        steps: &[(f64, f64)],
        block_max_volume_ul: Option<f64>,
    ) -> Result<(), ProtocolError> {
        if steps.is_empty() {
            return Err(ProtocolError::EmptyProfile);
        }
        let profile = steps
            .iter()
            .map(|(celsius, hold_seconds)| {
                require_temperature("thermocycler block", *celsius, 0.0, 99.0)?;
                Ok(RunProfileStep {
                    celsius: *celsius,
                    hold_seconds: *hold_seconds,
                })
            })
            .collect::<Result<Vec<_>, ProtocolError>>()?;
        // The profile leaves the block at its final step's temperature.
        self.modules[module.0].block_target_c = steps.last().map(|(celsius, _)| *celsius);
        let params = schema::ThermocyclerRunProfileParams {
            module_id: self.modules[module.0].id.clone(),
            profile,
            block_max_volume_ul,
        };
        self.push(CommandAction::ThermocyclerRunProfile(params));
        Ok(())
    }

    // Temperature module.

    /// Set the target. Prevents `InvalidTargetTemperatureError`: the module
    /// operates between -9 and 99 C. Non-blocking; pair with
    /// [`Self::temperature_module_wait_for_temperature`].
    pub fn temperature_module_set_target(
        &mut self,
        module: ModuleId<TemperatureModule>,
        celsius: f64,
    ) -> Result<(), ProtocolError> {
        require_temperature("temperature module", celsius, -9.0, 99.0)?;
        self.modules[module.0].temperature_target_c = Some(celsius);
        let params = schema::TemperatureModuleSetTargetTemperatureParams {
            module_id: self.modules[module.0].id.clone(),
            celsius,
        };
        self.push(CommandAction::TemperatureModuleSetTargetTemperature(params));
        Ok(())
    }

    /// Wait for the target. Prevents `NoTargetTemperatureSetError`.
    pub fn temperature_module_wait_for_temperature(
        &mut self,
        module: ModuleId<TemperatureModule>,
    ) -> Result<(), ProtocolError> {
        if self.modules[module.0].temperature_target_c.is_none() {
            return Err(ProtocolError::NoTargetTemperature {
                device: "temperature module",
            });
        }
        let params = schema::TemperatureModuleWaitForTemperatureParams {
            module_id: self.modules[module.0].id.clone(),
            celsius: None,
        };
        self.push(CommandAction::TemperatureModuleWaitForTemperature(params));
        Ok(())
    }

    pub fn temperature_module_deactivate(&mut self, module: ModuleId<TemperatureModule>) {
        self.modules[module.0].temperature_target_c = None;
        let params = self.module_only(module.0);
        self.push(CommandAction::TemperatureModuleDeactivate(params));
    }

    // Heater-shaker.

    /// Set the heater target. Prevents `InvalidTargetTemperatureError`: the
    /// heater operates between 27 and 95 C.
    pub fn heater_shaker_set_target_temperature(
        &mut self,
        module: ModuleId<HeaterShaker>,
        celsius: f64,
    ) -> Result<(), ProtocolError> {
        require_temperature("heater-shaker", celsius, 27.0, 95.0)?;
        self.modules[module.0].temperature_target_c = Some(celsius);
        let params = schema::HeaterShakerSetTargetTemperatureParams {
            module_id: self.modules[module.0].id.clone(),
            celsius,
        };
        self.push(CommandAction::HeaterShakerSetTargetTemperature(params));
        Ok(())
    }

    /// Wait for the heater target. Prevents `NoTargetTemperatureSetError`.
    pub fn heater_shaker_wait_for_temperature(
        &mut self,
        module: ModuleId<HeaterShaker>,
    ) -> Result<(), ProtocolError> {
        if self.modules[module.0].temperature_target_c.is_none() {
            return Err(ProtocolError::NoTargetTemperature {
                device: "heater-shaker",
            });
        }
        let params = schema::HeaterShakerWaitForTemperatureParams {
            module_id: self.modules[module.0].id.clone(),
            celsius: None,
        };
        self.push(CommandAction::HeaterShakerWaitForTemperature(params));
        Ok(())
    }

    /// Start shaking. Prevents `InvalidTargetSpeedError` (200–3000 rpm) and
    /// `HeaterShakerLabwareLatchNotOpenError`'s converse: the latch must be
    /// closed before shaking.
    pub fn heater_shaker_set_and_wait_for_shake_speed(
        &mut self,
        module: ModuleId<HeaterShaker>,
        rpm: f64,
    ) -> Result<(), ProtocolError> {
        if !(200.0..=3000.0).contains(&rpm) {
            return Err(ProtocolError::ShakeSpeedOutOfRange { rpm });
        }
        if self.modules[module.0].latch_open {
            return Err(ProtocolError::LatchOpenWhileShaking);
        }
        self.modules[module.0].shaking = true;
        let params = schema::HeaterShakerSetShakeSpeedParams {
            module_id: self.modules[module.0].id.clone(),
            rpm,
        };
        self.push(CommandAction::HeaterShakerSetAndWaitForShakeSpeed(params));
        Ok(())
    }

    /// Open the labware latch. Refused while shaking.
    pub fn heater_shaker_open_labware_latch(
        &mut self,
        module: ModuleId<HeaterShaker>,
    ) -> Result<(), ProtocolError> {
        if self.modules[module.0].shaking {
            return Err(ProtocolError::LatchCannotOpenWhileShaking);
        }
        self.modules[module.0].latch_open = true;
        let params = self.module_only(module.0);
        self.push(CommandAction::HeaterShakerOpenLabwareLatch(params));
        Ok(())
    }

    pub fn heater_shaker_close_labware_latch(&mut self, module: ModuleId<HeaterShaker>) {
        self.modules[module.0].latch_open = false;
        let params = self.module_only(module.0);
        self.push(CommandAction::HeaterShakerCloseLabwareLatch(params));
    }

    pub fn heater_shaker_deactivate_heater(&mut self, module: ModuleId<HeaterShaker>) {
        self.modules[module.0].temperature_target_c = None;
        let params = self.module_only(module.0);
        self.push(CommandAction::HeaterShakerDeactivateHeater(params));
    }

    pub fn heater_shaker_deactivate_shaker(&mut self, module: ModuleId<HeaterShaker>) {
        self.modules[module.0].shaking = false;
        let params = self.module_only(module.0);
        self.push(CommandAction::HeaterShakerDeactivateShaker(params));
    }

    // Absorbance reader.

    pub fn absorbance_reader_open_lid(&mut self, module: ModuleId<AbsorbanceReader>) {
        let params = self.module_only(module.0);
        self.push(CommandAction::AbsorbanceReaderOpenLid(params));
    }

    pub fn absorbance_reader_close_lid(&mut self, module: ModuleId<AbsorbanceReader>) {
        let params = self.module_only(module.0);
        self.push(CommandAction::AbsorbanceReaderCloseLid(params));
    }

    pub fn absorbance_reader_initialize(
        &mut self,
        module: ModuleId<AbsorbanceReader>,
        sample_wavelength_nm: u32,
    ) {
        let params = schema::AbsorbanceReaderMeasureParams {
            module_id: self.modules[module.0].id.clone(),
            sample_wavelength: sample_wavelength_nm,
        };
        self.push(CommandAction::AbsorbanceReaderInitialize(params));
    }

    pub fn absorbance_reader_read(
        &mut self,
        module: ModuleId<AbsorbanceReader>,
        sample_wavelength_nm: u32,
    ) {
        let params = schema::AbsorbanceReaderMeasureParams {
            module_id: self.modules[module.0].id.clone(),
            sample_wavelength: sample_wavelength_nm,
        };
        self.push(CommandAction::AbsorbanceReaderRead(params));
    }

    // Timing and miscellany.

    pub fn comment(&mut self, message: &str) {
        self.push(CommandAction::Comment(schema::CommentParams {
            message: message.into(),
        }));
    }

    pub fn wait_for_duration(&mut self, seconds: f64, message: Option<&str>) {
        self.push(CommandAction::WaitForDuration(
            schema::WaitForDurationParams {
                seconds,
                message: message.map(str::to_owned),
            },
        ));
    }

    /// Pause until the operator resumes. This is `waitForResume`; the legacy
    /// alias `pause` is not emitted.
    pub fn wait_for_resume(&mut self, message: Option<&str>) {
        self.push(CommandAction::WaitForResume(schema::WaitForResumeParams {
            message: message.map(str::to_owned),
        }));
    }

    pub fn home(&mut self) {
        self.push(CommandAction::Home(schema::HomeParams::default()));
    }

    /// Assemble the finished document. Ids are unique and every embedded
    /// definition is referenced by construction, so assembly cannot fail.
    pub fn build(self) -> ProtocolDocument {
        let mut labware_definitions = BTreeMap::new();
        for labware in &self.labware {
            labware_definitions.insert(
                labware.definition.definition_key(),
                labware.definition.raw().clone(),
            );
        }
        let liquids = self
            .liquids
            .into_iter()
            .enumerate()
            .map(|(index, liquid)| (format!("liquid-{index}"), liquid))
            .collect();
        ProtocolDocument {
            ot_shared_schema: ProtocolDocument::OT_SHARED_SCHEMA.into(),
            schema_version: ProtocolDocument::SCHEMA_VERSION,
            metadata: self.metadata,
            robot: schema::Robot::flex(),
            labware_definition_schema_id: ProtocolDocument::LABWARE_DEFINITION_SCHEMA_ID.into(),
            labware_definitions,
            command_schema_id: ProtocolDocument::COMMAND_SCHEMA_ID.into(),
            commands: self.commands,
            command_annotation_schema_id: ProtocolDocument::COMMAND_ANNOTATION_SCHEMA_ID.into(),
            command_annotations: Vec::new(),
            liquid_schema_id: ProtocolDocument::LIQUID_SCHEMA_ID.into(),
            liquids,
            designer_application: Some(schema::DesignerApplication {
                name: Some("lab-lang/lab-opentrons-protocol".into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                data: None,
            }),
        }
    }

    // Internal checks.

    fn push(&mut self, action: CommandAction) {
        self.commands.push(action.into());
    }

    fn insert_labware(&mut self, definition: LabwareDefinition, placement: Placement) -> usize {
        let index = self.labware.len();
        self.labware.push(LabwareState {
            id: format!("labware-{index}"),
            definition,
            placement,
            used_tip_wells: std::collections::BTreeSet::new(),
        });
        index
    }

    fn module_only(&self, module: usize) -> ModuleOnlyParams {
        ModuleOnlyParams {
            module_id: self.modules[module].id.clone(),
        }
    }

    fn describe_occupant(&self, occupant: &Occupant) -> String {
        match occupant {
            Occupant::Labware(index) => format!("labware '{}'", self.labware[*index].id),
            Occupant::Module(index) => format!("module '{}'", self.modules[*index].id),
            Occupant::Trash => "the trash bin".into(),
            Occupant::WasteChute => "the waste chute".into(),
        }
    }

    fn require_free(&self, slot: FlexSlot) -> Result<(), ProtocolError> {
        if let Some(occupant) = self.occupied.get(&slot) {
            return Err(ProtocolError::SlotOccupied {
                slot: slot.to_string(),
                occupant: self.describe_occupant(occupant),
            });
        }
        Ok(())
    }

    fn require_well(&self, labware: LabwareId, well: &str) -> Result<f64, ProtocolError> {
        let state = &self.labware[labware.0];
        state
            .definition
            .well_volume_ul(well)
            .ok_or_else(|| ProtocolError::WellDoesNotExist {
                labware: state.id.clone(),
                well: well.to_owned(),
                well_count: state.definition.well_count(),
            })
    }

    fn require_tip(&self, pipette: PipetteId) -> Result<(), ProtocolError> {
        if self.pipettes[pipette.0].tip_volume_ul.is_none() {
            return Err(ProtocolError::TipNotAttached {
                pipette: self.pipettes[pipette.0].id.clone(),
            });
        }
        Ok(())
    }

    fn require_on_deck(&self, labware: LabwareId) -> Result<(), ProtocolError> {
        match self.labware[labware.0].placement {
            Placement::Disposed => Err(ProtocolError::LabwareDisposed {
                labware: self.labware[labware.0].id.clone(),
            }),
            Placement::Slot(_) | Placement::OnModule(_) => Ok(()),
        }
    }

    fn require_movable(&self, labware: LabwareId) -> Result<(), ProtocolError> {
        self.require_on_deck(labware)?;
        if let Placement::OnModule(module) = self.labware[labware.0].placement {
            let module = &self.modules[module];
            if module.model == ModuleModel::ThermocyclerModuleV2 && !module.lid_open {
                return Err(ProtocolError::ThermocyclerLidClosed {
                    labware: self.labware[labware.0].id.clone(),
                });
            }
            if module.model == ModuleModel::HeaterShakerModuleV1 && !module.latch_open {
                return Err(ProtocolError::HeaterShakerLatchClosedForMove {
                    labware: self.labware[labware.0].id.clone(),
                });
            }
        }
        Ok(())
    }

    fn vacate(&mut self, labware: LabwareId) {
        match self.labware[labware.0].placement {
            Placement::Slot(slot) => {
                self.occupied.remove(&slot);
            }
            Placement::OnModule(module) => {
                self.modules[module].labware = None;
            }
            Placement::Disposed => {}
        }
    }

    /// Checks common to aspirate/dispense/blowout: an attached tip, a liquid
    /// well on an accessible labware, and a positive flow rate.
    fn require_liquid_access(
        &self,
        pipette: PipetteId,
        labware: LabwareId,
        well: &str,
        flow_rate_ul_s: f64,
    ) -> Result<(), ProtocolError> {
        self.require_tip(pipette)?;
        if self.labware[labware.0].definition.is_tip_rack() {
            return Err(ProtocolError::IsATipRack {
                labware: self.labware[labware.0].id.clone(),
            });
        }
        self.require_well(labware, well)?;
        if flow_rate_ul_s <= 0.0 {
            return Err(ProtocolError::NonPositiveFlowRate {
                found: flow_rate_ul_s,
            });
        }
        self.require_reachable(labware)
    }

    /// Module-state gates on reaching a labware's wells: an open thermocycler
    /// lid (`ThermocyclerNotOpenError`), a closed and stationary heater-shaker
    /// (`PipetteMovementRestrictedByHeaterShakerError`), and no shaking
    /// heater-shaker in a horizontally adjacent slot.
    fn require_reachable(&self, labware: LabwareId) -> Result<(), ProtocolError> {
        self.require_on_deck(labware)?;
        let slot = match self.labware[labware.0].placement {
            Placement::OnModule(module_index) => {
                let module = &self.modules[module_index];
                if module.model == ModuleModel::ThermocyclerModuleV2 && !module.lid_open {
                    return Err(ProtocolError::ThermocyclerLidClosed {
                        labware: self.labware[labware.0].id.clone(),
                    });
                }
                if module.model == ModuleModel::HeaterShakerModuleV1 {
                    if module.shaking {
                        return Err(ProtocolError::HeaterShakerShaking);
                    }
                    if module.latch_open {
                        return Err(ProtocolError::HeaterShakerLatchOpenForPipetting {
                            labware: self.labware[labware.0].id.clone(),
                        });
                    }
                }
                module.slot
            }
            Placement::Slot(slot) => slot,
            Placement::Disposed => {
                unreachable!("require_on_deck rejected labware that is not on the deck")
            }
        };
        let beside_shaker = self.modules.iter().any(|module| {
            module.model == ModuleModel::HeaterShakerModuleV1
                && module.shaking
                && module.slot.horizontal_neighbors().contains(&slot)
        });
        if beside_shaker {
            return Err(ProtocolError::HeaterShakerShaking);
        }
        Ok(())
    }
}

fn require_temperature(
    device: &'static str,
    celsius: f64,
    minimum: f64,
    maximum: f64,
) -> Result<(), ProtocolError> {
    if !(minimum..=maximum).contains(&celsius) {
        return Err(ProtocolError::TemperatureOutOfRange {
            device,
            celsius,
            minimum,
            maximum,
        });
    }
    Ok(())
}

fn is_display_color(color: &str) -> bool {
    let Some(digits) = color.strip_prefix('#') else {
        return false;
    };
    (digits.len() == 6 || digits.len() == 8) && digits.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests;
