//! Built-in domain algorithms own their complete Procedure registrations.

mod chemical_transformation;
mod golden_gate;
mod plating;
mod recovery;
mod serial_dilution;
mod thermal_cycle;

pub(crate) fn registrations()
-> impl Iterator<Item = crate::procedure::ProcedureProgramBuilderRegistration> {
    [
        golden_gate::registrations(),
        serial_dilution::registrations(),
        thermal_cycle::registrations(),
        chemical_transformation::registrations(),
        recovery::registrations(),
        plating::registrations(),
    ]
    .into_iter()
    .flatten()
}
