"""Provenance: Guide coverage for CamCraft changes merged after PR #1802."""

from pathlib import Path
import re
import unittest


GUIDE = Path(__file__).resolve().parents[1] / "content/guide"


def read_page(path):
    return " ".join((GUIDE / path).read_text().lower().split())


def section(path, heading):
    source = (GUIDE / path).read_text()
    parts = re.split(r"^## (.+)$", source, flags=re.M)
    contents = dict(zip(parts[1::2], parts[2::2]))
    return " ".join(contents[heading].lower().split())


class Release022Guide(unittest.TestCase):
    def test_shipped_sodick_machine_replaces_no_wire_machine_claim(self):
        edm = read_page("operations/operations-edm.md")
        machines = read_page("check-and-post/machines.md")
        self.assertNotIn("no wire edm machine file ships", edm)
        self.assertIn("alc400g", edm)
        self.assertIn("alc400g", machines)
        self.assertIn("condition", machines)

    def test_body_electrode_workflow_replaces_rectangular_only_description(self):
        edm = section("operations/operations-edm.md", "Sinker EDM")
        self.assertNotIn("it is a rectangular, sharp-cornered block", edm)
        self.assertIn("electrode_from_faces", edm)
        self.assertIn("outline", edm)
        self.assertIn("gap", edm)

    def test_explain_gcode_names_supported_posts_and_unchanged_words(self):
        posting = read_page("check-and-post/posting.md")
        self.assertIn("explain g-code", posting)
        self.assertIn("tormach", posting)
        self.assertIn("fanuc", posting)
        self.assertRegex(posting, r"(executable|motion) words")

    def test_trochoidal_controls_are_in_adaptive_section(self):
        adaptive = section("operations/operations-2d.md", "2D Adaptive")
        for control in ("trochoidal", "engagement", "return", "feed", "loop width"):
            with self.subTest(control=control):
                self.assertIn(control, adaptive)

    def test_bore_explains_wide_hole_rings_and_blind_floor(self):
        bore = section("operations/operations-2d.md", "Bore")
        for fact in ("concentric rings", "bull-nose", "blind"):
            with self.subTest(fact=fact):
                self.assertIn(fact, bore)

    def test_stock_carry_names_side_on_turn_and_retained_gouges(self):
        carry = section("set-up/setups.md", "Carrying stock to the next setup")
        self.assertIn("side-on", carry)
        self.assertRegex(carry, r"(quarter turn|90.degree)")
        self.assertIn("gouge", carry)

    def test_aligned_3d_operations_and_area_controls_are_documented(self):
        milling = read_page("operations/operations-3d.md")
        setups = read_page("set-up/setups.md")
        for fact in ("aligned", "3d pocket", "3d adaptive", "3d finish", "area"):
            with self.subTest(fact=fact):
                self.assertIn(fact, milling)
                self.assertIn(fact, setups)
        self.assertNotIn("extra area controls, tilt combined with picks", setups)

    def test_incomplete_step_shell_and_subdivision_refusals_have_remedies(self):
        refusals = read_page("reference/refusals.md")
        self.assertIn("incomplete", refusals)
        self.assertIn("step", refusals)
        self.assertIn("subdivision", refusals)
        self.assertRegex(refusals, r"(repair|re-export)")
        self.assertIn("post", refusals)


if __name__ == "__main__":
    unittest.main()
