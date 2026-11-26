---
name: camcraft-mcp
description: Drive CamCraft headlessly through its MCP server (camcraft-cli mcp) - the agent workflow, the generated tool reference, undo and refusals. For Codex CLI, Gemini CLI and Antigravity.
---

# CamCraft MCP (wrapper)

This file is a thin wrapper. It holds no tool list of its own, so it cannot
drift from the server.

- The full agent guide is `.claude/skills/camcraft-mcp/SKILL.md`. Read it
  first: the workflow, the parameter shapes, undo and refusals.
- The tool reference is `docs/agents/mcp-tools.md`. It is generated from the
  live tool table (`crates/app/src/mcp/tools.rs`) by
  `camcraft-cli mcp reference`. Never edit it by hand.
- The live `tools/list` reply of the server is the authority.
- A local model (Ollama, LM Studio, Unsloth Studio, `llama-server`) takes the
  compact prompt that `camcraft-cli mcp prompt` prints.

Start the server with `camcraft-cli mcp`. Attach to an open app window with
`camcraft-cli mcp --connect 127.0.0.1:<port>` after you start the app with
`--mcp-port <port>`.

`crates/app/tests/mcp_doc_drift.rs` checks that each tool name in this file,
the guide and the reference is a live tool.
