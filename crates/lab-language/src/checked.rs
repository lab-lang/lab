//! Verified, backend-neutral frontend IR.
//!
//! Every expression and action operand is structured and typed. Source text is
//! deliberately absent: later compiler passes must not reinterpret syntax.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::semantics::{DefinitionId, ModuleId, ModuleInterface};

/// The IR shape consumers must understand to read a checked module. A consumer
/// written against an earlier version cannot read this one: `Catalog` is a
/// declaration of its own and carries the properties its item states, `Data`
/// carries no category, a schema field states whether an instance may omit it,
/// an acceptance claim carries the evidence it is believed on, a role may name
/// the ontology term it stands for, and an artifact kind carries the roles its
/// produced type plays.
///
/// A consumer that ignores the last two reads a design with nothing said about
/// what it is, which is exactly the silence grounding exists to end. That is
/// why they raise the version rather than riding along as optional fields.
pub const PORTABLE_MODULE_SCHEMA_VERSION: &str = "lab.portable-module.v4";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckedModule {
    pub schema_version: String,
    pub module: ModuleId,
    pub doc: Option<String>,
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
    /// A part types can play. It has no members of its own: membership travels
    /// with the type that declares it, so a role stays open to types from other
    /// packages.
    Role {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        doc: Option<String>,
        name: String,
        /// The ontology term this role stands for, in its expanded IRI form.
        ///
        /// A role that names one grounds every type that plays it, which is how
        /// a Lab type resolves to the terms a document states about it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        term: Option<String>,
    },
    /// A name a supplier lists, and the Lab type it stands for.
    ///
    /// The identity is a field rather than an argument to a synthesized call,
    /// so a backend reads it directly instead of recognizing a call shape.
    Catalog {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        doc: Option<String>,
        name: String,
        r#type: CheckedType,
        identity: String,
        /// What the supplier's item states about itself, checked against the
        /// fields of the type it is listed as.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        properties: Vec<CheckedProperty>,
    },
    Circuit {
        doc: Option<String>,
        name: String,
        parameters: Vec<String>,
        /// What each type parameter is constrained to, where it is constrained.
        /// This is part of a circuit's public contract, not an internal detail
        /// of checking it.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        bounds: BTreeMap<String, CheckedType>,
        inputs: Vec<CheckedField>,
        output: CheckedType,
        sections: Vec<CheckedSection>,
    },
    /// A kind of artifact a package declares, and the schema its instances are
    /// checked against.
    ArtifactKind {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        doc: Option<String>,
        name: String,
        produces: CheckedType,
        /// The roles the produced type plays, in declaration order.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        roles: Vec<String>,
        fields: Vec<CheckedSchemaField>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        declares: Option<CheckedPresence>,
    },
    Artifact {
        doc: Option<String>,
        /// The word a package supplied for this kind.
        artifact: String,
        name: String,
        produces: CheckedType,
        properties: Vec<CheckedProperty>,
        requirements: Vec<TypedExpression>,
        acceptance: Vec<CheckedAcceptance>,
    },
    Data {
        doc: Option<String>,
        name: String,
        /// Type parameters in declaration order.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        parameters: Vec<String>,
        /// What each type parameter is constrained to, where it is constrained.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        bounds: BTreeMap<String, CheckedType>,
        /// Roles this type plays.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        roles: Vec<String>,
        fields: Vec<CheckedField>,
        cases: Vec<CheckedCase>,
    },
    Workflow {
        doc: Option<String>,
        name: String,
        /// Type parameters in declaration order.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        parameters: Vec<String>,
        /// What each type parameter is constrained to, where it is constrained.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        bounds: BTreeMap<String, CheckedType>,
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
    /// A type argument whose identity was deliberately discarded, constrained
    /// to a role.
    Any {
        role: String,
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
            Self::Any { role } => format!("any {role}"),
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

/// Which combinations of stated properties a kind calls complete.
///
/// A predicate over presence, not over values: its whole vocabulary is property
/// names combined with all, any, and not.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckedPresence {
    Property { name: String },
    All { parts: Vec<CheckedPresence> },
    Any { parts: Vec<CheckedPresence> },
    Not { part: Box<CheckedPresence> },
}

impl CheckedPresence {
    /// Whether a declaration that stated these properties is complete.
    pub fn satisfied_by(&self, stated: &std::collections::BTreeSet<String>) -> bool {
        match self {
            Self::Property { name } => stated.contains(name),
            Self::All { parts } => parts.iter().all(|part| part.satisfied_by(stated)),
            Self::Any { parts } => parts.iter().any(|part| part.satisfied_by(stated)),
            Self::Not { part } => !part.satisfied_by(stated),
        }
    }

    /// The rule written back as prose, for the error a reader meets.
    pub fn describe(&self) -> String {
        match self {
            Self::Property { name } => name.clone(),
            Self::All { parts } => parts
                .iter()
                .map(Self::describe)
                .collect::<Vec<_>>()
                .join(" and "),
            Self::Any { parts } => format!(
                "either {}",
                parts
                    .iter()
                    .map(Self::describe)
                    .collect::<Vec<_>>()
                    .join(", or ")
            ),
            Self::Not { part } => format!("no {}", part.describe()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckedField {
    pub name: String,
    pub r#type: CheckedType,
}

/// A property an artifact kind declares. Unlike a record's field, which every
/// value of that record has, a schema field may be one an instance omits.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckedSchemaField {
    pub name: String,
    pub r#type: CheckedType,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
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
    pub doc: Option<String>,
    pub targets: Vec<CheckedField>,
    pub value: TypedExpression,
}

/// An acceptance claim, with how much independent evidence it is believed on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckedAcceptance {
    pub predicate: TypedExpression,
    /// Independent biological lineages the evidence must span. Absent where
    /// neither the claim nor its declaration states a standard, which leaves
    /// the claim believed on whatever evidence is offered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicates: Option<u64>,
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
