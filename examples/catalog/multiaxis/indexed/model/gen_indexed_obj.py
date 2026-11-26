#!/usr/bin/env python3
# Provenance: original planar block and pocket construction using elementary plane intersections.
"""Generate a closed block with two indexed pockets and one sloped-face pocket."""
import itertools
import math
from pathlib import Path


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def add(a, b, scale=1.0):
    return tuple(x + scale * y for x, y in zip(a, b))


def cross(a, b):
    return (a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0])


def basis(n):
    seed = min(range(3), key=lambda i: abs(n[i]))
    u = add(tuple(float(i == seed) for i in range(3)), n, -n[seed])
    length = math.sqrt(dot(u, u))
    u = tuple(x / length for x in u)
    return u, cross(n, u)


N = (math.sqrt(3) / 4, -0.5, 0.75)  # Ry(30) Rx(30) Z.
PLANES = [((1., 0., 0.), 15.), ((-1., 0., 0.), 15.),
          ((0., 1., 0.), 15.), ((0., -1., 0.), 15.),
          ((0., 0., 1.), 17.5), ((0., 0., -1.), 17.5), (N, 15.)]
POCKETS = {2: (-5., 15., -5.), 3: (-5., -15., -5.),
           6: tuple(15*x for x in N)}
HALF = 4.0
DEPTH = 2.0


def vertices():
    """Intersect each three supporting planes of this convex blank."""
    points = set()
    for (a, ha), (b, hb), (c, hc) in itertools.combinations(PLANES, 3):
        det = dot(a, cross(b, c))
        if abs(det) < 1e-10:
            continue
        p = tuple((ha*x + hb*y + hc*z) / det
                  for x, y, z in zip(cross(b, c), cross(c, a), cross(a, b)))
        if all(dot(n, p) <= h + 1e-8 for n, h in PLANES):
            points.add(tuple(round(x, 9) for x in p))
    return sorted(points)


def panels():
    points = vertices()
    faces = []
    for index, (n, height) in enumerate(PLANES):
        outer = [p for p in points if abs(dot(n, p) - height) < 1e-7]
        u, v = basis(n)
        center = POCKETS.get(index, tuple(sum(p[i] for p in outer)/len(outer) for i in range(3)))

        def angle(p):
            delta = add(p, center, -1)
            return math.atan2(dot(delta, v), dot(delta, u))

        outer.sort(key=angle)
        if index not in POCKETS:
            faces.append(outer)
            continue
        # Rays through every outer vertex and pocket corner partition the annulus.
        corners = [add(add(center, u, x), v, y)
                   for x, y in [(-HALF, -HALF), (HALF, -HALF),
                                (HALF, HALF), (-HALF, HALF)]]
        angles = sorted(set(round(angle(p), 12) for p in outer + corners))
        rim, hole = [], []
        for theta in angles:
            x, y = math.cos(theta), math.sin(theta)
            direction = add(tuple(x*t for t in u), v, y)
            distance = min((h-dot(m, center))/dot(m, direction)
                           for m, h in PLANES if dot(m, direction) > 1e-8)
            inner = HALF / max(abs(x), abs(y))
            if distance <= inner + 0.1:
                raise ValueError("pocket crosses an outer edge")
            rim.append(add(center, direction, distance))
            hole.append(add(center, direction, inner))
        floor = [add(p, n, -DEPTH) for p in hole]
        for i in range(len(angles)):
            j = (i + 1) % len(angles)
            faces.append([rim[i], rim[j], hole[j], hole[i]])
            faces.append([hole[i], hole[j], floor[j], floor[i]])
        faces.append(floor)
    return faces


def mesh():
    faces = [[tuple(round(x, 8) for x in p) for p in f] for f in panels()]
    points = sorted(set(p for f in faces for p in f))
    triangles = []
    # Match subdivisions across adjacent panels before triangulation.
    for face in faces:
        border = []
        for p, q in zip(face, face[1:] + face[:1]):
            edge = add(q, p, -1)
            length2 = dot(edge, edge)
            on_edge = []
            for r in points:
                delta = add(r, p, -1)
                t = dot(delta, edge) / length2
                residual = add(delta, edge, -t)
                if -1e-9 <= t < 1-1e-9 and dot(residual, residual) < 1e-14:
                    on_edge.append((t, r))
            border.extend(r for _, r in sorted(on_edge))
        center = tuple(round(sum(p[i] for p in border)/len(border), 8) for i in range(3))
        for p, q in zip(border, border[1:] + border[:1]):
            triangles.append((center, p, q))
    # Every directed edge must occur once and have one reverse edge.
    edges = set()
    for tri in triangles:
        area = cross(add(tri[1], tri[0], -1), add(tri[2], tri[0], -1))
        if dot(area, area) < 1e-14:
            raise ValueError("degenerate triangle")
        for edge in zip(tri, tri[1:] + tri[:1]):
            if edge in edges:
                raise ValueError("repeated directed edge")
            edges.add(edge)
    if any((q, p) not in edges for p, q in edges):
        raise ValueError("mesh has an open edge")
    return triangles


def main():
    triangles = mesh()
    points = sorted(set(p for tri in triangles for p in tri))
    ids = {p: i+1 for i, p in enumerate(points)}
    lines = ["# Original indexed block: 30 x 30 x 35 mm; three 8 x 8 x 2 mm pockets."]
    lines += ["v %.8f %.8f %.8f" % p for p in points]
    lines += ["f " + " ".join(str(ids[p]) for p in tri) for tri in triangles]
    out = Path(__file__).with_name("indexed.obj")
    out.write_text("\n".join(lines) + "\n")
    print(f"{out.name}: {len(points)} vertices, {len(triangles)} triangles; closed oriented mesh")


if __name__ == "__main__":
    main()
