//! The shape of an ontology term a role names.
//!
//! A term is written as an absolute IRI or as a compact identifier, and this
//! module decides only whether what was written could name a term at all. What
//! the term *means* — whether it exists, which branch it belongs to, whether
//! two terms on one type contradict each other — needs an ontology snapshot,
//! which lives outside this crate so the frontend keeps no data dependency and
//! no network stack.
//!
//! Splitting it here is what lets a single file be checked without resolving a
//! package: a misspelled prefix is caught where it is written, and the
//! membership questions are asked once a whole program is being compiled.

use crate::semantic_error::SemanticError;
use crate::source::Identifier;

/// The prefixes SBOL draws terms from, in the spelling `identifiers.org` uses.
///
/// This list decides only how a compact identifier is recognized, not which
/// terms exist. A term from an ontology not listed here is written as a full
/// IRI, which stays open to vocabularies this crate has never heard of.
const KNOWN_PREFIXES: [&str; 7] = ["CHEBI", "CL", "EDAM", "GO", "NCIT", "SBO", "SO"];

/// Checks that `term` could name an ontology term, and returns it in the
/// spelling the rest of the compiler compares against.
///
/// A compact identifier expands to its `identifiers.org` IRI, so `SO:0000167`
/// and `https://identifiers.org/SO:0000167` are one term written two ways and
/// are not two terms that happen to agree.
pub(super) fn check_term(term: &Identifier) -> Result<String, SemanticError> {
    let text = term.value.trim();
    if text.is_empty() {
        return Err(
            SemanticError::new(term.span, "an ontology term is empty").help(
                "write the term a role stands for, such as \"https://identifiers.org/SO:0000167\"",
            ),
        );
    }
    if text != term.value {
        return Err(
            SemanticError::new(term.span, "an ontology term has surrounding whitespace")
                .help("write the term with no leading or trailing spaces"),
        );
    }
    if text.starts_with("http://") || text.starts_with("https://") {
        return check_iri(term, text);
    }
    check_compact(term, text)
}

fn check_iri(term: &Identifier, text: &str) -> Result<String, SemanticError> {
    let rest = text
        .strip_prefix("https://")
        .or_else(|| text.strip_prefix("http://"))
        .expect("caller checked the scheme");
    if rest.is_empty() || rest.starts_with('/') {
        return Err(
            SemanticError::new(term.span, format!("'{text}' has no host")).help(
                "an ontology term is an absolute IRI, such as \"https://identifiers.org/SO:0000167\"",
            ),
        );
    }
    if text.chars().any(char::is_whitespace) {
        return Err(SemanticError::new(
            term.span,
            format!("'{text}' contains whitespace, so it is not an IRI"),
        ));
    }
    Ok(text.to_owned())
}

fn check_compact(term: &Identifier, text: &str) -> Result<String, SemanticError> {
    let Some((prefix, local)) = text.split_once(':') else {
        return Err(SemanticError::new(
            term.span,
            format!("'{text}' is neither an IRI nor a compact identifier"),
        )
        .help("write a term as \"SO:0000167\" or as the IRI it stands for")
        .help("a role with no term classifies types without naming any ontology"));
    };
    if local.is_empty() {
        return Err(SemanticError::new(
            term.span,
            format!("'{text}' names the ontology '{prefix}' but no term in it"),
        ));
    }
    let matched = KNOWN_PREFIXES
        .iter()
        .find(|known| known.eq_ignore_ascii_case(prefix));
    let Some(matched) = matched else {
        return Err(SemanticError::new(
            term.span,
            format!("'{prefix}' is not an ontology this compiler recognizes"),
        )
        .help(format!(
            "recognized prefixes are {}",
            KNOWN_PREFIXES.join(", ")
        ))
        .help("a term from another vocabulary is written as its full IRI"));
    };
    Ok(format!("https://identifiers.org/{matched}:{local}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{Span, Spanned};

    fn term(text: &str) -> Identifier {
        Spanned::new(text.to_owned(), Span::new(0, text.len()))
    }

    #[test]
    fn a_compact_identifier_expands_to_its_iri() {
        assert_eq!(
            check_term(&term("SO:0000167")).expect("a known prefix"),
            "https://identifiers.org/SO:0000167"
        );
    }

    #[test]
    fn a_full_iri_is_kept_as_written() {
        let iri = "https://identifiers.org/SO:0000167";
        assert_eq!(check_term(&term(iri)).expect("an absolute IRI"), iri);
    }

    /// The two spellings must agree, or a type grounded one way would not
    /// match a document written the other.
    #[test]
    fn both_spellings_of_one_term_agree() {
        assert_eq!(
            check_term(&term("SO:0000167")).expect("compact"),
            check_term(&term("https://identifiers.org/SO:0000167")).expect("iri")
        );
    }

    #[test]
    fn an_iri_from_an_unlisted_vocabulary_is_accepted() {
        let iri = "https://lab-compiler.org/terms/Reporter";
        assert_eq!(check_term(&term(iri)).expect("an absolute IRI"), iri);
    }

    #[test]
    fn rejects_an_unknown_compact_prefix() {
        let error = check_term(&term("XX:1")).expect_err("an unknown prefix");
        assert!(error.message.contains("not an ontology"), "{error:?}");
    }

    #[test]
    fn rejects_a_bare_word() {
        let error = check_term(&term("promoter")).expect_err("not a term");
        assert!(
            error.message.contains("neither an IRI nor a compact"),
            "{error:?}"
        );
    }

    #[test]
    fn rejects_an_empty_term() {
        assert!(check_term(&term("")).is_err());
    }

    #[test]
    fn rejects_a_prefix_with_no_local_part() {
        let error = check_term(&term("SO:")).expect_err("no local part");
        assert!(error.message.contains("no term in it"), "{error:?}");
    }
}
