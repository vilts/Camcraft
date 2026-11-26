"""Provenance: Guide coverage for CamCraft changes merged after PR #1995 (#2118)."""

from pathlib import Path
import unittest


GUIDE = Path(__file__).resolve().parents[1] / "content/guide"


def read_page(path):
    return " ".join((GUIDE / path).read_text().lower().split())


class GuideAfter1995(unittest.TestCase):
    def test_ai_chat_pane_is_described_with_its_limits(self):
        mcp = read_page("automate/mcp.md")
        # #3271: 028495f5c gave the pane its transport, so the old limit
        # "no live transport endpoint" is gone; the limits that stay are
        # the missing key entry form and endpoint field.
        for fact in ("ai chat", "offline only", "undo turn", "allow",
                     "deny", "no key entry form", "no endpoint field"):
            with self.subTest(fact=fact):
                self.assertIn(fact, mcp)

    def test_open_example_and_view_panels_are_in_first_run(self):
        first = read_page("getting-started/first-run.md")
        self.assertIn("open example", first)
        self.assertIn("no example projects", first)
        self.assertIn("ai chat", first)

    def test_machine_pages_cover_waterjet_filter_and_c_axis(self):
        machines = read_page("check-and-post/machines.md")
        setups = read_page("set-up/setups.md")
        for fact in ("waterjet", "table plane", "rigid tapping", "-z"):
            with self.subTest(fact=fact):
                self.assertIn(fact, machines)
        self.assertIn("waterjet", setups)
        self.assertIn("part alignment", setups)

    def test_tools_page_covers_left_hand_taps_and_tool_library_pane(self):
        tools = read_page("set-up/tools.md")
        self.assertIn("left-hand tap", tools)
        self.assertIn("tool information", tools)
        self.assertIn("tool library", tools)

    def test_tools_page_covers_pathpilot_and_hurco_tool_tables(self):
        tools = read_page("set-up/tools.md")
        for fact in ("pathpilot", "offsets", "pathpilot hub", ".csv",
                     "hurco", ".tcm", "absolutegaugelength", "t<n>"):
            with self.subTest(fact=fact):
                self.assertIn(fact, tools)

    def test_contour_slot_and_bore_changes_are_documented(self):
        ops = read_page("operations/operations-2d.md")
        refusals = read_page("reference/refusals.md")
        self.assertIn("side auto on a picked ring", ops)
        self.assertIn("manual", ops)
        self.assertIn("spindle stop", ops)
        self.assertIn("meets its wall at no level", refusals)
        self.assertIn("tessellation stopped", refusals)


if __name__ == "__main__":
    unittest.main()
