//! STAR-local planning from one exact allocated Procedure task to a run document.

mod choreograph;
mod error;
mod execution;
mod invocation;
mod liquids;

pub(in crate::backend::hamilton::star) use choreograph::{
    FluidPathOperation, RunBuilder, TipFeeder, Transfer,
};
pub(in crate::backend::hamilton::star) use error::StarEmissionError;
pub(in crate::backend::hamilton::star) use execution::{
    ChannelLiquid, SourceFill, StarExecutionPlan, StarOperation, StarRunPlan, StarWell, TipClass,
    TipPickupPosition,
};
pub(in crate::backend::hamilton::star) use invocation::{
    execution_plan, seeded_liquids, source_fill, tip_usage,
};
pub(in crate::backend::hamilton::star) use liquids::{
    DeckIndex, LiquidState, PLATE_DEAD_VOLUME_UL, TUBE_DEAD_VOLUME_UL,
};
