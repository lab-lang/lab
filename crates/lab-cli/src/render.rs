//! The `lab render` command: the cinematic tier.
//!
//! Blender plays the same two documents the web player and the USD stage
//! consume, headless, and renders frames with Cycles or EEVEE. The player
//! script ships inside this binary and is written next to the output on
//! each run, so `lab render` works wherever the binary does. Blender
//! itself is found, never bundled: a missing installation is a clear
//! error naming the fix, not a build dependency.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::Output;

/// The Blender player, embedded at build time.
const PLAYER: &str = include_str!("../../../render/lab_blender.py");

pub(crate) struct RenderOptions {
    pub camera: String,
    pub speedup: f64,
    pub fps: u32,
    pub quality: String,
    pub still: Option<f64>,
    pub hdri: Option<PathBuf>,
    pub blender: Option<PathBuf>,
    pub out_dir: Option<PathBuf>,
}

/// Finds a Blender to run: the flag, the environment, the path, then the
/// standard macOS application bundle.
fn find_blender(flag: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = flag {
        return Ok(path.to_path_buf());
    }
    if let Ok(path) = std::env::var("LAB_BLENDER") {
        return Ok(PathBuf::from(path));
    }
    if let Ok(output) = Command::new("which").arg("blender").output()
        && output.status.success()
    {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    let bundle = Path::new("/Applications/Blender.app/Contents/MacOS/Blender");
    if bundle.is_file() {
        return Ok(bundle.to_path_buf());
    }
    bail!(
        "no Blender found; install it (macOS: `brew install --cask blender`) or point --blender/LAB_BLENDER at the executable"
    );
}

pub(crate) fn render(directory: PathBuf, options: RenderOptions, output: &Output) -> Result<()> {
    let scene_path = directory.join("scene.json");
    if !scene_path.is_file() {
        bail!(
            "no scene at {}; run `lab scene` on this package first",
            scene_path.display()
        );
    }
    let trace_path = directory.join("sim-trace.json");
    if !trace_path.is_file() {
        bail!(
            "no trace at {}; run `lab simulate` on this package first",
            trace_path.display()
        );
    }
    let blender = find_blender(options.blender.as_deref())?;
    let out_dir = options.out_dir.unwrap_or_else(|| directory.join("renders"));
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;

    let script_path = out_dir.join("lab_blender.py");
    std::fs::write(&script_path, PLAYER)
        .with_context(|| format!("failed to write {}", script_path.display()))?;

    let mut command = Command::new(&blender);
    command
        .arg("--background")
        .arg("--factory-startup")
        .arg("--python-exit-code")
        .arg("1")
        .arg("--python")
        .arg(&script_path)
        .arg("--")
        .arg("--scene")
        .arg(&scene_path)
        .arg("--trace")
        .arg(&trace_path)
        .arg("--out")
        .arg(&out_dir)
        .arg("--camera")
        .arg(&options.camera)
        .arg("--speedup")
        .arg(options.speedup.to_string())
        .arg("--fps")
        .arg(options.fps.to_string())
        .arg("--quality")
        .arg(&options.quality);
    if let Some(still) = options.still {
        command.arg("--still").arg(still.to_string());
    }
    if let Some(hdri) = &options.hdri {
        command.arg("--hdri").arg(hdri);
    }

    println!("rendering with {}", blender.display());
    let status = command
        .status()
        .with_context(|| format!("failed to run {}", blender.display()))?;
    if !status.success() {
        bail!("Blender exited with {status}");
    }

    // Assemble a movie when ffmpeg is around; the frames stay either way.
    let mut movie = None;
    if options.still.is_none()
        && Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_ok_and(|probe| probe.status.success())
    {
        let movie_path = out_dir.join("run.mp4");
        let assembled = Command::new("ffmpeg")
            .args(["-y", "-framerate", &options.fps.to_string(), "-i"])
            .arg(out_dir.join("frames/%04d.png"))
            .args(["-pix_fmt", "yuv420p"])
            .arg(&movie_path)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if assembled {
            movie = Some(movie_path);
        }
    }

    output.success(
        "render",
        serde_json::json!({
            "out": out_dir.display().to_string(),
            "movie": movie.as_ref().map(|path| path.display().to_string()),
        }),
        format!(
            "rendered under {}{}",
            out_dir.display(),
            match &movie {
                Some(path) => format!("\nmovie: {}", path.display()),
                None => String::new(),
            }
        ),
    )
}
