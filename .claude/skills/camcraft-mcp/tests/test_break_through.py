# Provenance: original documentation contract for Camcraft's public MCP
# params (#2529): the break-through fields that PR #2504 (#2441) added.
from pathlib import Path
import re
import unittest


class BreakThroughSkill(unittest.TestCase):
    def test_break_through_row_names_both_fields_and_their_values(self):
        skill_dir = Path(__file__).resolve().parents[1]
        root = skill_dir.parents[2]
        skill = (skill_dir / "SKILL.md").read_text()
        row = next(
            (line for line in skill.splitlines()
             if line.startswith("- **Through-hole break-through")),
            None,
        )
        self.assertIsNotNone(row, "missing break-through row")
        for text in ('"break_through"', '"break_through_from"', "mm", "null", "1.0 mm",
                     "Drill", "Bore", "Thread", "list_operation_types", "refused"):
            with self.subTest(text=text):
                self.assertIn(text, row)
        # The wire values are the file DTO's snake_case names, default first.
        params = (root / "crates/app/src/project/params.rs").read_text()
        dto = re.search(r"pub enum BreakThroughFromDto \{(.*?)\}", params, re.S)
        self.assertIsNotNone(dto, "BreakThroughFromDto in project/params.rs")
        variants = re.findall(r"^\s*(\w+),", dto[1], re.M)
        wire = [re.sub(r"(?<!^)(?=[A-Z])", "_", v).lower() for v in variants]
        self.assertEqual(wire, ["full_diameter", "tip"])
        for value in wire:
            with self.subTest(value=value):
                self.assertIn(f'"{value}"', row)
        self.assertRegex(row, r'"full_diameter"[^.;]*the default')


if __name__ == "__main__":
    unittest.main()
