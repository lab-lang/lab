//! Match-pattern checking against a matched type.

use crate::ast::Pattern;
use crate::checked::{CheckedPattern, CheckedPatternField};
use crate::semantic_error::SemanticError;
use crate::type_system::Ty;

use super::Checker;

impl Checker {
    pub fn check_pattern(&self, pattern: &Pattern, matched: &Ty) -> Result<(), SemanticError> {
        let Pattern::Constructor { path, .. } = pattern else {
            return Ok(());
        };
        let case = super::path_text(path);
        let expected_parent = self.cases.get(&case).ok_or_else(|| {
            SemanticError::new(path.span, format!("unknown outcome case '{case}'"))
        })?;
        if !self.compatible(&Ty::named(expected_parent), matched) {
            return Err(SemanticError::new(
                path.span,
                format!("case '{case}' does not belong to {matched}"),
            ));
        }
        Ok(())
    }

    pub fn checked_pattern(&self, pattern: &Pattern) -> CheckedPattern {
        match pattern {
            Pattern::Name(name) => CheckedPattern::Binding {
                name: name.value.clone(),
            },
            Pattern::Constructor { path, fields, .. } => {
                let name = super::path_text(path);
                let constructor = self
                    .cases
                    .get(&name)
                    .map_or_else(|| name.clone(), |parent| format!("outcome.{parent}.{name}"));
                CheckedPattern::Constructor {
                    constructor,
                    fields: fields
                        .iter()
                        .map(|field| CheckedPatternField {
                            field: field.field.value.clone(),
                            binding: field.binding.value.clone(),
                        })
                        .collect(),
                }
            }
        }
    }
}
