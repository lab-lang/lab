//! Shared contracts used by standard-library durable actions.

use std::collections::BTreeSet;

use crate::checked::OwnershipMode;
use crate::type_system::Ty;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ContractType {
    Concrete(Ty),
    SameAs(&'static str),
    AnyMaterial,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PhrasePart {
    Word(&'static str),
    Operand {
        name: &'static str,
        r#type: ContractType,
        mode: OwnershipMode,
    },
    Integer {
        name: &'static str,
        signed: bool,
    },
    Quantity {
        name: &'static str,
        signed: bool,
        units: &'static [&'static str],
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResultSpec {
    pub name: &'static str,
    pub r#type: ContractType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActionContractSpec {
    pub operation: &'static str,
    pub capability: &'static str,
    pub phrase: Vec<PhrasePart>,
    pub results: Vec<ResultSpec>,
}

impl ActionContractSpec {
    pub(crate) fn source_name(&self) -> Option<&'static str> {
        match self.phrase.first() {
            Some(PhrasePart::Word(name)) => Some(name),
            _ => None,
        }
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        let action = self
            .source_name()
            .ok_or_else(|| "action phrase must begin with its source name".to_owned())?;
        if action.is_empty() {
            return Err("action source name cannot be empty".to_owned());
        }
        if self.operation.is_empty() {
            return Err("action operation identity cannot be empty".to_owned());
        }
        if self.capability.is_empty() {
            return Err("action capability cannot be empty".to_owned());
        }

        let mut argument_names = BTreeSet::new();
        let mut operands = BTreeSet::new();
        for part in &self.phrase {
            let (name, units) = match part {
                PhrasePart::Word(word) => {
                    if word.is_empty() {
                        return Err("action phrase words cannot be empty".to_owned());
                    }
                    continue;
                }
                PhrasePart::Operand { name, r#type, .. } => {
                    if let ContractType::SameAs(reference) = r#type
                        && !operands.contains(reference)
                    {
                        return Err(format!(
                            "action argument '{name}' references unknown earlier operand '{reference}'"
                        ));
                    }
                    operands.insert(*name);
                    (*name, None)
                }
                PhrasePart::Integer { name, .. } => (*name, None),
                PhrasePart::Quantity { name, units, .. } => (*name, Some(*units)),
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

        let mut result_names = BTreeSet::new();
        for result in &self.results {
            if !result_names.insert(result.name) {
                return Err(format!(
                    "action result '{}' is declared more than once",
                    result.name
                ));
            }
            match &result.r#type {
                ContractType::Concrete(_) => {}
                ContractType::SameAs(reference) if operands.contains(reference) => {}
                ContractType::SameAs(reference) => {
                    return Err(format!(
                        "action result '{}' references unknown operand '{reference}'",
                        result.name
                    ));
                }
                ContractType::AnyMaterial => {
                    return Err(format!(
                        "action result '{}' cannot have unconstrained AnyMaterial type",
                        result.name
                    ));
                }
            }
        }
        Ok(())
    }
}
