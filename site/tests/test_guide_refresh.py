"""Provenance: Guide coverage for CamCraft's own merged v0.25 features (#2854)."""

from pathlib import Path
import unittest

from test_guide_icons import section_lead


GUIDE = Path(__file__).resolve().parents[1] / "content/guide"


def read_page(path):
    return " ".join((GUIDE / path).read_text().lower().split())


class GuideRefresh(unittest.TestCase):
    def test_protection_explains_file_compatibility(self):
        for path in ("operations/operations-2d.md", "operations/operations-3d.md"):
            with self.subTest(page=path):
                page = read_page(path)
                self.assertIn("do not touch", page)
                self.assertRegex(page, r"(older|earlier) (build|version)")
                self.assertRegex(page, r"(refuse|reject|cannot open)")

    def test_dxf_placement_has_setup_and_operation_guidance(self):
        setups = read_page("set-up/setups.md")
        for fact in ("dxf", "stock", "undo"):
            with self.subTest(fact=fact):
                self.assertIn(fact, setups)
        self.assertRegex(setups, r"(older|earlier) (build|version)")
        self.assertIn("dxf", read_page("operations/operations-2d.md"))

    def test_holder_reference_and_separate_editor_are_documented(self):
        tools = read_page("set-up/tools.md")
        for fact in ("library", "reference", "body", "profile", "window", "preview"):
            with self.subTest(fact=fact):
                self.assertIn(fact, tools)
        self.assertRegex(tools, r"(older|earlier) (build|version)")

    def test_plasma_recipes_name_material_and_thickness(self):
        for path in ("operations/operations-plasma.md", "set-up/tools.md"):
            with self.subTest(page=path):
                page = read_page(path)
                for fact in ("recipe", "material", "thickness"):
                    with self.subTest(fact=fact):
                        self.assertIn(fact, page)

    def test_plasma_profile_starts_with_icon_and_schematic(self):
        page = (GUIDE / "operations/operations-plasma.md").read_text()
        profile = page.split("## Plasma Profile\n", 1)[1]
        first_paragraph = section_lead(profile).split("\n\n", 1)[0]
        self.assertTrue(first_paragraph.startswith('{{ <icon name="op-plasma-profile" /> }}'))
        self.assertIn("../../../operation-paths.svg#PlasmaProfile", first_paragraph)
        self.assertIn("### Stored torch recipes", profile)

    def test_ai_chat_connection_names_panel_limits_and_guide(self):
        page = read_page("reference/troubleshooting.md")
        connection = page.split("## ai chat connection settings", 1)[1].split("## ", 1)[0]
        self.assertIn("all local panel presets use ollama", connection)
        self.assertIn("no endpoint field", connection)
        self.assertNotIn("lm studio", connection)
        self.assertNotIn("#1949", connection)
        self.assertIn("[ai guide](../../automate/ai/)", connection)
        self.assertIn("other runtimes and external agents", connection)

    def test_deburr_filters_and_unreachable_edges_are_documented(self):
        page = read_page("operations/operations-3d.md")
        deburr = page.split("## deburr", 1)[1].split("## ", 1)[0]
        for fact in ("picked", "excluded", "minimum", "angle", "hole", "reach"):
            with self.subTest(fact=fact):
                self.assertIn(fact, deburr)

    def test_finish_gap_links_are_documented(self):
        page = read_page("operations/operations-3d.md")
        self.assertIn("gap_link", page)
        self.assertIn("clearance", page)

    def test_linked_model_source_changes_are_documented(self):
        page = read_page("set-up/setups.md")
        for fact in ("linked model", "source file", "change", "stale"):
            with self.subTest(fact=fact):
                self.assertIn(fact, page)

    def test_linuxcnc_machine_and_post_are_documented(self):
        page = read_page("check-and-post/machines.md")
        self.assertIn("linuxcnc", page)
        self.assertIn("linuxcnc.cps", page)
        self.assertIn("linuxcnc-3axis.mch", page)

    def test_post_dialog_explains_script_location(self):
        page = read_page("check-and-post/posting.md")
        self.assertRegex(page, r"(script path|script file|path to the.*script)")

    def test_viewport_input_and_saved_panel_layouts_are_documented(self):
        inputs = read_page("getting-started/keyboard-and-mouse.md")
        for fact in ("shift", "snap", "typed", "viewport"):
            with self.subTest(fact=fact):
                self.assertIn(fact, inputs)
        self.assertRegex(inputs, r"(shift.{0,160}snap|snap.{0,160}shift)")
        self.assertRegex(inputs, r"(typed.{0,160}value|value.{0,160}typed)")
        workflows = read_page("getting-started/workflows.md")
        for fact in ("panel", "save", "restore"):
            with self.subTest(fact=fact):
                self.assertIn(fact, workflows)

    def test_startup_paths_are_documented_for_cli_and_first_run(self):
        for path in ("automate/cli.md", "getting-started/first-run.md"):
            with self.subTest(page=path):
                page = read_page(path)
                self.assertIn("command line", page)
                self.assertIn("camcraft-ui", page)
                self.assertIn(".camproj", page)
                self.assertIn(".step", page)

    def test_report_workflow_explains_clipboard_and_github(self):
        page = read_page("reference/troubleshooting.md")
        for fact in ("report", "github", "clipboard", "screenshot"):
            with self.subTest(fact=fact):
                self.assertIn(fact, page)


if __name__ == "__main__":
    unittest.main()
