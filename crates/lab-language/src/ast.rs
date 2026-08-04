//! Spanned source-language AST.
//!
//! These nodes preserve what the author wrote. Name resolution, type checking,
//! laboratory-kind checking, and lowering belong to later frontend phases.

use serde::{Deserialize, Serialize};

use super::source::{Identifier, Span};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "item", rename_all = "snake_case")]
pub enum Item {
    Use(UseDecl),
    Circuit(CircuitDecl),
    Plasmid(PlasmidDecl),
    Data(DataDecl),
    Workflow(WorkflowDecl),
    Binding(BindingStmt),
}

impl Item {
    pub fn span(&self) -> Span {
        match self {
            Self::Use(item) => item.span,
            Self::Circuit(item) => item.span,
            Self::Plasmid(item) => item.span,
            Self::Data(item) => item.span,
            Self::Workflow(item) => item.span,
            Self::Binding(item) => item.span,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UseDecl {
    pub path: Path,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CircuitDecl {
    pub name: Identifier,
    pub parameters: Vec<TypeParameter>,
    pub inputs: Vec<FieldDecl>,
    pub output: Option<TypeExpr>,
    pub sections: Vec<Section>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TypeParameter {
    pub name: Identifier,
    pub bound: Option<TypeExpr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlasmidDecl {
    pub name: Identifier,
    pub members: Vec<PlasmidMember>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlasmidMember {
    Binding(BindingStmt),
    Requirement(ClaimStmt),
    Acceptance(ClaimStmt),
    Section(Section),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataKind {
    Record,
    Material,
    Observation,
    Evidence,
    Event,
    Outcome,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DataDecl {
    pub kind: DataKind,
    pub name: Identifier,
    pub parameters: Vec<TypeParameter>,
    pub fields: Vec<FieldDecl>,
    pub cases: Vec<CaseDecl>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaseDecl {
    pub name: Identifier,
    pub fields: Vec<FieldDecl>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldDecl {
    pub name: Identifier,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkflowDecl {
    pub name: Identifier,
    pub inputs: Vec<FieldDecl>,
    pub output: Option<TypeExpr>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub name: Identifier,
    pub entries: Vec<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClaimStmt {
    pub predicate: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Stmt {
    State(StateStmt),
    Binding(BindingStmt),
    Effect(EffectStmt),
    Return(ReturnStmt),
    If(IfStmt),
    Match(MatchStmt),
    For(ForStmt),
    When(WhenStmt),
    Emit(EmitStmt),
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Self::State(stmt) => stmt.span,
            Self::Binding(stmt) => stmt.span,
            Self::Effect(stmt) => stmt.span,
            Self::Return(stmt) => stmt.span,
            Self::If(stmt) => stmt.span,
            Self::Match(stmt) => stmt.span,
            Self::For(stmt) => stmt.span,
            Self::When(stmt) => stmt.span,
            Self::Emit(stmt) => stmt.span,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StateStmt {
    pub name: Identifier,
    pub ty: TypeExpr,
    pub initial: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BindingStmt {
    pub names: Vec<Identifier>,
    pub annotation: Option<TypeExpr>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectStmt {
    pub names: Vec<Identifier>,
    /// The source-preserving action phrase after `<-`.
    ///
    /// Phrase segmentation is intentionally deferred until action signatures
    /// and their module-resolution rules are specified.
    pub action: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReturnStmt {
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IfStmt {
    pub condition: Expr,
    pub then_body: Vec<Stmt>,
    pub else_body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatchStmt {
    pub value: Expr,
    pub cases: Vec<MatchCase>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatchCase {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ForStmt {
    pub binding: Identifier,
    pub iterable: Expr,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WhenStmt {
    pub trigger: Trigger,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    Every(Expr),
    After(Expr),
    Event(Expr),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EmitStmt {
    pub event: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Pattern {
    Name(Identifier),
    Constructor {
        path: Path,
        fields: Vec<PatternField>,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PatternField {
    pub field: Identifier,
    pub binding: Identifier,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeExpr {
    Path {
        path: Path,
        arguments: Vec<TypeExpr>,
        span: Span,
    },
    Union {
        alternatives: Vec<TypeExpr>,
        span: Span,
    },
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            Self::Path { span, .. } | Self::Union { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Path {
    pub segments: Vec<Identifier>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Expr {
    Path(Path),
    Integer {
        value: u64,
        span: Span,
    },
    Decimal {
        text: String,
        span: Span,
    },
    String {
        value: String,
        span: Span,
    },
    Quantity {
        magnitude: Box<Expr>,
        unit: String,
        span: Span,
    },
    List {
        elements: Vec<Expr>,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        arguments: Vec<Argument>,
        span: Span,
    },
    Record {
        constructor: Path,
        fields: Vec<FieldValue>,
        span: Span,
    },
    Field {
        subject: Box<Expr>,
        field: Identifier,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Self::Path(path) => path.span,
            Self::Integer { span, .. }
            | Self::Decimal { span, .. }
            | Self::String { span, .. }
            | Self::Quantity { span, .. }
            | Self::List { span, .. }
            | Self::Call { span, .. }
            | Self::Record { span, .. }
            | Self::Field { span, .. }
            | Self::Unary { span, .. }
            | Self::Binary { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Argument {
    pub name: Option<Identifier>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldValue {
    pub name: Identifier,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnaryOp {
    Negate,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BinaryOp {
    Or,
    And,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Range,
    Add,
    Subtract,
    Multiply,
    Divide,
}
