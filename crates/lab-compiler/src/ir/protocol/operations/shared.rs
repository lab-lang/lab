use pliron::context::Context;
use pliron::location::Location;
use pliron::result::Result;
use pliron::r#type::Typed;
use pliron::value::Value;
use pliron::verify_err;

use crate::ir::protocol::{EvidenceType, MaterialType};

pub(super) fn require_attr(present: bool, name: &str, location: Location) -> Result<()> {
    if !present {
        return verify_err!(location, "operation is missing required attribute {name}");
    }
    Ok(())
}

pub(super) fn require_material(
    value: Value,
    expected: MaterialType,
    location: Location,
    ctx: &Context,
) -> Result<()> {
    let handle = value.get_type(ctx);
    let ty = handle.deref(ctx);
    let Some(actual) = ty.downcast_ref::<MaterialType>() else {
        return verify_err!(location, "expected Protocol material type {expected:?}");
    };
    if *actual != expected {
        return verify_err!(
            location,
            "expected Protocol material type {expected:?}, found {actual:?}"
        );
    }
    Ok(())
}

pub(super) fn require_evidence(
    value: Value,
    expected: EvidenceType,
    location: Location,
    ctx: &Context,
) -> Result<()> {
    let handle = value.get_type(ctx);
    let ty = handle.deref(ctx);
    let Some(actual) = ty.downcast_ref::<EvidenceType>() else {
        return verify_err!(location, "expected Protocol evidence type {expected:?}");
    };
    if *actual != expected {
        return verify_err!(
            location,
            "expected Protocol evidence type {expected:?}, found {actual:?}"
        );
    }
    Ok(())
}

pub(super) fn require_any_evidence(value: Value, location: Location, ctx: &Context) -> Result<()> {
    let handle = value.get_type(ctx);
    let ty = handle.deref(ctx);
    if ty.downcast_ref::<EvidenceType>().is_none() {
        return verify_err!(location, "expected a Protocol evidence operand");
    }
    Ok(())
}
