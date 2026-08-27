/// Recognizes an absolute IRI without pulling an RDF or URL stack into the language frontend.
pub fn is_absolute_iri(value: &str) -> bool {
    let Some((scheme, rest)) = value.split_once(':') else {
        return false;
    };
    !rest.is_empty()
        && scheme
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic())
        && scheme.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        })
        && !value.chars().any(|character| {
            character.is_whitespace()
                || character.is_control()
                || matches!(
                    character,
                    '<' | '>' | '"' | '{' | '}' | '|' | '\\' | '^' | '`'
                )
        })
}

#[cfg(test)]
mod tests {
    use super::is_absolute_iri;

    #[test]
    fn recognizes_absolute_iris_without_importing_an_rdf_stack() {
        assert!(is_absolute_iri("https://example.org/design"));
        assert!(is_absolute_iri(
            "urn:uuid:2ed8c319-58b7-46ad-aaf0-95c79be6b107"
        ));
        assert!(!is_absolute_iri("BBa_J23101"));
        assert!(!is_absolute_iri("https://example.org/a design"));
    }
}
