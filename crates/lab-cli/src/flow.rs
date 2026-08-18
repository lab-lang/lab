//! Package-aware resolution for the simulation commands.
//!
//! `lab simulate`, `lab scene`, and `lab render` accept either a run
//! directory (a workcell wave or a STAR package) or a package directory,
//! defaulting to the current one. Pointed at a package, they find the
//! built output of the manifest's default target under `.lab/build/` and
//! walk its waves in order.
//!
//! The facility resolves by convention unless `--facility` names one:
//! `facility.toml` at the package root (the single-facility case), then
//! the manifest's `[build] facility` pointer under `facilities/`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lab_package::LabManifest;

/// Where a command's run directories and facility ended up.
pub(crate) struct RunFlow {
    /// Ordered runnable directories: the waves of a build, or the one
    /// directory the caller named.
    pub waves: Vec<PathBuf>,
    pub facility: Option<PathBuf>,
}

/// True when the directory itself holds run documents.
fn is_run_directory(path: &Path) -> bool {
    path.join(lab_runfmt::WORKCELL_PLAN_FILE).is_file()
        || path.join("automation_manifest.json").is_file()
}

/// The nearest enclosing package root, for a wave directory that lives
/// under one.
fn package_root_above(path: &Path) -> Option<PathBuf> {
    let mut current = path.canonicalize().ok()?;
    for _ in 0..8 {
        if current.join("lab.toml").is_file() {
            return Some(current);
        }
        current = current.parent()?.to_path_buf();
    }
    None
}

fn package_manifest(root: &Path) -> Result<lab_package::PackageManifest> {
    let path = root.join("lab.toml");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    match LabManifest::parse(&text)
        .with_context(|| format!("failed to parse {}", path.display()))?
    {
        LabManifest::Package(manifest) => Ok(manifest),
        LabManifest::Workspace(_) => bail!(
            "{} is a workspace manifest; run this command from a member package",
            path.display()
        ),
    }
}

/// The facility a package implies: `facility.toml` at the root wins, then
/// the manifest's `[build] facility` pointer under `facilities/`.
fn facility_for_root(root: &Path) -> Result<Option<PathBuf>> {
    let single = root.join("facility.toml");
    if single.is_file() {
        return Ok(Some(single));
    }
    let manifest = package_manifest(root)?;
    if let Some(name) = &manifest.build.facility {
        let path = root
            .join(lab_runfmt::facility::FACILITY_DIR)
            .join(format!("{name}.toml"));
        if !path.is_file() {
            bail!(
                "the manifest names facility '{name}', but there is no {}",
                path.display()
            );
        }
        return Ok(Some(path));
    }
    Ok(None)
}

/// Resolves what to operate on. Explicit facility paths always win over
/// the package conventions.
pub(crate) fn resolve(path: &Path, explicit_facility: Option<PathBuf>) -> Result<RunFlow> {
    if is_run_directory(path) {
        let facility = match explicit_facility {
            Some(explicit) => Some(explicit),
            None => match package_root_above(path) {
                Some(root) => facility_for_root(&root)?,
                None => None,
            },
        };
        return Ok(RunFlow {
            waves: vec![path.to_path_buf()],
            facility,
        });
    }

    if !path.join("lab.toml").is_file() {
        bail!(
            "{} is neither a run directory nor a package: no {}, automation_manifest.json, or lab.toml",
            path.display(),
            lab_runfmt::WORKCELL_PLAN_FILE
        );
    }
    let manifest = package_manifest(path)?;
    let target = manifest
        .build
        .target
        .clone()
        .context("the manifest sets no [build] target; name a run directory instead")?;
    let build_dir = path.join(".lab").join("build").join(&target);
    if !build_dir.is_dir() {
        bail!(
            "no build output at {}; run `lab build` first",
            build_dir.display()
        );
    }

    let mut waves: Vec<PathBuf> = std::fs::read_dir(&build_dir)
        .with_context(|| format!("failed to read {}", build_dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|entry| {
            entry.is_dir()
                && entry
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("wave-"))
                && is_run_directory(entry)
        })
        .collect();
    waves.sort();
    if waves.is_empty() {
        if is_run_directory(&build_dir) {
            waves.push(build_dir.clone());
        } else {
            bail!(
                "{} holds no run documents to simulate; target '{target}' does not emit them — build a hamilton.star or workcell target",
                build_dir.display()
            );
        }
    }

    let facility = match explicit_facility {
        Some(explicit) => Some(explicit),
        None => facility_for_root(path)?,
    };
    Ok(RunFlow { waves, facility })
}

/// A short label for one wave in multi-wave output.
pub(crate) fn wave_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
