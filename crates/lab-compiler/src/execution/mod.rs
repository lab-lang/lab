//! Backend-neutral operational graph produced from scientific protocol plans.
//!
//! This graph is the shared boundary for scheduling, simulation, and robot
//! backends. It records only semantics supported by the compiler today while
//! leaving hardware allocation and native commands to concrete backends.

mod graph;

pub use graph::{ExecutionDependency, ExecutionGraph, ExecutionOperation};
