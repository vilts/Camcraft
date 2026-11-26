#!/usr/bin/env python3
"""Write first-continuous-4axis-part.obj: the journal of the guide's demo part.

A closed cylinder along X, 0.680 in across and 1.25 in long, with its free
end at X 0 and its axis through Y 0, Z 0. It is the finished surface that
the Rotary Parallel pass of "Your first continuous 4-axis part" cuts on a
3/4 in bar. Numbers are millimetres (an OBJ file has no unit).

Provenance: CamCraft's own demo geometry; no vendor material.
Run: python3 examples/models/gen_first_continuous_4axis_part_obj.py
"""

from math import cos, pi, sin
from pathlib import Path

IN = 25.4
RADIUS = 0.340 * IN
LENGTH = 1.25 * IN
SIDES = 96


def solid(name, title, rings):
    """A closed solid of revolution about X through the (x, radius) rings."""
    lines = [
        f"# CamCraft example: {title} (millimetres)",
        "# Made by gen_first_continuous_4axis_part_obj.py - do not edit by hand",
        f"o {name}",
    ]
    for x, r in rings:
        for j in range(SIDES):
            t = 2.0 * pi * j / SIDES
            lines.append(f"v {x:.6f} {r * cos(t):.6f} {r * sin(t):.6f}")
    lines.append(f"v {rings[0][0]:.6f} 0.000000 0.000000")
    lines.append(f"v {rings[-1][0]:.6f} 0.000000 0.000000")
    back, front = len(rings) * SIDES + 1, len(rings) * SIDES + 2
    last = (len(rings) - 1) * SIDES
    for j in range(SIDES):
        k = (j + 1) % SIDES
        for ring in range(len(rings) - 1):
            a0, a1 = ring * SIDES + j + 1, ring * SIDES + k + 1
            b0, b1 = a0 + SIDES, a1 + SIDES
            lines.append(f"f {a0} {a1} {b1}")
            lines.append(f"f {a0} {b1} {b0}")
        lines.append(f"f {back} {k + 1} {j + 1}")
        lines.append(f"f {front} {last + j + 1} {last + k + 1}")
    Path(__file__).with_name(name + ".obj").write_text("\n".join(lines) + "\n")


def main():
    # From the chuck side to the free end.
    solid("first-continuous-4axis-part", "Your first continuous 4-axis part",
          [(-LENGTH, RADIUS), (0.0, RADIUS)])


if __name__ == "__main__":
    main()
