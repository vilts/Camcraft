"""Provenance: User-guide coverage for the shipped features listed in #3403."""

from pathlib import Path
import re
import unittest


GUIDE = Path(__file__).resolve().parents[1] / "content/guide"


def page(path):
    return " ".join((GUIDE / path).read_text().lower().split())


def section(path, heading):
    source = (GUIDE / path).read_text()
    parts = re.split(r"^## (.+)$", source, flags=re.M)
    sections = dict(zip(parts[1::2], parts[2::2]))
    return " ".join(sections[heading].lower().split())


class Release026Guide(unittest.TestCase):
    def test_v_carve_explains_medial_axis_and_width_depth_example(self):
        text = section("operations/operations-2d.md", "V-Carve")
        self.assertIn("medial axis", text)
        self.assertIn("width", text)
        self.assertRegex(text, r"(example|for a|for example).*[0-9].*mm")
        self.assertIn("bottom z", text)

    def test_pocket_rest_machining_uses_previous_operations(self):
        text = section("operations/operations-2d.md", "2D Pocket")
        self.assertIn("rest", text)
        self.assertRegex(text, r"(previous|earlier|preceding) operations")
        self.assertIn("passes", text)
        self.assertRegex(text, r"(smaller|small) (tool|cutter)")

    def test_deburr_documents_chord_depth_passes_and_breaking_change(self):
        text = section("operations/operations-3d.md", "Deburr")
        for fact in ("chord", "constant depth", "pass", "breaking", "geometry"):
            with self.subTest(fact=fact):
                self.assertIn(fact, text)
        self.assertNotIn("the depth is not set", text)
        self.assertNotIn("it has no passes or lead in / lead out tab", text)

    def test_dxf_placement_has_stock_page_drop_and_unit_refusal(self):
        text = section("set-up/setups.md", "Place a DXF profile")
        for fact in ("stock", "drop", "import", "units", "refus"):
            with self.subTest(fact=fact):
                self.assertIn(fact, text)
        self.assertRegex(text, r"(stock (page|tab)|\*\*stock\*\* (page|tab))")
        self.assertIn("generation from", page("operations/operations-2d.md"))

    def test_cutting_data_names_nonmetal_materials_and_control(self):
        text = page("set-up/tools.md")
        for fact in ("wood", "mdf", "plywood", "acrylic", "hdpe", "cutting data"):
            with self.subTest(fact=fact):
                self.assertIn(fact, text)
        self.assertRegex(text, r"(material|workpiece)")

    def test_router_kind_exposes_milling_options(self):
        text = section("check-and-post/machines.md", "Router")
        self.assertNotIn("no separate router kind", text)
        self.assertIn("router", text)
        self.assertIn("milling", text)
        self.assertRegex(text, r"(tab|page)")
        self.assertRegex(text, r"(option|spoilboard|vacuum)")

    def test_fanuc_ai_contour_has_enable_accuracy_and_cancel(self):
        text = page("check-and-post/posting.md")
        for fact in ("fanuc", "ai contour", "g05.1", "q1", "q0"):
            with self.subTest(fact=fact):
                self.assertIn(fact, text)
        self.assertRegex(text, r"(accuracy|r[0-9]|r value|r-value)")

    def test_fanuc_manual_guide_blank_has_words_and_limits(self):
        text = page("check-and-post/posting.md")
        for fact in ("manual guide", "g1902", "g1900", "stock"):
            with self.subTest(fact=fact):
                self.assertIn(fact, text)
        self.assertRegex(text, r"(refus|unsupported|only)")

    def test_boundary_offsets_are_named_separately(self):
        text = page("operations/operations-2d.md")
        self.assertIn("inside boundary offset", text)
        self.assertIn("outside boundary offset", text)
        self.assertRegex(text, r"(positive|increase|inward|outward)")

    def test_wire_sharp_inside_corner_option_is_off_by_default(self):
        text = section("operations/operations-edm.md", "Wire Contour")
        self.assertRegex(text, r"sharp inside corners?")
        self.assertRegex(text, r"(default.{0,40}off|off.{0,40}default)")
        self.assertIn("radius", text)
        self.assertIn("geometry", text)

    def test_3d_adaptive_fallback_is_a_visible_warning(self):
        text = page("operations/operations-3d.md")
        for fact in ("3d adaptive", "offset rings", "warning"):
            with self.subTest(fact=fact):
                self.assertIn(fact, text)
        self.assertRegex(text, r"(fall.{0,10}back|fallback)")

    def test_circular_returns_to_clearance_between_rings(self):
        text = section("operations/operations-2d.md", "Circular")
        self.assertIn("clearance", text)
        self.assertIn("rings", text)
        self.assertRegex(text, r"(between|each|next).{0,100}(ring|clearance)")


if __name__ == "__main__":
    unittest.main()
