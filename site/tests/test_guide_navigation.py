"""Provenance: CamCraft's guide navigation and preserved public URLs (#1903)."""

from html.parser import HTMLParser
from pathlib import Path
import os
import re
import subprocess
import tempfile
from urllib.parse import unquote, urljoin, urlsplit
import unittest


SITE = Path(__file__).resolve().parents[1]
BASE = "https://example.test/Camcraft"
GROUPS = [
    ("getting-started", "Getting started"),
    ("set-up", "Set up the job"),
    ("operations", "Operations"),
    ("check-and-post", "Check and post"),
    ("multi-axis", "Multi-axis"),
    ("automate", "Automate"),
    ("reference", "Reference"),
    ("fundamentals", "CAM fundamentals"),
]
MOVED = {
    "getting-started": ["install", "first-run", "keyboard-and-mouse", "workflows"],
    "set-up": ["setups", "workholding", "tools"],
    "operations": ["operations-2d", "operations-3d", "operations-probing",
                   "operations-turning", "operations-plasma", "operations-edm"],
    "check-and-post": ["order-of-operations", "posting", "machines"],
    "multi-axis": ["operations-multiaxis"],
    "automate": ["cli", "mcp"],
    "reference": ["units-and-coordinates", "glossary", "refusals",
                  "troubleshooting", "migrating"],
}


class Document(HTMLParser):
    def __init__(self, source):
        super().__init__()
        self.stack = []
        self.links = []
        self.groups = []
        self.expanded_groups = []
        self.current_links = []
        self.sidebar_links = []
        self.ids = set()
        self.refresh = None
        self.text = []
        self.anchor = None
        self.feed(source)

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if attrs.get("id"):
            self.ids.add(attrs["id"])
        if tag == "meta" and attrs.get("http-equiv", "").lower() == "refresh":
            self.refresh = attrs.get("content", "")
        if tag == "a":
            self.anchor = [attrs, [], any(
                name == "nav" and values.get("aria-label") == "User guide"
                for name, values in self.stack
            )]
        if tag not in {"area", "base", "br", "col", "embed", "hr", "img",
                       "input", "link", "meta", "param", "source", "track", "wbr"}:
            self.stack.append((tag, attrs))

    def handle_endtag(self, tag):
        if tag == "a" and self.anchor is not None:
            attrs, parts, in_sidebar = self.anchor
            link = (attrs.get("href", ""), "".join(parts).strip())
            self.links.append(link)
            if in_sidebar:
                self.sidebar_links.append(link)
                if attrs.get("aria-current") == "page":
                    self.current_links.append(link[0])
                if "guide-group-link" in attrs.get("class", "").split():
                    self.groups.append(link)
                    if any(name == "details" and "open" in values
                           for name, values in self.stack):
                        self.expanded_groups.append(link[0])
            self.anchor = None
        for i in range(len(self.stack) - 1, -1, -1):
            if self.stack[i][0] == tag:
                del self.stack[i:]
                break

    def handle_data(self, data):
        self.text.append(data)
        if self.anchor is not None:
            self.anchor[1].append(data)


class GuideNavigation(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.directory.cleanup)
        cls.output = Path(cls.directory.name) / "public"
        result = subprocess.run(
            [os.environ.get("ZOLA", "zola"), "build", "--base-url", BASE,
             "--output-dir", str(cls.output)],
            cwd=SITE, capture_output=True, text=True,
        )
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)

    def read(self, path):
        file = self.output / path / "index.html"
        self.assertTrue(file.is_file(), f"Missing built page: {path}")
        return Document(file.read_text())

    def test_sidebar_groups_follow_a_job_on_every_guide_page(self):
        expected = [(f"{BASE}/guide/{slug}/", title) for slug, title in GROUPS]
        pages = ["guide"]
        pages.extend(f"guide/{slug}" for slug, _ in GROUPS)
        pages.extend(f"guide/{group}/{slug}" for group, slugs in MOVED.items()
                     for slug in slugs)
        pages.extend(f"guide/{section}/{path.stem}"
                     for section in ("fundamentals", "multi-axis")
                     for path in (SITE / "content/guide" / section).glob("*.md")
                     if path.stem != "_index")
        pages.append("guide/getting-started/first-router-part")
        pages.append("guide/getting-started/first-4axis-part")
        for page in pages:
            with self.subTest(page=page):
                document = self.read(page)
                self.assertEqual(document.groups, expected)
                self.assertEqual(document.current_links, [f"{BASE}/{page}/"])
                active_group = [url for url, _ in expected
                                if f"{BASE}/{page}/".startswith(url)]
                self.assertEqual(document.expanded_groups, active_group)

    def test_section_indexes_and_sidebar_reach_every_moved_page(self):
        root = self.read("guide")
        for group, slugs in MOVED.items():
            section = self.read(f"guide/{group}")
            for slug in slugs:
                with self.subTest(page=slug):
                    url = f"{BASE}/guide/{group}/{slug}/"
                    self.assertIn(url, [href for href, _ in section.links])
                    self.assertIn(url, [href for href, _ in root.sidebar_links])

    def test_old_urls_redirect_to_the_relocated_pages(self):
        for group, slugs in MOVED.items():
            for slug in slugs:
                with self.subTest(page=slug):
                    old = self.read(f"guide/{slug}")
                    self.assertIsNotNone(old.refresh, "Old URL needs a redirect alias")
                    match = re.search(r"url\s*=\s*(.+)", old.refresh, re.I)
                    self.assertIsNotNone(match)
                    self.assertEqual(match.group(1).strip(" '\""),
                                     f"{BASE}/guide/{group}/{slug}/")
                    self.assertIsNone(self.read(f"guide/{group}/{slug}").refresh)
        # The overview becomes the section index, retaining its URL and anchors.
        overview = self.read("guide/operations")
        self.assertIsNone(overview.refresh)
        self.assertIn("checking", overview.ids)
        self.assertIn("generating", overview.ids)

    def test_guide_index_has_a_start_here_path(self):
        root = self.read("guide")
        self.assertIn("start here", " ".join(root.text).lower())
        for path in ("getting-started/first-run", "set-up", "operations", "check-and-post"):
            self.assertIn(f"{BASE}/guide/{path}/", [href for href, _ in root.links])

    def test_built_internal_links_and_anchors_resolve(self):
        for file in self.output.rglob("*.html"):
            relative = file.relative_to(self.output).as_posix()
            source_url = f"{BASE}/{relative}"
            document = Document(file.read_text())
            for href, _ in document.links:
                target = urlsplit(urljoin(source_url, href))
                if target.netloc != "example.test":
                    continue
                with self.subTest(source=relative, href=href):
                    self.assertTrue(target.path.startswith("/Camcraft/"),
                                    "Internal link lost the deployment prefix")
                    path = self.output / unquote(target.path.removeprefix("/Camcraft/"))
                    if path.is_dir():
                        path /= "index.html"
                    self.assertTrue(path.is_file(), f"Missing link target: {path}")
                    if target.fragment and path.suffix == ".html":
                        self.assertIn(unquote(target.fragment), Document(path.read_text()).ids)


if __name__ == "__main__":
    unittest.main()
