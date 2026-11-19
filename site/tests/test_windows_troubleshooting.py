"""Provenance: Windows support guidance and discovery requested in issue #2423."""

from html.parser import HTMLParser
from pathlib import Path
import os
import subprocess
import tempfile
import unittest


SITE = Path(__file__).resolve().parents[1]
BASE = "https://example.test/Camcraft"
WINDOWS = "guide/reference/windows-troubleshooting"


class Article(HTMLParser):
    def __init__(self, source):
        super().__init__()
        self.in_article = False
        self.links = []
        self.parts = []
        self.feed(source)

    def handle_starttag(self, tag, attrs):
        if tag == "article":
            self.in_article = True
        if self.in_article and tag == "a":
            self.links.append(dict(attrs).get("href", ""))

    def handle_endtag(self, tag):
        if tag == "article":
            self.in_article = False

    def handle_data(self, data):
        if self.in_article:
            self.parts.append(data)

    @property
    def text(self):
        return " ".join(" ".join(self.parts).split())


class WindowsTroubleshooting(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        directory = tempfile.TemporaryDirectory()
        cls.addClassCleanup(directory.cleanup)
        cls.output = Path(directory.name) / "public"
        result = subprocess.run(
            [os.environ.get("ZOLA", "zola"), "build", "--base-url", BASE,
             "--output-dir", str(cls.output)],
            cwd=SITE, capture_output=True, text=True,
        )
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)

    def read(self, path):
        page = self.output / path / "index.html"
        self.assertTrue(page.is_file(), f"Missing built page: {path}")
        return Article(page.read_text())

    def test_install_and_faq_link_to_windows_help_in_their_articles(self):
        target = f"{BASE}/{WINDOWS}/"
        for path in ("guide/getting-started/install", "guide/reference/faq"):
            with self.subTest(page=path):
                self.assertIn(target, self.read(path).links)

    def test_windows_help_covers_startup_and_pc_diagnostics(self):
        text = self.read(WINDOWS).text
        for item in ("--gpu-backend dx12", "vulkan", "gl", "Target",
                     "XMP", "EXPO", "UMA Frame Buffer Size", "BIOS",
                     "MemTest86", "OCCT", "HWiNFO64"):
            with self.subTest(topic=item):
                self.assertIn(item, text)
        self.assertRegex(text.lower(), r"save.{0,30}work")
        self.assertRegex(text.lower(), r"driver.{0,120}hardware|hardware.{0,120}driver")

    def test_windows_help_explains_reports_and_arm_limits(self):
        text = self.read(WINDOWS).text
        for item in ("dxdiag", "Save All Information", "GitHub", "version",
                     "ARM", "Parallels", "software renderer"):
            with self.subTest(topic=item):
                self.assertIn(item, text)
        self.assertRegex(text.lower(), r"(pc|computer) name")
        self.assertRegex(text.lower(), r"emulat")
        self.assertRegex(text.lower(), r"slow")
        self.assertRegex(text.lower(), r"log")


if __name__ == "__main__":
    unittest.main()
