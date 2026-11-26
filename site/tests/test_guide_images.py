"""Pin rendered guide images, demo projects, alt text, and Zola URLs (#1902).

Provenance: CamCraft User Guide image requirements from issue #1902;
clean-room PNG header and HTML inspection using Python stdlib only.
"""

from html.parser import HTMLParser
from collections import Counter
import json
from pathlib import Path
import os
import re
import struct
import subprocess
import tempfile
import unittest
from urllib.parse import urljoin


ROOT = Path(__file__).resolve().parents[2]
SITE = ROOT / "site"
GUIDE = SITE / "content/guide"
IMAGES_DIR = SITE / "static/images/guide"
EXAMPLES_DIR = ROOT / "examples"

REQUIRED_PAGES = [
    "set-up/setups.md",
    "set-up/workholding.md",
    "operations/operations-2d.md",
    "operations/operations-3d.md",
    "multi-axis/operations-multiaxis.md",
    "multi-axis/indexed.md",
]

REQUIRED_DEMOS = [
    "first-part.camproj",
    "guide-workholding.camproj",
    "guide-3d.camproj",
    "guide-multiaxis.camproj",
]

IMAGE_RE = re.compile(r"!\[([^\]]*)\]\(([^)]*images/guide/([A-Za-z0-9._-]+\.png))\)")
PNG_SIG = b"\x89PNG\r\n\x1a\n"


def png_dimensions(path: Path) -> tuple[int, int]:
    data = path.read_bytes()
    if len(data) < 24 or data[:8] != PNG_SIG or data[12:16] != b"IHDR":
        raise AssertionError(f"{path} is not a valid PNG with IHDR")
    width, height = struct.unpack(">II", data[16:24])
    return width, height


class GuideImgParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.guide_imgs: list[tuple[str, str]] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag != "img":
            return
        mapping = dict(attrs)
        src = mapping.get("src") or ""
        if "images/guide/" in src:
            self.guide_imgs.append((src, mapping.get("alt") or ""))


class GuideImagesTest(unittest.TestCase):
    def test_unsafe_multiaxis_images_explain_cached_warnings_and_corrections(self) -> None:
        """Unsafe demo paths must carry a warning beside their screenshot (#2078)."""
        project = json.loads((EXAMPLES_DIR / "guide-multiaxis.camproj").read_text())
        operations = {entity["name"]: entity["operation"] for entity in project["entities"]
                      if "operation" in entity}
        cases = [
            ("multi-axis/indexed.md", "op-indexed-3plus2.png", "Indexed Face1",
             {"gouge": 30, "first_traverse_low": 1}, ("tilted plane", "cut depth", "clearance height")),
            ("multi-axis/operations-multiaxis.md", "op-multiaxis-swarf.png", "Swarf1",
             {"shank_hit": 884, "rapid_in_stock": 9, "sweep_hit": 2},
             ("tool", "clearance", "retract")),
        ]
        for page, image, name, expected, corrections in cases:
            with self.subTest(operation=name):
                text = (GUIDE / page).read_text()
                match = next(match for match in IMAGE_RE.finditer(text) if match[3] == image)
                self.assertIn("unsafe", match[1].lower(), "Alt text must identify the unsafe path")
                # Only the text immediately beside this image counts, not a distant disclaimer.
                note = text[match.end():].split("\n##", 1)[0].split("**Common refusals:**", 1)[0]
                note = " ".join(note.lower().split())
                for phrase in ("unsafe example", name.lower(), "do not post or machine",
                               "regenerate", "verify", *corrections):
                    self.assertIn(phrase, note, f"{page}: missing safety explanation: {phrase}")
                warnings = Counter(key for warning in operations[name]["cache"]["warnings"]
                                   for key in warning)
                self.assertEqual(warnings, expected, "Revisit the adjacent warning explanation when the demo changes")
                for kind, count in warnings.items():
                    self.assertIn(f"{count} {kind.replace('_', '-')}", note,
                                  f"{page}: explain every cached warning beside the image")

    def test_committed_demo_projects_exist(self) -> None:
        missing = [name for name in REQUIRED_DEMOS if not (EXAMPLES_DIR / name).is_file()]
        self.assertEqual(missing, [], f"Missing committed demo project(s): {missing}")

    def test_required_pages_embed_optimised_rendered_pngs_with_alt_text(self) -> None:
        self.assertTrue(IMAGES_DIR.is_dir(), f"Missing rendered guide image dir: {IMAGES_DIR}")
        referenced: set[str] = set()
        for rel in REQUIRED_PAGES:
            text = (GUIDE / rel).read_text()
            matches = IMAGE_RE.findall(text)
            self.assertGreater(len(matches), 0, f"{rel} must embed at least one images/guide PNG")
            for alt, _url, filename in matches:
                with self.subTest(page=rel, image=filename):
                    self.assertGreater(
                        len(alt.split()), 4,
                        f"{rel} image {filename} needs descriptive alt text (> 4 words), got: {alt!r}",
                    )
                    png_path = IMAGES_DIR / filename
                    self.assertTrue(png_path.is_file(), f"Missing PNG {png_path} referenced in {rel}")
                    width, height = png_dimensions(png_path)
                    self.assertGreaterEqual(width, 1000, f"{filename} width {width} < 1000 px")
                    self.assertLessEqual(width, 1400, f"{filename} width {width} > 1400 px")
                    self.assertGreater(height, 400, f"{filename} height {height} too small")
                    size = png_path.stat().st_size
                    self.assertGreater(size, 5_000, f"{filename} looks empty ({size} bytes)")
                    self.assertLess(size, 650_000, f"{filename} is unoptimised ({size} bytes)")
                    referenced.add(filename)

        ops_2d = (GUIDE / "operations/operations-2d.md").read_text().lower()
        for kind in ("stepover", "entry", "lead"):
            with self.subTest(before_after=kind):
                self.assertIn(f"{kind}-before.png", ops_2d, f"operations-2d.md needs {kind} before view")
                self.assertIn(f"{kind}-after.png", ops_2d, f"operations-2d.md needs {kind} after view")

        on_disk = {p.name for p in IMAGES_DIR.glob("*.png")}
        self.assertEqual(on_disk, referenced, "Unused or unreferenced PNGs in site/static/images/guide")

    def test_zola_build_resolves_guide_images_with_prefix(self) -> None:
        base_url = "https://example.test/Camcraft"
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "public"
            subprocess.run(
                [os.environ.get("ZOLA", "zola"), "build", "--base-url", base_url, "--output-dir", str(output)],
                cwd=SITE, check=True, capture_output=True, text=True,
            )
            for rel in REQUIRED_PAGES:
                slug = rel.removesuffix(".md")
                html_path = output / "guide" / slug / "index.html"
                self.assertTrue(html_path.is_file(), f"Missing built HTML {html_path}")
                parser = GuideImgParser()
                parser.feed(html_path.read_text())
                self.assertGreater(len(parser.guide_imgs), 0, f"No guide <img> rendered in {slug}")
                for src, alt in parser.guide_imgs:
                    self.assertGreater(len(alt.split()), 4, f"Missing alt text on rendered <img> {src}")
                    resolved = urljoin(f"{base_url}/guide/{slug}/", src)
                    self.assertTrue(resolved.startswith(f"{base_url}/images/guide/"), f"Bad URL {resolved}")
                    rel_asset = resolved.removeprefix(f"{base_url}/")
                    self.assertTrue((output / rel_asset).is_file(), f"Missing built asset {output / rel_asset}")


if __name__ == "__main__":
    unittest.main()
