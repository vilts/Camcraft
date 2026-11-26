#!/usr/bin/env python3
"""Regenerate rough-finish.obj, the model of the catalog case 3d/rough-finish.

The part is an original CamCraft fixture: a 50 x 50 x 8 mm plate with a
spherical dome on top. The dome has a 36 mm base diameter and is 10 mm
high, so the top of the part is at Z18.

Method: the solid is a height field over a regular XY grid. The top is
z = h(x, y) at each grid node, cut in two triangles per cell. The bottom is
the plane Z0, a fan from the centre to the boundary nodes. Four side walls
close the solid along the grid edges.
The top and the walls use the same boundary nodes, so the mesh is
watertight. The script checks that each edge is used exactly twice, in
opposite directions, before it writes.

Usage: python gen_rough_finish_obj.py [output.obj]
"""

import math
import sys

HALF = 25.0           # half side of the square plate
PLATE = 8.0           # plate thickness
DOME_A = 18.0         # dome base radius
DOME_H = 10.0         # dome height
SPHERE_R = (DOME_A ** 2 + DOME_H ** 2) / (2.0 * DOME_H)
N = 40                # grid cells per side (1.25 mm)


def height(x, y):
    r2 = x * x + y * y
    if r2 >= DOME_A * DOME_A:
        return PLATE
    return PLATE + math.sqrt(SPHERE_R * SPHERE_R - r2) - (SPHERE_R - DOME_H)


def node(i):
    return -HALF + 2.0 * HALF * i / N


def build():
    tris = []
    top = {}
    for i in range(N + 1):
        for j in range(N + 1):
            x, y = node(i), node(j)
            top[(i, j)] = (x, y, round(height(x, y), 6))
    bottom = {}
    for i in range(N + 1):
        for j in range(N + 1):
            bottom[(i, j)] = (node(i), node(j), 0.0)
    centre = (0.0, 0.0, 0.0)
    for i in range(N):
        for j in range(N):
            a, b, c, d = top[(i, j)], top[(i + 1, j)], top[(i + 1, j + 1)], top[(i, j + 1)]
            tris += [(a, b, c), (a, c, d)]           # counter-clockwise from +Z
    # Side walls: walk the boundary counter-clockwise seen from +Z.
    ring = ([(i, 0) for i in range(N)] + [(N, j) for j in range(N)]
            + [(i, N) for i in range(N, 0, -1)] + [(0, j) for j in range(N, 0, -1)])
    for n, k in enumerate(ring):
        m = ring[(n + 1) % len(ring)]
        t0, t1, b0, b1 = top[k], top[m], bottom[k], bottom[m]
        tris += [(b0, b1, t1), (b0, t1, t0)]
        tris.append((centre, b1, b0))               # bottom fan, faces -Z
    return tris


def check(tris):
    edges = {}
    for t in tris:
        for n in range(3):
            e = (t[n], t[(n + 1) % 3])
            edges[e] = edges.get(e, 0) + 1
    for (p, q), count in edges.items():
        if count != 1 or edges.get((q, p)) != 1:
            raise SystemExit(f"mesh is not closed at edge {p} -> {q}")


def write(tris, path):
    verts, index, faces = [], {}, []
    for t in tris:
        face = []
        for p in t:
            if p not in index:
                index[p] = len(verts) + 1
                verts.append(p)
            face.append(index[p])
        faces.append(face)
    with open(path, "w", newline="\n") as f:
        f.write("# rough-finish: 50 x 50 x 8 mm plate with a 36 mm x 10 mm spherical dome\n")
        for v in verts:
            f.write("v {:g} {:g} {:g}\n".format(*v))
        for face in faces:
            f.write("f " + " ".join(str(n) for n in face) + "\n")


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "rough-finish.obj"
    mesh = build()
    check(mesh)
    write(mesh, out)
