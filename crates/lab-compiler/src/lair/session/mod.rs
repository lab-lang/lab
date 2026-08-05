mod compiler_session;
mod error;
mod options;
mod pass_pipeline;

pub use compiler_session::CompilerSession;
pub use error::SessionError;
pub use options::SessionOptions;
pub(crate) use pass_pipeline::RegisteredPass;
pub use pass_pipeline::{PassInfo, PassPipeline, PassPipelineError, registered_passes};
