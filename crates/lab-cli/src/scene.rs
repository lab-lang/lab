//! The `lab scene` command: render a built run package as a 3D scene.
//!
//! Every STAR automation manifest embeds the full bench profile it was
//! planned against, so a scene rebuilds from build output alone — no
//! access to `targets/` is needed. The scene document is the semantic
//! source of truth; the glTF and USD files beside it are derived
//! projections for viewers and simulators.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lab_compiler::backend::hamilton::star::profile::StarTargetProfile;
use lab_scene::workcell::{StationScene, star_bench_scene, workcell_scene};
use lab_scene::{Scene, gltf::render_gltf, usda::render_usda};

use crate::Output;

/// Reads the bench profile out of a STAR automation manifest.
fn deck_from_manifest(path: &Path) -> Result<StarTargetProfile> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("no automation manifest at {}", path.display()))?;
    let manifest: serde_json::Value =
        serde_json::from_str(&text).context("failed to parse the automation manifest")?;
    let mut deck = manifest
        .get("deck")
        .cloned()
        .with_context(|| format!("{} carries no deck profile", path.display()))?;
    // A profile never reads its own name from disk (the loader names it),
    // so pull the serialized name out before deserializing and put it
    // back after.
    let name = deck
        .get("target")
        .and_then(|target| target.get("name"))
        .and_then(|name| name.as_str())
        .unwrap_or("bench")
        .to_string();
    if let Some(target) = deck
        .get_mut("target")
        .and_then(|target| target.as_object_mut())
    {
        target.remove("name");
    }
    let mut profile: StarTargetProfile = serde_json::from_value(deck).with_context(|| {
        format!(
            "{} carries a deck this scene builder cannot read",
            path.display()
        )
    })?;
    profile.target.name = name;
    Ok(profile)
}

/// A facility and the asset catalog rooted beside it.
struct FacilityContext {
    facility: lab_runfmt::facility::Facility,
    assets: lab_scene::assets::AssetCatalog,
}

fn load_facility_context(path: &Path) -> Result<FacilityContext> {
    let facility = lab_runfmt::facility::load_facility(path)?;
    let assets_dir = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("assets");
    Ok(FacilityContext {
        facility,
        assets: lab_scene::assets::AssetCatalog::new(assets_dir),
    })
}

fn build_scene(directory: &Path, context: Option<&FacilityContext>) -> Result<Scene> {
    let assets = context.map(|context| &context.assets);
    let facility = context.map(|context| &context.facility);
    if lab_runtime::workcell::is_workcell_directory(directory) {
        let plan = lab_runfmt::load_workcell_plan(directory)?;
        if let Some(facility) = facility {
            facility.check_stations(&plan.stations)?;
        }
        let mut stations = Vec::new();
        for station in &plan.stations {
            let star_profile = if station.kind == "hamilton.star" {
                let manifest = directory
                    .join(&station.program_dir)
                    .join("automation_manifest.json");
                Some(deck_from_manifest(&manifest)?)
            } else {
                None
            };
            stations.push(StationScene {
                name: station.name.clone(),
                kind: station.kind.clone(),
                star_profile,
            });
        }
        let name = facility
            .map(|facility| facility.facility.name.clone())
            .or_else(|| {
                directory
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "workcell".to_string());
        Ok(workcell_scene(&name, stations, assets, facility)?)
    } else {
        let manifest = directory.join("automation_manifest.json");
        if !manifest.is_file() {
            bail!(
                "{} holds neither a workcell plan nor a STAR automation manifest; point lab scene at a directory produced by `lab build`",
                directory.display()
            );
        }
        let profile = deck_from_manifest(&manifest)?;
        let name = profile.target.name.clone();
        Ok(star_bench_scene(&name, &profile, assets)?)
    }
}

pub(crate) fn scene(
    directory: PathBuf,
    out_dir: Option<PathBuf>,
    facility_path: Option<PathBuf>,
    output: &Output,
) -> Result<()> {
    let context = facility_path
        .as_deref()
        .map(load_facility_context)
        .transpose()?;
    let mut scene = build_scene(&directory, context.as_ref())?;
    let out_dir = out_dir.unwrap_or_else(|| directory.clone());
    lab_scene::assets::bundle_assets(&mut scene, &out_dir)
        .context("failed to bundle scene assets")?;
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;

    let scene_path = out_dir.join("scene.json");
    std::fs::write(&scene_path, serde_json::to_string_pretty(&scene)?)
        .with_context(|| format!("failed to write {}", scene_path.display()))?;
    let gltf_path = out_dir.join("scene.gltf");
    std::fs::write(&gltf_path, render_gltf(&scene))
        .with_context(|| format!("failed to write {}", gltf_path.display()))?;
    let usda_path = out_dir.join("scene.usda");
    std::fs::write(&usda_path, render_usda(&scene))
        .with_context(|| format!("failed to write {}", usda_path.display()))?;

    let mut nodes = 0usize;
    scene.root.walk(&mut |_, _| nodes += 1);
    output.success(
        "scene",
        serde_json::json!({
            "name": scene.name,
            "nodes": nodes,
            "scene": scene_path.display().to_string(),
            "gltf": gltf_path.display().to_string(),
            "usda": usda_path.display().to_string(),
        }),
        format!(
            "scene '{}': {nodes} node(s)\n  {}\n  {}\n  {}",
            scene.name,
            scene_path.display(),
            gltf_path.display(),
            usda_path.display()
        ),
    )
}
