"""Contact sheets of Horizon Open Creative's baked meshes, rendered in Blender
(docs/open-creative.md). Run inside Blender, on the output of the
open_content_mesh_probe example:

  blender -b --factory-startup --python tools/open_content_blender.py -- \
      --input MESHES_DIR --out OUT_DIR [--samples 16]

It builds every exported mesh with its UVs and textures (nearest filtering,
texture alpha), checks scale, winding, face orientation, UV ranges, texture
availability and alpha, and renders with Cycles on the CPU:

  contact_front.png / contact_back.png              the real textures, two views
  calibration_front.png / calibration_back.png      every face with a labelled
                                                     calibration image (mirrored or
                                                     rotated faces show at once)
  report.json                                       the checks, per mesh

Blender checks how the assets are built. Horizon's own screenshots stay the
reference for its lighting, transparency sorting, animation and shaders.
"""
import argparse
import json
import math
import os
import sys

import bpy
from mathutils import Euler, Matrix, Vector

FACING = {
    "Down": (0, -1, 0), "Up": (0, 1, 0), "North": (0, 0, -1),
    "South": (0, 0, 1), "West": (-1, 0, 0), "East": (1, 0, 0),
}
COLUMNS = 7
SPACING = 2.2


def args():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--samples", type=int, default=16)
    return parser.parse_args(argv)


def game_to_blender(p):
    """Horizon's Y-up, north = -Z space to Blender's Z-up: (x, y, z) -> (x, -z, y)."""
    return Vector((p[0], -p[2], p[1]))


def material(name, image_path, frames=1, cache={}):
    key = (name, image_path)
    if key in cache:
        return cache[key]
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    bsdf = nodes["Principled BSDF"]
    tex = nodes.new("ShaderNodeTexImage")
    tex.interpolation = "Closest"
    tex.image = bpy.data.images.load(image_path, check_existing=True)
    if frames > 1:
        # Animated sprites stack their frames: show the first.
        mapping = nodes.new("ShaderNodeMapping")
        coords = nodes.new("ShaderNodeTexCoord")
        mapping.inputs["Scale"].default_value = (1.0, 1.0 / frames, 1.0)
        mapping.inputs["Location"].default_value = (0.0, 1.0 - 1.0 / frames, 0.0)
        links.new(coords.outputs["UV"], mapping.inputs["Vector"])
        links.new(mapping.outputs["Vector"], tex.inputs["Vector"])
    links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
    links.new(tex.outputs["Alpha"], bsdf.inputs["Alpha"])
    bsdf.inputs["Roughness"].default_value = 0.9
    cache[key] = mat
    return mat


def check_faces(faces, textures, texdir):
    """Numerical checks of one mesh's faces."""
    problems = []
    for i, f in enumerate(faces):
        p = [Vector(v) for v in f["positions"]]
        normal = (p[1] - p[0]).cross(p[2] - p[0])
        if normal.length < 1e-9:
            problems.append(f"face {i}: degenerate")
            continue
        normal.normalize()
        expected = Vector(FACING.get(f["facing"], (0, 0, 0)))
        if expected.length and normal.dot(expected) < 0.99 and f["facing"] in FACING:
            # Crossed plant planes face diagonally: only their sign matters.
            if normal.dot(expected) <= 0:
                problems.append(f"face {i}: winding faces {tuple(round(c, 3) for c in normal)}, not {f['facing']}")
        if any(c < -1e-6 or c > 1 + 1e-6 for v in f["positions"] for c in v):
            problems.append(f"face {i}: outside the block's unit cube")
        if any(c < -1e-6 or c > 1 + 1e-6 for uv in f["uv"] for c in uv):
            problems.append(f"face {i}: UV outside its sprite")
        sprite = textures.get(f["sprite"])
        if sprite is None or not os.path.isfile(os.path.join(texdir, sprite["file"])):
            problems.append(f"face {i}: missing texture {f['sprite']}")
    return problems


def alpha_stats(path):
    image = bpy.data.images.load(path, check_existing=True)
    px = list(image.pixels)
    alphas = px[3::4]
    transparent = sum(1 for a in alphas if a < 0.5)
    partial = sum(1 for a in alphas if 0.02 < a < 0.98)
    return {"pixels": len(alphas), "transparent": transparent, "partial": partial}


def build(data, texdir, calibration):
    objects = []
    cal_path = os.path.join(texdir, data["calibration"])
    report = {}
    for index, block in enumerate(data["blocks"]):
        col, row = index % COLUMNS, index // COLUMNS
        origin = Vector((col * SPACING, -row * SPACING * 1.3, 0))
        mesh = bpy.data.meshes.new(block["label"])
        verts, polys, uvs, mats = [], [], [], []
        slots = []
        for face in block["faces"]:
            base = len(verts)
            verts += [game_to_blender(p) for p in face["positions"]]
            polys.append([base, base + 1, base + 2, base + 3])
            uvs += [(uv[0], 1.0 - uv[1]) for uv in face["uv"]]
            sprite = data["textures"][face["sprite"]]
            key = "calibration" if calibration else face["sprite"]
            if key not in slots:
                slots.append(key)
            mats.append(slots.index(key))
        mesh.from_pydata(verts, [], polys)
        uv_layer = mesh.uv_layers.new(name="UV")
        for loop in mesh.loops:
            uv_layer.data[loop.index].uv = uvs[loop.vertex_index]
        for key in slots:
            if calibration:
                mesh.materials.append(material("calibration", cal_path))
            else:
                sprite = data["textures"][key]
                mesh.materials.append(material(key, os.path.join(texdir, sprite["file"]), sprite["frames"]))
        for poly, m in zip(mesh.polygons, mats):
            poly.material_index = m
        obj = bpy.data.objects.new(block["label"], mesh)
        obj.location = origin
        bpy.context.collection.objects.link(obj)
        label(block["label"], origin + Vector((0.5, -0.85, 0.0)))
        objects.append(obj)
        report[block["label"]] = {
            "faces": len(block["faces"]),
            "sprites": sorted({f["sprite"] for f in block["faces"]}),
            "problems": check_faces(block["faces"], data["textures"], texdir),
            "translucent_faces": sum(1 for f in block["faces"] if f.get("translucent")),
        }
    next_cell = len(data["blocks"])
    # The first-person arm, in model units (1/16 block, Y down), at its pose.
    arm = data["arm"]
    col, row = next_cell % COLUMNS, next_cell // COLUMNS
    origin = Vector((col * SPACING + 0.5, -row * SPACING * 1.3, 0.9))
    mesh = bpy.data.meshes.new("arm")
    verts, polys, uvs = [], [], []
    pose = arm["pose"]
    for poly in arm["polygons"]:
        base = len(verts)
        for v in poly["vertices"]:
            p = ((v[0] + pose[0]) / 16.0, -(v[1] + pose[1]) / 16.0, (v[2] + pose[2]) / 16.0)
            verts.append(game_to_blender(p))
            uvs.append((v[3], 1.0 - v[4]))
        polys.append([base, base + 1, base + 2, base + 3])
    mesh.from_pydata(verts, [], polys)
    uv_layer = mesh.uv_layers.new(name="UV")
    for loop in mesh.loops:
        uv_layer.data[loop.index].uv = uvs[loop.vertex_index]
    mesh.materials.append(material("calibration", cal_path) if calibration
                          else material("skin", os.path.join(texdir, arm["texture"])))
    obj = bpy.data.objects.new("first-person arm", mesh)
    obj.location = origin
    bpy.context.collection.objects.link(obj)
    label("first-person arm", Vector((col * SPACING + 0.5, -row * SPACING * 1.3 - 0.85, 0.0)))
    arm_problems = []
    for i, poly in enumerate(arm["polygons"]):
        p = [Vector(v[:3]) for v in poly["vertices"]]
        n = (p[1] - p[0]).cross(p[2] - p[0])
        if n.length < 1e-9 or n.normalized().dot(Vector(poly["normal"])) < 0.99:
            arm_problems.append(f"polygon {i}: winding disagrees with its normal")
        if any(not (0 <= v[3] <= 1 and 0 <= v[4] <= 1) for v in poly["vertices"]):
            arm_problems.append(f"polygon {i}: UV outside the skin")
    report["first-person arm"] = {"faces": len(arm["polygons"]), "problems": arm_problems}
    # Held blocks under the first-person display transform.
    display = data["held"]["display"]
    for k, name in enumerate(data["held"]["blocks"]):
        source = bpy.data.objects.get(name)
        if source is None:
            continue
        cell = next_cell + 1 + k
        col, row = cell % COLUMNS, cell // COLUMNS
        held = source.copy()
        held.data = source.data.copy()
        rx, ry, rz = (math.radians(a) for a in display["rotation"])
        s = display["scale"]
        t = display["translation"]
        # ItemTransform: about the model's centre, in game axes (x, y, z) -> Blender (x, -z, y).
        centre = Matrix.Translation(Vector((-0.5, 0.5, -0.5)))
        rotation = Euler((rx, -rz, ry), "XZY").to_matrix().to_4x4()
        scale = Matrix.Diagonal(Vector((s[0], s[2], s[1], 1.0)))
        held.data.transform(scale @ rotation @ centre)
        held.location = Vector((col * SPACING + 0.5, -row * SPACING * 1.3 - t[2] / 16, 0.5 + t[1] / 16))
        bpy.context.collection.objects.link(held)
        label(f"held {name}", Vector((col * SPACING + 0.5, -row * SPACING * 1.3 - 0.85, 0.0)))
    return report


def label(text, at):
    curve = bpy.data.curves.new(type="FONT", name=text)
    curve.body = text
    curve.size = 0.2
    curve.align_x = "CENTER"
    obj = bpy.data.objects.new(f"label {text}", curve)
    obj.location = at
    # Flat on the ground in front of its mesh, readable from the cameras.
    obj.rotation_euler = (0, 0, 0)
    mat = bpy.data.materials.get("label") or bpy.data.materials.new("label")
    mat.use_nodes = True
    mat.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.05, 0.05, 0.08, 1)
    curve.materials.append(mat)
    bpy.context.collection.objects.link(obj)


def scene(samples):
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = samples
    scene.cycles.use_denoising = False
    scene.render.resolution_x, scene.render.resolution_y = 1800, 1400
    scene.render.film_transparent = False
    world = bpy.data.worlds.new("world")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.82, 0.85, 0.9, 1)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.9
    scene.world = world
    sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    sun.data.energy = 3.0
    sun.rotation_euler = (math.radians(40), math.radians(15), math.radians(30))
    bpy.context.collection.objects.link(sun)
    camera = bpy.data.objects.new("camera", bpy.data.cameras.new("camera"))
    camera.data.type = "ORTHO"
    bpy.context.collection.objects.link(camera)
    scene.camera = camera
    return camera


def frame(camera, back):
    """An isometric view of the whole grid, from the front-top or back-bottom."""
    objs = [o for o in bpy.context.scene.objects if o.type in ("MESH", "FONT")]
    lo = Vector((min(o.location.x for o in objs), min(o.location.y for o in objs), 0))
    hi = Vector((max(o.location.x for o in objs) + 1, max(o.location.y for o in objs) + 1, 1))
    centre = (lo + hi) / 2
    direction = Vector((1, -1.4, 1.1)) if not back else Vector((-1, 1.4, -0.9))
    camera.location = centre + direction.normalized() * 40
    camera.rotation_euler = (centre - camera.location).to_track_quat("-Z", "Y").to_euler()
    camera.data.ortho_scale = max(hi.x - lo.x, hi.y - lo.y) * 1.25


def main():
    a = args()
    with open(os.path.join(a.input, "meshes.json")) as f:
        data = json.load(f)
    texdir = os.path.join(a.input, "textures")
    os.makedirs(a.out, exist_ok=True)
    report = {"blender": bpy.app.version_string, "textures": {}}
    for name, info in data["textures"].items():
        report["textures"][name] = alpha_stats(os.path.join(texdir, info["file"]))
    for calibration in (False, True):
        bpy.ops.wm.read_factory_settings(use_empty=True)
        camera = scene(a.samples)
        meshes = build(data, texdir, calibration)
        if not calibration:
            report["meshes"] = meshes
        for back in (False, True):
            frame(camera, back)
            kind = "calibration" if calibration else "contact"
            bpy.context.scene.render.filepath = os.path.join(a.out, f"{kind}_{'back' if back else 'front'}.png")
            bpy.ops.render.render(write_still=True)
    problems = {k: v["problems"] for k, v in report["meshes"].items() if v["problems"]}
    report["ok"] = not problems
    with open(os.path.join(a.out, "report.json"), "w") as f:
        json.dump(report, f, indent=2)
    print("OPEN_CONTENT_BLENDER", "ok" if not problems else f"problems: {problems}")


main()
