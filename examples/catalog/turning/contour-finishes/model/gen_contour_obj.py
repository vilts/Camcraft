#!/usr/bin/env python3
"""Regenerate the original contour and its cylindrical blank."""
# Provenance: original meridians and elementary surface-of-revolution geometry.
import math
from pathlib import Path

SEGMENTS = 48
PROFILES = {
    "contour-finishes": [(0, 0), (8, 0), (10, 2), (10, 12),
                         (9, 12), (9, 60), (0, 60)],
    "contour-blank": [(0, 0), (10, 0), (10, 60), (0, 60)],
}


def build(profile):
    vertices, rings, faces = [], [], []
    for radius, z in profile:
        if radius == 0:
            vertices.append((0, 0, z))
            rings.append([len(vertices)] * SEGMENTS)
        else:
            ring = []
            for j in range(SEGMENTS):
                angle = math.tau * j / SEGMENTS
                vertices.append((radius * math.cos(angle), radius * math.sin(angle), z))
                ring.append(len(vertices))
            rings.append(ring)
    for a, b in zip(rings, rings[1:]):
        for j in range(SEGMENTS):
            k = (j + 1) % SEGMENTS
            if a[j] == a[k]:
                faces.append((a[j], b[k], b[j]))
            elif b[j] == b[k]:
                faces.append((a[j], a[k], b[j]))
            else:
                faces.extend([(a[j], a[k], b[k]), (a[j], b[k], b[j])])
    edges = [(f[i], f[(i + 1) % 3]) for f in faces for i in range(3)]
    directed = set(edges)
    if len(directed) != len(edges) or any((b, a) not in directed for a, b in edges):
        raise ValueError("contour mesh must be closed with opposite edge directions")
    return vertices, faces


def main():
    for name, profile in PROFILES.items():
        vertices, faces = build(profile)
        path = Path(__file__).with_name(name + ".obj")
        with path.open("w", newline="\n") as output:
            output.write(f"# Original contour; gen_contour_obj.py\no {name}\n")
            for x, y, z in vertices:
                output.write(f"v {x:.6f} {y:.6f} {z:.6f}\n")
            for a, b, c in faces:
                output.write(f"f {a} {b} {c}\n")


if __name__ == "__main__":
    main()
