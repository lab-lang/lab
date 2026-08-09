//! Protocol schema v8 (command schema v8), accepted by robot software
//! 7.1.0 and later.
//!
//! [`schema`] is the faithful wire model; [`builder`] is the checked
//! authoring API over it. The crate root re-exports both as the current
//! version's API.

pub mod builder;
pub mod schema;
