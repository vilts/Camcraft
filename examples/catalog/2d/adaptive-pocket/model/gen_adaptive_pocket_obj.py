#!/usr/bin/env python3
# Provenance: original box-grid boundary mesh using the repository fixture method.
"""Generate the watertight U-shaped open pocket; no third-party geometry."""

import sys

PLANES = [[-30.0, -10.0, 30.0], [-20.0, -10.0, 10.0, 20.0], [0.0, 6.0, 12.0]]


def solid(p):
    # Remove the upper middle cell through the positive-X edge.
    return not (p[0] > -10.0 and -10.0 < p[1] < 10.0 and p[2] > 6.0)


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
    out = sys.argv[1] if len(sys.argv) > 1 else "adaptive-pocket.obj"
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


HEADER = "# Original CamCraft open pocket: 60 x 40 x 12 mm, floor Z6, open at X30."

if __name__ == "__main__":
    main()
