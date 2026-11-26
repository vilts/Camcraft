"""Provenance: original rectangular block from its dimensions; no external model."""
from pathlib import Path


def main():
    vertices = [
        (-50, -30, 0), (50, -30, 0), (50, 30, 0), (-50, 30, 0),
        (-50, -30, 12), (50, -30, 12), (50, 30, 12), (-50, 30, 12),
    ]
    faces = [
        (1, 4, 3, 2), (5, 6, 7, 8), (1, 2, 6, 5),
        (2, 3, 7, 6), (3, 4, 8, 7), (4, 1, 5, 8),
    ]
    lines = ["# Original 100 x 60 x 12 mm block, Z0 to Z12."]
    lines += ["v " + " ".join(map(str, vertex)) for vertex in vertices]
    lines += ["f " + " ".join(map(str, face)) for face in faces]
    Path(__file__).with_name("probe-face.obj").write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
