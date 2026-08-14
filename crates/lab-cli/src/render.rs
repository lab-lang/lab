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
    pub facility: Option<PathBuf>,
    pub jobs: Option<usize>,
}

/// How many Blender processes render one wave. Previews are dominated by
/// per-frame overhead, so several processes scale nearly linearly; a
/// path-traced final already saturates the GPU, so it defaults to one.
fn effective_jobs(options: &RenderOptions) -> usize {
    if let Some(jobs) = options.jobs {
        return jobs.max(1);
    }
    if options.quality == "final" {
        return 1;
    }
    let cores = std::thread::available_parallelism()
        .map(|cores| cores.get())
        .unwrap_or(4);
    (cores / 2).clamp(1, 4)
}

/// Splits `1..=frame_end` into up to `jobs` contiguous slices.
fn frame_chunks(frame_end: u32, jobs: usize) -> Vec<(u32, u32)> {
    let jobs = (jobs as u32).clamp(1, frame_end);
    let base = frame_end / jobs;
    let remainder = frame_end % jobs;
    let mut chunks = Vec::new();
    let mut start = 1u32;
    for index in 0..jobs {
        let size = base + u32::from(index < remainder);
        if size == 0 {
            continue;
        }
        chunks.push((start, start + size - 1));
        start += size;
    }
    chunks
}

/// The footage frame count the player computes for this trace, mirrored
/// here so chunks can be assigned before Blender starts.
fn footage_frames(trace_path: &Path, options: &RenderOptions) -> Result<u32> {
    let text = std::fs::read_to_string(trace_path)
        .with_context(|| format!("failed to read {}", trace_path.display()))?;
    let trace: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse {}", trace_path.display()))?;
    let total = trace["summary"]["total_seconds"].as_f64().unwrap_or(0.0);
    Ok((total / options.speedup * f64::from(options.fps))
        .ceil()
        .max(2.0) as u32)
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

/// Renders one run directory whose scene and trace already exist.
/// Returns the movie path when ffmpeg assembled one.
fn render_wave(
    directory: &Path,
    options: &RenderOptions,
    out_dir_override: Option<PathBuf>,
) -> Result<(PathBuf, Option<PathBuf>)> {
    let scene_path = directory.join("scene.json");
    let trace_path = directory.join("sim-trace.json");
    let blender = find_blender(options.blender.as_deref())?;
    let out_dir = out_dir_override.unwrap_or_else(|| directory.join("renders"));
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;

    let script_path = out_dir.join("lab_blender.py");
    std::fs::write(&script_path, PLAYER)
        .with_context(|| format!("failed to write {}", script_path.display()))?;

    let build_command = |frames: Option<(u32, u32)>| {
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
        if let Some((start, stop)) = frames {
            command
                .arg("--frame-start")
                .arg(start.to_string())
                .arg("--frame-end")
                .arg(stop.to_string());
        }
        command
    };

    let jobs = if options.still.is_some() {
        1
    } else {
        effective_jobs(options)
    };
    if jobs == 1 {
        println!("rendering with {}", blender.display());
        let status = build_command(None)
            .status()
            .with_context(|| format!("failed to run {}", blender.display()))?;
        if !status.success() {
            bail!("Blender exited with {status}");
        }
    } else {
        // Every process builds the identical timeline and renders its own
        // slice of the frame range into the shared frames directory.
        let chunks = frame_chunks(footage_frames(&trace_path, options)?, jobs);
        println!(
            "rendering with {} across {} process(es)",
            blender.display(),
            chunks.len()
        );
        let mut children = Vec::new();
        for chunk in &chunks {
            let child = build_command(Some(*chunk))
                .spawn()
                .with_context(|| format!("failed to run {}", blender.display()))?;
            children.push((*chunk, child));
        }
        let mut failed = Vec::new();
        for (chunk, mut child) in children {
            let status = child
                .wait()
                .with_context(|| format!("failed to wait for frames {}..={}", chunk.0, chunk.1))?;
            if !status.success() {
                failed.push(format!("frames {}..={}: {status}", chunk.0, chunk.1));
            }
        }
        if !failed.is_empty() {
            bail!("Blender chunk(s) failed: {}", failed.join("; "));
        }
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

    Ok((out_dir, movie))
}

/// The whole cinematic flow: simulate, build the animated scene, render.
/// Every step is idempotent, so `lab render` alone is always enough.
pub(crate) fn render(directory: PathBuf, options: RenderOptions, output: &Output) -> Result<()> {
    let flow = crate::flow::resolve(&directory, options.facility.clone())?;
    let single = flow.waves.len() == 1;

    let mut sections = Vec::new();
    let mut reports = Vec::new();
    for wave in &flow.waves {
        let label = crate::flow::wave_label(wave);
        println!("== {label}: simulate ==");
        crate::simulate::simulate_wave(wave, flow.facility.as_deref(), None)?;
        println!("== {label}: scene ==");
        crate::scene::generate_for(wave, flow.facility.as_deref(), true, None)?;
        println!("== {label}: render ==");
        let out_override = if single {
            options.out_dir.clone()
        } else {
            None
        };
        let (out_dir, movie) = render_wave(wave, &options, out_override)?;
        sections.push(format!(
            "{label}: rendered under {}{}",
            out_dir.display(),
            match &movie {
                Some(path) => format!("\nmovie: {}", path.display()),
                None => String::new(),
            }
        ));
        reports.push(serde_json::json!({
            "wave": label,
            "out": out_dir.display().to_string(),
            "movie": movie.as_ref().map(|path| path.display().to_string()),
        }));
    }

    if let [report] = reports.as_slice() {
        let human = sections.remove(0);
        return output.success("render", report, human);
    }
    output.success("render", reports, sections.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_chunks_cover_the_range_exactly_once() {
        assert_eq!(frame_chunks(10, 4), [(1, 3), (4, 6), (7, 8), (9, 10)]);
        assert_eq!(frame_chunks(2, 8), [(1, 1), (2, 2)], "jobs cap at frames");
        assert_eq!(frame_chunks(315, 1), [(1, 315)]);
        let chunks = frame_chunks(1351, 4);
        assert_eq!(chunks.first().unwrap().0, 1);
        assert_eq!(chunks.last().unwrap().1, 1351);
        for pair in chunks.windows(2) {
            assert_eq!(pair[1].0, pair[0].1 + 1, "no gap, no overlap");
        }
    }
}
