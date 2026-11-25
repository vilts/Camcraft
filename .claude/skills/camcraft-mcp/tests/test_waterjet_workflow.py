# Provenance: original documentation contracts for the Waterjet Profile MCP workflow (#3048).
import json
from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[4]


class WaterjetWorkflowDocumentation(unittest.TestCase):
    def workflow(self):
        skill = (ROOT / ".claude/skills/camcraft-mcp/SKILL.md").read_text()
        heading = "## Waterjet Profile workflow (#1883)"
        self.assertTrue(heading in skill, "The MCP guide must document Waterjet Profile")
        start = skill.index(heading)
        end = skill.index("## Plasma Profile workflow", start)
        return skill[start:end]

    def commands(self):
        return {verb: json.loads(args) for verb, args in re.findall(
            r"^(load_machine|add_tool|add_operation) (\{.*\})$",
            self.workflow(), re.MULTILINE)}

    def test_waterjet_workflow_documents_machine_and_nozzle(self):
        commands = self.commands()
        self.assertTrue(commands["load_machine"]["path"].endswith(".mch"))
        self.assertNotIn("builtin", commands["load_machine"])
        tool = commands["add_tool"]
        self.assertEqual(tool["name"], "Nozzle")
        self.assertEqual(tool["definition"]["kind"], "waterjet")
        self.assertGreater(tool["definition"]["diameter"], 0)
        self.assertEqual(tool["definition"]["rpm"], 0)

    def test_waterjet_workflow_documents_operation_parameters(self):
        operation = self.commands()["add_operation"]
        self.assertEqual(operation["kind"], "Waterjet Profile")
        self.assertEqual(operation["tool"], "Nozzle")
        params = operation["params"]
        expected = {"kerf", "control_comp", "reversed", "lead", "lead_in",
                    "lead_out", "open_side", "tabs", "feed", "pierce_time",
                    "abrasive_delay", "linearize"}
        self.assertTrue(expected <= params.keys(), expected - params.keys())
        self.assertTrue({"count", "width", "at"} <= params["tabs"].keys())
        self.assertIn(params["lead"], ("arc", "straight"))
        self.assertIn(params["open_side"], ("center", "left", "right"))
        workflow = self.workflow()
        for value in ("arc", "straight", "center", "left", "right"):
            with self.subTest(value=value):
                self.assertIn(f"`{value}`", workflow)

    def test_waterjet_workflow_documents_availability_and_limits(self):
        workflow = " ".join(self.workflow().split())
        self.assertRegex(workflow, r"(?i)no built-in waterjet")
        self.assertRegex(workflow, r"(?i)diameter.{0,40}kerf|kerf.{0,40}diameter")
        for term in ("pierce_time", "abrasive_delay", "notes", "#1882",
                     "crates/app/tests/mcp_waterjet.rs"):
            with self.subTest(term=term):
                self.assertIn(term, workflow)
        self.assertRegex(workflow, r"(?i)no.{0,30}post.{0,30}waterjet")
        self.assertRegex(workflow, r"(?i)requires.{0,80}#1883")


if __name__ == "__main__":
    unittest.main()
