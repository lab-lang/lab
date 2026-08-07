//! Parsing and validation of the natural-language action-contract phrase
//! grammar (e.g. `store plasmid at -20 C`) against an `ActionContractSpec`.

use std::collections::HashMap;

use crate::ast::EffectStmt;
use crate::checked::{
    CheckedActionArgument, CheckedExpression, CheckedType, OwnershipMode, ResolvedAction,
    TypedExpression,
};
use crate::semantic_error::SemanticError;
use crate::semantics::DefinitionId;
use crate::source::Span;
use crate::standard_library::{ActionContractSpec, ContractType, PhrasePart};
use crate::type_system::{Ty, compatible, to_checked_type};

use super::Checker;

impl Checker {
    pub fn check_standard_action_contract(
        &self,
        effect: &EffectStmt,
        words: &[&str],
        environment: &HashMap<String, Ty>,
        contract: ActionContractSpec,
    ) -> Result<(ResolvedAction, Vec<Ty>), SemanticError> {
        let mut cursor = 0;
        let mut operands = HashMap::new();
        let mut arguments = Vec::new();
        for part in &contract.phrase {
            if let PhrasePart::Optional(clause) = part {
                // A clause is present exactly when the word introducing it is,
                // so an omitted one is not mistaken for a malformed phrase.
                let introducer = match clause.first() {
                    Some(PhrasePart::Word(word)) => *word,
                    _ => unreachable!("contract validation requires a leading word"),
                };
                if words.get(cursor) == Some(&introducer) {
                    for nested in clause {
                        self.match_phrase_part(
                            nested,
                            effect,
                            words,
                            environment,
                            &contract,
                            &mut cursor,
                            &mut operands,
                            &mut arguments,
                        )?;
                    }
                } else {
                    for nested in clause {
                        let PhrasePart::Operand { name, r#type, mode } = nested else {
                            continue;
                        };
                        let ty = resolve_contract_type(r#type, &operands, effect.span)?;
                        operands.insert((*name).to_owned(), ty.clone());
                        arguments.push(CheckedActionArgument {
                            name: (*name).to_owned(),
                            mode: *mode,
                            value: TypedExpression {
                                r#type: to_checked_type(&ty),
                                value: CheckedExpression::List {
                                    elements: Vec::new(),
                                },
                            },
                        });
                    }
                }
                continue;
            }
            self.match_phrase_part(
                part,
                effect,
                words,
                environment,
                &contract,
                &mut cursor,
                &mut operands,
                &mut arguments,
            )?;
        }
        if cursor != words.len() {
            return Err(SemanticError::new(
                effect.span,
                format!("malformed '{}' action phrase", contract.operation),
            ));
        }
        Self::finish_action_contract(effect, contract, operands, arguments)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn match_phrase_part(
        &self,
        part: &PhrasePart,
        effect: &EffectStmt,
        words: &[&str],
        environment: &HashMap<String, Ty>,
        contract: &ActionContractSpec,
        cursor: &mut usize,
        operands: &mut HashMap<String, Ty>,
        arguments: &mut Vec<CheckedActionArgument>,
    ) -> Result<(), SemanticError> {
        match part {
            PhrasePart::Optional(_) => {
                unreachable!("contract validation rejects a nested optional clause")
            }
            PhrasePart::Word(expected) => {
                if words.get(*cursor) != Some(expected) {
                    return Err(SemanticError::new(
                        effect.span,
                        format!("malformed '{}' action phrase", contract.operation),
                    ));
                }
                *cursor += 1;
            }
            PhrasePart::Operand { name, r#type, mode } => {
                let word = words.get(*cursor).ok_or_else(|| {
                    SemanticError::new(
                        effect.span,
                        format!(
                            "action '{}' is missing operand '{name}'",
                            contract.operation
                        ),
                    )
                })?;
                let actual = self.resolve_action_operand(word, environment, effect.span)?;
                if matches!(r#type, ContractType::AnyMaterial) {
                    if !matches!(&actual, Ty::Named(name, arguments) if name == "Material" && arguments.len() == 1)
                    {
                        return Err(SemanticError::new(
                            effect.span,
                            format!(
                                "operation '{}' expects physical Material<T>, found {actual}",
                                contract.operation
                            ),
                        ));
                    }
                } else {
                    let expected = resolve_contract_type(r#type, operands, effect.span)?;
                    require_action_type(actual.clone(), expected, effect.span, contract.operation)?;
                }
                operands.insert((*name).to_owned(), actual.clone());
                arguments.push(CheckedActionArgument {
                    name: (*name).to_owned(),
                    mode: *mode,
                    value: action_reference(self.definition_for_action_word(word), word, &actual),
                });
                *cursor += 1;
            }
            PhrasePart::Integer { name, signed } => {
                let word = words.get(*cursor).ok_or_else(|| {
                    SemanticError::new(
                        effect.span,
                        format!(
                            "action '{}' is missing integer '{name}'",
                            contract.operation
                        ),
                    )
                })?;
                let value = checked_integer_literal(word, *signed, effect.span)?;
                arguments.push(CheckedActionArgument {
                    name: (*name).to_owned(),
                    mode: OwnershipMode::Copy,
                    value,
                });
                *cursor += 1;
            }
            PhrasePart::Quantity {
                name,
                signed,
                units,
            } => {
                let magnitude = words.get(*cursor).ok_or_else(|| {
                    SemanticError::new(
                        effect.span,
                        format!(
                            "action '{}' is missing quantity '{name}'",
                            contract.operation
                        ),
                    )
                })?;
                checked_integer_literal(magnitude, *signed, effect.span)?;
                let unit = words.get(*cursor + 1).ok_or_else(|| {
                    SemanticError::new(
                        effect.span,
                        format!(
                            "action '{}' is missing a unit for '{name}'",
                            contract.operation
                        ),
                    )
                })?;
                if !units.contains(unit) {
                    return Err(SemanticError::new(
                        effect.span,
                        format!(
                            "action '{}' expects unit {units:?} for '{name}', found '{unit}'",
                            contract.operation
                        ),
                    ));
                }
                arguments.push(CheckedActionArgument {
                    name: (*name).to_owned(),
                    mode: OwnershipMode::Copy,
                    value: TypedExpression {
                        r#type: CheckedType::Quantity {
                            unit: (*unit).to_owned(),
                        },
                        value: CheckedExpression::Quantity {
                            magnitude: (*magnitude).to_owned(),
                            unit: (*unit).to_owned(),
                        },
                    },
                });
                *cursor += 2;
            }
        }
        Ok(())
    }

    pub fn finish_action_contract(
        effect: &EffectStmt,
        contract: ActionContractSpec,
        operands: HashMap<String, Ty>,
        arguments: Vec<CheckedActionArgument>,
    ) -> Result<(ResolvedAction, Vec<Ty>), SemanticError> {
        let result_contracts = contract
            .results
            .iter()
            .map(|result| {
                let ty = resolve_contract_type(&result.r#type, &operands, effect.span)?;
                Ok((super::checked_field(result.name, &ty), ty))
            })
            .collect::<Result<Vec<_>, SemanticError>>()?;
        let results = result_contracts
            .iter()
            .map(|(_, ty)| ty.clone())
            .collect::<Vec<_>>();
        let checked_results = result_contracts
            .into_iter()
            .map(|(field, _)| field)
            .collect::<Vec<_>>();
        Ok((
            ResolvedAction {
                operation: contract.operation.to_owned(),
                capability: Some(contract.capability.to_owned()),
                arguments,
                results: checked_results,
            },
            results,
        ))
    }
}

pub(super) fn resolve_contract_type(
    r#type: &ContractType,
    operands: &HashMap<String, Ty>,
    span: Span,
) -> Result<Ty, SemanticError> {
    match r#type {
        ContractType::Concrete(ty) => Ok(ty.clone()),
        ContractType::SameAs(name) => operands.get(*name).cloned().ok_or_else(|| {
            SemanticError::new(
                span,
                format!("action contract references unknown operand '{name}'"),
            )
        }),
        ContractType::AnyMaterial => Err(SemanticError::new(
            span,
            "action result cannot use unconstrained AnyMaterial",
        )),
    }
}

pub(super) fn action_reference(definition: DefinitionId, path: &str, ty: &Ty) -> TypedExpression {
    TypedExpression {
        r#type: to_checked_type(ty),
        value: CheckedExpression::Reference {
            definition,
            path: path.split('.').map(str::to_owned).collect(),
        },
    }
}

pub(super) fn checked_integer_literal(
    text: &str,
    signed: bool,
    span: Span,
) -> Result<TypedExpression, SemanticError> {
    if signed {
        let value = text.parse::<i64>().map_err(|_| {
            SemanticError::new(span, format!("expected an integer, found '{text}'"))
        })?;
        if value < 0 {
            return Ok(TypedExpression {
                r#type: CheckedType::Integer,
                value: CheckedExpression::Unary {
                    operator: "negate".to_owned(),
                    operand: Box::new(TypedExpression {
                        r#type: CheckedType::Integer,
                        value: CheckedExpression::Integer {
                            value: value.unsigned_abs(),
                        },
                    }),
                },
            });
        }
        return Ok(TypedExpression {
            r#type: CheckedType::Integer,
            value: CheckedExpression::Integer {
                value: value as u64,
            },
        });
    }
    let value = text.parse::<u64>().map_err(|_| {
        SemanticError::new(
            span,
            format!("expected a non-negative integer, found '{text}'"),
        )
    })?;
    Ok(TypedExpression {
        r#type: CheckedType::Integer,
        value: CheckedExpression::Integer { value },
    })
}

pub(super) fn require_action_type(
    actual: Ty,
    expected: Ty,
    span: Span,
    operation: &str,
) -> Result<(), SemanticError> {
    if compatible(&actual, &expected) {
        Ok(())
    } else {
        Err(SemanticError::new(
            span,
            format!("operation '{operation}' expects {expected}, found {actual}"),
        ))
    }
}
