//! Recognizing which Lab kind an SBOL object describes.
//!
//! [`Grounding`] answers "what does this type stand for". Reading a document
//! needs the other direction: given the terms an object states about itself,
//! which Lab type is it. The map is the same map, inverted.
//!
//! Inverting it is not a bijection, and that is the interesting part. Ontology
//! terms say what a thing *is*; a Lab kind also says what part it plays in a
//! method. A backbone and a plasmid are both an engineered region of nucleic
//! acid, and SBOL has no term that separates the vector you cut open from the
//! construct you build. So a term set can name one kind, several, or none, and
//! this module reports which rather than guessing.

use std::collections::{BTreeMap, BTreeSet};

use lab_language::Grounding;
use lab_language::ast::instance_word;

/// The namespace Lab writes its own statements in.
///
/// Anything Lab needs to say that SBOL has no vocabulary for lives under this
/// prefix, so a third-party reader can ignore it wholesale and a Lab reader can
/// recognize its own documents.
pub const LAB_NAMESPACE: &str = "https://lab-lang.org/ns#";

/// The predicate naming the Lab kind an object was written as.
///
/// A document Lab emitted carries this, so reading one back recovers the kind
/// exactly rather than inferring it. A document from anywhere else does not,
/// and inference is all there is.
pub const LAB_KIND: &str = "https://lab-lang.org/ns#kind";

/// What the terms an object states resolve to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// Exactly one kind stands for these terms.
    Resolved(String),
    /// Several kinds stand for these terms and nothing separates them. The
    /// candidates are ordered so a diagnostic lists them the same way twice.
    Ambiguous(Vec<String>),
    /// No kind in scope stands for these terms. The object may still be
    /// readable; it is this program's vocabulary that does not cover it.
    Unresolved,
}

/// Which Lab type each set of ontology terms names.
#[derive(Clone, Debug, Default)]
pub struct KindIndex {
    grounded: BTreeMap<String, BTreeSet<String>>,
    modules: BTreeMap<String, String>,
}

impl KindIndex {
    /// Builds the index by inverting a grounding.
    pub fn new(grounding: &Grounding) -> Self {
        let grounded: BTreeMap<String, BTreeSet<String>> = grounding
            .grounded_types()
            .map(|(name, terms)| (name.to_owned(), terms))
            .collect();
        let modules = grounded
            .keys()
            .filter_map(|name| {
                grounding
                    .module_declaring(name)
                    .map(|module| (name.clone(), module.to_owned()))
            })
            .collect();
        Self { grounded, modules }
    }

    /// The module a design has to import to name `kind`.
    pub fn module_declaring(&self, kind: &str) -> Option<&str> {
        self.modules.get(kind).map(String::as_str)
    }

    /// The same, found by the word instances are written with rather than by
    /// the type's own name, because that is what a declaration records.
    pub fn module_for_word(&self, word: &str) -> Option<&str> {
        self.modules
            .iter()
            .find(|(kind, _)| instance_word(kind) == word)
            .map(|(_, module)| module.as_str())
    }

    /// The kind an object stating `terms` describes.
    ///
    /// A candidate is a kind whose every term the object also states, so an
    /// object may say more than a kind requires and still be that kind: an SBOL
    /// document is free to be more specific than the vocabulary reading it.
    /// Among candidates, one that another strictly contains is discarded, since
    /// the more specific kind is the better answer. What survives is the answer
    /// when it is alone and an ambiguity when it is not.
    pub fn resolve(&self, terms: &BTreeSet<String>) -> Resolution {
        let candidates: Vec<(&str, &BTreeSet<String>)> = self
            .grounded
            .iter()
            .filter(|(_, required)| required.is_subset(terms))
            .map(|(name, required)| (name.as_str(), required))
            .collect();

        let maximal: Vec<String> = candidates
            .iter()
            .filter(|(_, required)| {
                !candidates
                    .iter()
                    .any(|(_, other)| other.len() > required.len() && required.is_subset(other))
            })
            .map(|(name, _)| (*name).to_owned())
            .collect();

        match maximal.as_slice() {
            [] => Resolution::Unresolved,
            [only] => Resolution::Resolved(only.clone()),
            _ => Resolution::Ambiguous(maximal),
        }
    }

    /// The terms a kind stands for, for a caller checking a stated kind against
    /// what the object actually says about itself.
    pub fn terms(&self, kind: &str) -> Option<&BTreeSet<String>> {
        self.grounded.get(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> KindIndex {
        KindIndex::new(&Grounding::bundled())
    }

    fn terms(values: &[&str]) -> BTreeSet<String> {
        values
            .iter()
            .map(|value| format!("https://identifiers.org/{value}"))
            .collect()
    }

    /// A promoter states one term more than a bare part, and the more specific
    /// kind is the answer rather than an ambiguity with `Part`.
    #[test]
    fn the_most_specific_kind_wins() {
        assert_eq!(
            index().resolve(&terms(&["SBO:0000251", "SO:0000167"])),
            Resolution::Resolved("Promoter".to_owned())
        );
    }

    #[test]
    fn a_single_term_resolves_the_kind_that_states_only_it() {
        assert_eq!(
            index().resolve(&terms(&["SBO:0000247"])),
            Resolution::Resolved("Antibiotic".to_owned())
        );
    }

    /// An object may be more specific than the vocabulary reading it. Extra
    /// terms narrow nothing away, they just fail to add a candidate.
    #[test]
    fn unrecognized_extra_terms_do_not_prevent_a_match() {
        let mut stated = terms(&["SBO:0000251", "SO:0000167"]);
        stated.insert("https://example.org/private/term".to_owned());
        assert_eq!(
            index().resolve(&stated),
            Resolution::Resolved("Promoter".to_owned())
        );
    }

    /// The bundled Sequence Ontology snapshot has no term separating the vector
    /// you cut open from the construct you build, so both kinds stand for the
    /// same two terms. Reporting that is the honest answer; picking one would
    /// silently mistype half the designs in a registry.
    #[test]
    fn a_backbone_and_a_plasmid_are_not_distinguishable_by_terms_alone() {
        let Resolution::Ambiguous(candidates) =
            index().resolve(&terms(&["SBO:0000251", "SO:0000804"]))
        else {
            panic!("nothing in SBOL separates a backbone from a plasmid");
        };
        assert_eq!(
            candidates,
            vec!["Backbone".to_owned(), "Plasmid".to_owned()]
        );
    }

    #[test]
    fn terms_no_kind_states_resolve_to_nothing() {
        assert_eq!(
            index().resolve(&terms(&["SO:0000694"])),
            Resolution::Unresolved
        );
    }

    #[test]
    fn an_object_stating_no_terms_resolves_to_nothing() {
        assert_eq!(index().resolve(&BTreeSet::new()), Resolution::Unresolved);
    }
}
