use thiserror::Error;

use super::PassPipelineError;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("this compiler session already contains an IR module")]
    ModuleAlreadyLoaded,
    #[error("this compiler session does not contain an IR module")]
    NoModule,
    #[error("failed to parse compiler IR: {0}")]
    ParseIr(String),
    #[error("expected a builtin.module root operation, found '{0}'")]
    ExpectedModule(String),
    #[error("compiler IR failed verification: {0}")]
    VerificationFailed(String),
    #[error("IR stage contract failed: {0}")]
    StageContract(String),
    #[error("pass '{name}' failed: {diagnostic}")]
    PassFailed { name: String, diagnostic: String },
    #[error(transparent)]
    PassPipeline(#[from] PassPipelineError),
}
