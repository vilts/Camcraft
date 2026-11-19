# Provenance: original documentation contracts for operation notes and reach checks.
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[4]


class NotesDocumentation(unittest.TestCase):
    def test_generate_and_verify_document_optional_operation_notes(self):
        skill = (ROOT / ".claude/skills/camcraft-mcp/SKILL.md").read_text()
        for verb in ("generate", "verify"):
            with self.subTest(verb=verb):
                row = next(line for line in skill.splitlines()
                           if line.startswith(f"- `{verb} {{"))
                self.assertIn("notes", row)
                self.assertRegex(row, r"(?i)array of strings|string array")
                self.assertRegex(row, r"(?i)per.operation|each operation|operation row")
                self.assertRegex(row, r"(?i)omitted when empty|absent when empty")
                self.assertRegex(row, r"(?i)informational|non.blocking")
                self.assertIn("warnings", row)

    def test_unreached_module_documents_bands_and_stock_corners(self):
        source = (ROOT / "crates/motion/src/machining2d/unreached.rs").read_text()
        header = " ".join(line.removeprefix("//!").strip()
                          for line in source.splitlines() if line.startswith("//!"))
        for concept in ("band", "corner", "stock", "noise"):
            with self.subTest(concept=concept):
                self.assertIn(concept, header.lower())
        self.assertRegex(header, r"(?i)outside.*stock|stock.*outside|air")


if __name__ == "__main__":
    unittest.main()
