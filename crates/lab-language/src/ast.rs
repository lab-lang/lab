//! Spanned source-language AST.
//!
//! These nodes preserve what the author wrote. Name resolution, type checking,
//! laboratory-kind checking, and lowering belong to later frontend phases.

use serde::{Deserialize, Serialize};

use crate::source::{Identifier, Span};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub doc: Option<String>,
    pub items: Vec<Item>,
    pub span: Span,
}

/// The word instances of a type are written with: the type's own name, in
/// snake_case.
///
/// A break belongs where a word does: after a lowercase run, or at the end of
/// an acronym. `RestrictionEnzyme` gives `restriction_enzyme` and `DNA` gives
/// `dna` rather than `d_n_a`.
///
/// A tool building declarations without parsing them needs this, because a kind
/// names a type and an instance is written with the word. Deriving it anywhere
/// else would let the two disagree.
pub fn instance_word(type_name: &str) -> String {
    let characters = type_name.chars().collect::<Vec<_>>();
    let mut word = String::new();
    for (index, character) in characters.iter().enumerate() {
        let previous = index.checked_sub(1).map(|index| characters[index]);
        let next = characters.get(index + 1).copied();
        let opens_word = previous.is_some_and(|previous| !previous.is_uppercase());
        let ends_acronym = previous.is_some_and(char::is_uppercase)
            && next.is_some_and(|next| next.is_lowercase());
        if character.is_uppercase() && (opens_word || ends_acronym) {
            word.push('_');
        }
        word.extend(character.to_lowercase());
    }
    word
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "item", rename_all = "snake_case")]
pub enum Item {
    Use(UseDecl),
    Role(RoleDecl),
    ArtifactKind(ArtifactKindDecl),
    Circuit(CircuitDecl),
    Artifact(ArtifactDecl),
    Data(DataDecl),
    Workflow(WorkflowDecl),
    Binding(BindingStmt),
}

impl Item {
    pub fn span(&self) -> Span {
        match self {
            Self::Use(item) => item.span,
            Self::Role(item) => item.span,
            Self::ArtifactKind(item) => item.span,
            Self::Circuit(item) => item.span,
            Self::Artifact(item) => item.span,
            Self::Data(item) => item.span,
            Self::Workflow(item) => item.span,
            Self::Binding(item) => item.span,
        }
    }
}

/// A named part a type can play, such as `Signal` or `Reporter`.
///
/// A role classifies types; it has no values of its own. It carries no members
/// because membership is declared by the type that plays it, which keeps a role
/// open to types declared in other packages.
///
/// A role may name the ontology term it stands for. A role's whole content is
/// its identity, so the term is written after `=` rather than as a property:
/// `role Promoter = "https://identifiers.org/SO:0000167"`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoleDecl {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    pub name: Identifier,
    /// The ontology term this role stands for, where it names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub term: Option<Identifier>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UseDecl {
    pub path: Path,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CircuitDecl {
    pub doc: Option<String>,
    pub name: Identifier,
    pub inputs: Vec<FieldDecl>,
    pub output: TypeExpr,
    pub sections: Vec<Section>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TypeParameter {
    pub name: Identifier,
    pub bound: Option<TypeExpr>,
    pub span: Span,
}

/// A kind of artifact a package declares, such as `plasmid`.
///
/// The word introducing the declaration is the vocabulary; the block is the
/// schema its instances are checked against. A package supplies both, so the
/// parser never learns a new production — an unknown word followed by a name
/// and a block is always an artifact instance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArtifactKindDecl {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    pub name: Identifier,
    /// The type instances of this kind have, which is what a workflow names in
    /// `Material<Plasmid>` and what `require` and `accept` read fields from.
    pub produces: TypeExpr,
    /// The roles the produced type plays. A kind grounded in an ontology names
    /// the terms it stands for this way, so grounding is ordinary membership
    /// rather than a mechanism of its own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<Path>,
    pub fields: Vec<FieldDecl>,
    /// Which combinations of stated properties make a declaration complete.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declares: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArtifactDecl {
    pub doc: Option<String>,
    /// How this one came to exist. Being built is a fact about a particular
    /// thing rather than about its type: a plasmid may be assembled here or
    /// ordered from a supplier, and the same kind covers both.
    #[serde(default)]
    pub provenance: Provenance,
    /// The word that introduced this declaration, resolved to a kind while
    /// checking rather than while parsing.
    pub kind: Identifier,
    pub name: Identifier,
    /// The type this instance has, where its kind is generic and the word alone
    /// cannot say. `buy promoter pTet: Promoter<Tetracycline>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ascribed: Option<TypeExpr>,
    pub members: Vec<ArtifactMember>,
    pub span: Span,
}

/// Where a declared thing came from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// A laboratory makes it. It has a recipe, acceptance criteria, and a place
    /// in a build order.
    #[default]
    Build,
    /// A supplier lists it. It has an identity to order against and is never
    /// built.
    Buy,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArtifactMember {
    Property(PropertyDecl),
    Requirement(ClaimStmt),
    Acceptance(ClaimStmt),
    /// The evidentiary standard every claim in this declaration takes unless it
    /// states one of its own.
    Replication(Replication),
    Section(Section),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PropertyDecl {
    pub name: Identifier,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DataDecl {
    pub doc: Option<String>,
    pub name: Identifier,
    pub parameters: Vec<TypeParameter>,
    /// Roles this type plays, declared with `is`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<Path>,
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
    /// Whether a declaration may leave this field unstated, written `name?:`.
    ///
    /// The mark sits on the name because absence is a property of the field
    /// rather than of the type: an optional `Antibiotic` field still holds an
    /// `Antibiotic` whenever it is stated. Only an artifact kind's schema
    /// admits the mark, since only there is a field something an author may
    /// omit.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkflowDecl {
    pub doc: Option<String>,
    pub name: Identifier,
    pub inputs: Vec<FieldDecl>,
    pub outputs: WorkflowOutputs,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowOutputs {
    Single { ty: TypeExpr },
    Named { fields: Vec<FieldDecl> },
}

impl WorkflowOutputs {
    /// Every result type, however the results are declared.
    pub fn types(&self) -> Box<dyn Iterator<Item = &TypeExpr> + '_> {
        match self {
            Self::Single { ty } => Box::new(std::iter::once(ty)),
            Self::Named { fields } => Box::new(fields.iter().map(|field| &field.ty)),
        }
    }
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
    /// How many independent biological lineages the evidence for this claim
    /// must span, when the claim states its own standard rather than taking
    /// the declaration's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicates: Option<Replication>,
    pub span: Span,
}

/// `across 3 biological replicates` — how much independent evidence a claim
/// needs before it is believed.
///
/// Written on a declaration it sets the standard for every claim in it, and
/// written on one claim it sets the standard for that claim alone. Three
/// measurements of one colony are one biological replicate however many times
/// they are repeated, so this counts entities rather than measurements.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Replication {
    pub count: u64,
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
    pub doc: Option<String>,
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
    /// Where the phrase sits in the source, so a diagnostic about one operand
    /// can underline that operand instead of the whole statement.
    pub phrase: Span,
    pub span: Span,
}

impl EffectStmt {
    /// The phrase's words paired with their source ranges.
    pub fn words(&self) -> Vec<(&str, Span)> {
        let mut words = Vec::new();
        let mut cursor = 0;
        for word in self.action.split_whitespace() {
            let offset = self.action[cursor..]
                .find(word)
                .expect("the word came from this phrase")
                + cursor;
            let start = self.phrase.start + offset;
            words.push((word, Span::new(start, start + word.len())));
            cursor = offset + word.len();
        }
        words
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReturnStmt {
    pub values: Vec<Expr>,
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
        arguments: Vec<TypeArgument>,
        span: Span,
    },
    Union {
        alternatives: Vec<TypeExpr>,
        span: Span,
    },
    /// `Quantity<uL>` — a measurement in a stated unit.
    ///
    /// The argument is a unit rather than a type, so it is written the way a
    /// unit is written everywhere else: a name, optionally over a denominator.
    Quantity { unit: String, span: Span },
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            Self::Path { span, .. } | Self::Union { span, .. } | Self::Quantity { span, .. } => {
                *span
            }
        }
    }
}

/// One argument inside `<...>`.
///
/// An argument may introduce the type parameter it stands for, which is how a
/// signature says "some signal, and I am calling it S so I can name it again":
/// `Promoter<S: Signal>`. A binding is only meaningful in a declaration's
/// signature, so the parser accepts it anywhere and the checker decides where
/// it means something.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeArgument {
    Type(TypeExpr),
    Binding {
        name: Identifier,
        role: Path,
        span: Span,
    },
    /// `any Signal` — some type playing the role, deliberately not named.
    ///
    /// Naming a parameter and forgetting one are the same idea with and
    /// without a name: `S: Signal` can be pointed at again, `any Signal`
    /// cannot.
    Any {
        role: Path,
        span: Span,
    },
}

impl TypeArgument {
    pub fn span(&self) -> Span {
        match self {
            Self::Type(ty) => ty.span(),
            Self::Binding { span, .. } | Self::Any { span, .. } => *span,
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
