#!/usr/bin/env python3
"""Regenerate drill-overfeed.obj, the model of the catalog refusal case.

The part is an original CamCraft fixture: an 80 x 50 x 10 mm plate with two
through-bores at (+/-25, 0). Each bore is a 6.4 mm square: the envelope of
the 6 mm drilled hole with 0.2 mm clearance. The verify gate compares the
cut volume with the model solid, so the model carries the voids.

Method: the solid is a set of grid cells over one plane set. Each face
between an inside cell and an outside cell becomes one quad, oriented
outward. The cells tile each plane the same way from both sides, so the
mesh is watertight and has no T-junctions. The script checks that each
edge is used exactly twice, in opposite directions, before it writes.

Usage: python gen_drill_overfeed_obj.py [output.obj]
"""

import sys

HALF_X, HALF_Y, TOP = 40.0, 25.0, 10.0
BORE_HALF = 3.2
BORE_X = (-25.0, 25.0)

XS = sorted({-HALF_X, HALF_X} | {c + s * BORE_HALF for c in BORE_X for s in (-1, 1)})
YS = [-HALF_Y, -BORE_HALF, BORE_HALF, HALF_Y]
ZS = [0.0, TOP]
PLANES = [XS, YS, ZS]


def solid(p):
    in_bore = any(abs(p[0] - c) < BORE_HALF for c in BORE_X) and abs(p[1]) < BORE_HALF
    return not in_bore


def corner(axis, plane, a, ia, b, ib):
    pt = [0.0, 0.0, 0.0]
    pt[axis] = PLANES[axis][plane]
    pt[a] = PLANES[a][ia]
    pt[b] = PLANES[b][ib]
    return tuple(pt)


def build():
    dims = [len(p) - 1 for p in PLANES]
    inside = set()
    for i in range(dims[0]):
        for j in range(dims[1]):
            for k in range(dims[2]):
                mid = [(PLANES[ax][n] + PLANES[ax][n + 1]) / 2 for ax, n in enumerate((i, j, k))]
                if solid(mid):
                    inside.add((i, j, k))
    quads = []
    for cell in sorted(inside):
        for axis in range(3):
            for sign in (1, -1):
                nxt = list(cell)
                nxt[axis] += sign
                if tuple(nxt) in inside:
                    continue
                plane = cell[axis] + (1 if sign > 0 else 0)
                a, b = (d for d in range(3) if d != axis)
                ia, ib = cell[a], cell[b]
                ring = [corner(axis, plane, a, ia, b, ib), corner(axis, plane, a, ia + 1, b, ib),
                        corner(axis, plane, a, ia + 1, b, ib + 1), corner(axis, plane, a, ia, b, ib + 1)]
                # Walking a then b turns about +axis for x and z, -axis for y.
                if sign * (1 if axis != 1 else -1) < 0:
                    ring.reverse()
                quads.append(ring)
    return quads


def check(quads):
    edges = {}
    for q in quads:
        for n in range(4):
            e = (q[n], q[(n + 1) % 4])
            edges[e] = edges.get(e, 0) + 1
    for (p, q), count in edges.items():
        if count != 1 or edges.get((q, p)) != 1:
            raise SystemExit(f"mesh is not closed at edge {p} -> {q}")


def write(quads, path):
    verts, index, faces = [], {}, []
    for q in quads:
        face = []
        for p in q:
            if p not in index:
                index[p] = len(verts) + 1
                verts.append(p)
            face.append(index[p])
        faces.append(face)
    with open(path, "w", newline="\n") as f:
        f.write("# drill-overfeed: 80 x 50 x 10 mm plate, two 6 mm through-bores\n")
        for v in verts:
            f.write("v {:g} {:g} {:g}\n".format(*v))
        for face in faces:
            f.write("f " + " ".join(str(n) for n in face) + "\n")


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "drill-overfeed.obj"
    mesh = build()
    check(mesh)
    write(mesh, out)
