use std::env;
use std::fs;
use std::io;
use std::path::Path;

use anyhow::{Context, Result, bail};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Output;

const REPO: &str = "lab-lang/lab";
const USER_AGENT: &str = concat!("lab-cli/", env!("CARGO_PKG_VERSION"));
const BINARIES: [&str; 3] = ["lab", "labc", "lab-opt"];

pub(crate) fn update(check: bool, output: &Output) -> Result<()> {
    let current = Version::parse(env!("CARGO_PKG_VERSION"))
        .context("failed to parse the running lab version")?;
    let release = latest_release().context("failed to check the latest lab release")?;
    let latest = Version::parse(release.tag_name.trim_start_matches('v'))
        .with_context(|| format!("failed to parse release tag {}", release.tag_name))?;

    if latest <= current {
        return output.success(
            "up-to-date",
            UpdateStatus {
                current: current.to_string(),
                latest: latest.to_string(),
                updated: Vec::new(),
            },
            format!("lab {current} is already the latest version"),
        );
    }

    if check {
        return output.success(
            "update-available",
            UpdateStatus {
                current: current.to_string(),
                latest: latest.to_string(),
                updated: Vec::new(),
            },
            format!("lab {latest} is available (running {current}); run `lab update` to install it"),
        );
    }

    let target = target_triple().context(
        "no published lab release covers this platform; build from source instead",
    )?;
    let install_dir = env::current_exe()
        .context("failed to locate the running lab executable")?
        .parent()
        .context("the running lab executable has no parent directory")?
        .to_path_buf();

    let asset_name = format!("lab-{target}.tar.gz");
    let asset_bytes = download_bytes(&release.tag_name, &asset_name)
        .with_context(|| format!("failed to download {asset_name}"))?;
    let checksums = download_text(&release.tag_name, "SHA256SUMS")
        .context("failed to download SHA256SUMS")?;
    verify_checksum(&checksums, &asset_name, &asset_bytes)?;

    let extract_dir = tempfile::tempdir().context("failed to create a temporary directory")?;
    extract_tar_gz(&asset_bytes, extract_dir.path())?;

    let mut updated = Vec::new();
    for binary in BINARIES {
        let file_name = binary_file_name(binary);
        let extracted = extract_dir.path().join(&file_name);
        let installed = install_dir.join(&file_name);
        // install.sh always places lab/labc/lab-opt together; a binary
        // missing from the install directory was placed there some other
        // way, so leave it alone rather than guessing.
        if !extracted.is_file() || !installed.is_file() {
            continue;
        }

        if binary == "lab" {
            self_replace::self_replace(&extracted)
                .with_context(|| format!("failed to replace the running {file_name}"))?;
        } else {
            replace_sibling(&installed, &extracted)
                .with_context(|| format!("failed to replace {file_name}"))?;
        }
        updated.push(binary.to_string());
    }

    if updated.is_empty() {
        bail!(
            "no lab/labc/lab-opt binaries found next to the running executable at {}",
            install_dir.display()
        );
    }

    output.success(
        "updated",
        UpdateStatus {
            current: current.to_string(),
            latest: latest.to_string(),
            updated: updated.clone(),
        },
        format!("Updated {} from {current} to {latest}", updated.join(", ")),
    )
}

fn latest_release() -> Result<LatestRelease> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let body = ureq::get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .with_context(|| format!("request to {url} failed"))?
        .body_mut()
        .read_to_string()
        .with_context(|| format!("failed to read the response body from {url}"))?;
    serde_json::from_str(&body)
        .with_context(|| format!("failed to parse the response body from {url}"))
}

fn download_bytes(tag: &str, asset: &str) -> Result<Vec<u8>> {
    let url = release_asset_url(tag, asset);
    ureq::get(&url)
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("request to {url} failed"))?
        .body_mut()
        .read_to_vec()
        .with_context(|| format!("failed to read the response body from {url}"))
}

fn download_text(tag: &str, asset: &str) -> Result<String> {
    let url = release_asset_url(tag, asset);
    ureq::get(&url)
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("request to {url} failed"))?
        .body_mut()
        .read_to_string()
        .with_context(|| format!("failed to read the response body from {url}"))
}

fn release_asset_url(tag: &str, asset: &str) -> String {
    format!("https://github.com/{REPO}/releases/download/{tag}/{asset}")
}

fn verify_checksum(checksums: &str, asset_name: &str, bytes: &[u8]) -> Result<()> {
    let expected = checksums
        .lines()
        .find_map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next()?;
            let name = parts.next()?.trim_start_matches('*');
            (name == asset_name).then(|| hash.to_owned())
        })
        .with_context(|| format!("SHA256SUMS has no entry for {asset_name}"))?;

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let actual = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    if !actual.eq_ignore_ascii_case(&expected) {
        bail!("checksum mismatch for {asset_name}: expected {expected}, got {actual}");
    }
    Ok(())
}

fn extract_tar_gz(bytes: &[u8], dest: &Path) -> Result<()> {
    let decoder = flate2::read::GzDecoder::new(bytes);
    tar::Archive::new(decoder)
        .unpack(dest)
        .with_context(|| format!("failed to extract archive into {}", dest.display()))
}

fn target_triple() -> Option<&'static str> {
    match (env::consts::OS, env::consts::ARCH) {
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
        ("windows", "x86_64") => Some("x86_64-pc-windows-msvc"),
        _ => None,
    }
}

fn binary_file_name(binary: &str) -> String {
    if cfg!(windows) {
        format!("{binary}.exe")
    } else {
        binary.to_owned()
    }
}

/// Replace an installed binary that isn't the currently running process
/// (`self_replace` only knows how to replace `current_exe()`). Stages the
/// new file next to the target and renames over it, which is atomic on
/// every platform Rust's `fs::rename` supports.
fn replace_sibling(installed: &Path, new_binary: &Path) -> Result<()> {
    let permissions = fs::metadata(installed)
        .with_context(|| format!("failed to read metadata for {}", installed.display()))?
        .permissions();
    let parent = installed
        .parent()
        .with_context(|| format!("{} has no parent directory", installed.display()))?;

    let mut staged = tempfile::Builder::new()
        .prefix(".lab-update-")
        .tempfile_in(parent)
        .with_context(|| format!("failed to create a temporary file in {}", parent.display()))?;
    let mut source = fs::File::open(new_binary)
        .with_context(|| format!("failed to open {}", new_binary.display()))?;
    io::copy(&mut source, staged.as_file_mut())
        .with_context(|| format!("failed to stage the replacement for {}", installed.display()))?;
    staged
        .as_file()
        .set_permissions(permissions)
        .with_context(|| format!("failed to set permissions on the replacement for {}", installed.display()))?;

    let (_, staged_path) = staged
        .keep()
        .context("failed to persist the staged replacement")?;
    fs::rename(&staged_path, installed).with_context(|| {
        format!(
            "failed to move the staged replacement into {}",
            installed.display()
        )
    })
}

#[derive(Deserialize)]
struct LatestRelease {
    tag_name: String,
}

#[derive(Serialize)]
struct UpdateStatus {
    current: String,
    latest: String,
    updated: Vec<String>,
}
