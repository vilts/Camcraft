#!/usr/bin/env python3
"""Regenerate first-part.obj, the fixture model for the docs example project.

The part is a clean-room Camcraft fixture: a 100 x 60 x 10 mm flange with a
60 x 30 mm central island rising to z = 15, and four through-bores about
(+/-38, +/-23). The bores are 6.4 mm squares - the machined envelope of
the drill op's 6 mm round holes with 0.2 mm clearance - because Camcraft's
verify gate compares the program's cut volume to the model solid, so the
model must already carry the holes' voids.

Method: the solid is exactly a union of boxes whose faces all sit on one
plane set (x: +/-30, +/-34.8, +/-41.2, +/-50; y: +/-15, +/-19.8,
+/-26.2, +/-30; z: 0, 10, 15), so the boundary can be built exactly by
voxel CSG over that grid: every inside/outside cell face is emitted as a
unit quad. Unit quads tile every plane the same way from both sides, so
the mesh is conforming (no T-junctions), watertight, and consistently
outward-oriented; the script self-checks that before writing.

Usage: python gen_first_part_obj.py [output.obj]
"""

import sys

FLANGE = (-50.0, 50.0, -30.0, 30.0, 0.0, 10.0)   # x0 x1 y0 y1 z0 z1
ISLAND = (-30.0, 30.0, -15.0, 15.0, 10.0, 15.0)
BORE_HALF = 3.2  # 6 mm hole radius plus 0.2 mm envelope clearance
BORE_XY = [(38.0, 23.0), (-38.0, 23.0), (38.0, -23.0), (-38.0, -23.0)]

XS = sorted({-50.0, -41.2, -34.8, -30.0, 30.0, 34.8, 41.2, 50.0})
YS = sorted({-30.0, -26.2, -19.8, -15.0, 15.0, 19.8, 26.2, 30.0})
ZS = [0.0, 10.0, 15.0]
PLANES = [XS, YS, ZS]


def inside_box(p, box):
    x0, x1, y0, y1, z0, z1 = box
    return x0 <= p[0] <= x1 and y0 <= p[1] <= y1 and z0 <= p[2] <= z1


def in_bore(p):
    if not (0.0 <= p[2] <= 10.0):
        return False
    return any(abs(p[0] - cx) <= BORE_HALF and abs(p[1] - cy) <= BORE_HALF
               for cx, cy in BORE_XY)


def solid(p):
    return (inside_box(p, FLANGE) or inside_box(p, ISLAND)) and not in_bore(p)


def build():
    dims = [len(p) - 1 for p in PLANES]
    inside = set()
    for i in range(dims[0]):
        for j in range(dims[1]):
            for k in range(dims[2]):
                mid = [(PLANES[a][idx] + PLANES[a][idx + 1]) / 2
                       for a, idx in enumerate((i, j, k))]
                if solid(tuple(mid)):
                    inside.add((i, j, k))

    groups = {}
    for cell in inside:
        for axis in range(3):
            for sign in (+1, -1):
                nxt = list(cell)
                nxt[axis] += sign
                if tuple(nxt) in inside:
                    continue
                plane = cell[axis] + (1 if sign > 0 else 0)
                a, b = (d for d in range(3) if d != axis)
                groups.setdefault((axis, plane, sign, a, b), set()).add(
                    (cell[a], cell[b]))

    # with a, b the other two axes in ascending order, walking (a then b)
    # gives a normal of +axis for axis 0 and 2 and -axis for axis 1
    # (hat{x} x hat{z} = -hat{y}), so check that sign before emitting
    WALK_SIGN = {0: +1, 1: -1, 2: +1}
    quads = []  # each a list of 4 (x, y, z) coordinate tuples, oriented
    for (axis, plane, sign, a, b), cells in groups.items():
        for (ca, cb) in sorted(cells):  # unit quads tile each plane exactly,
            quad = []                   # from both sides, so no T-junctions
            for p, q in [(ca, cb), (ca + 1, cb), (ca + 1, cb + 1), (ca, cb + 1)]:
                pos = [None, None, None]
                pos[axis] = PLANES[axis][plane]
                pos[a] = PLANES[a][p]
                pos[b] = PLANES[b][q]
                quad.append(tuple(pos))
            if WALK_SIGN[axis] != sign:
                quad.reverse()
            quads.append(quad)
    return quads


def check(quads):
    """Watertight and consistently oriented: every directed edge appears
    exactly once and its reverse appears exactly once."""
    seen = {}
    for quad in quads:
        for p, q in zip(quad, quad[1:] + quad[:1]):
            if p == q:
                raise SystemExit("degenerate quad: %r" % (quad,))
            if (p, q) in seen:
                raise SystemExit("directed edge reused: %r -> %r" % (p, q))
            seen[(p, q)] = True
    for (p, q) in seen:
        if (q, p) not in seen:
            raise SystemExit("open edge: %r -> %r" % (p, q))


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "first-part.obj"
    quads = build()
    check(quads)

    vid = {}
    vlines = []
    for quad in quads:
        for p in quad:
            if p not in vid:
                vid[p] = len(vlines) + 1
                vlines.append("v %g %g %g" % p)
    flines = []
    for quad in quads:
        flines.append("f " + " ".join(str(vid[p]) for p in quad))

    with open(out, "w") as fh:
        fh.write(HEADER)
        fh.write("\n" + "\n".join(vlines) + "\n\n" + "\n".join(flines) + "\n")
    print("wrote %s: %d vertices, %d faces" % (out, len(vlines), len(flines)))


HEADER = """\
# Camcraft docs example: "first part" - a 100 x 60 x 15 mm plate with a
# 5 mm shelf milled around a 60 x 30 mm central island and four 6.4 mm
# square through-bores about (+/-38, +/-23): the machined envelope of the
# drill op's 6 mm round holes, 0.2 mm clearance. The verify gate judges
# the program against the model solid, so the model carries the holes'
# voids. Authored for this repository (clean-room fixture); generated by
# gen_first_part_obj.py, this watertight quad mesh is what the example
# project embeds as facets."""

if __name__ == "__main__":
    main()
