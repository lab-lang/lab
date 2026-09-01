use thiserror::Error;

use crate::ProcedureLocalId;

/// One position, the volume a step moves, what it already holds, and the bound that is crossed.
///
/// Boxed because carrying these inline makes every `Result` in the crate pay for the widest error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VolumeConflict {
    pub step: ProcedureLocalId,
    pub vessel: ProcedureLocalId,
    pub position: u32,
    /// Volume the step moves into or out of the position.
    pub moved: String,
    /// Volume already present before the step.
    pub present: String,
    /// The dead volume or working capacity the step crosses.
    pub limit: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PipettingProgramValidationError {
    #[error("pipetting program contains no logical vessels")]
    NoVessels,
    #[error("pipetting program contains no operations")]
    NoSteps,
    #[error("pipetting program contains no liquid operations")]
    NoLiquidOperations,
    #[error("pipetting program repeats material `{material}`")]
    DuplicateMaterial { material: ProcedureLocalId },
    #[error("pipetting program repeats output `{output}`")]
    DuplicateOutput { output: ProcedureLocalId },
    #[error("pipetting program repeats vessel `{vessel}`")]
    DuplicateVessel { vessel: ProcedureLocalId },
    #[error("pipetting vessel `{vessel}` has no addressable positions")]
    EmptyVessel { vessel: ProcedureLocalId },
    #[error("pipetting vessel `{vessel}` refers to unknown material `{material}`")]
    UnknownMaterial {
        vessel: ProcedureLocalId,
        material: ProcedureLocalId,
    },
    #[error("pipetting vessel `{vessel}` refers to unknown output `{output}`")]
    UnknownOutput {
        vessel: ProcedureLocalId,
        output: ProcedureLocalId,
    },
    #[error("pipetting program repeats step `{step}`")]
    DuplicateStep { step: ProcedureLocalId },
    #[error("pipetting step `{step}` refers to unknown vessel `{vessel}`")]
    UnknownVessel {
        step: ProcedureLocalId,
        vessel: ProcedureLocalId,
    },
    #[error(
        "pipetting step `{step}` refers to position {position} outside vessel `{vessel}` with {positions} positions"
    )]
    PositionOutOfRange {
        step: ProcedureLocalId,
        vessel: ProcedureLocalId,
        position: u32,
        positions: u32,
    },
    #[error("pipetting step `{step}` has no targets")]
    EmptyTargets { step: ProcedureLocalId },
    #[error("pipetting step `{step}` repeats target `{vessel}` position {position}")]
    DuplicateTarget {
        step: ProcedureLocalId,
        vessel: ProcedureLocalId,
        position: u32,
    },
    #[error("pipetting step `{step}` transfers a location into itself")]
    SelfTransfer { step: ProcedureLocalId },
    #[error("pipetting step `{step}` has zero mix cycles")]
    ZeroMixCycles { step: ProcedureLocalId },
    #[error("pipetting barrier `{step}` has no reason")]
    EmptyBarrierReason { step: ProcedureLocalId },
    #[error(
        "pipetting step `{step}` withdraws {required} uL from `{vessel}` position {position}, which contains only {available} uL"
    )]
    InsufficientVolume {
        step: ProcedureLocalId,
        vessel: ProcedureLocalId,
        position: u32,
        required: String,
        available: String,
    },
    #[error(
        "pipetting step `{}` withdraws {} uL from `{}` position {}, leaving less than its {} uL dead volume from {} uL",
        .0.step, .0.moved, .0.vessel, .0.position, .0.limit, .0.present
    )]
    BelowDeadVolume(Box<VolumeConflict>),
    #[error(
        "pipetting step `{}` dispenses {} uL into `{}` position {}, taking it past its {} uL working capacity from {} uL",
        .0.step, .0.moved, .0.vessel, .0.position, .0.limit, .0.present
    )]
    ExceedsWorkingCapacity(Box<VolumeConflict>),
    #[error(
        "pipetting step `{step}` aspirates from `{vessel}`, which states no initial volume; only a material source may leave its fill to the adapter"
    )]
    UnvaluedSourceAspiration {
        step: ProcedureLocalId,
        vessel: ProcedureLocalId,
    },
    #[error(
        "pipetting step `{step}` tracks the liquid surface of `{vessel}`, which states no initial volume, so the planned surface cannot be computed"
    )]
    UntrackableSource {
        step: ProcedureLocalId,
        vessel: ProcedureLocalId,
    },
    #[error(
        "pipetting mix `{step}` requires {required} uL in `{vessel}` position {position}, which contains only {available} uL"
    )]
    InsufficientMixVolume {
        step: ProcedureLocalId,
        vessel: ProcedureLocalId,
        position: u32,
        required: String,
        available: String,
    },
}
