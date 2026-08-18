# Lab trace player

Watch a simulated run in the browser: the scene a bench renders to, played
back against the trace a simulation records. The player computes nothing —
every state change on screen is an event from the trace, and seeking is
replaying events up to the chosen time.

## Use

Produce the two documents from a built run package:

```sh
lab simulate path/to/wave-001          # writes sim-trace.json
lab scene path/to/wave-001             # writes scene.json (+ .gltf, .usda)
```

Then either drop both files onto the page, or serve them next to the
built player:

```sh
npm install
npm run dev        # open the printed URL, drop the two files
```

For a served deployment, `npm run build` and copy `dist/` next to the
`scene.json` and `sim-trace.json`; the player auto-loads both from its own
directory (or from `?scene=` and `?trace=` URLs).

## What it shows

- The bench in millimeter-exact positions: deck, carriers, labware, wells.
- Labware moving between stations on each confirmed handoff.
- Station state: doors amber while open, cyclers tinted while running.
- The attention timeline: amber spans are when an operator is needed;
  everything else is walk-away time. Click to seek, 1× to 600× playback.

The `.gltf` and `.usda` files beside `scene.json` are the same scene for
standard tools (Blender, three.js, Isaac Sim, Omniverse, Unreal).
