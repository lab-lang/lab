//! Interpreters for Lab's run documents.
//!
//! A run document (see `lab-runfmt`) is a reviewed execution boundary, and
//! everything in this crate interprets those documents without ever
//! planning or deriving new work. Two execution modes share one node walk:
//!
//! - **live execution** (`lab run`) drives exact bound Assets on a wall clock;
//! - **simulation** (`lab run --simulate`) uses no-hardware executors and mode-bound evidence.
//!
//! A dry run validates every document and narrates the walk without opening a ledger or touching
//! hardware.
//!
//! Facility execution is parameterized over a [`clock::Clock`], an
//! [`operator::Operator`] for confirmations, an [`events::EventSink`] for
//! narration, and exact Asset-bound document executors.

pub mod clock;
pub mod device_executors;
pub mod events;
pub mod execution;
pub mod ledger;
pub mod mode;
pub mod operator;
pub mod provenance;
pub mod reviewed_documents;
pub mod star;

pub use hamilton_star;
