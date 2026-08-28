//! Adapter discovery and profile validation commands.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lab_compiler::backend::{
    AdapterCatalog, AdapterDescriptor, ValidatedAdapterProfile, adapter_catalog,
    default_adapter_profile, validate_adapter_profile,
};

use crate::Output;

pub(crate) fn describe(driver: Option<String>, output: &Output) -> Result<()> {
    let catalog = adapter_catalog()?;
    match driver {
        Some(driver) => {
            let descriptor = catalog
                .adapters
                .into_iter()
                .find(|adapter| adapter.id == driver)
                .with_context(|| format!("this compiler does not provide adapter '{driver}'"))?;
            let human = render_descriptor(&descriptor);
            output.success("adapter-described", descriptor, human)
        }
        None => {
            let human = render_catalog(&catalog);
            output.success("adapters-described", catalog, human)
        }
    }
}

pub(crate) fn default(driver: String, name: String, output: &Output) -> Result<()> {
    let profile = default_adapter_profile(&driver, &name)?;
    let human = profile.canonical_toml.clone();
    output.success("adapter-default", profile, human)
}

pub(crate) fn validate(driver: String, path: PathBuf, output: &Output) -> Result<()> {
    let profile = load_and_validate(&driver, &path)?;
    let human = format!(
        "Validated {} as {}\n  schema: {}\n  sha256: {}",
        path.display(),
        profile.driver,
        profile.schema_version,
        profile.sha256
    );
    output.success("adapter-validated", profile, human)
}

pub(crate) fn render(driver: String, path: PathBuf, output: &Output) -> Result<()> {
    let profile = load_and_validate(&driver, &path)?;
    let human = profile.canonical_toml.clone();
    output.success("adapter-rendered", profile, human)
}

pub(crate) fn load_and_validate(driver: &str, path: &Path) -> Result<ValidatedAdapterProfile> {
    if !path.is_file() {
        bail!("no adapter profile at {}", path.display());
    }
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .context("an adapter profile file needs a UTF-8 file name")?;
    validate_adapter_profile(driver, name, &contents).map_err(Into::into)
}

fn render_catalog(catalog: &AdapterCatalog) -> String {
    let mut lines = vec![format!(
        "Lab {} adapters ({})",
        catalog.compiler_version, catalog.profile_schema_version
    )];
    for adapter in &catalog.adapters {
        let services = [
            adapter.services.planning.then_some("planning"),
            adapter.services.simulation.then_some("simulation"),
            adapter.services.runtime.then_some("runtime"),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
        lines.push(format!(
            "  {:<24} {} ({services})",
            adapter.id, adapter.display_name
        ));
    }
    lines.join("\n")
}

fn render_descriptor(adapter: &AdapterDescriptor) -> String {
    format!(
        "{}\n  driver: {}\n  capabilities: {}\n  features: {}\n  control modes: {}\n  accepts: {}\n  emits: {}\n\n{}",
        adapter.display_name,
        adapter.id,
        join(&adapter.capabilities),
        join(&adapter.features),
        join(&adapter.control_modes),
        join(&adapter.accepted_run_formats),
        join(&adapter.emitted_run_formats),
        adapter.default_profile.canonical_toml
    )
}

fn join(values: &std::collections::BTreeSet<String>) -> String {
    if values.is_empty() {
        "none".to_owned()
    } else {
        values.iter().cloned().collect::<Vec<_>>().join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_validation_uses_the_explicit_driver_and_file_stem() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("star-1.toml");
        fs::write(&path, "[target]\nbackend = \"hamilton.star\"\n").unwrap();

        let profile = load_and_validate("hamilton.star", &path).unwrap();

        assert_eq!(profile.name, "star-1");
        assert_eq!(profile.driver, "hamilton.star");
    }
}
