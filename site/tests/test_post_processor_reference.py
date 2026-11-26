"""Provenance: CamCraft post authoring contract and user guide."""

from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
PAGE = ROOT / "site/content/guide/reference/post-processors.md"


class PostProcessorReference(unittest.TestCase):
    def test_manifest_properties_have_reference_rows(self):
        contract = (ROOT / "assets/posts/AGENTS.md").read_text()
        manifest = contract.split("Manifest: top-level assignments only", 1)[1].split("## The hook contract", 1)[0]
        properties = set(re.findall(r"`([A-Za-z][A-Za-z0-9]*)`", manifest.split("All but", 1)[0]))
        properties.update(re.findall(r"optionally `([A-Za-z][A-Za-z0-9]*)`", manifest))
        properties.update(re.findall(r"`([A-Za-z][A-Za-z0-9]*)\s*=(?!=)", contract))
        properties.add("declaredOptions")
        properties.update({"lineEnding", "linearizeArcs", "uprightReturn"})
        self.assertIn("description", properties)
        self.assertIn("continuousThreading", properties)
        self.assertTrue(PAGE.is_file(), "Add the post-processor API reference")
        rows = set(re.findall(r"^\| `([A-Za-z][A-Za-z0-9]*)` \|", PAGE.read_text(), re.M))
        self.assertFalse(properties - rows, f"Missing manifest reference rows: {sorted(properties - rows)}")

    def test_engine_hooks_have_reference_entries(self):
        host = (ROOT / "crates/post/src/post/js_host.rs").read_text()
        hooks_block = host.split("pub const HOOKS:", 1)[1].split("];", 1)[0]
        hooks = set(re.findall(r"\"(on[A-Za-z]+)\"", hooks_block))
        hooks.update({"onJetOff", "onJetOn"})
        self.assertIn("onOpen", hooks)
        self.assertTrue(PAGE.is_file(), "Add the post-processor API reference")
        documented = set(re.findall(r"`(on[A-Za-z]+)\(", PAGE.read_text()))
        self.assertFalse(hooks - documented, f"Missing hook signatures: {sorted(hooks - documented)}")

    def test_posting_guide_links_to_reference(self):
        posting = (ROOT / "site/content/guide/check-and-post/posting.md").read_text()
        self.assertIn("@/guide/reference/post-processors.md", posting)


if __name__ == "__main__":
    unittest.main()
