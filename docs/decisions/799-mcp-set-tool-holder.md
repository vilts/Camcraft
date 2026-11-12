## #799 - MCP: set_tool_holder and list_holder_library

**What was decided.** Two MCP tools in `crates/app/src/mcp/holders.rs`:
`set_tool_holder` (exactly one of `library` + optional `gauge_len` /
`projection`, `points`, or `clear: true`) and `list_holder_library`
(every entry with taper, clamp and default / min / max gauge length). All
three modes build `SetToolBody::library` / `::spun`, so each call is one
undo step and every refusal comes from the command's own validation.
`get_job` gains a `holder` field per tool (`null` for the plain holder
cylinder) so the result is readable headlessly.

**Why.** The tool editor already had this through `SetToolBody`; the
agent surface did not (AGENTS.md "Commands ship with an MCP tool").

**Considered and dropped.**
- A field on `list_tool_catalog` instead of a new list tool: the catalog
  lists tools, the library lists holders; a separate tool reads cleaner.
- A script verb: the script host has no tool-body verbs today; adding one
  would be a second surface for the same command. Not filed (the MCP
  tool is the agent surface the issue asks for).
- Rejecting a `gauge_len`/`projection` of 0 at the tool: the library's
  own `TooShort` / `BelowTip` checks decide that, by name. The tool only
  refuses negative and non-finite numbers.
- Process: the camcraft-mcp skill table edit bumps PROCESS_VERSION 10 to 11.
