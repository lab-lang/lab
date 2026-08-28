//! Interpreters for Lab's run documents.
//!
//! A run document (see `lab-runfmt`) is a reviewed execution boundary, and
//! everything in this crate interprets those documents without ever
//! planning or deriving new work. Two execution modes share one node walk:
//!
//! - **live execution** (`lab run`) drives real stations on a wall clock;
//! - **dry run** validates every document and narrates the walk without
//!   touching hardware.
//!
//! The walk is parameterized over four ports: a [`clock::Clock`], an
//! [`operator::Operator`] for confirmations, an [`events::EventSink`] for
//! narration, and a [`stations::Connector`] that opens station sessions.

pub mod clock;
pub mod events;
pub mod execution;
pub mod ledger;
pub mod operator;
pub mod star;
pub mod stations;
pub mod workcell;

#[cfg(test)]
pub(crate) mod testing;

pub use hamilton_star;
