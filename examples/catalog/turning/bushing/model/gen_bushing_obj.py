#!/usr/bin/env python3
"""Regenerate the original bushing and its pre-bored cylindrical blank."""
# Provenance: original meridians and elementary surface-of-revolution geometry.
import math
from pathlib import Path

SEGMENTS = 48
CROWN = 6.0


def crown(d):
    return 8.0 + CROWN - math.sqrt(CROWN * CROWN - d * d)


PROFILES = {
    "bushing": [(0, 0), (10, 0), (10, 28), (crown(2), 28),
                (crown(1), 29), (8, 30), (crown(1), 31),
                (crown(2), 32), (10, 32), (10, 60),
                (6, 60), (6, 20), (0, 20)],
    "bushing-blank": [(0, 0), (10, 0), (10, 60),
                      (5.6, 60), (5.6, 20), (0, 20)],
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
        raise ValueError("bushing mesh must be closed with opposite edge directions")
    return vertices, faces


def main():
    for name, profile in PROFILES.items():
        vertices, faces = build(profile)
        path = Path(__file__).with_name(name + ".obj")
        with path.open("w", newline="\n") as output:
            output.write(f"# Original bushing; gen_bushing_obj.py\no {name}\n")
            for x, y, z in vertices:
                output.write(f"v {x:.6f} {y:.6f} {z:.6f}\n")
            for a, b, c in faces:
                output.write(f"f {a} {b} {c}\n")


if __name__ == "__main__":
    main()
