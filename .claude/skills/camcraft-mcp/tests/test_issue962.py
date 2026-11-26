# Provenance: original documentation contracts for the remaining rows in #962.
from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[4]


class RemainingMcpRows(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.skill = (ROOT / ".claude/skills/camcraft-mcp/SKILL.md").read_text()

    def row(self, name):
        rows = [line for line in self.skill.splitlines()
                if line.startswith(f"- `{name} {{")]
        self.assertEqual(len(rows), 1, f"expected one {name} row")
        return rows[0]

    def test_units_row_matches_schema_and_explains_the_lens(self):
        source = (ROOT / "crates/app/src/mcp/units_tools.rs").read_text()
        row = self.row("set_units")
        match = re.search(r'"enum": \[([^]]+)\]', source)
        self.assertIsNotNone(match)
        assert match is not None
        schema_words = match[1]
        self.assertEqual(re.findall(r'"([^" ]+)"', row.split("`", 2)[1]),
                         ["unit", *re.findall(r'"([^" ]+)"', schema_words)])
        for phrase in ("display", "post", "mm", "undo", "redo", "unchanged",
                       "Refused", "already", "unit"):
            with self.subTest(phrase=phrase):
                self.assertIn(phrase, row)
        gaps = self.skill.split("## Known Remaining Gaps", 1)[1].split("---", 1)[0]
        self.assertNotIn("SetUnits", gaps)

    def test_import_budget_matches_the_registered_default(self):
        source = (ROOT / "crates/app/src/mcp/tools.rs").read_text()
        match = re.search(r'STEP facet budget:.*?default (\d+)', source)
        self.assertIsNotNone(match)
        assert match is not None
        default = match[1]
        row = self.row("import_model")
        self.assertIn('"facet_budget"?: integer', row)
        for phrase in (default, "STEP", "full detail", "undo"):
            with self.subTest(phrase=phrase):
                self.assertIn(phrase, row)

    def test_job_documents_the_requested_readback_fields(self):
        source = (ROOT / "crates/app/src/mcp/job.rs").read_text()
        row = self.row("get_job")
        for key in ("units", "machine", "path", "definition", "post", "posts_with"):
            with self.subTest(key=key):
                self.assertIn(f'"{key}":', source)
                self.assertIn(f"`{key}`", row)
        self.assertIn('"mm"', row)
        self.assertIn('"inch"', row)
        self.assertIn("null", row)

    def test_previously_shipped_rows_stay_present_once(self):
        for name in ("rename_operation", "reorder_operation", "edit_tool",
                     "pick_in_box", "set_wcs_from_features", "edit_machine",
                     "set_setup_post", "set_operation_coolant"):
            with self.subTest(name=name):
                self.row(name)


if __name__ == "__main__":
    unittest.main()
