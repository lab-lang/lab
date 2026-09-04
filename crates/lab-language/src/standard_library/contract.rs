//! Shared contracts used by standard-library durable actions.

use std::collections::BTreeSet;

use crate::checked::OwnershipMode;
use crate::type_system::Ty;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ContractType {
    Concrete(Ty),
    SameAs(String),
    AnyMaterial,
    /// Any declared thing, whatever its type. Fetching one off the shelf does
    /// not depend on what it is.
    AnyValue,
    /// Material of whatever an earlier operand was. What comes back from the
    /// shelf is the thing that was asked for.
    MaterialOf(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PhrasePart {
    Word(String),
    Operand {
        name: String,
        r#type: ContractType,
        mode: OwnershipMode,
    },
    Integer {
        name: String,
        signed: bool,
    },
    Quantity {
        name: String,
        signed: bool,
        units: Vec<String>,
    },
    /// A clause a phrase may leave out. Omitting it binds every operand it
    /// carries to the empty collection, so an optional clause may only carry
    /// collections: a list has an empty value and a material does not.
    ///
    /// The clause begins with a word, which is what tells an omitted clause
    /// apart from a present one.
    Optional(Vec<PhrasePart>),
}

impl PhrasePart {
    /// A literal word in a phrase, such as `capture` or `from`.
    pub(crate) fn word(word: impl Into<String>) -> Self {
        Self::Word(word.into())
    }

    /// A material or value operand.
    pub(crate) fn operand(
        name: impl Into<String>,
        r#type: ContractType,
        mode: OwnershipMode,
    ) -> Self {
        Self::Operand {
            name: name.into(),
            r#type,
            mode,
        }
    }

    /// A whole-number operand.
    pub(crate) fn integer(name: impl Into<String>, signed: bool) -> Self {
        Self::Integer {
            name: name.into(),
            signed,
        }
    }

    /// A measurement operand, in one of the stated units.
    pub(crate) fn quantity(name: impl Into<String>, signed: bool, units: &[&str]) -> Self {
        Self::Quantity {
            name: name.into(),
            signed,
            units: units.iter().map(|unit| (*unit).to_owned()).collect(),
        }
    }

    /// The words and operands a phrase part contributes, flattening an optional
    /// clause into the parts it would contribute when present.
    pub(crate) fn parts(&self) -> &[PhrasePart] {
        match self {
            Self::Optional(parts) => parts,
            part => std::slice::from_ref(part),
        }
    }
}

/// Whether a result is a new biological entity or the same one further along.
///
/// This is what separates a technical replicate from a biological one. Picking
/// two colonies gives two independent transformants; splitting one culture into
/// two tubes gives one organism measured twice. Averaging the second and
/// reporting `n = 2` counts pipetting variance as biology.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Lineage {
    /// The result carries on the lineage of the material it came from. Diluting,
    /// recovering, and plating all leave you with the same organism.
    #[default]
    Continues,
    /// The result begins a lineage of its own, independent of its siblings and
    /// of anything produced by another invocation. Each picked colony is an
    /// independent transformant; each transformation establishes a new one.
    Begins,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResultSpec {
    pub name: String,
    pub r#type: ContractType,
    pub lineage: Lineage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActionContractSpec {
    pub operation: String,
    pub phrase: Vec<PhrasePart>,
    pub results: Vec<ResultSpec>,
    /// Operands whose lineage a result does not carry on.
    ///
    /// Lineage answers which samples are the same organism, so only what an
    /// organism is made of contributes to it. A plate is a culture spread on
    /// agar: the culture is the organism and the agar is what it sits on, and
    /// counting the agar would make one plate look like two independent things.
    pub inert: Vec<String>,
}

impl ActionContractSpec {
    pub(crate) fn source_name(&self) -> Option<&str> {
        match self.phrase.first() {
            Some(PhrasePart::Word(name)) => Some(name),
            _ => None,
        }
    }

    pub(in crate::standard_library) fn validate(&self) -> Result<(), String> {
        let action = self
            .source_name()
            .ok_or_else(|| "action phrase must begin with its source name".to_owned())?;
        if action.is_empty() {
            return Err("action source name cannot be empty".to_owned());
        }
        if self.operation.is_empty() {
            return Err("action operation identity cannot be empty".to_owned());
        }
        let mut argument_names = BTreeSet::new();
        let mut operands = BTreeSet::new();
        for part in self.phrase.iter().flat_map(PhrasePart::parts) {
            let (name, units) = match part {
                PhrasePart::Word(word) => {
                    if word.is_empty() {
                        return Err("action phrase words cannot be empty".to_owned());
                    }
                    continue;
                }
                PhrasePart::Operand { name, r#type, .. } => {
                    if let ContractType::SameAs(reference) = r#type
                        && !operands.contains(reference.as_str())
                    {
                        return Err(format!(
                            "action argument '{name}' references unknown earlier operand '{reference}'"
                        ));
                    }
                    operands.insert(name.as_str());
                    (name.as_str(), None)
                }
                PhrasePart::Integer { name, .. } => (name.as_str(), None),
                PhrasePart::Quantity { name, units, .. } => (name.as_str(), Some(units.as_slice())),
                PhrasePart::Optional(_) => {
                    return Err("an optional clause cannot nest another".to_owned());
                }
            };
            if !argument_names.insert(name) {
                return Err(format!(
                    "action argument '{name}' is declared more than once"
                ));
            }
            if units.is_some_and(<[_]>::is_empty) {
                return Err(format!(
                    "quantity argument '{name}' must allow at least one unit"
                ));
            }
        }

        for part in &self.phrase {
            let PhrasePart::Optional(parts) = part else {
                continue;
            };
            match parts.first() {
                Some(PhrasePart::Word(word)) if !word.is_empty() => {}
                _ => {
                    return Err(
                        "an optional clause must begin with a word that marks its presence"
                            .to_owned(),
                    );
                }
            }
            for nested in parts {
                if let PhrasePart::Operand { name, r#type, .. } = nested
                    && !matches!(r#type, ContractType::Concrete(Ty::List(_)))
                {
                    return Err(format!(
                        "optional operand '{name}' must be a list, because omitting its clause binds it to the empty list"
                    ));
                }
            }
        }

        let mut result_names = BTreeSet::new();
        for result in &self.results {
            if !result_names.insert(result.name.as_str()) {
                return Err(format!(
                    "action result '{}' is declared more than once",
                    result.name
                ));
            }
            match &result.r#type {
                ContractType::Concrete(_) => {}
                ContractType::MaterialOf(reference) | ContractType::SameAs(reference)
                    if operands.contains(reference.as_str()) => {}
                ContractType::MaterialOf(reference) | ContractType::SameAs(reference) => {
                    return Err(format!(
                        "action result '{}' references unknown operand '{reference}'",
                        result.name
                    ));
                }
                ContractType::AnyMaterial | ContractType::AnyValue => {
                    return Err(format!(
                        "action result '{}' cannot have an unconstrained type",
                        result.name
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract(phrase: Vec<PhrasePart>) -> ActionContractSpec {
        ActionContractSpec {
            operation: "test.action".to_owned(),
            phrase,
            inert: Vec::new(),
            results: Vec::new(),
        }
    }

    fn optional_operand(r#type: ContractType) -> PhrasePart {
        PhrasePart::Optional(vec![
            PhrasePart::word("from"),
            PhrasePart::operand("items", r#type, OwnershipMode::Take),
        ])
    }

    #[test]
    fn an_optional_clause_may_only_carry_collections() {
        let listed = contract(vec![
            PhrasePart::word("act"),
            optional_operand(ContractType::Concrete(Ty::List(Box::new(Ty::named(
                "Plasmid",
            ))))),
        ]);
        listed.validate().unwrap();

        let scalar = contract(vec![
            PhrasePart::word("act"),
            optional_operand(ContractType::Concrete(Ty::named("Plasmid"))),
        ]);
        let error = scalar
            .validate()
            .expect_err("a material has no empty value to fall back to");
        assert!(error.contains("must be a list"), "{error}");
    }

    #[test]
    fn an_optional_clause_must_announce_itself_with_a_word() {
        let error = contract(vec![
            PhrasePart::word("act"),
            PhrasePart::Optional(vec![PhrasePart::operand(
                "items",
                ContractType::Concrete(Ty::List(Box::new(Ty::named("Plasmid")))),
                OwnershipMode::Take,
            )]),
        ])
        .validate()
        .expect_err("without a leading word an omitted clause is indistinguishable");
        assert!(error.contains("must begin with a word"), "{error}");
    }

    #[test]
    fn an_optional_operand_still_shares_the_one_argument_namespace() {
        let error = contract(vec![
            PhrasePart::word("act"),
            PhrasePart::operand(
                "items",
                ContractType::Concrete(Ty::named("Plasmid")),
                OwnershipMode::Copy,
            ),
            optional_operand(ContractType::Concrete(Ty::List(Box::new(Ty::named(
                "Plasmid",
            ))))),
        ])
        .validate()
        .expect_err("two operands cannot share one name");
        assert!(error.contains("declared more than once"), "{error}");
    }
}
