//! A Rust implementation of the Opentrons JSON protocol format, with typed
//! authoring and construction-time validation.
//!
//! Protocol-schema versions are sibling modules, so one program can address
//! more than one version without renames: [`v8`] carries protocol schema v8
//! (command schema v8), and the crate root re-exports its API as the
//! current version. Inside a version:
//!
//! - [`v8::schema`] is the faithful wire model. Everything in it serializes
//!   to exactly what the Opentrons protocol schema accepts, and nothing in
//!   it validates semantics. It is public so protocols the builder does not
//!   model can still be authored by hand.
//! - [`v8::builder`] is the checked authoring API. Load methods return
//!   typed handles, command methods accept only handles, and every semantic
//!   rule the protocol engine enforces during analysis is checked at
//!   construction time, so a protocol that reaches
//!   [`v8::builder::FlexProtocolBuilder::build`] is one `opentrons analyze`
//!   accepts.
//!
//! [`labware`] sits outside the version namespace: labware definitions
//! follow their own schema (labware schema 2) and are shared by every
//! protocol-schema version. The builder embeds them into emitted documents
//! and validates well names and volumes against them.
//!
//! A protocol document produced here is executed by uploading it to a robot:
//! `POST /protocols` (multipart, port 31950, header `Opentrons-Version: 3`),
//! poll the created analysis, `POST /runs {"data":{"protocolId": ...}}`, then
//! `POST /runs/{id}/actions {"data":{"actionType":"play"}}`. The same command
//! envelopes in [`schema::Command`] are accepted one at a time at
//! `POST /runs/{id}/commands`, so this command layer also serves live drivers.

pub mod labware;
pub mod v8;

pub use v8::{builder, schema};

pub use labware::{LabwareDefinition, LabwareDefinitionError, standard_definition};
pub use v8::builder::{
    AbsorbanceReader, FlexModule, FlexPipetteName, FlexProtocolBuilder, FlexSlot, HeaterShaker,
    LabwareId, LiquidId, MagneticBlock, ModuleId, PipetteId, PipetteMount, ProtocolError,
    TemperatureModule, Thermocycler, TrashArea,
};
