#!/usr/bin/env python3
"""Regenerate the two models of the catalog case turning/cone-thread.

The part is a clean-room Camcraft fixture: a spun profile with a conical
shape and a threaded end. From the chuck side (low Z) to the free end:

  z 15 .. 20   collar, diameter 30 (the bar's own diameter)
  z 20 .. 60   cone, diameter 20 down to diameter 10 (a 40 mm taper)
  z 60 .. 63   neck (thread relief), diameter 8
  z 63 .. 78   M10x1.5 external thread, 15 mm long

The bar is 30 mm stock on the Z axis from z 0 to z 79. The 15 mm below the
part is the chuck's grip; the part-off plane is the part's own floor, z 15.

Two meshes are written:

  cone-blank.obj   the turned blank: the same part before the neck and the
                   thread are cut (a plain diameter 10 journal, z 60 .. 78).
                   The facing and profile operations cut to this model.
  cone-thread.obj  the finished part. The neck groove, the thread and the
                   part-off cut to this model.

The thread is modelled as ten annular 60-degree V grooves, one every
1.5 mm, 0.92 mm deep (0.613 x pitch, the ISO 68-1 external thread height).
Camcraft's threading operation cuts a MODELLED thread: it reads where the
grooves stand from the model, and its verdict compares the cut with them.
A lathe cuts a helix; the annular grooves are its meridian section.

Method: one meridian polyline (radius, z) revolved about Z in SEGS equal
steps. Axis points collapse to one vertex, so each mesh is closed and
conforming; the script checks that every edge has exactly two faces of
opposite direction before it writes.

Usage: python gen_cone_thread_obj.py [output directory]
"""

import math
import os
import sys

SEGS = 48

PART_Z0 = 15.0     # part-off plane: the part's floor
COLLAR_R = 15.0
COLLAR_Z1 = 20.0
CONE_R0 = 10.0     # diameter 20 at the collar
CONE_R1 = 5.0      # diameter 10 at the neck
CONE_Z1 = 60.0
NECK_R = 4.0
NECK_Z1 = 63.0
MAJOR_R = 5.0      # M10
PITCH = 1.5
DEPTH = 0.92       # 0.613 x pitch
TOP_Z = 78.0
# Root centres at z 63.9, 65.4, ... 77.4. The last groove is placed so
# that a 1.2 mm wide insert centred on it ends flush with the end face.
FIRST_ROOT_Z = 63.9
ROOTS = 10


def meridian(threaded):
    pts = [(0.0, PART_Z0), (COLLAR_R, PART_Z0), (COLLAR_R, COLLAR_Z1)]
    pts += [(CONE_R0, COLLAR_Z1), (CONE_R1, CONE_Z1)]
    if threaded:
        half = DEPTH * math.tan(math.radians(30.0))
        root = MAJOR_R - DEPTH
        pts += [(NECK_R, CONE_Z1), (NECK_R, NECK_Z1), (MAJOR_R, NECK_Z1)]
        for i in range(ROOTS):
            c = FIRST_ROOT_Z + i * PITCH
            pts += [(MAJOR_R, c - half), (root, c), (MAJOR_R, c + half)]
    pts += [(MAJOR_R, TOP_Z), (0.0, TOP_Z)]
    return pts


def build(pts):
    verts, faces, ring = [], [], []
    for r, z in pts:
        if r <= 1e-12:
            verts.append((0.0, 0.0, z))
            ring.append([len(verts)] * SEGS)
        else:
            ids = []
            for j in range(SEGS):
                th = 2.0 * math.pi * j / SEGS
                verts.append((r * math.cos(th), r * math.sin(th), z))
                ids.append(len(verts))
            ring.append(ids)
    for a, b in zip(ring, ring[1:]):
        for j in range(SEGS):
            k = (j + 1) % SEGS
            a1, a2, b1, b2 = a[j], a[k], b[j], b[k]
            if a1 == a2:
                faces.append((a1, b2, b1))
            elif b1 == b2:
                faces.append((a1, a2, b1))
            else:
                faces.append((a1, a2, b2))
                faces.append((a1, b2, b1))
    return verts, faces


def check_closed(faces):
    edges = set()
    for f in faces:
        for i in range(3):
            e = (f[i], f[(i + 1) % 3])
            if e in edges:
                raise SystemExit(f"edge {e} is used two times in one direction")
            edges.add(e)
    for a, b in edges:
        if (b, a) not in edges:
            raise SystemExit(f"edge {(a, b)} is open")


def write(path, name, threaded):
    verts, faces = build(meridian(threaded))
    check_closed(faces)
    with open(path, "w", newline="\n") as f:
        f.write(f"# {name}: clean-room Camcraft catalog model (gen_cone_thread_obj.py)\n")
        f.write(f"o {name}\n")
        for x, y, z in verts:
            f.write(f"v {x:.6f} {y:.6f} {z:.6f}\n")
        for a, b, c in faces:
            f.write(f"f {a} {b} {c}\n")
    print(f"{path}: {len(verts)} vertices, {len(faces)} triangles")


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.dirname(os.path.abspath(__file__))
    write(os.path.join(out, "cone-blank.obj"), "cone-blank", False)
    write(os.path.join(out, "cone-thread.obj"), "cone-thread", True)


if __name__ == "__main__":
    main()
