//! Verified, backend-neutral frontend IR.
//!
//! Every expression and action operand is structured and typed. Source text is
//! deliberately absent: later compiler passes must not reinterpret syntax.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::semantics::{DefinitionId, ModuleId, ModuleInterface};

pub const PORTABLE_MODULE_SCHEMA_VERSION: &str = "lab.portable-module.v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckedModule {
    pub schema_version: String,
    pub module: ModuleId,
    pub interface: ModuleInterface,
    pub imports: Vec<ResolvedImport>,
    pub declarations: Vec<CheckedDeclaration>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedImport {
    pub module: String,
    pub provider: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckedDeclaration {
    Circuit {
        name: String,
        parameters: Vec<String>,
        inputs: Vec<CheckedField>,
        output: CheckedType,
        sections: Vec<CheckedSection>,
    },
    Plasmid {
        name: String,
        properties: Vec<CheckedProperty>,
        requirements: Vec<TypedExpression>,
        acceptance: Vec<TypedExpression>,
    },
    Data {
        category: String,
        name: String,
        fields: Vec<CheckedField>,
        cases: Vec<CheckedCase>,
    },
    Workflow {
        name: String,
        inputs: Vec<CheckedField>,
        outputs: Vec<CheckedField>,
        state: Vec<CheckedState>,
        body: Vec<CheckedStatement>,
    },
    Binding(CheckedBinding),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckedType {
    Named {
        name: String,
        arguments: Vec<CheckedType>,
    },
    Union {
        alternatives: Vec<CheckedType>,
    },
    List {
        element: Box<CheckedType>,
    },
    Quantity {
        unit: String,
    },
    Integer,
    Decimal,
    String,
    Bool,
    None,
}

impl CheckedType {
    pub fn display_name(&self) -> String {
        match self {
            Self::Named { name, arguments } if arguments.is_empty() => name.clone(),
            Self::Named { name, arguments } => format!(
                "{name}<{}>",
                arguments
                    .iter()
                    .map(Self::display_name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Union { alternatives } => alternatives
                .iter()
                .map(Self::display_name)
                .collect::<Vec<_>>()
                .join(" | "),
            Self::List { element } => format!("List<{}>", element.display_name()),
            Self::Quantity { unit } => format!("Quantity<{unit}>"),
            Self::Integer => "Integer".to_owned(),
            Self::Decimal => "Decimal".to_owned(),
            Self::String => "String".to_owned(),
            Self::Bool => "Bool".to_owned(),
            Self::None => "None".to_owned(),
        }
    }
}

impl fmt::Display for CheckedType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.display_name())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckedField {
    pub name: String,
    pub r#type: CheckedType,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckedCase {
    pub name: String,
    pub fields: Vec<CheckedField>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckedSection {
    pub name: String,
    pub entries: Vec<TypedExpression>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckedBinding {
    pub targets: Vec<CheckedField>,
    pub value: TypedExpression,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckedProperty {
    pub name: String,
    pub value: TypedExpression,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckedState {
    pub name: String,
    pub r#type: CheckedType,
    pub initial: TypedExpression,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypedExpression {
    pub r#type: CheckedType,
    pub value: CheckedExpression,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckedExpression {
    Reference {
        definition: DefinitionId,
        path: Vec<String>,
    },
    Integer {
        value: u64,
    },
    Decimal {
        text: String,
    },
    String {
        value: String,
    },
    Quantity {
        magnitude: String,
        unit: String,
    },
    List {
        elements: Vec<TypedExpression>,
    },
    Call {
        operation: String,
        arguments: Vec<CheckedArgument>,
    },
    Construct {
        constructor: String,
        fields: Vec<CheckedFieldValue>,
    },
    Field {
        subject: Box<TypedExpression>,
        field: String,
    },
    Unary {
        operator: String,
        operand: Box<TypedExpression>,
    },
    Binary {
        operator: String,
        left: Box<TypedExpression>,
        right: Box<TypedExpression>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckedArgument {
    pub name: Option<String>,
    pub value: TypedExpression,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckedFieldValue {
    pub name: String,
    pub value: TypedExpression,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnershipMode {
    Copy,
    Borrow,
    Take,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedAction {
    pub operation: String,
    pub capability: Option<String>,
    pub arguments: Vec<CheckedActionArgument>,
    pub results: Vec<CheckedField>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckedActionArgument {
    pub name: String,
    pub mode: OwnershipMode,
    pub value: TypedExpression,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckedStatement {
    Binding(CheckedBinding),
    StateUpdate {
        state: String,
        value: TypedExpression,
    },
    Effect {
        results: Vec<CheckedField>,
        action: ResolvedAction,
    },
    Return {
        values: Vec<CheckedFieldValue>,
    },
    If {
        condition: TypedExpression,
        body: Vec<CheckedStatement>,
        else_body: Vec<CheckedStatement>,
    },
    Match {
        value: TypedExpression,
        cases: Vec<CheckedMatchCase>,
    },
    For {
        binding: CheckedField,
        iterable: TypedExpression,
        body: Vec<CheckedStatement>,
    },
    When {
        trigger: CheckedTrigger,
        body: Vec<CheckedStatement>,
    },
    Emit {
        event: TypedExpression,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckedMatchCase {
    pub pattern: CheckedPattern,
    pub body: Vec<CheckedStatement>,
    pub terminates: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckedPattern {
    Binding {
        name: String,
    },
    Constructor {
        constructor: String,
        fields: Vec<CheckedPatternField>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckedPatternField {
    pub field: String,
    pub binding: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckedTrigger {
    Every { duration: TypedExpression },
    After { duration: TypedExpression },
    Event { expression: TypedExpression },
}
