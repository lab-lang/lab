//! Shared semantic identities and public module interfaces.
//!
//! Syntax uses source names. Once a name is resolved, compiler consumers use
//! these identities so that packages, the IDE, and lowering agree on which
//! declaration a reference denotes.

mod ids;
mod interface;

pub use ids::{DefinitionId, ModuleId};
pub use interface::{
    CallableSignature, ExportKind, ModuleExport, ModuleInterface, SemanticEnvironment,
};
