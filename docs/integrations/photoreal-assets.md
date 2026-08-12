# Preparing photoreal assets

A facility's `assets/` directory turns schematic boxes into real
instruments. Asset preparation is a human workflow, not a build step:
vendor CAD is licensed to you, not to this repository, so assets live
beside your facility file and never in version control here.

## The recipe

1. **Get the CAD.** Vendors supply STEP/IGES integration models on
   request (Hamilton, Inheco, and peers all do this for workcell
   integrators). Opentrons publishes OT-2 hardware files openly.
2. **Import and clean.** FreeCAD or Blender imports STEP. Delete
   internals, decimate to a sensible polygon budget (an instrument body
   needs tens of thousands of triangles, not millions), and join loose
   shells.
3. **Material it.** Assign PBR materials (anodized aluminum, powder
   coat, polycarbonate). Texture painting is optional; measured material
   values carry most of the realism.
4. **Export both flavors** into the facility's `assets/` directory:
   - `<key>.glb` for the web player and the Blender harness;
   - `<key>.usd` for Omniverse, Isaac Sim, and usdview.

## Conventions

- **Keys** are the identities the scene speaks: station kind strings
  (`hamilton.star.glb`, `inheco.odtc.usd`), labware catalog ids
  (`pcr_plate_96.glb`), carrier catalog ids, and `room` for a modeled or
  scanned environment.
- **Units are millimeters** in both formats. USD layers declare
  `metersPerUnit = 0.001`; glTF is nominally meters, so export with the
  scene's numeric values unchanged (1 unit = 1 mm) — the players scale
  the whole lab frame once.
- **Origin at the node anchor**: stations at floor-center of their
  footprint's front-left, labware at the footprint's minimum corner.
  +X right, +Y toward the back, Z up.
- **Fallback always works.** A missing or failing asset renders as the
  dimensioned box; nothing ever blocks on an asset.

## Rendering tiers

- `lab scene --facility …` bundles referenced assets beside the scene.
- The web player and `lab render` (Blender) load the `.glb` flavor.
- Omniverse and Isaac Sim open `scene.usda`, which composes the `.usd`
  flavor by reference; `lab scene --animated` adds the run's timeline.
- `lab render --quality final` path-traces with Cycles; pass `--hdri`
  for measured environment lighting, or use the built-in sky.
