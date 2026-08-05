pub(crate) mod attributes {
    use pliron::builtin::attributes::IntegerAttr;
    use pliron::builtin::types::{IntegerType, Signedness};
    use pliron::context::Context;
    use pliron::location::Location;
    use pliron::result::Result;
    use pliron::utils::apint::{APInt, bw};
    use pliron::verify_err;

    #[allow(dead_code)]
    pub(crate) fn u32_attr(context: &Context, value: u32) -> IntegerAttr {
        IntegerAttr::new(
            IntegerType::get(context, 32, Signedness::Unsigned),
            APInt::from_u32(value, bw(32)),
        )
    }

    pub(crate) fn u32_value(attribute: &IntegerAttr) -> u32 {
        attribute.value().to_u32()
    }

    pub(crate) fn verify_u32_attr(
        attribute: &IntegerAttr,
        name: &str,
        location: Location,
        context: &Context,
    ) -> Result<()> {
        let handle = attribute.get_type();
        let ty = handle.deref(context);
        if ty.width() != 32 || ty.signedness() != Signedness::Unsigned {
            return verify_err!(location, "{name} must be an unsigned 32-bit integer");
        }
        Ok(())
    }
}

pub(crate) mod design;
pub(crate) mod protocol;
pub(crate) mod workflow;
