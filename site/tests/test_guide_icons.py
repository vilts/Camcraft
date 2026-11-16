"""Pin UI icons on the documentation site against crates/ui (#1901).

Provenance: CamCraft's own icon table (`crates/ui/src/egui_app/icons.rs`),
`op_icon`, and user guide markdown; no vendor material.
"""

from html.parser import HTMLParser
from pathlib import Path
import os
import re
import subprocess
import tempfile
from urllib.parse import urljoin, urlsplit
import unittest

from test_operation_reference import FAMILIES


ROOT = Path(__file__).resolve().parents[2]
SITE = ROOT / "site"
ASSETS_UI = ROOT / "assets/ui"
ICONS_RS = ROOT / "crates/ui/src/egui_app/icons.rs"
OP_ICONS_RS = ROOT / "crates/ui/src/egui_app/op_icons.rs"
BASE = "https://example.test/Camcraft"
CONTROL_ICONS = [
    "act-generate",
    "act-post",
    "act-simulate",
    "act-show-paths",
    "tree-stock",
    "tree-wcs",
    "tree-setup",
    "tools",
]


def parse_icon_table():
    source = ICONS_RS.read_text()
    block = source.split("icon_table! {", 1)[1].split("\n}", 1)[0]
    return dict(re.findall(r"([A-Za-z0-9_]+)\s*=>\s*\"([^\"]+)\"", block))


def parse_op_icons(variant_to_file):
    source = OP_ICONS_RS.read_text()
    pairs = re.findall(r"OpType::([A-Za-z0-9_]+)\s*=>\s*Icon::([A-Za-z0-9_]+)", source)
    return {op: variant_to_file[icon] for op, icon in pairs}


class RenderedIcons(HTMLParser):
    def __init__(self):
        super().__init__()
        self.icon_sources = []
        self.links = []

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "img":
            classes = attrs.get("class", "").split()
            if "ui-icon" in classes:
                self.icon_sources.append(attrs.get("src", ""))
        elif tag == "a":
            self.links.append(attrs.get("href", ""))


class GuideIcons(unittest.TestCase):
    def test_component_static_icons_and_license_match_app(self):
        component = SITE / "templates/shortcodes/icon.html"
        self.assertTrue(component.is_file(), "Missing Zola icon component")
        component_text = component.read_text()
        self.assertIn("component icon(", component_text)
        self.assertIn("get_url", component_text)
        self.assertIn("'icons/' ~ name ~ '.png'", component_text)

        table = parse_icon_table()
        expected_stems = set(table.values())
        static_icons = SITE / "static/icons"
        self.assertTrue(static_icons.is_dir(), "Missing site/static/icons directory")

        for stem in sorted(expected_stems):
            with self.subTest(icon=stem):
                app_png = ASSETS_UI / f"{stem}.png"
                site_png = static_icons / f"{stem}.png"
                self.assertTrue(app_png.is_file(), f"Missing app icon {app_png}")
                self.assertTrue(site_png.is_file(), f"Missing site icon {site_png}")
                self.assertEqual(
                    site_png.read_bytes(),
                    app_png.read_bytes(),
                    f"{stem}.png in site/static/icons/ drifted from assets/ui/",
                )

        actual_stems = {p.stem for p in static_icons.glob("*.png")}
        self.assertEqual(actual_stems, expected_stems, "Stale or missing PNGs in site/static/icons/")

        app_notice = (ASSETS_UI / "LICENSES.md").read_text().replace("\r\n", "\n")
        site_notice = (static_icons / "LICENSES.txt").read_text().replace("\r\n", "\n")
        self.assertEqual(site_notice, app_notice)
        self.assertIn("icons/LICENSES.txt", (SITE / "templates/base.html").read_text())

        css = (SITE / "static/style.css").read_text()
        self.assertIn(".ui-icon", css)
        self.assertIn("invert(", css)

    def test_operation_sections_and_guide_components_match_app(self):
        table = parse_icon_table()
        known = set(table.values())
        op_map = parse_op_icons(table)

        for family, sections in FAMILIES.items():
            group = "multi-axis" if family == "multiaxis" else "operations"
            page = SITE / "content/guide" / group / f"operations-{family}.md"
            text = page.read_text().replace("\r\n", "\n")
            parts = re.split(r"^## (.+)$", text, flags=re.M)
            contents = dict(zip(parts[1::2], parts[2::2]))
            for kind, heading in sections.items():
                with self.subTest(kind=kind):
                    stem = op_map[kind]
                    body = contents[heading].lstrip()
                    self.assertTrue(
                        body.startswith(f'{{{{ <icon name="{stem}" /> }}}}'),
                        f"{page.name} ## {heading} must start with {{{{ <icon name=\"{stem}\" /> }}}}",
                    )

        used = set()
        for md in (SITE / "content").rglob("*.md"):
            for name in re.findall(r'\{\{\s*<icon\s+name="([^"]+)"[^>]*?/\s*>\s*\}\}', md.read_text()):
                self.assertIn(name, known, f"{md} references unknown/renamed icon '{name}'")
                used.add(name)

        for control in CONTROL_ICONS:
            self.assertIn(control, used, f"Guide should include inline icon '{control}'")

    def test_zola_build_renders_all_icons_and_notice_under_prefix(self):
        table = parse_icon_table()
        op_map = parse_op_icons(table)
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "public"
            result = subprocess.run(
                [os.environ.get("ZOLA", "zola"), "build", "--base-url", BASE, "--output-dir", str(output)],
                cwd=SITE,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertTrue((output / "icons/LICENSES.txt").is_file())
            for family, sections in FAMILIES.items():
                group = "multi-axis" if family == "multiaxis" else "operations"
                rel = f"guide/{group}/operations-{family}/"
                parser = RenderedIcons()
                parser.feed((output / rel / "index.html").read_text())
                resolved = [urljoin(f"{BASE}/{rel}", s) for s in parser.icon_sources]
                for kind in sections:
                    want = f"{BASE}/icons/{op_map[kind]}.png"
                    self.assertIn(want, resolved, f"{rel} missing rendered {want}")
                notice_urls = [urljoin(f"{BASE}/{rel}", h) for h in parser.links]
                self.assertIn(f"{BASE}/icons/LICENSES.txt", notice_urls)

            for html in output.rglob("*.html"):
                rel = html.relative_to(output).as_posix()
                parser = RenderedIcons()
                parser.feed(html.read_text())
                for src in parser.icon_sources:
                    target = urlsplit(urljoin(f"{BASE}/{rel}", src))
                    self.assertTrue(target.path.startswith("/Camcraft/icons/"), f"{rel}: {src}")
                    file_path = output / target.path.removeprefix("/Camcraft/")
                    self.assertTrue(file_path.is_file(), f"Missing built icon {file_path}")


if __name__ == "__main__":
    unittest.main()
