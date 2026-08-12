//! Interpreters for Lab's run documents.
//!
//! A run document (see `lab-runfmt`) is a reviewed execution boundary, and
//! everything in this crate interprets those documents without ever
//! planning or deriving new work. Three interpreters share one node walk:
//!
//! - **live execution** (`lab run`) drives real stations on a wall clock;
//! - **dry run** validates every document and narrates the walk;
//! - **simulation** (`lab simulate`) drives simulated stations on a
//!   virtual clock and records a `lab.sim-trace.v0` trace.
//!
//! The walk is parameterized over four ports: a [`clock::Clock`], an
//! [`operator::Operator`] for confirmations, an [`events::EventSink`] for
//! narration and traces, and a [`stations::Connector`] that opens station
//! sessions. The live and simulated interpreters differ only in which
//! implementations they plug in.

pub mod clock;
pub mod durations;
pub mod events;
pub use lab_runfmt::facility;
pub mod ledger;
pub mod operator;
pub mod simulate;
pub mod star;
pub mod stations;
pub mod trace;
pub mod workcell;

#[cfg(test)]
pub(crate) mod testing;

pub use hamilton_star;
