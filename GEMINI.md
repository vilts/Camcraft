# GEMINI.md

The rules for every agent in this repository are in `AGENTS.md`. Read it
first. If this file and `AGENTS.md` disagree, `AGENTS.md` is correct.

## Driving CamCraft through MCP

- The skill is `.agents/skills/camcraft-mcp/SKILL.md`. It points to the full
  guide and to the generated tool reference, `docs/agents/mcp-tools.md`.
- `camcraft-cli mcp reference` prints the tool reference from the live tool
  table. `camcraft-cli mcp prompt` prints the compact prompt for a local
  model.
- `crates/app/tests/mcp_doc_drift.rs` checks that these files name only live
  tools.
