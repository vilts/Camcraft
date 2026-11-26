#!/usr/bin/env python3
"""Draw the pictures of the "Your first continuous 4-axis part" guide page.

Each picture is drawn from the committed example, not by hand:
`examples/first-continuous-4axis-part.camproj` (the stock), the model
`examples/models/first-continuous-4axis-part.obj` (the journal) and the
posted program `examples/first-continuous-4axis-part.nc` (each move).
`site/tests/test_first_continuous_4axis_part.py` runs `render()` and holds
the committed files to its result.

Provenance: CamCraft's own example files; original drawings, no vendor
artwork. Colours are those of `site/static/operation-paths.svg`.
Run: python3 site/tools/first_continuous_4axis_pictures.py
"""

from pathlib import Path
import json

ROOT = Path(__file__).resolve().parents[2]
PROJECT = ROOT / "examples/first-continuous-4axis-part.camproj"
PROGRAM = ROOT / "examples/first-continuous-4axis-part.nc"
MODEL = ROOT / "examples/models/first-continuous-4axis-part.obj"
OUT = ROOT / "site/static/guide-pictures"
IN = 25.4
STYLE = (
    "<style>text{font:14px sans-serif;fill:#172b3a}.small{font-size:12px;fill:#4a5b68}"
    ".stock{fill:#e1e5e9;stroke:#7c8791;stroke-width:2}"
    ".steel{fill:#b9c1c8;stroke:#7c8791;stroke-width:2}"
    ".part{fill:none;stroke:#263b4a;stroke-width:2;stroke-dasharray:6 4}"
    ".cut{fill:none;stroke:#006ba6;stroke-width:2;stroke-linecap:round;stroke-linejoin:round}"
    ".link{fill:none;stroke:#af5900;stroke-width:1.5;stroke-dasharray:5 4}"
    ".axis{fill:none;stroke:#263b4a;stroke-width:2}"
    ".grid{fill:none;stroke:#c9d0d6;stroke-width:1}"
    ".limit{fill:none;stroke:#7c8791;stroke-width:1.5;stroke-dasharray:3 5}"
    ".center{fill:none;stroke:#7c8791;stroke-dasharray:10 4 2 4}</style>"
)


def n(value):
    """A coordinate with one decimal, without a trailing zero."""
    return f"{value:.1f}".rstrip("0").rstrip(".")


def svg(width, height, title, desc, body):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        f'viewBox="0 0 {width} {height}" role="img" aria-labelledby="t d">\n'
        "<!-- Made by site/tools/first_continuous_4axis_pictures.py from the committed "
        "example - do not edit by hand. Original drawing; no vendor artwork. -->\n"
        f"{STYLE}\n<title id=\"t\">{title}</title><desc id=\"d\">{desc}</desc>\n"
        f'<rect width="{width}" height="{height}" fill="white"/>\n' + "\n".join(body) + "\n</svg>\n"
    )


def stock_and_journal():
    """The stock box and the journal, in inches: (length, radius) each."""
    project = json.loads(PROJECT.read_text())
    box = [e for e in project["entities"] if "stock" in e][0]["stock"]["box"]
    xs, rs = [], []
    for line in MODEL.read_text().splitlines():
        if line.startswith("v "):
            x, y, z = (float(v) for v in line.split()[1:4])
            xs.append(x)
            rs.append((y * y + z * z) ** 0.5)
    return (box["dx"] / IN, box["dy"] / 2 / IN), ((max(xs) - min(xs)) / IN, max(rs) / IN)


def moves():
    """Each motion line of the program: (line number, kind, x, z, a, g93)."""
    x = z = a = None
    g93 = False
    out = []
    for number, line in enumerate(PROGRAM.read_text().splitlines(), 1):
        words = line.split()
        if not words or line.startswith("("):
            continue
        if "G93" in words:
            g93 = True
        if "G94" in words:
            g93 = False
        if words[0] == "G53":
            x = z = None  # a machine-frame move: the work position is not stated
            continue
        kind = "G0" if words[0] == "G0" else "G1" if "G1" in words[:2] else None
        if kind is None:
            continue
        start = (x, z, a)
        for word in words:
            if word[0] == "X":
                x = float(word[1:])
            elif word[0] == "Z":
                z = float(word[1:])
            elif word[0] == "A":
                a = float(word[1:])
        out.append((number, kind, start, (x, z, a), g93 and kind == "G1"))
    return out


def setup_picture():
    (stick, bar_r), (length, journal_r) = stock_and_journal()
    s, x0, cy = 170.0, 440.0, 150.0  # pixels per inch, the pixel of X0, the centreline
    px = lambda x: n(x0 + x * s)
    py = lambda z: n(cy - z * s)
    ex = 620.0  # the end view's centre
    body = [
        '<text x="20" y="26">Side view</text>',
        f'<rect class="steel" x="{px(-stick - 0.35)}" y="{py(0.62)}" width="{n(0.35 * s)}" height="{n(1.24 * s)}"/>',
        f'<text class="small" x="{px(-stick - 0.33)}" y="{n(cy + 0.62 * s + 18)}">chuck jaws</text>',
        f'<rect class="stock" x="{px(-stick)}" y="{py(bar_r)}" width="{n(stick * s)}" height="{n(2 * bar_r * s)}"/>',
        f'<rect class="part" x="{px(-length)}" y="{py(journal_r)}" width="{n(length * s)}" height="{n(2 * journal_r * s)}"/>',
        f'<path class="center" d="M{px(-stick - 0.45)} {n(cy)} H{px(0.3)}"/>',
        f'<circle cx="{px(0)}" cy="{n(cy)}" r="5" fill="#263b4a"/>',
        f'<path class="axis" d="M{px(0)} {n(cy)} H{px(0.42)} m-8 -5 l8 5 l-8 5 M{px(0)} {n(cy)} V{py(0.72)} m-5 8 l5 -8 l5 8"/>',
        f'<text x="{px(0.45)}" y="{n(cy + 5)}">X</text><text x="{px(0.05)}" y="{py(0.74)}">Z</text>',
        f'<text class="small" x="{px(-0.02)}" y="{n(cy + 0.62 * s + 18)}" text-anchor="end">G54 X0 at the free end face</text>',
        f'<path class="grid" d="M{px(-stick)} {py(-0.5)} V{py(-0.62)} M{px(0)} {py(-0.5)} V{py(-0.62)} M{px(-stick)} {py(-0.56)} H{px(0)}"/>',
        f'<text class="small" x="{px(-stick / 2)}" y="{py(-0.56) and n(cy + 0.56 * s - 6)}" text-anchor="middle">{stick:g} in out of the jaws</text>',
        f'<text class="small" x="{px(-length / 2)}" y="{n(cy - 4)}" text-anchor="middle">model: {2 * journal_r:.3f} in across, {length:g} in long</text>',
        f'<text class="small" x="{px(-stick + 0.05)}" y="{py(bar_r + 0.06)}">bar: {2 * bar_r:g} in</text>',
        f'<text x="{n(ex - 80)}" y="26">End view</text>',
        f'<circle class="stock" cx="{n(ex)}" cy="{n(cy)}" r="{n(bar_r * s)}"/>',
        f'<circle class="part" cx="{n(ex)}" cy="{n(cy)}" r="{n(journal_r * s)}"/>',
        f'<path class="center" d="M{n(ex - 95)} {n(cy)} H{n(ex + 95)} M{n(ex)} {n(cy - 95)} V{n(cy + 95)}"/>',
        f'<circle cx="{n(ex)}" cy="{n(cy)}" r="5" fill="#263b4a"/>',
        f'<text class="small" x="{n(ex)}" y="{n(cy + 0.62 * s + 18)}" text-anchor="middle">G54 Y0 Z0 on the A centreline</text>',
    ]
    return svg(720, 300, "The bar in the microARC 4 chuck and the work offset",
               "Side and end views of the bar: G54 X0 is at the free end face, and Y0 and Z0 are on "
               "the A centreline. The dashed outline is the model.", body)


def a_count_picture():
    data = [(number, end[2], g93) for number, _, _, end, g93 in moves() if end[2] is not None]
    last = sum(1 for _ in PROGRAM.read_text().splitlines())
    left, right, top, bottom = 70.0, 690.0, 40.0, 250.0
    px = lambda line: n(left + (right - left) * line / last)
    py = lambda a: n(bottom - (bottom - top) * (a + 360.0) / 720.0)
    body = ['<text x="20" y="24">The A word, line by line (degrees)</text>']
    for a in (-360, -180, 0, 180, 360):
        cls = "grid"
        body.append(f'<path class="{cls}" d="M{n(left)} {py(a)} H{n(right)}"/>')
        body.append(f'<text class="small" x="{n(left - 8)}" y="{n(float(py(a)) + 4)}" text-anchor="end">{a}</text>')
    body.append(f'<text class="small" x="{n(right)}" y="{n(float(py(360)) - 6)}" text-anchor="end">Chart scale only; the shipped file states no A travel limit</text>')
    points = " ".join(f"{px(line)},{py(a)}" for line, a, _ in data)
    body.append(f'<polyline class="cut" points="{points}"/>')
    hi = max(data, key=lambda d: d[1])
    lo = min(data, key=lambda d: d[1])
    body.append(f'<text class="small" x="{px(hi[0])}" y="{n(float(py(hi[1])) - 8)}" text-anchor="middle">A{hi[1]:g}</text>')
    body.append(f'<text class="small" x="{px(lo[0])}" y="{n(float(py(lo[1])) + 16)}" text-anchor="middle">A{lo[1]:g}</text>')
    ticks = "".join(f"M{px(line)} 262 v10" for line, _, g93 in data if g93)
    body.append(f'<path d="{ticks}" stroke="#006ba6" stroke-width="1" fill="none"/>')
    body.append(f'<text class="small" x="{n(left - 8)}" y="271" text-anchor="end">G93</text>')
    for line in (1, 1000, 2000, 3000):
        body.append(f'<text class="small" x="{px(line)}" y="292" text-anchor="middle">line {line}</text>')
    return svg(720, 300, "The A word of each line of the program",
               f"A stays between {lo[1]:g} and {hi[1]:g} degrees and passes 180 without a jump back. "
               "The chart scale is not a machine limit. The marks under the chart are the lines "
               "that run in G93.", body)


def paths_picture():
    (_, _), (length, _) = stock_and_journal()
    left, right, top, bottom = 70.0, 690.0, 40.0, 300.0
    x_lo, x_hi, a_lo, a_hi = -1.4, 0.15, -90.0, 270.0
    px = lambda x: n(left + (right - left) * (x - x_lo) / (x_hi - x_lo))
    py = lambda a: n(bottom - (bottom - top) * (a - a_lo) / (a_hi - a_lo))
    body = ['<text x="20" y="24">Each move, unrolled: X along the bar against the A angle</text>']
    for a in (-90, 0, 90, 180, 270):
        body.append(f'<path class="grid" d="M{n(left)} {py(a)} H{n(right)}"/>')
        body.append(f'<text class="small" x="{n(left - 8)}" y="{n(float(py(a)) + 4)}" text-anchor="end">A{a}</text>')
    for x, label in ((-length, f"X-{length:g}: chuck-side end"), (0.0, "X0: free end")):
        body.append(f'<path class="limit" d="M{px(x)} {n(top)} V{n(bottom)}"/>')
        anchor = "start" if x < 0 else "end"
        body.append(f'<text class="small" x="{px(x)}" y="{n(bottom + 18)}" text-anchor="{anchor}">{label}</text>')
    feed, rapid = [], []
    for _, kind, start, end, _ in moves():
        if None in (start[0], start[2], end[0], end[2]):
            continue
        if (start[0], start[2]) == (end[0], end[2]):
            continue
        segment = f"M{px(start[0])} {py(start[2])}L{px(end[0])} {py(end[2])}"
        (rapid if kind == "G0" else feed).append(segment)
    body.append(f'<path class="link" d="{"".join(rapid)}"/>')
    body.append(f'<path class="cut" style="stroke-width:1.2" d="{"".join(feed)}"/>')
    body.append(f'<path class="cut" d="M{n(left)} 330 h30"/><text class="small" x="{n(left + 38)}" y="334">feed (G1)</text>')
    body.append(f'<path class="link" d="M{n(left + 130)} 330 h30"/><text class="small" x="{n(left + 168)}" y="334">rapid (G0)</text>')
    return svg(720, 345, "The moves of the program, unrolled",
               "Feed moves: a straight pass along the bar at one A angle, a turn of A across "
               "the free end face past the end of the model, and a pass back at another angle. At the "
               "chuck-side end the passes stop at the end of the model and rapids turn A. One family "
               "of passes is about A0 and one is about A180.",
               body)


def render():
    """Each picture's file name and text."""
    return {
        "first-continuous-4axis-setup.svg": setup_picture(),
        "first-continuous-4axis-a-count.svg": a_count_picture(),
        "first-continuous-4axis-paths.svg": paths_picture(),
    }


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    for name, text in render().items():
        (OUT / name).write_text(text)
