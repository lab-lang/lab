//! STAR-local planning from one exact allocated Procedure task to a run document.

mod choreograph;
mod error;
mod execution;
mod invocation;
mod liquids;

pub(in crate::backend::hamilton::star) use error::StarEmissionError;
pub(in crate::backend::hamilton::star) use execution::{
    ChannelLiquid, SourceFill, StarExecutionPlan, StarOperation, StarRunPlan, TipClass,
    TipPickupPosition,
};
pub(in crate::backend::hamilton::star) use invocation::{
    SetupAddition, plan_dilution_invocation, plan_setup_invocation,
};
