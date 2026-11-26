# Provenance: original documentation contracts for the rotary-stack tools (#1084).
from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[4]


class RotaryStackSkill(unittest.TestCase):
    def row(self, name):
        skill = (ROOT / ".claude/skills/camcraft-mcp/SKILL.md").read_text()
        rows = [line for line in skill.splitlines()
                if line.startswith(f"- `{name} {{")]
        self.assertEqual(len(rows), 1, f"expected one {name} row")
        return rows[0]

    def test_readback_names_fields_and_does_not_promise_every_stack(self):
        source = (ROOT / "crates/app/src/mcp/rotary_tools.rs").read_text()
        row = self.row("get_rotary_stack")
        for field in ("setup", "stated_nose_to_axis", "machine_rotaries",
                      "rotary", "nose_to_axis", "stack", "thickness", "layers",
                      "fixture", "refused"):
            with self.subTest(field=field):
                self.assertIn(f'"{field}":', source)
                self.assertIn(f"`{field}`", row)
        for phrase in ("active setup", "tallest", "top down", "null", "stated figure"):
            with self.subTest(phrase=phrase):
                self.assertIn(phrase, row)

    def test_setter_requires_setup_and_documents_history_and_refusals(self):
        source = (ROOT / "crates/app/src/mcp/rotary_tools.rs").read_text()
        schema = source.split("fn s_set()", 1)[1]
        required_match = re.search(r'&\[([^]]+)\]', schema)
        assert required_match is not None
        required = required_match[1]
        row = self.row("set_setup_nose_to_axis")
        signature = row.split("`", 2)[1]
        self.assertEqual(re.findall(r'"(\w+)"\s*:', signature),
                         re.findall(r'"(\w+)"', required))
        self.assertIn('"nose_to_axis": number | null', signature)
        command = (ROOT / "crates/app/src/commands/setup_nose.rs").read_text()
        limit_match = re.search(r'MAX_NOSE_MM: f64 = ([\d.]+)', command)
        assert limit_match is not None
        limit = f"{float(limit_match[1]):g}"
        for phrase in ("Both arguments are required", "null", "One undo step",
                       "undo", "redo", "previous", "finite", limit,
                       "unknown setup", "unchanged"):
            with self.subTest(phrase=phrase):
                self.assertIn(phrase, row)

    def test_subplate_dimensions_and_pin_law_match_source(self):
        add = self.row("add_accessory")
        resize = self.row("set_accessory")
        place = self.row("place_on_plate")
        self.assertIn("`rotary_subplate`", add)
        self.assertIn("`rotary_subplate`", resize)
        source = (ROOT / "crates/app/src/mcp/accessory_tools.rs").read_text()
        dims = source.split("rotary_subplate: ", 1)[1].split(". three_jaw_chuck", 1)[0]
        for field in dims.split(", "):
            with self.subTest(field=field):
                self.assertIn(f"`{field.split(' (', 1)[0]}`", resize)
        for phrase in ("pins", "Refused", "One undo step"):
            self.assertIn(phrase, resize)
        subplate = (ROOT / "crates/app/src/accessory/subplate.rs").read_text()
        tolerance_match = re.search(r'SPACING_TOL: f64 = ([\d.]+)', subplate)
        assert tolerance_match is not None
        self.assertIn(f"{float(tolerance_match[1]):g} mm", place)
        for phrase in ("dowel pair", "bore_spacing", "single hole", "Refused"):
            self.assertIn(phrase, place)


if __name__ == "__main__":
    unittest.main()
