pub(crate) mod attributes {
    use pliron::attribute::AttrObj;
    use pliron::builtin::attributes::{DictAttr, IntegerAttr, StringAttr, VecAttr};
    use pliron::builtin::types::{IntegerType, Signedness};
    use pliron::context::Context;
    use pliron::identifier::Identifier;
    use pliron::location::Location;
    use pliron::result::Result;
    use pliron::utils::apint::{APInt, bw};
    use pliron::verify_err;

    /// An ordered list of names, such as an artifact's components or the
    /// artifacts a realization depends on.
    pub(crate) fn string_vec(values: Vec<String>) -> VecAttr {
        VecAttr::new(
            values
                .into_iter()
                .map(|value| StringAttr::new(value).into())
                .collect(),
        )
    }

    pub(crate) fn require_string(
        value: Option<&StringAttr>,
        name: &str,
        location: Location,
    ) -> Result<()> {
        if value.is_none_or(|value| value.as_str().is_empty()) {
            return verify_err!(location, "operation requires non-empty attribute {name}");
        }
        Ok(())
    }

    /// A named set of whole-number reaction parameters, such as reagent volumes
    /// and thermal-profile temperatures and holds. Chemistry travels as one
    /// dictionary rather than as a separate attribute per value, so a recipe
    /// stays inspectable in printed IR without the dialect growing a key per
    /// reagent.
    pub(crate) fn quantity_dict(entries: &[(&str, u32)], context: &Context) -> DictAttr {
        DictAttr::new(
            entries
                .iter()
                .map(|(name, value)| {
                    (
                        Identifier::try_from(*name).expect("chemistry keys are identifiers"),
                        u32_attr(context, *value).into(),
                    )
                })
                .collect(),
        )
    }

    /// Read a chemistry entry, falling back when a dictionary predates the key.
    pub(crate) fn quantity_entry(dict: Option<&DictAttr>, name: &str, fallback: u32) -> u32 {
        dict.and_then(|dict| {
            let key = Identifier::try_from(name).ok()?;
            dict.lookup(&key)?
                .downcast_ref::<IntegerAttr>()
                .map(u32_value)
        })
        .unwrap_or(fallback)
    }

    pub(crate) fn require_quantity_dict(
        value: Option<&DictAttr>,
        name: &str,
        keys: &[&str],
        location: Location,
    ) -> Result<()> {
        let Some(dict) = value else {
            return verify_err!(location, "operation is missing attribute {name}");
        };
        for key in keys {
            let Ok(identifier) = Identifier::try_from(*key) else {
                return verify_err!(location, "attribute {name} key {key} is not an identifier");
            };
            let entry = dict.lookup(&identifier);
            if entry.is_none_or(|entry| entry.downcast_ref::<IntegerAttr>().is_none()) {
                return verify_err!(
                    location,
                    "attribute {name} is missing whole-number entry {key}"
                );
            }
        }
        Ok(())
    }

    pub(crate) fn require_string_vec(
        value: Option<&VecAttr>,
        name: &str,
        location: Location,
    ) -> Result<()> {
        let Some(value) = value else {
            return verify_err!(location, "operation is missing attribute {name}");
        };
        if value.0.iter().any(|item: &AttrObj| {
            item.downcast_ref::<StringAttr>()
                .is_none_or(|value| value.as_str().is_empty())
        }) {
            return verify_err!(
                location,
                "attribute {name} must contain only non-empty strings"
            );
        }
        Ok(())
    }

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

pub(crate) mod chemistry;
pub(crate) mod design;
pub(crate) mod protocol;
pub(crate) mod workflow;
