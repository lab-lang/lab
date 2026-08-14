//! Freshness stamps for the simulation flow.
//!
//! Each step of `lab simulate` / `lab scene` / `lab render` fingerprints
//! its inputs (file identity, size, and modification time) together with
//! the settings that shape its output, and writes the fingerprint beside
//! the output. Rerunning with nothing changed skips the step; touching
//! any input, or changing a setting that matters, regenerates it. The
//! fingerprint is a local cache key, never a portable artifact.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

/// A fingerprint over input files and a settings string. Missing files
/// hash as absent rather than erroring: their appearance later changes
/// the fingerprint, which is exactly the point.
pub(crate) fn fingerprint(inputs: &[PathBuf], settings: &str) -> String {
    let mut paths: Vec<&PathBuf> = inputs.iter().collect();
    paths.sort();
    let mut hasher = DefaultHasher::new();
    settings.hash(&mut hasher);
    for path in paths {
        path.hash(&mut hasher);
        match std::fs::metadata(path) {
            Ok(metadata) => {
                metadata.len().hash(&mut hasher);
                if let Ok(modified) = metadata.modified() {
                    modified.hash(&mut hasher);
                }
            }
            Err(_) => "absent".hash(&mut hasher),
        }
    }
    format!("{:016x}", hasher.finish())
}

/// True when the stamp file records exactly this fingerprint.
pub(crate) fn is_fresh(stamp: &Path, fingerprint: &str) -> bool {
    std::fs::read_to_string(stamp)
        .map(|recorded| recorded.trim() == fingerprint)
        .unwrap_or(false)
}

pub(crate) fn write(stamp: &Path, fingerprint: &str) {
    // A failed stamp write only costs a rerun next time; never the run.
    let _ = std::fs::write(stamp, fingerprint);
}

/// The run documents a wave's simulation and scene derive from: the
/// coordination plan and every station document, or the STAR package's
/// manifest and run documents.
pub(crate) fn run_document_inputs(directory: &Path) -> Vec<PathBuf> {
    let mut inputs = Vec::new();
    let plan = directory.join(lab_runfmt::WORKCELL_PLAN_FILE);
    if plan.is_file() {
        inputs.push(plan);
        let stations = directory.join("stations");
        if let Ok(entries) = std::fs::read_dir(&stations) {
            for station in entries.flatten() {
                if let Ok(documents) = std::fs::read_dir(station.path()) {
                    for document in documents.flatten() {
                        let path = document.path();
                        if path
                            .extension()
                            .is_some_and(|extension| extension == "json")
                        {
                            inputs.push(path);
                        }
                    }
                }
            }
        }
    } else if let Ok(entries) = std::fs::read_dir(directory) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == "automation_manifest.json" || name.ends_with(".star.json") {
                inputs.push(path);
            }
        }
    }
    inputs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fingerprint_tracks_content_and_settings_changes() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("plan.json");
        std::fs::write(&input, "one").unwrap();
        let inputs = vec![input.clone()];

        let first = fingerprint(&inputs, "camera=dolly");
        assert_eq!(first, fingerprint(&inputs, "camera=dolly"), "stable");
        assert_ne!(
            first,
            fingerprint(&inputs, "camera=orbit"),
            "settings participate"
        );

        // A size change definitely lands regardless of mtime resolution.
        std::fs::write(&input, "changed").unwrap();
        assert_ne!(first, fingerprint(&inputs, "camera=dolly"));

        let stamp = directory.path().join(".step.stamp");
        write(&stamp, &first);
        assert!(is_fresh(&stamp, &first));
        assert!(!is_fresh(&stamp, "something-else"));
    }
}
