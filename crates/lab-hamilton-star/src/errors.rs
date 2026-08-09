//! Firmware error decoding: the complete master error-code and per-module
//! trace tables, and the mapping from a reply's error section to typed
//! errors.
//!
//! Master (`C0`) replies embed `er<code:2>/<trace:2>`; code `99` means the
//! failure happened in a slave module and the per-module entries that follow
//! carry the real cause. Slave-direct replies carry only `er<trace:2>`.

use crate::framing::Module;
use crate::response::{ErrorSection, ModuleEntry};

/// A master error code: the two digits before the slash in `er##/##`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MasterErrorCode {
    #[error("command syntax error")]
    CommandSyntax,
    #[error("hardware error: a drive is blocked or the power is low")]
    Hardware,
    #[error("command not completed because a previous sequence errored")]
    NotCompletedAfterPreviousError,
    #[error("a clot was detected")]
    ClotDetected,
    #[error("a barcode could not be read")]
    BarcodeUnreadable,
    #[error("too little liquid, or liquid level detection was not met")]
    TooLittleLiquid,
    #[error("a tip is already fitted; discard it before picking up another")]
    TipAlreadyFitted,
    #[error("no tip is fitted; pick one up before pipetting")]
    NoTips,
    #[error("no carrier is present")]
    NoCarrier,
    #[error("command not completed: it was aborted")]
    Aborted,
    #[error("dispensing with pressure liquid level detection is not permitted")]
    DispenseWithPressureLldNotPermitted,
    #[error("no Teach-In signal was received")]
    NoTeachInSignal,
    #[error("loading-tray error")]
    LoadingTray,
    #[error("sequenced aspiration with pressure liquid level detection is not permitted")]
    SequencedAspirationWithPressureLldNotPermitted,
    #[error("the parameter combination is not allowed")]
    DisallowedParameterCombination,
    #[error("the cover failed to close")]
    CoverClose,
    #[error("aspiration error")]
    Aspiration,
    #[error("wash-fluid or waste error")]
    WashFluidOrWaste,
    #[error("incubation temperature is out of limit")]
    IncubationTemperatureOutOfLimit,
    #[error("the TADM pressure trace overshot its limit curve")]
    TadmOvershoot,
    #[error("no element was detected")]
    NoElementDetected,
    #[error("an element is still being held")]
    ElementStillHolding,
    #[error("the held element was lost")]
    ElementLost,
    #[error("the iSWAP target plate position is illegal")]
    IllegalTargetPlatePosition,
    #[error("illegal user access caused an immediate stop")]
    IllegalUserAccess,
    #[error("the requested position is not reachable")]
    PositionNotReachable,
    #[error("unexpected liquid level detection")]
    UnexpectedLld,
    #[error("the deck area is already occupied")]
    AreaAlreadyOccupied,
    #[error("the deck area cannot be occupied")]
    ImpossibleToOccupyArea,
    #[error("anti-drop control is out of tolerance")]
    AntiDropOutOfTolerance,
    #[error("decapper lock error")]
    DecapperLock,
    #[error("decapper handling error")]
    DecapperHandling,
    #[error("stop: the machine halted, typically because the hood is open")]
    Stop,
    #[error("a slave module reported the error; see the per-module entries")]
    Slave,
    #[error(
        "VENUS-layer error code {0} (carrier or labware barcode, LLD, volume tolerance, or kit-lot fault)"
    )]
    Venus(u8),
    #[error("unknown master error code {0}")]
    Unknown(u8),
}

impl MasterErrorCode {
    /// Decodes the two-digit (or three-digit VENUS) code.
    pub fn from_code(code: u8) -> MasterErrorCode {
        use MasterErrorCode::*;
        match code {
            1 => CommandSyntax,
            2 => Hardware,
            3 => NotCompletedAfterPreviousError,
            4 => ClotDetected,
            5 => BarcodeUnreadable,
            6 => TooLittleLiquid,
            7 => TipAlreadyFitted,
            8 => NoTips,
            9 => NoCarrier,
            10 => Aborted,
            11 => DispenseWithPressureLldNotPermitted,
            12 => NoTeachInSignal,
            13 => LoadingTray,
            14 => SequencedAspirationWithPressureLldNotPermitted,
            15 => DisallowedParameterCombination,
            16 => CoverClose,
            17 => Aspiration,
            18 => WashFluidOrWaste,
            19 => IncubationTemperatureOutOfLimit,
            20 | 26 => TadmOvershoot,
            21 => NoElementDetected,
            22 => ElementStillHolding,
            23 => ElementLost,
            24 => IllegalTargetPlatePosition,
            25 => IllegalUserAccess,
            27 => PositionNotReachable,
            28 => UnexpectedLld,
            29 => AreaAlreadyOccupied,
            30 => ImpossibleToOccupyArea,
            31 => AntiDropOutOfTolerance,
            32 => DecapperLock,
            33 => DecapperHandling,
            36 => Stop,
            99 => Slave,
            100..=113 => Venus(code),
            other => Unknown(other),
        }
    }
}

/// A `C0` master trace code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MasterTrace {
    #[error("no trace")]
    None,
    #[error("CAN bus error")]
    CanBus,
    #[error("a slave command timed out")]
    SlaveCommandTimeout,
    #[error("E2PROM error")]
    E2prom,
    #[error("unknown command")]
    UnknownCommand,
    #[error("unknown parameter")]
    UnknownParameter,
    #[error("a parameter is out of range")]
    ParameterOutOfRange,
    #[error("a parameter does not belong to the command, or not all parameters were sent")]
    ParameterMismatch,
    #[error("node name unknown")]
    NodeNameUnknown,
    #[error("id parameter error")]
    IdParameter,
    #[error("node name defined twice")]
    NodeNameDefinedTwice,
    #[error("faulty XL channel settings")]
    FaultyXlChannelSettings,
    #[error("faulty robotic channel settings")]
    FaultyRoboticChannelSettings,
    #[error("the {0} is busy and accepts no parallel command")]
    Busy(&'static str),
    #[error("the carrier sensor is faulty")]
    CarrierSensorFaulty,
    #[error("unknown master trace code {0}")]
    Unknown(u8),
}

impl MasterTrace {
    pub fn from_code(code: u8) -> MasterTrace {
        use MasterTrace::*;
        match code {
            0 => None,
            10 => CanBus,
            11 => SlaveCommandTimeout,
            20 => E2prom,
            30 => UnknownCommand,
            31 => UnknownParameter,
            32 => ParameterOutOfRange,
            33 => ParameterMismatch,
            34 => NodeNameUnknown,
            35 => IdParameter,
            37 => NodeNameDefinedTwice,
            38 => FaultyXlChannelSettings,
            39 => FaultyRoboticChannelSettings,
            40 => Busy("pipetting channel task"),
            41 => Busy("autoload task"),
            42 => Busy("miscellaneous task"),
            43 => Busy("incubator task"),
            44 => Busy("washer task"),
            45 => Busy("iSWAP task"),
            46 => Busy("CoRe 96 head task"),
            47 => CarrierSensorFaulty,
            48 => Busy("CoRe 384 head task"),
            49 => Busy("nano-pipettor task"),
            50 => Busy("XL channel task"),
            51 => Busy("tube gripper task"),
            52 => Busy("imaging channel task"),
            53 => Busy("robotic channel task"),
            other => Unknown(other),
        }
    }
}

/// A pipetting-channel (`P1`–`PG`) trace code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ChannelTrace {
    #[error("no trace")]
    None,
    #[error("EEPROM communication error")]
    EepromComms,
    #[error("unknown command")]
    UnknownCommand,
    #[error("unknown parameter")]
    UnknownParameter,
    #[error("a parameter is out of range")]
    ParameterOutOfRange,
    #[error("voltage error")]
    Voltage,
    #[error("stop during command execution")]
    StopDuringExecution,
    #[error("the channel accepts no parallel processes")]
    NoParallelProcesses,
    #[error("dispensing drive initialization position not found")]
    DispensingDriveInitNotFound,
    #[error("dispensing drive not initialized")]
    DispensingDriveNotInitialized,
    #[error("dispensing drive movement error")]
    DispensingDriveMovement,
    #[error("the maximum tip volume was reached")]
    MaxTipVolumeReached,
    #[error("the position is outside the permitted area")]
    PositionOutsidePermittedArea,
    #[error("Y drive blocked")]
    YDriveBlocked,
    #[error("Y drive not initialized")]
    YDriveNotInitialized,
    #[error("Y drive movement error")]
    YDriveMovement,
    #[error("Z drive blocked")]
    ZDriveBlocked,
    #[error("Z drive not initialized")]
    ZDriveNotInitialized,
    #[error("Z drive movement error")]
    ZDriveMovement,
    #[error("Z drive limit stop not found")]
    ZDriveLimitStopNotFound,
    #[error("squeezer drive error (trace {0})")]
    SqueezerDrive(u8),
    #[error("no liquid level was found")]
    NoLiquidLevelFound,
    #[error("not enough liquid")]
    NotEnoughLiquid,
    #[error("pressure-sensor auto-calibration failed")]
    PressureSensorCalibration,
    #[error("dual liquid level detection found no level")]
    DualLldNoLevel,
    #[error("liquid detected at a position where none is allowed")]
    LiquidAtNotAllowedPosition,
    #[error("no tip has been picked up")]
    NoTipPickedUp,
    #[error("a tip has already been picked up")]
    TipAlreadyPickedUp,
    #[error("the tip was not dropped")]
    TipNotDropped,
    #[error("the wrong tip was picked up")]
    WrongTipPickedUp,
    #[error("the liquid was not correctly aspirated")]
    LiquidNotCorrectlyAspirated,
    #[error("a clot was detected")]
    ClotDetected,
    #[error("the TADM pressure trace fell below its limit curve")]
    TadmBelowLimitCurve,
    #[error("the TADM pressure trace rose above its limit curve")]
    TadmAboveLimitCurve,
    #[error("TADM memory error")]
    TadmMemory,
    #[error("digital potentiometer communication error")]
    PotentiometerComms,
    #[error("ADC algorithm error")]
    AdcAlgorithm,
    #[error("the second liquid phase was not found")]
    SecondPhaseNotFound,
    #[error("immersion is below the minimal range")]
    ImmersionBelowMinimalRange,
    #[error("limit-curve storage error (trace {0})")]
    LimitCurveStorage(u8),
    #[error("unknown channel trace code {0}")]
    Unknown(u8),
}

impl ChannelTrace {
    pub fn from_code(code: u8) -> ChannelTrace {
        use ChannelTrace::*;
        match code {
            0 => None,
            20 => EepromComms,
            30 => UnknownCommand,
            31 => UnknownParameter,
            32 => ParameterOutOfRange,
            35 => Voltage,
            36 | 37 => StopDuringExecution,
            40 => NoParallelProcesses,
            50 => DispensingDriveInitNotFound,
            51 => DispensingDriveNotInitialized,
            52 => DispensingDriveMovement,
            53 => MaxTipVolumeReached,
            54 => PositionOutsidePermittedArea,
            55 => YDriveBlocked,
            56 => YDriveNotInitialized,
            57 => YDriveMovement,
            60 => ZDriveBlocked,
            61 => ZDriveNotInitialized,
            62 => ZDriveMovement,
            63 => ZDriveLimitStopNotFound,
            65..=68 => SqueezerDrive(code),
            70 => NoLiquidLevelFound,
            71 => NotEnoughLiquid,
            72 => PressureSensorCalibration,
            73 => DualLldNoLevel,
            74 => LiquidAtNotAllowedPosition,
            75 => NoTipPickedUp,
            76 => TipAlreadyPickedUp,
            77 => TipNotDropped,
            78 => WrongTipPickedUp,
            80 => LiquidNotCorrectlyAspirated,
            81 => ClotDetected,
            82 => TadmBelowLimitCurve,
            83 => TadmAboveLimitCurve,
            84 => TadmMemory,
            85 => PotentiometerComms,
            86 => AdcAlgorithm,
            87 => SecondPhaseNotFound,
            88 => ImmersionBelowMinimalRange,
            90..=96 => LimitCurveStorage(code),
            other => Unknown(other),
        }
    }
}

/// A CoRe 96 head (`H0`) trace code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Head96Trace {
    #[error("no trace")]
    None,
    #[error("communication error (trace {0})")]
    Comms(u8),
    #[error("flash EPROM error (trace {0})")]
    FlashEprom(u8),
    #[error("unknown command")]
    UnknownCommand,
    #[error("unknown parameter")]
    UnknownParameter,
    #[error("a parameter is out of range")]
    ParameterOutOfRange,
    #[error("voltage error")]
    Voltage,
    #[error("stop during command execution")]
    StopDuringExecution,
    #[error("adjustment sensor error")]
    AdjustmentSensor,
    #[error("the head accepts no parallel processes")]
    NoParallelProcesses,
    #[error("dispensing drive initialization position not found")]
    DispensingDriveInitNotFound,
    #[error("dispensing drive not initialized")]
    DispensingDriveNotInitialized,
    #[error("dispensing drive movement error")]
    DispensingDriveMovement,
    #[error("the maximum tip volume was reached")]
    MaxTipVolumeReached,
    #[error("the position is outside the permitted area")]
    PositionOutsidePermittedArea,
    #[error("Y drive error (trace {0})")]
    YDrive(u8),
    #[error("Z drive error (trace {0})")]
    ZDrive(u8),
    #[error("squeezer drive error (trace {0})")]
    SqueezerDrive(u8),
    #[error("no liquid level was found")]
    NoLiquidLevelFound,
    #[error("not enough liquid")]
    NotEnoughLiquid,
    #[error("no tip has been picked up")]
    NoTipPickedUp,
    #[error("a tip has already been picked up")]
    TipAlreadyPickedUp,
    #[error("a clot was detected")]
    ClotDetected,
    #[error("TADM error (trace {0})")]
    Tadm(u8),
    #[error("limit-curve storage error (trace {0})")]
    LimitCurveStorage(u8),
    #[error("unknown 96-head trace code {0}")]
    Unknown(u8),
}

impl Head96Trace {
    pub fn from_code(code: u8) -> Head96Trace {
        use Head96Trace::*;
        match code {
            0 => None,
            20 | 21 => Comms(code),
            25..=28 => FlashEprom(code),
            30 => UnknownCommand,
            31 => UnknownParameter,
            32 => ParameterOutOfRange,
            35 => Voltage,
            36 => StopDuringExecution,
            37 => AdjustmentSensor,
            40 => NoParallelProcesses,
            50 => DispensingDriveInitNotFound,
            51 => DispensingDriveNotInitialized,
            52 => DispensingDriveMovement,
            53 => MaxTipVolumeReached,
            54 => PositionOutsidePermittedArea,
            55..=58 => YDrive(code),
            60..=63 => ZDrive(code),
            65..=68 => SqueezerDrive(code),
            70 => NoLiquidLevelFound,
            71 => NotEnoughLiquid,
            75 => NoTipPickedUp,
            76 => TipAlreadyPickedUp,
            81 => ClotDetected,
            82..=84 => Tadm(code),
            90..=96 => LimitCurveStorage(code),
            other => Unknown(other),
        }
    }
}

/// An iSWAP (`R0`) trace code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IswapTrace {
    #[error("no trace")]
    None,
    #[error("EEPROM error")]
    Eeprom,
    #[error("unknown command")]
    UnknownCommand,
    #[error("unknown parameter")]
    UnknownParameter,
    #[error("a parameter is out of range")]
    ParameterOutOfRange,
    #[error("firmware and hardware versions do not match")]
    FirmwareHardwareMismatch,
    #[error("stop during command execution")]
    StopDuringExecution,
    #[error("adjustment sensor error (trace {0})")]
    AdjustmentSensor(u8),
    #[error("the iSWAP accepts no parallel processes (trace {0})")]
    NoParallelProcesses(u8),
    #[error("Y drive error (trace {0})")]
    YDrive(u8),
    #[error("Z drive error (trace {0})")]
    ZDrive(u8),
    #[error("rotation drive error (trace {0})")]
    RotationDrive(u8),
    #[error("wrist drive error (trace {0})")]
    WristDrive(u8),
    #[error("gripper DMS potentiometer error (trace {0})")]
    GripperPotentiometer(u8),
    #[error("the gripper locked while gripping")]
    GripperLockedDuringGrip,
    #[error("gripper initialization failed")]
    GripperInitFailed,
    #[error("the iSWAP is not initialized")]
    NotInitialized,
    #[error("the gripper locked while releasing")]
    GripperLockedDuringRelease,
    #[error("gripper counter overflow")]
    GripperCounterOverflow,
    #[error("the plate was not found")]
    PlateNotFound,
    #[error("the plate is not available")]
    PlateNotAvailable,
    #[error("an unexpected object was found")]
    UnexpectedObjectFound,
    #[error("unknown iSWAP trace code {0}")]
    Unknown(u8),
}

impl IswapTrace {
    pub fn from_code(code: u8) -> IswapTrace {
        use IswapTrace::*;
        match code {
            0 => None,
            20 => Eeprom,
            30 => UnknownCommand,
            31 => UnknownParameter,
            32 => ParameterOutOfRange,
            33 => FirmwareHardwareMismatch,
            36 => StopDuringExecution,
            37 | 38 => AdjustmentSensor(code),
            40..=42 => NoParallelProcesses(code),
            50..=53 => YDrive(code),
            60..=63 => ZDrive(code),
            70..=73 => RotationDrive(code),
            80..=83 => WristDrive(code),
            85 | 86 => GripperPotentiometer(code),
            89 => GripperLockedDuringGrip,
            90 => GripperInitFailed,
            91 => NotInitialized,
            92 => GripperLockedDuringRelease,
            93 => GripperCounterOverflow,
            94 => PlateNotFound,
            96 => PlateNotAvailable,
            97 => UnexpectedObjectFound,
            other => Unknown(other),
        }
    }
}

/// An autoload (`I0`) trace code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AutoloadTrace {
    #[error("no trace")]
    None,
    #[error("EEPROM error")]
    Eeprom,
    #[error("unknown command")]
    UnknownCommand,
    #[error("unknown parameter")]
    UnknownParameter,
    #[error("a parameter is out of range")]
    ParameterOutOfRange,
    #[error("voltage error")]
    Voltage,
    #[error("the hood is open; the machine stops until it is closed")]
    HoodOpen,
    #[error("the autoload accepts no parallel processes")]
    NoParallelProcesses,
    #[error("scanner X drive error (trace {0})")]
    ScannerXDrive(u8),
    #[error("scanner rotation drive blocked")]
    ScannerRotationBlocked,
    #[error("carrier Y drive error (trace {0})")]
    CarrierYDrive(u8),
    #[error("carrier Z drive error (trace {0})")]
    CarrierZDrive(u8),
    #[error("barcode-scanner communication error")]
    BarcodeScannerComms,
    #[error("loading-LED communication error")]
    LoadingLedComms,
    #[error("the identification barcode could not be read")]
    IdBarcodeUnreadable,
    #[error("no carrier is present")]
    NoCarrierPresent,
    #[error("no carrier is loaded")]
    NoCarrierLoaded,
    #[error("the loading tray is occupied")]
    LoadingTrayOccupied,
    #[error("free-definable-carrier data is incorrect")]
    FreeDefinableCarrierData,
    #[error("unknown autoload trace code {0}")]
    Unknown(u8),
}

impl AutoloadTrace {
    pub fn from_code(code: u8) -> AutoloadTrace {
        use AutoloadTrace::*;
        match code {
            0 => None,
            20 => Eeprom,
            30 => UnknownCommand,
            31 => UnknownParameter,
            32 => ParameterOutOfRange,
            35 => Voltage,
            36 => HoodOpen,
            40 => NoParallelProcesses,
            50..=52 => ScannerXDrive(code),
            55 => ScannerRotationBlocked,
            60..=62 => CarrierYDrive(code),
            65..=67 => CarrierZDrive(code),
            70 => BarcodeScannerComms,
            75 => LoadingLedComms,
            80 => IdBarcodeUnreadable,
            81 => NoCarrierPresent,
            82 => NoCarrierLoaded,
            83 => LoadingTrayOccupied,
            84 => FreeDefinableCarrierData,
            other => Unknown(other),
        }
    }
}

/// An X-drive (`X0`) trace code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum XDrivesTrace {
    #[error("no trace")]
    None,
    #[error("I2C/EEPROM error")]
    I2cEeprom,
    #[error("flash EPROM error (trace {0})")]
    FlashEprom(u8),
    #[error("unknown command")]
    UnknownCommand,
    #[error("unknown parameter")]
    UnknownParameter,
    #[error("a parameter is out of range")]
    ParameterOutOfRange,
    #[error("voltage error")]
    Voltage,
    #[error("stop during command execution")]
    StopDuringExecution,
    #[error("the X drives accept no parallel processes (trace {0})")]
    NoParallelProcesses(u8),
    #[error(
        "X drive 1 error: initialization, blockage, displacement, or dispense-on-fly fault (trace {0})"
    )]
    XDrive1(u8),
    #[error(
        "X drive 2 error: initialization, blockage, displacement, or dispense-on-fly fault (trace {0})"
    )]
    XDrive2(u8),
    #[error("reserve drive error (trace {0})")]
    ReserveDrive(u8),
    #[error("unknown X-drive trace code {0}")]
    Unknown(u8),
}

impl XDrivesTrace {
    pub fn from_code(code: u8) -> XDrivesTrace {
        use XDrivesTrace::*;
        match code {
            0 => None,
            20 => I2cEeprom,
            25..=28 => FlashEprom(code),
            30 => UnknownCommand,
            31 => UnknownParameter,
            32 => ParameterOutOfRange,
            35 => Voltage,
            36 => StopDuringExecution,
            40..=42 => NoParallelProcesses(code),
            50..=55 => XDrive1(code),
            70..=75 => XDrive2(code),
            80..=82 => ReserveDrive(code),
            other => Unknown(other),
        }
    }
}

/// A trace code decoded per the reporting module's table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Trace {
    #[error("{0}")]
    Master(MasterTrace),
    #[error("{0}")]
    Channel(ChannelTrace),
    #[error("{0}")]
    Head96(Head96Trace),
    #[error("{0}")]
    Iswap(IswapTrace),
    #[error("{0}")]
    Autoload(AutoloadTrace),
    #[error("{0}")]
    XDrives(XDrivesTrace),
    #[error("trace code {code}")]
    Other { code: u8 },
}

impl Trace {
    /// Decodes a trace code using the table for the given module address.
    pub fn decode(address: &str, code: u8) -> Trace {
        match Module::from_address(address) {
            Some(Module::Master) => Trace::Master(MasterTrace::from_code(code)),
            Some(Module::PipettingChannel(_)) => Trace::Channel(ChannelTrace::from_code(code)),
            Some(Module::Head96) => Trace::Head96(Head96Trace::from_code(code)),
            Some(Module::Iswap) => Trace::Iswap(IswapTrace::from_code(code)),
            Some(Module::Autoload) => Trace::Autoload(AutoloadTrace::from_code(code)),
            Some(Module::XDrives) => Trace::XDrives(XDrivesTrace::from_code(code)),
            _ => Trace::Other { code },
        }
    }

    /// The raw trace code.
    pub fn code(&self) -> u8 {
        match *self {
            Trace::Master(t) => master_trace_code(t),
            Trace::Channel(t) => channel_trace_code(t),
            Trace::Head96(t) => head96_trace_code(t),
            Trace::Iswap(t) => iswap_trace_code(t),
            Trace::Autoload(t) => autoload_trace_code(t),
            Trace::XDrives(t) => xdrives_trace_code(t),
            Trace::Other { code } => code,
        }
    }
}

// The raw-code accessors below exist because a decoded trace still needs to
// surface its numeric code in messages and in the trace-31 follow-up logic.
fn master_trace_code(trace: MasterTrace) -> u8 {
    (0..=255u8)
        .find(|&c| MasterTrace::from_code(c) == trace)
        .unwrap_or(0)
}
fn channel_trace_code(trace: ChannelTrace) -> u8 {
    match trace {
        ChannelTrace::SqueezerDrive(c)
        | ChannelTrace::LimitCurveStorage(c)
        | ChannelTrace::Unknown(c) => c,
        other => (0..=255u8)
            .find(|&c| ChannelTrace::from_code(c) == other)
            .unwrap_or(0),
    }
}
fn head96_trace_code(trace: Head96Trace) -> u8 {
    match trace {
        Head96Trace::Comms(c)
        | Head96Trace::FlashEprom(c)
        | Head96Trace::YDrive(c)
        | Head96Trace::ZDrive(c)
        | Head96Trace::SqueezerDrive(c)
        | Head96Trace::Tadm(c)
        | Head96Trace::LimitCurveStorage(c)
        | Head96Trace::Unknown(c) => c,
        other => (0..=255u8)
            .find(|&c| Head96Trace::from_code(c) == other)
            .unwrap_or(0),
    }
}
fn iswap_trace_code(trace: IswapTrace) -> u8 {
    match trace {
        IswapTrace::AdjustmentSensor(c)
        | IswapTrace::NoParallelProcesses(c)
        | IswapTrace::YDrive(c)
        | IswapTrace::ZDrive(c)
        | IswapTrace::RotationDrive(c)
        | IswapTrace::WristDrive(c)
        | IswapTrace::GripperPotentiometer(c)
        | IswapTrace::Unknown(c) => c,
        other => (0..=255u8)
            .find(|&c| IswapTrace::from_code(c) == other)
            .unwrap_or(0),
    }
}
fn autoload_trace_code(trace: AutoloadTrace) -> u8 {
    match trace {
        AutoloadTrace::ScannerXDrive(c)
        | AutoloadTrace::CarrierYDrive(c)
        | AutoloadTrace::CarrierZDrive(c)
        | AutoloadTrace::Unknown(c) => c,
        other => (0..=255u8)
            .find(|&c| AutoloadTrace::from_code(c) == other)
            .unwrap_or(0),
    }
}
fn xdrives_trace_code(trace: XDrivesTrace) -> u8 {
    match trace {
        XDrivesTrace::FlashEprom(c)
        | XDrivesTrace::NoParallelProcesses(c)
        | XDrivesTrace::XDrive1(c)
        | XDrivesTrace::XDrive2(c)
        | XDrivesTrace::ReserveDrive(c)
        | XDrivesTrace::Unknown(c) => c,
        other => (0..=255u8)
            .find(|&c| XDrivesTrace::from_code(c) == other)
            .unwrap_or(0),
    }
}

/// One module's error from a master reply's per-module entries.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("module {address}{channel_label} reports: {code}; trace: {trace}", channel_label = channel_label(*.channel))]
pub struct ModuleError {
    /// The two-character module address that reported the error.
    pub address: String,
    /// The 0-based channel index when the module is a pipetting channel.
    pub channel: Option<usize>,
    pub code: MasterErrorCode,
    pub trace: Trace,
}

fn channel_label(channel: Option<usize>) -> String {
    match channel {
        Some(i) => format!(" (pipetting channel {})", i + 1),
        None => String::new(),
    }
}

/// The semantic condition a firmware error maps to, when one of the
/// documented mappings applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Semantic {
    /// Master code 07 or channel trace 76.
    TipAlreadyFitted,
    /// Master code 08 or channel trace 75.
    NoTipFitted,
    /// Channel traces 70 and 71.
    InsufficientLiquid,
    /// Channel trace 54 at the dispensing drive: the volume exceeds the tip.
    VolumeExceedsTip,
    /// Trace 40: the module is busy and rejects parallel commands.
    ModuleBusy,
}

/// A decoded firmware error from any reply.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FirmwareError {
    #[error("the master controller reports: {code}; trace: {trace}")]
    Master {
        code: MasterErrorCode,
        trace: MasterTrace,
    },
    #[error("{}", format_module_errors(.0))]
    Modules(Vec<ModuleError>),
    #[error("module {module} reports trace: {trace}")]
    SlaveDirect { module: String, trace: Trace },
    #[error("the hood is open; close it before the machine will move again")]
    HoodOpen,
}

fn format_module_errors(errors: &[ModuleError]) -> String {
    errors
        .iter()
        .map(ModuleError::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

impl FirmwareError {
    /// Decodes a reply's error section: `Ok(())` on success, the typed error
    /// otherwise. `module_address` is the module the reply came from, which
    /// selects the trace table for slave-direct replies.
    pub fn from_section(section: &ErrorSection, module_address: &str) -> Result<(), FirmwareError> {
        match section {
            ErrorSection::Master {
                code: 0,
                trace: 0,
                modules,
            } if modules.iter().all(|m| m.code == 0 && m.trace == 0) => Ok(()),
            ErrorSection::Master {
                code,
                trace,
                modules,
            } => {
                let failed: Vec<&ModuleEntry> = modules
                    .iter()
                    .filter(|m| m.code != 0 || m.trace != 0)
                    .collect();
                if failed.iter().any(|m| m.address == "I0" && m.trace == 36) {
                    return Err(FirmwareError::HoodOpen);
                }
                // Master code 99 delegates to the slave entries; drop the
                // master's own entry and surface the modules that failed.
                if *code == 99 || (*code == 0 && *trace == 0) {
                    let errors = failed
                        .iter()
                        .map(|entry| decode_module_entry(entry))
                        .collect();
                    Err(FirmwareError::Modules(errors))
                } else {
                    Err(FirmwareError::Master {
                        code: MasterErrorCode::from_code(*code),
                        trace: MasterTrace::from_code(*trace),
                    })
                }
            }
            ErrorSection::Slave { trace: 0 } => Ok(()),
            ErrorSection::Slave { trace } => {
                if module_address == "I0" && *trace == 36 {
                    return Err(FirmwareError::HoodOpen);
                }
                Err(FirmwareError::SlaveDirect {
                    module: module_address.to_string(),
                    trace: Trace::decode(module_address, *trace),
                })
            }
        }
    }

    /// The semantic condition this error maps to, when a documented mapping
    /// applies. Multi-module errors map when every failed module agrees.
    pub fn semantic(&self) -> Option<Semantic> {
        fn from_code_and_trace(code: Option<MasterErrorCode>, trace: u8) -> Option<Semantic> {
            match (code, trace) {
                (Some(MasterErrorCode::TipAlreadyFitted), _) | (_, 76) => {
                    Some(Semantic::TipAlreadyFitted)
                }
                (Some(MasterErrorCode::NoTips), _) | (_, 75) => Some(Semantic::NoTipFitted),
                (_, 70) | (_, 71) => Some(Semantic::InsufficientLiquid),
                (_, 53) => Some(Semantic::VolumeExceedsTip),
                (_, 40) => Some(Semantic::ModuleBusy),
                _ => None,
            }
        }
        match self {
            FirmwareError::Master { code, trace } => {
                from_code_and_trace(Some(*code), master_trace_code(*trace))
            }
            FirmwareError::Modules(errors) => {
                let mut semantics = errors
                    .iter()
                    .map(|e| from_code_and_trace(Some(e.code), e.trace.code()));
                let first = semantics.next()??;
                semantics.all(|s| s == Some(first)).then_some(first)
            }
            FirmwareError::SlaveDirect { trace, .. } => from_code_and_trace(None, trace.code()),
            FirmwareError::HoodOpen => None,
        }
    }

    /// Whether any reported trace is 31 (unknown parameter), the condition
    /// that warrants a follow-up `C0 VP` query for the offending parameter
    /// name.
    pub fn has_unknown_parameter_trace(&self) -> bool {
        match self {
            FirmwareError::Master { trace, .. } => *trace == MasterTrace::UnknownParameter,
            FirmwareError::Modules(errors) => errors.iter().any(|e| e.trace.code() == 31),
            FirmwareError::SlaveDirect { trace, .. } => trace.code() == 31,
            FirmwareError::HoodOpen => false,
        }
    }
}

fn decode_module_entry(entry: &ModuleEntry) -> ModuleError {
    let channel = match Module::from_address(&entry.address) {
        Some(Module::PipettingChannel(i)) => Some(usize::from(i)),
        _ => None,
    };
    ModuleError {
        address: entry.address.clone(),
        channel,
        code: MasterErrorCode::from_code(entry.code),
        trace: Trace::decode(&entry.address, entry.trace),
    }
}

/// The error raised when a command constructor receives a value outside its
/// documented range.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error(
    "parameter {parameter} ({meaning}) is {value} {unit}; the firmware accepts {min} to {max} {unit}"
)]
pub struct OutOfRange {
    /// The two-character wire name of the parameter.
    pub parameter: &'static str,
    /// What the parameter controls.
    pub meaning: &'static str,
    pub value: f64,
    pub unit: &'static str,
    pub min: f64,
    pub max: f64,
}

/// Checks a value against a documented range, in the value's own unit.
pub fn check_range(
    parameter: &'static str,
    meaning: &'static str,
    unit: &'static str,
    value: f64,
    min: f64,
    max: f64,
) -> Result<(), OutOfRange> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(OutOfRange {
            parameter,
            meaning,
            value,
            unit,
            min,
            max,
        })
    }
}

/// The error raised when a typed command cannot be constructed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CommandError {
    #[error(transparent)]
    OutOfRange(#[from] OutOfRange),
    #[error(transparent)]
    Channels(#[from] crate::framing::ChannelValuesError),
    #[error("per-channel lists disagree: {message}")]
    InconsistentChannels { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response::split_error_section;

    fn section(payload: &str) -> ErrorSection {
        split_error_section(payload)
            .expect("the test payload parses")
            .0
            .expect("the test payload carries an error section")
    }

    #[test]
    fn a_success_section_decodes_to_ok() {
        assert_eq!(
            FirmwareError::from_section(&section("er00/00"), "C0"),
            Ok(()),
            "er00/00 is success"
        );
    }

    #[test]
    fn a_master_error_carries_the_documented_meaning() {
        let error = FirmwareError::from_section(&section("er07/00"), "C0")
            .expect_err("code 07 is an error");
        assert_eq!(
            error,
            FirmwareError::Master {
                code: MasterErrorCode::TipAlreadyFitted,
                trace: MasterTrace::None
            },
            "code 07 means a tip is already fitted"
        );
        assert_eq!(
            error.semantic(),
            Some(Semantic::TipAlreadyFitted),
            "the semantic mapping surfaces the condition"
        );
    }

    #[test]
    fn slave_code_99_surfaces_the_per_module_entries() {
        let error =
            FirmwareError::from_section(&section(" er99/00 P100/00 P235/00 P402/98 PG08/76"), "C0")
                .expect_err("code 99 delegates to failing modules");
        let FirmwareError::Modules(errors) = &error else {
            panic!("the master entry is dropped in favor of the module entries");
        };
        assert_eq!(
            errors.len(),
            3,
            "the 00/00 entry for P1 is success and is dropped"
        );
        assert_eq!(
            errors[0].channel,
            Some(1),
            "P2 (code 35) is pipetting channel index 1"
        );
        assert_eq!(
            errors[1].channel,
            Some(3),
            "P4 is pipetting channel index 3"
        );
        assert_eq!(
            errors[2],
            ModuleError {
                address: "PG".to_string(),
                channel: Some(15),
                code: MasterErrorCode::NoTips,
                trace: Trace::Channel(ChannelTrace::TipAlreadyPickedUp),
            },
            "each entry decodes with the channel trace table"
        );
    }

    #[test]
    fn a_slave_direct_trace_uses_the_reporting_modules_table() {
        let error =
            FirmwareError::from_section(&section("er75"), "P3").expect_err("trace 75 is an error");
        assert_eq!(
            error,
            FirmwareError::SlaveDirect {
                module: "P3".to_string(),
                trace: Trace::Channel(ChannelTrace::NoTipPickedUp)
            },
            "P-module traces decode with the channel table"
        );
        assert_eq!(
            error.semantic(),
            Some(Semantic::NoTipFitted),
            "trace 75 means no tip"
        );
    }

    #[test]
    fn autoload_trace_36_is_the_distinct_hood_open_stop() {
        let direct = FirmwareError::from_section(&section("er36"), "I0")
            .expect_err("trace 36 from I0 is an error");
        assert_eq!(
            direct,
            FirmwareError::HoodOpen,
            "a slave-direct I0 trace 36 is hood-open"
        );

        let embedded = FirmwareError::from_section(&section(" er99/00 I000/36"), "C0")
            .expect_err("an embedded I0 trace 36 is an error");
        assert_eq!(
            embedded,
            FirmwareError::HoodOpen,
            "the embedded form decodes the same way"
        );
    }

    #[test]
    fn trace_31_requests_the_faulty_parameter_follow_up() {
        let error = FirmwareError::from_section(&section("er01/31"), "C0")
            .expect_err("trace 31 is an error");
        assert!(
            error.has_unknown_parameter_trace(),
            "unknown-parameter errors trigger the C0 VP follow-up query"
        );
    }

    #[test]
    fn busy_traces_map_to_the_module_busy_semantic() {
        let error =
            FirmwareError::from_section(&section("er40"), "H0").expect_err("trace 40 is an error");
        assert_eq!(
            error.semantic(),
            Some(Semantic::ModuleBusy),
            "trace 40 is the concurrency-violation signal"
        );
    }

    #[test]
    fn out_of_range_errors_name_parameter_unit_and_range() {
        let error = check_range("zj", "channel Z position", "0.1 mm", 3400.0, 0.0, 3347.0)
            .expect_err("3400 exceeds the ceiling");
        assert_eq!(
            error.to_string(),
            "parameter zj (channel Z position) is 3400 0.1 mm; the firmware accepts 0 to 3347 0.1 mm",
            "the message names the parameter, its meaning, the unit, and the permitted range"
        );
    }
}
