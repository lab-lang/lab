//! C3 implementation of Lab's batch compute control-plane boundary.

use std::{
    ffi::{OsStr, OsString},
    fmt, fs,
    path::Path,
    process::{Command, Output},
};

use serde::Deserialize;

use crate::{
    ArtifactReference, ComputeError, ComputeJob, ComputeJobState, ComputeProvider, HardwareCatalog,
    HardwareProfile, JobSubmission,
};

/// A C3 provider driven through the stable machine-readable CLI surface.
pub struct C3Provider {
    program: OsString,
    api_key: Option<String>,
}

impl fmt::Debug for C3Provider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("C3Provider")
            .field("program", &self.program)
            .field("api_key", &self.api_key.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

impl Default for C3Provider {
    fn default() -> Self {
        Self::new("c3")
    }
}

impl C3Provider {
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            api_key: None,
        }
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn from_env_file(path: &Path) -> Result<Self, ComputeError> {
        let api_key = dotenv_value(path, "C3_API_KEY")?.ok_or_else(|| {
            ComputeError::EnvironmentFile(format!("{} has no non-empty C3_API_KEY", path.display()))
        })?;
        Ok(Self::default().with_api_key(api_key))
    }

    fn command<I, S>(&self, arguments: I, directory: Option<&Path>) -> Result<Output, ComputeError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(&self.program);
        command.args(arguments);
        if let Some(directory) = directory {
            command.current_dir(directory);
        }
        if let Some(api_key) = &self.api_key {
            command.env("C3_API_KEY", api_key);
        }
        let output = command.output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let message = if stderr.is_empty() {
                format!("exited with {}", output.status)
            } else {
                stderr
            };
            return Err(ComputeError::Command(message));
        }
        Ok(output)
    }

    fn json<I, S, T>(&self, arguments: I, directory: Option<&Path>) -> Result<T, ComputeError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
        T: for<'de> Deserialize<'de>,
    {
        let output = self.command(arguments, directory)?;
        Ok(serde_json::from_slice(&output.stdout)?)
    }

    pub fn artifact_reference(provider_job_id: &str) -> ArtifactReference {
        ArtifactReference {
            provider: "c3".to_owned(),
            provider_job_id: provider_job_id.to_owned(),
            remote_path: format!("/jobs/{provider_job_id}"),
        }
    }
}

impl ComputeProvider for C3Provider {
    fn name(&self) -> &'static str {
        "c3"
    }

    fn authenticate(&self) -> Result<(), ComputeError> {
        let identity: serde_json::Value = self.json(["whoami", "--json"], None)?;
        if !identity.is_object() {
            return Err(ComputeError::MissingField("C3 identity object"));
        }
        Ok(())
    }

    fn hardware_catalog(&self) -> Result<HardwareCatalog, ComputeError> {
        let catalog: C3Catalog = self.json(["list", "--json"], None)?;
        Ok(HardwareCatalog {
            provider: "c3".to_owned(),
            profiles: catalog
                .hardware
                .into_iter()
                .map(|profile| HardwareProfile {
                    selector: profile
                        .hardware_profile
                        .or(profile.gpu_profile)
                        .unwrap_or_else(|| profile.hardware_class.clone()),
                    display_name: profile
                        .display_name
                        .unwrap_or_else(|| profile.hardware_class.clone()),
                    accelerator: profile
                        .accelerator_kind
                        .unwrap_or_else(|| "unknown".to_owned()),
                    accelerator_count: profile.gpu_count.unwrap_or(0),
                    accelerator_memory_gb: profile.vram_gb,
                    available: profile.available.unwrap_or(false),
                    availability: profile.availability_tier,
                    price_per_hour: profile.rate_per_hour_gbp,
                    price_currency: profile.rate_per_hour_gbp.map(|_| "GBP".to_owned()),
                })
                .collect(),
        })
    }

    fn submit(&self, project_directory: &Path) -> Result<JobSubmission, ComputeError> {
        let submitted: C3Submission = self.json(["deploy", "--json"], Some(project_directory))?;
        Ok(JobSubmission {
            provider: "c3".to_owned(),
            provider_job_id: submitted.id,
            state: normalize_state(&submitted.status),
            hardware_profile: submitted.hardware_profile.or(submitted.gpu_profile),
            routed_provider: submitted.provider,
            dashboard_url: submitted.dashboard_url,
        })
    }

    fn jobs(&self) -> Result<Vec<ComputeJob>, ComputeError> {
        let jobs: Vec<C3Job> = self.json(["squeue", "--json"], None)?;
        Ok(jobs
            .into_iter()
            .map(|job| ComputeJob {
                provider: "c3".to_owned(),
                provider_job_id: job.id,
                name: job.job_name.or(job.name),
                project: job.project,
                state: normalize_state(&job.status),
                raw_state: job.status,
                hardware_profile: job.hardware_profile.or(job.gpu_profile),
                routed_provider: job.provider,
            })
            .collect())
    }

    fn logs(&self, provider_job_id: &str) -> Result<String, ComputeError> {
        let output = self.command(["logs", provider_job_id], None)?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn cancel(&self, provider_job_id: &str) -> Result<(), ComputeError> {
        self.command(["cancel", provider_job_id], None)?;
        Ok(())
    }

    fn pull(&self, provider_job_id: &str, destination: &Path) -> Result<(), ComputeError> {
        fs::create_dir_all(destination)?;
        let _: serde_json::Value =
            self.json(["pull", provider_job_id, "--json"], Some(destination))?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct C3Catalog {
    #[serde(default)]
    hardware: Vec<C3Hardware>,
}

#[derive(Debug, Deserialize)]
struct C3Hardware {
    hardware_class: String,
    hardware_profile: Option<String>,
    gpu_profile: Option<String>,
    display_name: Option<String>,
    accelerator_kind: Option<String>,
    gpu_count: Option<u32>,
    vram_gb: Option<u32>,
    available: Option<bool>,
    availability_tier: Option<String>,
    rate_per_hour_gbp: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct C3Submission {
    id: String,
    status: String,
    provider: Option<String>,
    hardware_profile: Option<String>,
    gpu_profile: Option<String>,
    dashboard_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct C3Job {
    #[serde(alias = "job_id")]
    id: String,
    status: String,
    project: Option<String>,
    job_name: Option<String>,
    name: Option<String>,
    provider: Option<String>,
    hardware_profile: Option<String>,
    gpu_profile: Option<String>,
}

fn normalize_state(state: &str) -> ComputeJobState {
    match state.to_ascii_uppercase().as_str() {
        "PENDING" => ComputeJobState::Queued,
        "SCHEDULING" | "PROVISIONING" | "STAGING" => ComputeJobState::Starting,
        "RUNNING" | "UPLOADING" => ComputeJobState::Running,
        "COMPLETED" | "SYNCED" => ComputeJobState::Succeeded,
        "FAILED" => ComputeJobState::Failed,
        "CANCELED" | "CANCELLED" => ComputeJobState::Canceled,
        "TIMED_OUT" | "TIMEOUT" => ComputeJobState::TimedOut,
        _ => ComputeJobState::Unknown,
    }
}

/// Read one value from a dotenv file without evaluating it as shell code.
pub fn dotenv_value(path: &Path, key: &str) -> Result<Option<String>, ComputeError> {
    let text = fs::read_to_string(path)?;
    for (index, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
        let Some((candidate, raw_value)) = line.split_once('=') else {
            continue;
        };
        if candidate.trim() != key {
            continue;
        }
        let raw_value = raw_value.trim();
        let value = if raw_value.len() >= 2
            && ((raw_value.starts_with('"') && raw_value.ends_with('"'))
                || (raw_value.starts_with('\'') && raw_value.ends_with('\'')))
        {
            &raw_value[1..raw_value.len() - 1]
        } else {
            raw_value
        };
        if value.is_empty() {
            return Err(ComputeError::EnvironmentFile(format!(
                "{} line {} has an empty {key}",
                path.display(),
                index + 1
            )));
        }
        return Ok(Some(value.to_owned()));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use std::{os::unix::fs::PermissionsExt, path::PathBuf};

    use super::*;

    fn fake_c3(script: &str) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("c3");
        fs::write(&path, format!("#!/bin/sh\nset -eu\n{script}\n")).unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
        (directory, path)
    }

    #[test]
    fn dotenv_reader_does_not_evaluate_shell_text() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(".env");
        fs::write(
            &path,
            "OTHER=value\nexport C3_API_KEY='c3_key_$(touch should-not-exist)'\n",
        )
        .unwrap();

        let value = dotenv_value(&path, "C3_API_KEY").unwrap().unwrap();
        assert_eq!(value, "c3_key_$(touch should-not-exist)");
        assert!(!directory.path().join("should-not-exist").exists());
    }

    #[test]
    fn provider_debug_output_redacts_the_key() {
        let provider = C3Provider::default().with_api_key("c3_key_secret");
        let text = format!("{provider:?}");
        assert!(text.contains("[redacted]"));
        assert!(!text.contains("c3_key_secret"));
    }

    #[test]
    fn catalog_is_normalized_from_c3_json() {
        let (_directory, program) = fake_c3(
            r#"
if [ "$1" = "list" ]; then
  printf '%s\n' '{"hardware":[{"hardware_class":"l40","hardware_profile":"l40","display_name":"NVIDIA L40","accelerator_kind":"cuda","gpu_count":1,"vram_gb":48,"available":true,"availability_tier":"high","rate_per_hour_gbp":0.948}]}'
fi
"#,
        );
        let provider = C3Provider::new(program);
        let catalog = provider.hardware_catalog().unwrap();
        assert_eq!(catalog.provider, "c3");
        assert_eq!(catalog.profiles.len(), 1);
        assert_eq!(catalog.profiles[0].selector, "l40");
        assert_eq!(catalog.profiles[0].accelerator_memory_gb, Some(48));
        assert_eq!(catalog.profiles[0].price_currency.as_deref(), Some("GBP"));
    }

    #[test]
    fn submission_and_job_states_are_normalized() {
        let (_directory, program) = fake_c3(
            r#"
case "$1" in
  deploy)
    printf '%s\n' '{"id":"job_train","status":"PENDING","provider":"nextgen","hardware_profile":"l40","dashboard_url":"https://example.invalid/job_train"}'
    ;;
  squeue)
    printf '%s\n' '[{"id":"job_train","status":"RUNNING","project":"lab-unitree","job_name":"train","provider":"nextgen","hardware_profile":"l40"}]'
    ;;
esac
"#,
        );
        let provider = C3Provider::new(program);
        let submission = provider.submit(Path::new(".")).unwrap();
        assert_eq!(submission.provider_job_id, "job_train");
        assert_eq!(submission.state, ComputeJobState::Queued);

        let jobs = provider.jobs().unwrap();
        assert_eq!(jobs[0].state, ComputeJobState::Running);
        assert_eq!(jobs[0].routed_provider.as_deref(), Some("nextgen"));
    }

    #[test]
    fn artifact_paths_are_provider_stable() {
        assert_eq!(
            C3Provider::artifact_reference("job_train").remote_path,
            "/jobs/job_train"
        );
    }
}
