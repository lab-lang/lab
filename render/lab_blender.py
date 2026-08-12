# The Blender player: a third interpreter of the same two documents the
# web player and the USD stage consume. It reads scene.json and
# sim-trace.json, builds Blender objects, keyframes motion from trace
# events, and renders with Cycles (final) or EEVEE (preview). It computes
# nothing about the run itself: every state change comes from an event.
#
# Run headless through Blender's bundled Python; bpy and stdlib only:
#
#   blender --background --factory-startup \
#       --python render/lab_blender.py -- \
#       --scene scene.json --trace sim-trace.json --out renders \
#       --camera dolly --speedup 600 --fps 24 --quality preview

import argparse
import json
import math
import os
import sys

import bpy
import mathutils

# ---------------------------------------------------------------- documents


def parse_args():
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser(prog="lab_blender")
    parser.add_argument("--scene", required=True)
    parser.add_argument("--trace", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--camera", default="dolly", choices=["orbit", "dolly"])
    parser.add_argument("--speedup", type=float, default=600.0)
    parser.add_argument("--fps", type=int, default=24)
    parser.add_argument("--quality", default="preview", choices=["preview", "final"])
    parser.add_argument("--from-t", dest="from_t", type=float, default=0.0)
    parser.add_argument("--to-t", dest="to_t", type=float, default=None)
    parser.add_argument("--still", type=float, default=None,
                        help="render one frame at this simulated second")
    parser.add_argument("--hdri", default=None,
                        help="environment .hdr/.exr; the built-in sky otherwise")
    return parser.parse_args(argv)


def load_documents(args):
    with open(args.scene, encoding="utf-8") as handle:
        scene = json.load(handle)
    with open(args.trace, encoding="utf-8") as handle:
        trace = json.load(handle)
    if scene.get("format") != "lab.scene.v0":
        raise SystemExit(f"{args.scene} is not a lab.scene.v0 document")
    if trace.get("format") != "lab.sim-trace.v0":
        raise SystemExit(f"{args.trace} is not a lab.sim-trace.v0 document")
    return scene, trace


# ---------------------------------------------------------------- materials

# name -> (rgb, roughness, metallic, alpha)
MATERIALS = {
    "lab.deck": ((0.35, 0.36, 0.38), 0.55, 0.6, 1.0),
    "lab.carrier": ((0.16, 0.17, 0.20), 0.4, 0.9, 1.0),
    "lab.plate": ((0.86, 0.86, 0.88), 0.35, 0.0, 1.0),
    "lab.tips": ((0.75, 0.35, 0.06), 0.45, 0.0, 1.0),
    "lab.well": ((0.12, 0.30, 0.75), 0.15, 0.0, 0.35),
    "lab.station": ((0.55, 0.57, 0.60), 0.5, 0.4, 1.0),
    "lab.room": ((0.75, 0.74, 0.71), 0.9, 0.0, 1.0),
    "lab.head": ((0.08, 0.08, 0.10), 0.35, 0.6, 1.0),
    "lab.frame": ((0.06, 0.065, 0.08), 0.4, 0.85, 1.0),
    "lab.panel": ((0.58, 0.59, 0.62), 0.5, 0.3, 1.0),
    "lab.glass": ((0.6, 0.72, 0.78), 0.03, 0.0, 0.15),
    "lab.accent": ((0.02, 0.25, 0.45), 0.3, 0.0, 1.0),
}


def build_materials():
    built = {}
    for name, (rgb, roughness, metallic, alpha) in MATERIALS.items():
        material = bpy.data.materials.new(name)
        material.use_nodes = True
        bsdf = material.node_tree.nodes["Principled BSDF"]
        bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
        bsdf.inputs["Roughness"].default_value = roughness
        bsdf.inputs["Metallic"].default_value = metallic
        bsdf.inputs["Alpha"].default_value = alpha
        if alpha < 1.0:
            material.blend_method = "BLEND"
        built[name] = material
    return built


def material_for(node, materials):
    kind = node.get("semantic", {}).get("kind", "")
    if kind == "part":
        name = "lab." + str(node.get("semantic", {}).get("material", ""))
        if name in materials:
            return materials[name]
        return materials["lab.station"]
    catalog = str(node.get("semantic", {}).get("catalog", "")) + node.get("id", "")
    if kind == "room":
        return materials["lab.room"]
    if kind == "deck":
        return materials["lab.deck"]
    if kind == "carrier":
        return materials["lab.carrier"]
    if kind in ("labware", "well") and "tip" in catalog:
        return materials["lab.tips"]
    if kind == "well":
        return materials["lab.well"]
    if kind == "labware":
        return materials["lab.plate"]
    return materials["lab.station"]


# ---------------------------------------------------------------- geometry


def unit_box_mesh():
    mesh = bpy.data.meshes.get("lab-unit-box")
    if mesh:
        return mesh
    mesh = bpy.data.meshes.new("lab-unit-box")
    verts = [(x, y, z) for z in (0, 1) for y in (0, 1) for x in (0, 1)]
    faces = [
        (0, 2, 3, 1), (4, 5, 7, 6), (0, 1, 5, 4),
        (2, 6, 7, 3), (0, 4, 6, 2), (1, 3, 7, 5),
    ]
    mesh.from_pydata(verts, [], faces)
    mesh.update()
    return mesh


def unit_cylinder_mesh(segments=24):
    mesh = bpy.data.meshes.get("lab-unit-cylinder")
    if mesh:
        return mesh
    mesh = bpy.data.meshes.new("lab-unit-cylinder")
    verts, faces = [], []
    for z in (0.0, 1.0):
        for segment in range(segments):
            angle = math.tau * segment / segments
            verts.append((0.5 * math.cos(angle), 0.5 * math.sin(angle), z))
    for segment in range(segments):
        following = (segment + 1) % segments
        faces.append((segment, following, segments + following, segments + segment))
    faces.append(tuple(range(segments - 1, -1, -1)))
    faces.append(tuple(range(segments, 2 * segments)))
    mesh.from_pydata(verts, [], faces)
    mesh.update()
    for polygon in mesh.polygons:
        polygon.use_smooth = True
    return mesh


def link(obj, parent):
    bpy.context.scene.collection.objects.link(obj)
    if parent is not None:
        obj.parent = parent
    return obj


def assign_material(obj, material):
    """Object-level material over a shared mesh: the mesh contributes one
    empty slot, each object overrides it."""
    if not obj.data.materials:
        obj.data.materials.append(None)
    obj.material_slots[0].link = "OBJECT"
    obj.material_slots[0].material = material


class Builder:
    """Builds the Blender object tree from scene.json."""

    def __init__(self, materials, scene_dir):
        self.materials = materials
        self.scene_dir = scene_dir
        self.by_id = {}
        self.station_heights = {}
        self.bounds_min = [math.inf] * 3
        self.bounds_max = [-math.inf] * 3

    def grow_bounds(self, origin, extent):
        for axis in range(3):
            self.bounds_min[axis] = min(self.bounds_min[axis], origin[axis])
            self.bounds_max[axis] = max(self.bounds_max[axis], origin[axis] + extent[axis])

    def geometry_object(self, node, geometry, parent):
        shape = geometry.get("shape")
        material = material_for(node, self.materials)
        if shape == "mesh" and geometry.get("gltf"):
            path = os.path.join(self.scene_dir, geometry["gltf"])
            if os.path.isfile(path):
                before = set(bpy.data.objects)
                try:
                    bpy.ops.import_scene.gltf(filepath=path)
                    for imported in set(bpy.data.objects) - before:
                        if imported.parent is None:
                            imported.parent = parent
                    return
                except Exception as error:  # noqa: BLE001 - fall back to the box
                    print(f"asset {path} failed to import ({error}); using fallback")
        if shape == "cylinder":
            obj = bpy.data.objects.new(node["id"] + "#geometry", unit_cylinder_mesh())
            obj.scale = (geometry["diameter"], geometry["diameter"], geometry["height"])
        else:
            extent = (
                (geometry["x"], geometry["y"], geometry["z"])
                if shape == "box"
                else tuple(geometry["fallback"])
            )
            obj = bpy.data.objects.new(node["id"] + "#geometry", unit_box_mesh())
            obj.scale = extent
        link(obj, parent)
        assign_material(obj, material)

    def node(self, node, parent, origin):
        group = bpy.data.objects.new(node["id"], None)
        group.location = tuple(node.get("translation", [0, 0, 0]))
        group.rotation_euler = (0, 0, math.radians(node.get("rotation_z_deg", 0.0)))
        link(group, parent)
        self.by_id[node["id"]] = group

        here = [origin[axis] + group.location[axis] for axis in range(3)]
        geometry = node.get("geometry")
        if geometry:
            extent = {
                "box": lambda g: (g["x"], g["y"], g["z"]),
                "cylinder": lambda g: (g["diameter"], g["diameter"], g["height"]),
                "mesh": lambda g: tuple(g["fallback"]),
            }[geometry["shape"]](geometry)
            # The room shell renders but never drives the camera framing.
            if node.get("semantic", {}).get("kind") != "room":
                self.grow_bounds(here, extent)
            if node.get("semantic", {}).get("kind") == "station":
                self.station_heights[node["id"]] = extent[2]
            self.geometry_object(node, geometry, group)
        for child in node.get("children", []):
            self.node(child, group, here)


# ---------------------------------------------------------------- animation


def world_origin(obj):
    matrix = obj.matrix_world
    return (matrix[0][3], matrix[1][3], matrix[2][3])


class Animator:
    """Keyframes trace events onto the built objects."""

    def __init__(self, builder, materials, fps, speedup, from_t):
        self.builder = builder
        self.materials = materials
        self.fps = fps
        self.speedup = speedup
        self.from_t = from_t
        self.heads = {}

    def frame(self, t):
        return 1 + max(0.0, t - self.from_t) / self.speedup * self.fps

    def key_location(self, obj, t, interpolation="LINEAR"):
        # Interpolation rides the insert itself: the fcurve API changed
        # shape across Blender 4/5, the new-keyframe preference did not.
        prefs = bpy.context.preferences.edit
        previous = prefs.keyframe_new_interpolation_type
        prefs.keyframe_new_interpolation_type = interpolation
        obj.keyframe_insert(data_path="location", frame=self.frame(t))
        prefs.keyframe_new_interpolation_type = previous

    def head_for(self, station):
        if station in self.heads:
            return self.heads[station]
        parent = self.builder.by_id.get(station)
        head = bpy.data.objects.new(f"{station}:head", None)
        link(head, parent)
        carriage = bpy.data.objects.new(f"{station}:head#carriage", unit_box_mesh())
        carriage.scale = (60, 60, 110)
        carriage.location = (-30, -30, 0)
        link(carriage, head)
        assign_material(carriage, self.materials["lab.head"])
        tip = bpy.data.objects.new(f"{station}:head#tip", unit_cylinder_mesh())
        tip.scale = (5, 5, 70)
        tip.location = (0, 0, -70)
        link(tip, head)
        assign_material(tip, self.materials["lab.plate"])
        head.hide_render = True
        head.hide_viewport = True
        head.keyframe_insert(data_path="hide_render", frame=1)
        head.keyframe_insert(data_path="hide_viewport", frame=1)
        self.heads[station] = head
        return head

    def set_head_visible(self, station, t, visible):
        head = self.head_for(station)
        head.hide_render = not visible
        head.hide_viewport = not visible
        head.keyframe_insert(data_path="hide_render", frame=self.frame(t))
        head.keyframe_insert(data_path="hide_viewport", frame=self.frame(t))

    def play(self, trace):
        visible = {}
        for timed in trace["events"]:
            t = timed["t"]
            event = timed["event"]
            if event == "frame" and timed.get("x_mm") is not None:
                station = timed["station"]
                head = self.head_for(station)
                if not visible.get(station):
                    self.set_head_visible(station, t, True)
                    visible[station] = True
                    head.location = (timed["x_mm"], timed["y_mm"], 260)
                    self.key_location(head, t)
                else:
                    head.location = (timed["x_mm"], timed["y_mm"], 260)
                    self.key_location(head, t)
            elif event == "node-completed":
                for station, is_visible in list(visible.items()):
                    if is_visible:
                        self.set_head_visible(station, t, False)
                        visible[station] = False
            elif event == "labware-moved":
                mover = self.builder.by_id.get(timed["labware"])
                station = self.builder.by_id.get(timed["to"])
                if mover is None or station is None or mover.parent is None:
                    continue
                self.key_location(mover, t, interpolation="CONSTANT")
                bpy.context.view_layer.update()
                seat = list(world_origin(station))
                seat[2] += self.builder.station_heights.get(timed["to"], 0.0) + 5.0
                inverse = mover.parent.matrix_world.inverted()
                local = inverse @ mathutils.Vector(seat)
                mover.location = tuple(local)
                self.key_location(mover, t + 2.0)


# ---------------------------------------------------------------- shooting


def build_world(hdri):
    world = bpy.data.worlds.new("lab-world")
    world.use_nodes = True
    nodes = world.node_tree.nodes
    background = nodes["Background"]
    if hdri and os.path.isfile(hdri):
        environment = nodes.new("ShaderNodeTexEnvironment")
        environment.image = bpy.data.images.load(hdri)
        world.node_tree.links.new(
            environment.outputs["Color"], background.inputs["Color"]
        )
        background.inputs["Strength"].default_value = 1.0
    else:
        sky = nodes.new("ShaderNodeTexSky")
        sky.sun_elevation = math.radians(35.0)
        world.node_tree.links.new(sky.outputs["Color"], background.inputs["Color"])
        background.inputs["Strength"].default_value = 0.35
    bpy.context.scene.world = world


def build_camera(preset, bounds_min, bounds_max, frame_end, fps):
    center = [(bounds_min[i] + bounds_max[i]) / 2.0 for i in range(3)]
    size = max(bounds_max[i] - bounds_min[i] for i in range(3))
    target = bpy.data.objects.new("lab-camera-target", None)
    target.location = (center[0], center[1], max(900.0, center[2]))
    bpy.context.scene.collection.objects.link(target)

    camera_data = bpy.data.cameras.new("lab-camera")
    camera_data.lens = 35
    # The scene is millimeters; the default clip range is meters-sized.
    camera_data.clip_start = 5.0
    camera_data.clip_end = max(100000.0, size * 30.0)
    camera_data.dof.use_dof = True
    camera_data.dof.focus_object = target
    camera_data.dof.aperture_fstop = 2.8
    camera = bpy.data.objects.new("lab-camera", camera_data)
    bpy.context.scene.collection.objects.link(camera)
    bpy.context.scene.camera = camera
    constraint = camera.constraints.new("TRACK_TO")
    constraint.target = target

    if preset == "orbit":
        radius = size * 0.9
        steps = 32
        for step in range(steps + 1):
            angle = math.tau * step / steps
            camera.location = (
                center[0] + radius * math.cos(angle),
                center[1] + radius * math.sin(angle),
                center[2] + size * 0.5,
            )
            camera.keyframe_insert(
                data_path="location", frame=1 + step * (frame_end - 1) / steps
            )
    else:  # dolly
        camera.location = (
            bounds_min[0] - size * 0.15,
            bounds_min[1] - size * 0.55,
            center[2] + size * 0.35,
        )
        camera.keyframe_insert(data_path="location", frame=1)
        camera.location = (
            bounds_max[0] + size * 0.15,
            bounds_min[1] - size * 0.55,
            center[2] + size * 0.35,
        )
        camera.keyframe_insert(data_path="location", frame=frame_end)


def configure_render(args, out_dir):
    scene = bpy.context.scene
    scene.render.resolution_x = 1920
    scene.render.resolution_y = 1080
    scene.render.fps = args.fps
    scene.render.image_settings.file_format = "PNG"
    scene.render.filepath = os.path.join(out_dir, "frames", "")
    if args.quality == "final":
        scene.render.engine = "CYCLES"
        scene.cycles.samples = 512
        scene.cycles.use_denoising = True
    else:
        # EEVEE's identifier moved between Blender generations.
        for engine in ("BLENDER_EEVEE_NEXT", "BLENDER_EEVEE"):
            try:
                scene.render.engine = engine
                break
            except TypeError:
                continue
        scene.eevee.taa_render_samples = 64


def main():
    args = parse_args()
    document, trace = load_documents(args)
    os.makedirs(os.path.join(args.out, "frames"), exist_ok=True)

    # A clean stage: factory startup still ships a cube, camera, and light.
    for obj in list(bpy.data.objects):
        bpy.data.objects.remove(obj, do_unlink=True)
    bpy.context.scene.unit_settings.system = "METRIC"
    bpy.context.scene.unit_settings.scale_length = 0.001
    bpy.context.preferences.edit.keyframe_new_interpolation_type = "LINEAR"

    materials = build_materials()
    builder = Builder(materials, os.path.dirname(os.path.abspath(args.scene)))
    builder.node(document["root"], None, [0.0, 0.0, 0.0])
    bpy.context.view_layer.update()

    total = trace.get("summary", {}).get("total_seconds", 0.0)
    to_t = args.to_t if args.to_t is not None else total
    animator = Animator(builder, materials, args.fps, args.speedup, args.from_t)
    animator.play(trace)

    frame_end = max(2, math.ceil((to_t - args.from_t) / args.speedup * args.fps))
    bpy.context.scene.frame_start = 1
    bpy.context.scene.frame_end = frame_end

    build_world(args.hdri)
    build_camera(args.camera, builder.bounds_min, builder.bounds_max, frame_end, args.fps)
    configure_render(args, args.out)

    if args.still is not None:
        frame = int(animator.frame(args.still))
        bpy.context.scene.frame_set(min(max(1, frame), frame_end))
        bpy.context.scene.render.filepath = os.path.join(args.out, "frames", "still")
        bpy.ops.render.render(write_still=True)
        print(f"lab render: wrote {bpy.context.scene.render.filepath}.png")
    else:
        bpy.ops.render.render(animation=True)
        print(f"lab render: wrote {frame_end} frame(s) under {args.out}/frames")


if __name__ == "__main__":
    main()
