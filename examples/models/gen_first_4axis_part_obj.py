#!/usr/bin/env python3
"""Regenerate first-4axis-part.obj, the model of the "Your first 4-axis part" guide.

The part is a clean-room CamCraft fixture: the 1.5 in of a 3/4 in round bar
that sticks out of a 4th-axis chuck. The bar axis is the X axis, the free
end is at X0, and the axis passes through Y0 Z0 (the A centreline).
The model is the bar as the example program cuts it:

- four flats, 0.015 in deep (0.720 in across each pair), one on each of
  the +Z, -Y, -Z and +Y sides. A 1/4 in flat end mill cuts each one in a
  pass with its centre from X-0.75 to X-0.25. Thus each flat has round
  ends of the tool's radius: it reaches X-0.875 and X-0.125 on its centre
  line only.
- one cross slot on the +Z flat, 0.125 in wide about X-0.5, with its
  floor 0.030 in below the bar's skin (Z0.345). The 1/8 in tool runs out
  of the two sides of the bar, so the slot has straight walls.

An OBJ file has no unit and CamCraft reads its numbers as millimetres, so
the script writes millimetres.

Method: the skin is a set of lines along X ("longitudes"), one for each
sampled angle about the axis. Each longitude has the same ten points: the
two bar ends, and for its face the two ends of the flat (on the skin and
on the floor) and the two walls of the slot (top and floor). A point that
a longitude does not need lies on its neighbour point. The strips between
adjacent longitudes then close the surface with no gap, and the script
checks that before it writes the file. The round end of a flat is a wall
of flat strips between the samples: the samples are 1 degree apart on the
flats, so each strip is less than 0.0002 in from the true arc.

Usage: python3 gen_first_4axis_part_obj.py [output.obj]
"""

import math
import sys

IN = 25.4
R = 0.375            # bar radius, in
FLAT = 0.360         # centreline to each flat, in
GROOVE = 0.345       # centreline to the cross-slot floor, in
LENGTH = 1.5         # the bar out of the chuck, in
FLAT_X = (-0.75, -0.25)        # the 1/4 in tool's centre along each flat
FLAT_TOOL_R = 0.125
SLOT_X = (-0.5625, -0.4375)    # the walls of the cross slot
FACES = [math.pi / 2, math.pi, 3 * math.pi / 2, 0.0]   # the normals: +Z, -Y, -Z, +Y
SLOT_FACE = 0                  # the cross slot is on the +Z face
UNIFORM = 96
FINE_DEG = 1.0


def angles():
    """Uniform samples, 1 degree samples on the flats, and the exact edges."""
    out = {math.tau * k / UNIFORM for k in range(UNIFORM)}
    half = math.acos(FLAT / R)
    steps = int(math.degrees(half) / FINE_DEG)
    for face in FACES:
        out.update({face - half, face + half})
        for k in range(-steps, steps + 1):
            out.add(face + math.radians(k * FINE_DEG))
    slot = math.acos(GROOVE / R)
    out.update({FACES[SLOT_FACE] - slot, FACES[SLOT_FACE] + slot})
    unique = []
    for t in sorted(a % math.tau for a in out):
        if not unique or t - unique[-1] > 1e-9:
            unique.append(t)
    if unique[-1] - unique[0] > math.tau - 1e-9:
        unique.pop()
    return unique


def longitude(t):
    """The ten points of the skin line at angle t, as (x, height, side, face)."""
    face, side, height = None, 0.0, R
    for k, normal in enumerate(FACES):
        d = (t - normal + math.pi) % math.tau - math.pi
        if abs(d) < math.pi / 4 + 1e-12:
            face, side, height = k, R * math.sin(d), R * math.cos(d)
            break
    floor = min(height, FLAT)
    if height > FLAT + 1e-12:
        reach = math.sqrt(max(FLAT_TOOL_R ** 2 - side ** 2, 0.0))
        x_in, x_out = FLAT_X[0] - reach, FLAT_X[1] + reach
    else:
        x_in, x_out = FLAT_X[0] - FLAT_TOOL_R, FLAT_X[1] + FLAT_TOOL_R
    slot = min(floor, GROOVE) if face == SLOT_FACE else floor
    rows = [(-LENGTH, height), (x_in, height), (x_in, floor), (SLOT_X[0], floor), (SLOT_X[0], slot),
            (SLOT_X[1], slot), (SLOT_X[1], floor), (x_out, floor), (x_out, height), (0.0, height)]
    normal = FACES[face]
    out = []
    for x, h in rows:
        y = h * math.cos(normal) - side * math.sin(normal)
        z = h * math.sin(normal) + side * math.cos(normal)
        out.append((x, y, z))
    return out


def build():
    verts, index, tris = [], {}, []

    def vid(p):
        key = (round(p[0] * IN, 6) + 0.0, round(p[1] * IN, 6) + 0.0, round(p[2] * IN, 6) + 0.0)
        if key not in index:
            index[key] = len(verts)
            verts.append(key)
        return index[key]

    def tri(a, b, c):
        if a != b and b != c and a != c:
            tris.append((a, b, c))

    lines = [[vid(p) for p in longitude(t)] for t in angles()]
    back, front = vid((-LENGTH, 0.0, 0.0)), vid((0.0, 0.0, 0.0))
    for j, a in enumerate(lines):
        b = lines[(j + 1) % len(lines)]
        for i in range(len(a) - 1):
            tri(a[i], b[i + 1], a[i + 1])
            tri(a[i], b[i], b[i + 1])
        tri(back, b[0], a[0])
        tri(front, a[-1], b[-1])
    return verts, tris


def check(verts, tris):
    """Closed, and wound outward: each edge once in each direction, volume > 0."""
    seen = {}
    for a, b, c in tris:
        for e in ((a, b), (b, c), (c, a)):
            seen[e] = seen.get(e, 0) + 1
    for (a, b), count in seen.items():
        if count != 1 or seen.get((b, a), 0) != 1:
            raise SystemExit(f"open or doubled edge {verts[a]}-{verts[b]}")
    volume = 0.0
    for a, b, c in tris:
        (ax, ay, az), (bx, by, bz), (cx, cy, cz) = verts[a], verts[b], verts[c]
        volume += ax * (by * cz - bz * cy) - ay * (bx * cz - bz * cx) + az * (bx * cy - by * cx)
    if volume <= 0.0:
        raise SystemExit("the mesh is wound inward")
    return volume / 6.0


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "first-4axis-part.obj"
    verts, tris = build()
    volume = check(verts, tris)
    with open(out, "w", newline="\n") as f:
        f.write("# CamCraft example: Your first 4-axis part (millimetres)\n")
        f.write("# Made by gen_first_4axis_part_obj.py - do not edit by hand\n")
        f.write("o first-4axis-part\n")
        for x, y, z in verts:
            f.write(f"v {x:.6f} {y:.6f} {z:.6f}\n")
        for a, b, c in tris:
            f.write(f"f {a + 1} {b + 1} {c + 1}\n")
    print(f"{out}: {len(verts)} vertices, {len(tris)} triangles, {volume:.3f} mm3")


if __name__ == "__main__":
    main()
