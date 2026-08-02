use labc::CompilerError;
use labc::ParseError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    Compile(#[from] CompilerError),
}
