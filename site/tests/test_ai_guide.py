"""Provenance: shipped CamCraft AI workflows and issue #1949 guide scope."""

from pathlib import Path
import unittest


SITE = Path(__file__).resolve().parents[1]
PAGE = "guide/automate/ai.md"


class AiGuide(unittest.TestCase):
    def guide(self):
        path = SITE / "content" / PAGE
        self.assertTrue(path.is_file(), "The AI workflow guide is missing")
        return path.read_text()

    def test_provider_setup_and_shipped_limits(self):
        text = self.guide()
        for word in ["Anthropic", "OpenAI", "Gemini", "Ollama", "Unsloth Studio",
                     "LM Studio", "llama-server", "--jinja", "Offline only",
                     "ANTHROPIC_API_KEY", "OPENAI_API_KEY", "GEMINI_API_KEY",
                     "camcraft-ai", "keychain", "no endpoint field"]:
            with self.subTest(word=word):
                self.assertIn(word, text)

    def test_external_agents_and_four_worked_tasks(self):
        text = self.guide()
        for word in ["Claude Code", "Codex CLI", "Antigravity", "Gemini CLI",
                     "camcraft-cli mcp --connect", "--mcp-port",
                     "## 1. Import", "## 2. Pocket", "## 3. Change",
                     "## 4. Verify", "Undo Turn", "Allow", "Deny",
                     "small context", "connection refused", "401", "403",
                     "#1944", "#1947"]:
            with self.subTest(word=word):
                self.assertIn(word, text)

    def test_provider_setup_explains_ollama_options_and_gemini_subscription(self):
        text = self.guide()
        for instruction in [
                "A Gemini app subscription does not include an API key",
                "ollama serve", "curl http://localhost:11434/api/version",
                "ollama list", "OLLAMA_HOST=127.0.0.1:11500",
                "OLLAMA_HOST=0.0.0.0:11434", "Anyone on that network can use it",
                "OLLAMA_CONTEXT_LENGTH=32768", "OLLAMA_KEEP_ALIVE=30m"]:
            with self.subTest(instruction=instruction):
                self.assertIn(instruction, text)

    def test_indexes_and_mcp_page_link_the_guide(self):
        self.guide()
        for path in ["guide/_index.md", "guide/automate/_index.md",
                     "guide/automate/mcp.md"]:
            with self.subTest(path=path):
                text = (SITE / "content" / path).read_text()
                self.assertIn("@/" + PAGE, text)
                self.assertNotIn("pane has no network transport", text)
        self.assertIn('aliases = ["guide/ai/"]', self.guide())


if __name__ == "__main__":
    unittest.main()
