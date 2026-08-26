//! Target-profile discovery and validation commands.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lab_compiler::backend::{
    TargetCapabilitiesDocument, TargetCapability, ValidatedTargetProfile, default_target_profile,
    target_capabilities, validate_target_profile,
};

use crate::Output;

pub(crate) fn describe(backend: Option<String>, output: &Output) -> Result<()> {
    let document = target_capabilities()?;
    match backend {
        Some(backend) => {
            let capability = document
                .targets
                .into_iter()
                .find(|target| target.backend == backend)
                .with_context(|| {
                    format!("this compiler does not provide target backend '{backend}'")
                })?;
            let human = render_capability(&capability);
            output.success("target-described", capability, human)
        }
        None => {
            let human = render_capabilities(&document);
            output.success("targets-described", document, human)
        }
    }
}

pub(crate) fn default(backend: String, name: String, output: &Output) -> Result<()> {
    let profile = default_target_profile(&backend, &name)?;
    let human = profile.canonical_toml.clone();
    output.success("target-default", profile, human)
}

pub(crate) fn validate(path: PathBuf, output: &Output) -> Result<()> {
    let profile = load_and_validate(&path)?;
    let human = format!(
        "Validated {} as {}\n  schema: {}\n  sha256: {}",
        path.display(),
        profile.backend,
        profile.schema_version,
        profile.sha256
    );
    output.success("target-validated", profile, human)
}

pub(crate) fn render(path: PathBuf, output: &Output) -> Result<()> {
    let profile = load_and_validate(&path)?;
    let human = profile.canonical_toml.clone();
    output.success("target-rendered", profile, human)
}

fn load_and_validate(path: &Path) -> Result<ValidatedTargetProfile> {
    if !path.is_file() {
        bail!("no target profile at {}", path.display());
    }
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .context("a target profile file needs a UTF-8 file name")?;
    validate_target_profile(name, &contents).map_err(Into::into)
}

fn render_capabilities(document: &TargetCapabilitiesDocument) -> String {
    let mut lines = vec![format!(
        "Lab {} target profiles ({})",
        document.compiler_version, document.profile_schema_version
    )];
    for target in &document.targets {
        lines.push(format!("  {:<18} {}", target.backend, target.display_name));
    }
    lines.push("\nWorkcell station kinds:".to_string());
    for station in &document.station_kinds {
        let execution = match (station.planner_assigns_work, station.runtime_executor) {
            (true, true) => "planned and executable",
            (true, false) => "planned; no runtime executor",
            (false, true) => "executable; no planner assignment",
            (false, false) => "declared only",
        };
        lines.push(format!(
            "  {:<24} {} ({execution})",
            station.kind, station.display_name
        ));
    }
    lines.join("\n")
}

fn render_capability(capability: &TargetCapability) -> String {
    format!(
        "{}\n  backend: {}\n  capabilities: {}\n\n{}",
        capability.display_name,
        capability.backend,
        capability
            .capabilities
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        capability.default_profile.canonical_toml
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_uses_the_file_stem_as_the_profile_name() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flex-bay.toml");
        fs::write(&path, "[target]\nbackend = \"opentrons.flex\"\n").unwrap();
        let profile = load_and_validate(&path).unwrap();
        assert_eq!(profile.name, "flex-bay");
        assert_eq!(profile.backend, "opentrons.flex");
    }
}
