"""Pin the operation reference pictures against the public operation enum.

Provenance: CamCraft's own OpType and guide sections; no vendor material.
"""

from html.parser import HTMLParser
from pathlib import Path
import os
import re
import subprocess
import tempfile
from urllib.parse import urljoin
import unittest
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[2]
SITE = ROOT / "site"
SVG = "{http://www.w3.org/2000/svg}"
FAMILIES = {
    "2d": {
        "Face": "Face", "Contour": "2D Contour", "Pocket": "2D Pocket",
        "Adaptive2D": "2D Adaptive", "Offset2D": "2D Offset", "Slot": "Slot",
        "TrochoidalSlot": "Trochoidal slot", "SlotUndercut": "Slot Undercut",
        "Trace": "Trace", "Engrave": "Engrave", "VCarve": "V-Carve",
        "Chamfer2D": "2D Chamfer", "Circular": "Circular", "Thread": "Thread",
        "Drill": "Drill", "Bore": "Bore",
    },
    "3d": {"Pocket3D": "3D Pocket", "Finish3D": "3D Finish", "Deburr": "Deburr"},
    "multiaxis": {
        "Contour5D": "5-Axis Contour", "Parallel5D": "5-Axis Parallel",
        "Flow5D": "5-Axis Flow", "MultiAxisContour": "Multi-Axis Contour",
        "Swarf": "Swarf", "AdvancedSwarf": "Advanced Swarf",
        "RotaryContour": "Rotary Contour", "RotaryParallel": "Rotary Parallel",
        "RotaryOffset": "Rotary Offset", "MultiAxisRough": "Multi-Axis Rough",
        "MultiAxisDeburr": "Multi-Axis Deburr",
    },
    "turning": {
        "Facing": "Facing", "ProfileRough": "Profile Rough",
        "ProfileFinish": "Profile Finish", "GrooveRough": "Groove Rough",
        "GrooveFinish": "Groove Finish", "Part": "Part", "Threading": "Threading",
        "Chamfering": "Chamfer", "TaperTurning": "Taper",
        "MultiPassTurning": "Multi-Pass", "FormTurning": "Form", "BoringTurning": "Bore",
    },
    "probing": {"Probe": "Probe"},
    "plasma": {"PlasmaProfile": "Plasma Profile"},
    "waterjet": {"WaterjetProfile": "Waterjet Profile"},
    "edm": {
        "WireContour": "Wire Contour", "SinkerBurn": "Sinker Burn",
        "SinkerSideBurn": "Sinker Side Burn",
    },
}


class OperationReferencePictures(unittest.TestCase):
    def test_every_operation_has_section_and_picture(self):
        source = (ROOT / "crates/app/src/operation/types/sel.rs").read_text()
        enum = source.split("pub enum OpType {", 1)[1].split("\n}", 1)[0]
        kinds = set(re.findall(r"^    ([A-Za-z][A-Za-z0-9]*),$", enum, re.M))
        documented = {kind for family in FAMILIES.values() for kind in family}
        self.assertEqual(kinds, documented, "Update the reference when OpType changes")
        for family, sections in FAMILIES.items():
            group = "multi-axis" if family == "multiaxis" else "operations"
            page = SITE / "content/guide" / group / f"operations-{family}.md"
            parts = re.split(r"^## (.+)$", page.read_text(), flags=re.M)
            contents = dict(zip(parts[1::2], parts[2::2]))
            for kind, heading in sections.items():
                with self.subTest(kind=kind):
                    self.assertIn(heading, contents, f"Missing section in {page}")
                    image = re.search(
                        rf"!\[([^\]]+)\]\(\.\./\.\./\.\./operation-paths\.svg#{kind}\)",
                        contents[heading],
                    )
                    self.assertIsNotNone(image, f"{heading} needs its toolpath picture")
                    self.assertGreater(len(image.group(1).split()), 4, "Describe the pictured path")

    def test_generated_picture_urls_preserve_site_prefix(self):
        class Pictures(HTMLParser):
            def __init__(self):
                super().__init__()
                self.sources = []

            def handle_starttag(self, tag, attrs):
                if tag == "img":
                    source = dict(attrs).get("src", "")
                    if "operation-paths.svg" in source:
                        self.sources.append(source)

        base_url = "https://example.test/Camcraft"
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "public"
            subprocess.run(
                [os.environ.get("ZOLA", "zola"), "build", "--base-url", base_url, "--output-dir", str(output)],
                cwd=SITE, check=True, capture_output=True, text=True,
            )
            self.assertTrue((output / "operation-paths.svg").is_file())
            for family, sections in FAMILIES.items():
                with self.subTest(family=family):
                    group = "multi-axis" if family == "multiaxis" else "operations"
                    relative_page = f"guide/{group}/operations-{family}/"
                    parser = Pictures()
                    parser.feed((output / relative_page / "index.html").read_text())
                    resolved = [urljoin(f"{base_url}/{relative_page}", src)
                                for src in parser.sources]
                    expected = [f"{base_url}/operation-paths.svg#{kind}" for kind in sections]
                    self.assertCountEqual(resolved, expected)

    def test_picture_views_are_valid_and_described(self):
        path = SITE / "static/operation-paths.svg"
        self.assertTrue(path.is_file(), "The reference toolpath pictures must ship with the site")
        root = ET.parse(path).getroot()
        self.assertEqual(root.tag, SVG + "svg")
        views = {view.get("id"): view for view in root.findall(SVG + "view")}
        kinds = {kind for family in FAMILIES.values() for kind in family}
        self.assertEqual(set(views), kinds)
        for kind, view in views.items():
            with self.subTest(kind=kind):
                numbers = [float(n) for n in view.attrib["viewBox"].split()]
                self.assertEqual(len(numbers), 4)
                self.assertGreater(numbers[2], 0)
                self.assertGreater(numbers[3], 0)
                group = root.find(f".//{SVG}g[@id='{kind}-diagram']")
                self.assertIsNotNone(group, f"{kind} needs its visible drawing")
                translation = re.fullmatch(r"translate\(([-\d.]+) ([-\d.]+)\)", group.get("transform", ""))
                self.assertIsNotNone(translation, f"{kind} needs its view origin")
                self.assertEqual([float(n) for n in translation.groups()], numbers[:2])
                self.assertTrue(any(
                    element.get("class") == "cut" for element in group.iter()
                ), f"{kind} needs a drawn path")
                description = group.find(SVG + "desc")
                self.assertIsNotNone(description, f"{kind} needs a diagram description")
                self.assertGreater(len("".join(description.itertext()).split()), 4)


if __name__ == "__main__":
    unittest.main()
