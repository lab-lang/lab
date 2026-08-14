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
    pub hold_seconds: f64,
    pub uniform: bool,
}

/// The spans of a run where something visibly moves on camera: a liquid
/// handler working its frames (the head glides program-long), and labware
/// traveling for two simulated seconds after each confirmed handoff.
fn motion_intervals(trace: &lab_runfmt::SimTraceDocument) -> Vec<(f64, f64)> {
    use lab_runfmt::RunEvent;
    let mut intervals: Vec<(f64, f64)> = Vec::new();
    let mut program_start: Option<f64> = None;
    for timed in &trace.events {
        match &timed.event {
            RunEvent::Frame { x_mm: Some(_), .. } => {
                if program_start.is_none() {
                    program_start = Some(timed.t);
                }
            }
            RunEvent::NodeCompleted { .. } => {
                if let Some(start) = program_start.take() {
                    intervals.push((start, timed.t));
                }
            }
            RunEvent::LabwareMoved { .. } => {
                intervals.push((timed.t, timed.t + 2.0));
            }
            _ => {}
        }
    }
    if let Some(start) = program_start {
        intervals.push((start, trace.summary.total_seconds));
    }
    intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Merge overlaps so the warp is strictly increasing.
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for interval in intervals {
        match merged.last_mut() {
            Some(last) if interval.0 <= last.1 => last.1 = last.1.max(interval.1),
            _ => merged.push(interval),
        }
    }
    merged
}

/// Warps simulated seconds onto footage seconds: motion plays at the
/// requested speedup, every hold between motions compresses to a fixed
/// beat. The result is a strictly increasing piecewise-linear map.
struct TimeWarp {
    /// `(sim_start, sim_end, footage_start, footage_per_sim_second)`
    segments: Vec<(f64, f64, f64, f64)>,
    footage_end: f64,
}

impl TimeWarp {
    fn build(trace: &lab_runfmt::SimTraceDocument, options: &RenderOptions) -> Self {
        let total = trace.summary.total_seconds;
        let motions = motion_intervals(trace);
        let motion_rate = 1.0 / options.speedup.max(1.0);
        let mut segments = Vec::new();
        let mut sim_cursor = 0.0;
        let mut footage_cursor = 0.0;
        let push = |from: f64,
                    to: f64,
                    rate: f64,
                    footage: &mut f64,
                    segments: &mut Vec<(f64, f64, f64, f64)>| {
            if to > from {
                segments.push((from, to, *footage, rate));
                *footage += (to - from) * rate;
            }
        };
        for (start, end) in motions {
            let hold = start - sim_cursor;
            if hold > 0.0 {
                // A hold always gets its beat, however long it really is.
                let rate = options.hold_seconds / hold;
                push(sim_cursor, start, rate, &mut footage_cursor, &mut segments);
            }
            push(
                start,
                end.min(total),
                motion_rate,
                &mut footage_cursor,
                &mut segments,
            );
            sim_cursor = end.min(total);
        }
        if sim_cursor < total {
            let rate = options.hold_seconds / (total - sim_cursor);
            push(sim_cursor, total, rate, &mut footage_cursor, &mut segments);
        }
        TimeWarp {
            segments,
            footage_end: footage_cursor,
        }
    }

    fn warp(&self, t: f64) -> f64 {
        for (from, to, footage_start, rate) in &self.segments {
            if t <= *to {
                return footage_start + (t.max(*from) - from) * rate;
            }
        }
        self.footage_end
    }
}

/// Writes the render-ready trace: event times in footage seconds, so the
/// player runs at speedup one and the camera spans the condensed length.
fn condensed_trace(
    trace_path: &Path,
    out_dir: &Path,
    options: &RenderOptions,
) -> Result<(PathBuf, f64, Option<f64>)> {
    let text = std::fs::read_to_string(trace_path)
        .with_context(|| format!("failed to read {}", trace_path.display()))?;
    let mut trace: lab_runfmt::SimTraceDocument = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse {}", trace_path.display()))?;
    let warp = TimeWarp::build(&trace, options);
    for timed in &mut trace.events {
        timed.t = warp.warp(timed.t);
    }
    for window in &mut trace.summary.attention_windows {
        window.from_seconds = warp.warp(window.from_seconds);
        window.to_seconds = warp.warp(window.to_seconds);
    }
    trace.summary.total_seconds = warp.footage_end;
    let still = options.still.map(|t| warp.warp(t));
    let path = out_dir.join("render-trace.json");
    std::fs::write(&path, serde_json::to_string_pretty(&trace)?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok((path, warp.footage_end, still))
}

/// How many Blender processes render one wave. Previews are dominated by
/// per-frame overhead, so they use every core; a path-traced final
/// already saturates the GPU, so it defaults to one.
fn effective_jobs(options: &RenderOptions) -> usize {
    if let Some(jobs) = options.jobs {
        return jobs.max(1);
    }
    if options.quality == "final" {
        return 1;
    }
    std::thread::available_parallelism()
        .map(|cores| cores.get())
        .unwrap_or(1)
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
) -> Result<(PathBuf, Option<PathBuf>, bool)> {
    let scene_path = directory.join("scene.json");
    let trace_path = directory.join("sim-trace.json");
    let out_dir = out_dir_override.unwrap_or_else(|| directory.join("renders"));
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;

    // Skip Blender entirely when the documents and the settings that
    // shape the footage both match the last run.
    let settings = format!(
        "render-v2;camera={};speedup={};fps={};quality={};still={:?};hdri={:?};hold={};uniform={}",
        options.camera,
        options.speedup,
        options.fps,
        options.quality,
        options.still,
        options.hdri,
        options.hold_seconds,
        options.uniform
    );
    let print = crate::stamp::fingerprint(&[scene_path.clone(), trace_path.clone()], &settings);
    let stamp_path = out_dir.join(".render.stamp");
    let outputs_exist = match options.still {
        Some(_) => out_dir.join("frames/still.png").is_file(),
        None => out_dir.join("run.mp4").is_file() || out_dir.join("frames/0001.png").is_file(),
    };
    if outputs_exist && crate::stamp::is_fresh(&stamp_path, &print) {
        let movie = out_dir.join("run.mp4");
        return Ok((out_dir.clone(), movie.is_file().then_some(movie), true));
    }
    let blender = find_blender(options.blender.as_deref())?;

    // Condensed time is the default: motion at --speedup, holds squeezed
    // to their beat. The warped trace lives beside the renders and the
    // player runs it at speedup one; --uniform keeps real proportions.
    let (script_trace, script_speedup, script_still, footage_seconds) = if options.uniform {
        (trace_path.clone(), options.speedup, options.still, None)
    } else {
        let (path, footage_end, still) = condensed_trace(&trace_path, &out_dir, options)?;
        (path, 1.0, still, Some(footage_end))
    };

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
            .arg(&script_trace)
            .arg("--out")
            .arg(&out_dir)
            .arg("--camera")
            .arg(&options.camera)
            .arg("--speedup")
            .arg(script_speedup.to_string())
            .arg("--fps")
            .arg(options.fps.to_string())
            .arg("--quality")
            .arg(&options.quality);
        if let Some(still) = script_still {
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

    // Stale frames from an earlier, longer cut would ride into the movie:
    // an animation render owns its frames directory outright.
    if options.still.is_none() {
        let frames_dir = out_dir.join("frames");
        if frames_dir.is_dir() {
            std::fs::remove_dir_all(&frames_dir)
                .with_context(|| format!("failed to clear {}", frames_dir.display()))?;
        }
    }

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
        let frame_total = match footage_seconds {
            Some(seconds) => (seconds * f64::from(options.fps)).ceil().max(2.0) as u32,
            None => footage_frames(&trace_path, options)?,
        };
        let chunks = frame_chunks(frame_total, jobs);
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
    crate::stamp::write(&stamp_path, &print);

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

    Ok((out_dir, movie, false))
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
        let (_, _, sim_fresh) =
            crate::simulate::simulate_wave(wave, flow.facility.as_deref(), None)?;
        println!(
            "== {label}: simulate{} ==",
            if sim_fresh { " (up to date)" } else { "" }
        );
        let (_, scene_fresh) =
            crate::scene::generate_for(wave, flow.facility.as_deref(), true, None)?;
        println!(
            "== {label}: scene{} ==",
            if scene_fresh { " (up to date)" } else { "" }
        );
        let out_override = if single {
            options.out_dir.clone()
        } else {
            None
        };
        let (out_dir, movie, render_fresh) = render_wave(wave, &options, out_override)?;
        if render_fresh {
            println!("== {label}: render (up to date) ==");
        }
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
    fn holds_compress_to_their_beat_and_motion_keeps_its_speed() {
        use lab_runfmt::{ProgramExtent, RunEvent, SimSummary, SimTraceDocument, TimedEvent};
        let timed = |t: f64, event: RunEvent| TimedEvent { t, event };
        let trace = SimTraceDocument {
            format: lab_runfmt::SIM_TRACE_FORMAT.to_string(),
            plan: "plan.workcell.json".to_string(),
            durations: "default-v0".to_string(),
            events: vec![
                timed(
                    0.0,
                    RunEvent::ProgramStarted {
                        station: "star-1".to_string(),
                        title: "assembly".to_string(),
                        extent: ProgramExtent::Frames { frames: 2 },
                    },
                ),
                timed(
                    0.0,
                    RunEvent::Frame {
                        station: "star-1".to_string(),
                        index: 1,
                        description: "pick up".to_string(),
                        x_mm: Some(100.0),
                        y_mm: Some(200.0),
                    },
                ),
                timed(
                    600.0,
                    RunEvent::NodeCompleted {
                        id: "run".to_string(),
                    },
                ),
                timed(
                    700.0,
                    RunEvent::LabwareMoved {
                        labware: "plate".to_string(),
                        from: "star-1".to_string(),
                        to: "odtc-1".to_string(),
                    },
                ),
            ],
            summary: SimSummary {
                total_seconds: 34_000.0,
                ..SimSummary::default()
            },
        };
        let options = RenderOptions {
            camera: "dolly".to_string(),
            speedup: 60.0,
            fps: 24,
            quality: "preview".to_string(),
            still: None,
            hdri: None,
            blender: None,
            out_dir: None,
            facility: None,
            jobs: None,
            hold_seconds: 2.0,
            uniform: false,
        };
        let warp = TimeWarp::build(&trace, &options);
        // Motion 0..600 at 60x = 10 s, hold 600..700 = 2 s, travel
        // 700..702 at 60x, final hold to 34 000 s = 2 s.
        assert!((warp.warp(600.0) - 10.0).abs() < 1e-9);
        assert!((warp.warp(700.0) - 12.0).abs() < 1e-9);
        assert!(
            (warp.footage_end - (10.0 + 2.0 + 2.0 / 60.0 + 2.0)).abs() < 1e-9,
            "nine hours of hold cost two seconds of footage: {}",
            warp.footage_end
        );
        // Strictly increasing across the whole run.
        let mut previous = -1.0;
        for t in [0.0, 1.0, 599.0, 650.0, 701.0, 5_000.0, 34_000.0] {
            let footage = warp.warp(t);
            assert!(footage > previous, "monotonic at t={t}");
            previous = footage;
        }
    }

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
