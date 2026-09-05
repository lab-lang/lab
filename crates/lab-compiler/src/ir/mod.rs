pub(crate) mod attributes {
    use pliron::builtin::attributes::{StringAttr, VecAttr};
    use pliron::location::Location;
    use pliron::result::Result;
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
}
