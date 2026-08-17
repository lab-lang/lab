//! User-facing read-only checks for Lab's C3-first compute boundary.

use std::{env, ffi::OsString, path::PathBuf};

use anyhow::{Context, Result};
use lab_compute::{
    ComputeProvider, HardwareCatalog, HardwareProfile,
    c3::{C3Provider, dotenv_value},
};
use serde::Serialize;

use crate::Output;

#[derive(Debug, Serialize)]
struct ComputeDoctorReport {
    provider: &'static str,
    authenticated: bool,
    catalog_profiles: usize,
    isaac_compatible_profiles: Vec<HardwareProfile>,
}

fn provider(env_file: PathBuf) -> Result<C3Provider> {
    let program = env::var_os("LAB_C3_BIN").unwrap_or_else(|| OsString::from("c3"));
    let mut provider = C3Provider::new(program);
    if env_file.exists() {
        let api_key = dotenv_value(&env_file, "C3_API_KEY")?
            .with_context(|| format!("{} has no non-empty C3_API_KEY", env_file.display()))?;
        provider = provider.with_api_key(api_key);
    }
    Ok(provider)
}

fn isaac_compatible(profile: &HardwareProfile) -> bool {
    profile.available
        && profile.accelerator == "cuda"
        && profile
            .accelerator_memory_gb
            .is_some_and(|memory| memory >= 16)
        && profile.selector.to_ascii_lowercase().starts_with("l40")
}

pub(crate) fn doctor(env_file: PathBuf, output: &Output) -> Result<()> {
    let provider = provider(env_file)?;
    provider
        .authenticate()
        .context("C3 authentication failed")?;
    let catalog = provider
        .hardware_catalog()
        .context("failed to read C3 hardware catalog")?;
    let compatible = catalog
        .profiles
        .iter()
        .filter(|profile| isaac_compatible(profile))
        .cloned()
        .collect::<Vec<_>>();
    if compatible.is_empty() {
        anyhow::bail!(
            "C3 authentication succeeded, but no L40-class Isaac-compatible profile is catalogued"
        );
    }
    let names = compatible
        .iter()
        .map(|profile| profile.display_name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    output.success(
        "compute-doctor",
        ComputeDoctorReport {
            provider: provider.name(),
            authenticated: true,
            catalog_profiles: catalog.profiles.len(),
            isaac_compatible_profiles: compatible,
        },
        format!(
            "C3 authentication and catalog access passed\n  Isaac-compatible: {names}\n  no job was submitted"
        ),
    )
}

pub(crate) fn list(env_file: PathBuf, output: &Output) -> Result<()> {
    let provider = provider(env_file)?;
    let catalog = provider
        .hardware_catalog()
        .context("failed to read C3 hardware catalog")?;
    let human = catalog_table(&catalog);
    output.success("compute-catalog", catalog, human)
}

fn catalog_table(catalog: &HardwareCatalog) -> String {
    let mut lines = vec![format!("{} hardware", catalog.provider.to_uppercase())];
    for profile in &catalog.profiles {
        let memory = profile
            .accelerator_memory_gb
            .map(|value| format!("{value} GB"))
            .unwrap_or_else(|| "n/a".to_owned());
        let price = profile
            .price_per_hour
            .zip(profile.price_currency.as_deref())
            .map(|(value, currency)| format!("{value:.3} {currency}/h"))
            .unwrap_or_else(|| "price unavailable".to_owned());
        let availability = profile.availability.as_deref().unwrap_or("unknown");
        let isaac = if isaac_compatible(profile) {
            " [Isaac-compatible class]"
        } else {
            ""
        };
        lines.push(format!(
            "  {:<10} {:<20} {:<8} {:<18} {}{}",
            profile.selector, profile.display_name, memory, price, availability, isaac
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_l40_cuda_profiles_are_accepted_for_isaac() {
        let profile = |selector: &str, accelerator: &str, memory| HardwareProfile {
            selector: selector.to_owned(),
            display_name: selector.to_owned(),
            accelerator: accelerator.to_owned(),
            accelerator_count: 1,
            accelerator_memory_gb: memory,
            available: true,
            availability: Some("high".to_owned()),
            price_per_hour: None,
            price_currency: None,
        };
        assert!(isaac_compatible(&profile("l40", "cuda", Some(48))));
        assert!(isaac_compatible(&profile("l40s", "cuda", Some(48))));
        assert!(!isaac_compatible(&profile("a100", "cuda", Some(80))));
        assert!(!isaac_compatible(&profile("h100", "cuda", Some(80))));
        assert!(!isaac_compatible(&profile("l40", "none", Some(48))));
        let mut unavailable = profile("l40", "cuda", Some(48));
        unavailable.available = false;
        assert!(!isaac_compatible(&unavailable));
    }
}
