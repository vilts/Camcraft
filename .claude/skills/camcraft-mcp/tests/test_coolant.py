# Provenance: original documentation contract for Camcraft's public MCP schema.
import json
from pathlib import Path
import re
import unittest


class CoolantSkill(unittest.TestCase):
    def test_coolant_tool_row_matches_schema_and_explains_refusals(self):
        skill_dir = Path(__file__).resolve().parents[1]
        root = skill_dir.parents[2]
        source = (root / "crates/app/src/mcp/coolant_tools.rs").read_text()
        skill = (skill_dir / "SKILL.md").read_text()
        row = next(
            (line for line in skill.splitlines()
             if line.startswith("- `set_operation_coolant {")),
            None,
        )
        self.assertIsNotNone(row, "missing set_operation_coolant tool row")
        signature = row.split("`", 2)[1]
        required = json.loads(re.search(r'"required": (\[[^\]]+\])', source)[1])
        modes = json.loads(re.search(r'"enum": (\[[^\]]+\])', source)[1])
        self.assertEqual(re.findall(r'"(\w+)"\s*:', signature), required)
        self.assertIn('"name": string', signature)
        self.assertEqual(re.findall(r'"([^"{}: ]+)"', signature)[2:], modes)
        for phrase in (
            "key", "label", "undo", "cached", "missing", "non-string", "non-empty",
            "unknown operation", "unknown coolant", "warning", "still set",
            "coded", "post", "refuse",
        ):
            with self.subTest(phrase=phrase):
                self.assertIn(phrase, row.lower())


if __name__ == "__main__":
    unittest.main()
