//! Provider-neutral contracts for finite, artifact-producing compute jobs.
//!
//! A compute provider owns placement, provisioning, and transport. Lab owns
//! the identity and state of the work it requested and the provenance of the
//! artifacts it receives. Robot-task semantics do not cross this boundary.

pub mod c3;

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The normalized lifecycle shared by every batch compute provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComputeJobState {
    Queued,
    Starting,
    Running,
    Succeeded,
    Failed,
    Canceled,
    TimedOut,
    Unknown,
}

impl ComputeJobState {
    /// Whether the provider has reached a terminal state for this job.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Canceled | Self::TimedOut
        )
    }
}

/// One provider-visible hardware choice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardwareProfile {
    pub selector: String,
    pub display_name: String,
    pub accelerator: String,
    pub accelerator_count: u32,
    pub accelerator_memory_gb: Option<u32>,
    pub available: bool,
    pub availability: Option<String>,
    pub price_per_hour: Option<f64>,
    pub price_currency: Option<String>,
}

/// A provider's current public hardware catalog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardwareCatalog {
    pub provider: String,
    pub profiles: Vec<HardwareProfile>,
}

/// The durable identity returned after a provider accepts a job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobSubmission {
    pub provider: String,
    pub provider_job_id: String,
    pub state: ComputeJobState,
    pub hardware_profile: Option<String>,
    pub routed_provider: Option<String>,
    pub dashboard_url: Option<String>,
}

/// A normalized provider job-list entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputeJob {
    pub provider: String,
    pub provider_job_id: String,
    pub name: Option<String>,
    pub project: Option<String>,
    pub state: ComputeJobState,
    pub raw_state: String,
    pub hardware_profile: Option<String>,
    pub routed_provider: Option<String>,
}

/// A provider-neutral reference to one remote artifact tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactReference {
    pub provider: String,
    pub provider_job_id: String,
    pub remote_path: String,
}

/// The small control-plane surface Lab needs from a batch provider.
///
/// `submit` accepts a provider-ready project directory. Compiling a semantic
/// robot task into such a project belongs to the robot-training integration,
/// not to this control-plane trait.
pub trait ComputeProvider {
    fn name(&self) -> &'static str;

    fn authenticate(&self) -> Result<(), ComputeError>;

    fn hardware_catalog(&self) -> Result<HardwareCatalog, ComputeError>;

    fn submit(&self, project_directory: &Path) -> Result<JobSubmission, ComputeError>;

    fn jobs(&self) -> Result<Vec<ComputeJob>, ComputeError>;

    fn logs(&self, provider_job_id: &str) -> Result<String, ComputeError>;

    fn cancel(&self, provider_job_id: &str) -> Result<(), ComputeError>;

    fn pull(&self, provider_job_id: &str, destination: &Path) -> Result<(), ComputeError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ComputeError {
    #[error("failed to run compute provider command: {0}")]
    Io(#[from] std::io::Error),
    #[error("compute provider command failed: {0}")]
    Command(String),
    #[error("compute provider returned invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("compute provider response is missing {0}")]
    MissingField(&'static str),
    #[error("invalid environment file: {0}")]
    EnvironmentFile(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_terminal_states_stop_polling() {
        assert!(!ComputeJobState::Queued.is_terminal());
        assert!(!ComputeJobState::Running.is_terminal());
        assert!(ComputeJobState::Succeeded.is_terminal());
        assert!(ComputeJobState::Failed.is_terminal());
        assert!(ComputeJobState::Canceled.is_terminal());
        assert!(ComputeJobState::TimedOut.is_terminal());
    }
}
