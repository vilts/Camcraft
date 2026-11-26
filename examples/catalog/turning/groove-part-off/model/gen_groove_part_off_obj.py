#!/usr/bin/env python3
"""Original CamCraft fixture: revolve a grooved bar meridian about Z.

Run this script to regenerate groove-part-off.obj beside the script.
The finished bar spans Z15..90, radius 15, with a radius 11 groove
between Z35 and Z41. Z0..15 is the stock held by the chuck.
"""
# Provenance: original meridian and elementary surface-of-revolution geometry.
import math
from pathlib import Path

SEGMENTS = 48
PROFILE = [(0, 15), (15, 15), (15, 35), (11, 35),
           (11, 41), (15, 41), (15, 90), (0, 90)]


def build():
    vertices, rings, faces = [], [], []
    for radius, z in PROFILE:
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
        raise ValueError("groove-part-off mesh must be closed with opposite edge directions")
    return vertices, faces


def main():
    vertices, faces = build()
    path = Path(__file__).with_name("groove-part-off.obj")
    with path.open("w", newline="\n") as output:
        output.write("# Original grooved bar; gen_groove_part_off_obj.py\no groove-part-off\n")
        for x, y, z in vertices:
            output.write(f"v {x:.6f} {y:.6f} {z:.6f}\n")
        for a, b, c in faces:
            output.write(f"f {a} {b} {c}\n")


if __name__ == "__main__":
    main()
