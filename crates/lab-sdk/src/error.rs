use lab_compiler::ModuleError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Module(#[from] ModuleError),
}
