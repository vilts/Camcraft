#!/usr/bin/env python3
"""Regenerate slots.obj, the model of the catalog case 2d/slots.

The part is a clean-room Camcraft fixture: a 100 x 60 x 12 mm block, centred
on the XY origin, z from 0 to 12. It has two closed slots along X. Each slot
is 10 mm wide and 6 mm deep, with round ends. The centre line of each slot
goes from x = -25 to x = 25. One slot is at y = 15 and one is at y = -15.

Method: the top and the bottom of the block use one grid of lines in X and
in Y. A grid cell is a quad, a round slot end, or the open middle of a slot.
A round end is two fans of triangles from the two outer corners of its cell
to the arc. The arc touches the next cell at one point, and that cell is a
fan from the point. The slot floor is a triangle fan (the outline is convex). Each wall
is one quad for each outline edge. All faces share the grid corners and the
arc points, so the mesh has no T-junctions. The script checks that the mesh
is watertight and consistently oriented, and that the top area is correct,
before it writes the file.

The arc points are on the true circle. The chords are inside the circle by
less than 0.002 mm (64 chords for each half circle).

Usage: python gen_slots_obj.py [output.obj]
"""

import math
import sys

HALF_X = 50.0
HALF_Y = 30.0
Z_BOTTOM = 0.0
Z_FLOOR = 6.0
Z_TOP = 12.0
SLOT_HALF_LENGTH = 25.0   # the centre line goes from -25 to 25
SLOT_RADIUS = 5.0         # the slot is 10 mm wide
SLOT_Y = 15.0             # the two centre lines are at y = 15 and y = -15
ARC_CHORDS = 64           # chords for each half circle; an even number

XS = [-HALF_X, -SLOT_HALF_LENGTH - SLOT_RADIUS, -SLOT_HALF_LENGTH,
      SLOT_HALF_LENGTH, SLOT_HALF_LENGTH + SLOT_RADIUS, HALF_X]
YS = [-HALF_Y, -SLOT_Y - SLOT_RADIUS, -SLOT_Y + SLOT_RADIUS,
      SLOT_Y - SLOT_RADIUS, SLOT_Y + SLOT_RADIUS, HALF_Y]
SLOT_ROWS = {1: -SLOT_Y, 3: SLOT_Y}   # grid row -> y of the slot centre line


def arc(cx, cy, start_deg):
    """The points of a half circle, counter-clockwise from start_deg. The
    first and the last point are exact grid corners."""
    pts = []
    for i in range(ARC_CHORDS + 1):
        if i == 0 or i == ARC_CHORDS or 2 * i == ARC_CHORDS:
            quarter = (start_deg + 90 * (2 * i // ARC_CHORDS)) % 360
            dx, dy = {0: (1, 0), 90: (0, 1), 180: (-1, 0), 270: (0, -1)}[quarter]
        else:
            t = math.radians(start_deg + 180.0 * i / ARC_CHORDS)
            dx, dy = math.cos(t), math.sin(t)
        pts.append((cx + SLOT_RADIUS * dx, cy + SLOT_RADIUS * dy))
    return pts


def build():
    faces = []
    top_area = 0.0
    for row in range(5):
        y0, y1 = YS[row], YS[row + 1]
        for col in range(5):
            x0, x1 = XS[col], XS[col + 1]
            # The bottom of the block: one quad for each cell, normal -Z.
            faces.append([(x0, y0, Z_BOTTOM), (x0, y1, Z_BOTTOM),
                          (x1, y1, Z_BOTTOM), (x1, y0, Z_BOTTOM)])
            if row in SLOT_ROWS and col in (1, 2, 3):
                continue
            top_area += (x1 - x0) * (y1 - y0)
            if row in SLOT_ROWS:
                # A round end touches this cell at one point. The cell is a
                # fan from that point, so the point is a corner of each face.
                cy = SLOT_ROWS[row]
                if col == 4:
                    ring = [(x0, cy), (x0, y0), (x1, y0), (x1, y1), (x0, y1)]
                else:
                    ring = [(x1, cy), (x1, y1), (x0, y1), (x0, y0), (x1, y0)]
                for i in range(1, 4):
                    faces.append([(x, y, Z_TOP) for x, y in
                                  (ring[0], ring[i], ring[i + 1])])
                continue
            faces.append([(x0, y0, Z_TOP), (x1, y0, Z_TOP),
                          (x1, y1, Z_TOP), (x0, y1, Z_TOP)])

    for row, cy in SLOT_ROWS.items():
        y0, y1 = YS[row], YS[row + 1]
        right = arc(SLOT_HALF_LENGTH, cy, 270)    # bottom -> right -> top
        left = arc(-SLOT_HALF_LENGTH, cy, 90)     # top -> left -> bottom
        half = ARC_CHORDS // 2
        # The top of the block around each round end, normal +Z.
        for pts, first, second in (
            (right, (XS[4], y0), (XS[4], y1)),
            (left, (XS[1], y1), (XS[1], y0)),
        ):
            tris = [[first, pts[i + 1], pts[i]] for i in range(half)]
            tris += [[second, pts[i + 1], pts[i]] for i in range(half, ARC_CHORDS)]
            for tri in tris:
                top_area += area2(tri) / 2.0
                faces.append([(x, y, Z_TOP) for x, y in tri])
        # The slot outline, counter-clockwise seen from +Z.
        outline = right + left
        n = len(outline)
        for i in range(1, n - 1):                 # the floor, normal +Z
            faces.append([(x, y, Z_FLOOR) for x, y in
                          (outline[0], outline[i], outline[i + 1])])
        for i in range(n):                        # the walls, normal into the slot
            (ax, ay), (bx, by) = outline[i], outline[(i + 1) % n]
            faces.append([(bx, by, Z_FLOOR), (ax, ay, Z_FLOOR),
                          (ax, ay, Z_TOP), (bx, by, Z_TOP)])

    # The four sides of the block: one quad for each grid edge, normal outward.
    for i in range(5):
        x0, x1 = XS[i], XS[i + 1]
        y0, y1 = YS[i], YS[i + 1]
        faces.append([(x0, -HALF_Y, Z_BOTTOM), (x1, -HALF_Y, Z_BOTTOM),
                      (x1, -HALF_Y, Z_TOP), (x0, -HALF_Y, Z_TOP)])
        faces.append([(x1, HALF_Y, Z_BOTTOM), (x0, HALF_Y, Z_BOTTOM),
                      (x0, HALF_Y, Z_TOP), (x1, HALF_Y, Z_TOP)])
        faces.append([(HALF_X, y0, Z_BOTTOM), (HALF_X, y1, Z_BOTTOM),
                      (HALF_X, y1, Z_TOP), (HALF_X, y0, Z_TOP)])
        faces.append([(-HALF_X, y1, Z_BOTTOM), (-HALF_X, y0, Z_BOTTOM),
                      (-HALF_X, y0, Z_TOP), (-HALF_X, y1, Z_TOP)])
    return faces, top_area


def area2(tri):
    """Two times the signed area of a 2D triangle (positive: counter-clockwise)."""
    (ax, ay), (bx, by), (cx, cy) = tri
    return (bx - ax) * (cy - ay) - (cx - ax) * (by - ay)


def check(faces, top_area):
    """Watertight and consistently oriented: every directed edge appears
    exactly once and its reverse appears exactly once. The top area is the
    block area minus the two slot outlines."""
    seen = set()
    for face in faces:
        for p, q in zip(face, face[1:] + face[:1]):
            if p == q:
                raise SystemExit("degenerate face: %r" % (face,))
            if (p, q) in seen:
                raise SystemExit("directed edge reused: %r -> %r" % (p, q))
            seen.add((p, q))
    for (p, q) in seen:
        if (q, p) not in seen:
            raise SystemExit("open edge: %r -> %r" % (p, q))

    step = math.pi / ARC_CHORDS
    half_disc = ARC_CHORDS * 0.5 * SLOT_RADIUS ** 2 * math.sin(step)
    slot = 2.0 * SLOT_HALF_LENGTH * 2.0 * SLOT_RADIUS + 2.0 * half_disc
    want = 4.0 * HALF_X * HALF_Y - 2.0 * slot
    if abs(top_area - want) > 1e-6:
        raise SystemExit("top area %.9f, expected %.9f" % (top_area, want))


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "slots.obj"
    faces, top_area = build()
    check(faces, top_area)

    vid = {}
    vlines = []
    for face in faces:
        for p in face:
            if p not in vid:
                vid[p] = len(vlines) + 1
                vlines.append("v %.9g %.9g %.9g" % p)
    flines = ["f " + " ".join(str(vid[p]) for p in face) for face in faces]

    with open(out, "w") as fh:
        fh.write(HEADER)
        fh.write("\n" + "\n".join(vlines) + "\n\n" + "\n".join(flines) + "\n")
    print("wrote %s: %d vertices, %d faces" % (out, len(vlines), len(flines)))


HEADER = """\
# Camcraft example catalog, case 2d/slots: a 100 x 60 x 12 mm block with two
# closed slots along X, each 10 mm wide and 6 mm deep, centred on the XY
# origin, z 0 to 12.
# Authored for this repository (clean-room fixture); generated by
# gen_slots_obj.py as a watertight mesh."""

if __name__ == "__main__":
    main()
