//! Platform-neutral editor intelligence for native, browser, and embedded hosts.

mod model;
mod semantic;
mod workspace;

pub use model::{
    CompletionItem, CompletionKind, DocumentSymbol, Hover, Location, SemanticToken,
    SemanticTokenKind, SymbolKind, TextEdit,
};
pub use workspace::Workspace;
