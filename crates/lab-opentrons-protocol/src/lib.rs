//! Typed, validating authoring interface for Opentrons JSON protocols
//! (protocol schema v8, command schema v8).
//!
//! Two layers, cleanly separated:
//!
//! - [`schema`] is the faithful wire model. Everything in it serializes to
//!   exactly what the Opentrons protocol schema accepts, and nothing in it
//!   validates semantics. It is public so protocols the builder does not model
//!   can still be authored by hand.
//! - [`builder`] is the checked authoring API. Load methods return typed
//!   handles, command methods accept only handles, and every semantic rule the
//!   protocol engine enforces during analysis is checked at construction time,
//!   so a protocol that reaches [`builder::FlexProtocolBuilder::build`] is one
//!   `opentrons analyze` accepts.
//! - [`labware`] carries the standard labware definitions the builder embeds
//!   into emitted documents and validates well names and volumes against.
//!
//! A protocol document produced here is executed by uploading it to a robot:
//! `POST /protocols` (multipart, port 31950, header `Opentrons-Version: 3`),
//! poll the created analysis, `POST /runs {"data":{"protocolId": ...}}`, then
//! `POST /runs/{id}/actions {"data":{"actionType":"play"}}`. The same command
//! envelopes in [`schema::Command`] are accepted one at a time at
//! `POST /runs/{id}/commands`, so this command layer also serves live drivers.

pub mod builder;
pub mod labware;
pub mod schema;

pub use builder::{
    AbsorbanceReader, FlexModule, FlexPipetteName, FlexProtocolBuilder, FlexSlot, HeaterShaker,
    LabwareId, LiquidId, MagneticBlock, ModuleId, PipetteId, PipetteMount, ProtocolError,
    TemperatureModule, Thermocycler, TrashArea,
};
pub use labware::{LabwareDefinition, LabwareDefinitionError, standard_definition};
