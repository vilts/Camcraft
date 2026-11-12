## #45 - Setup navigation, operation rename and ordering: the closing slice

**What was decided.** #45's GUI scope had already landed in pieces: PR #58
(v0.12.0) shipped `RenameOperation` / `ReorderOperation`, the saved
`OperationMeta.order` and drag and drop; #322 (`fb957ec`) added the Setups
ribbon cell and an always-present Setups browser row, so a new document
reaches the Setups page; #343 grouped every setup's operations under its
own node in the browser. What was still owed was the agent surface: the two
commands had no MCP tool (AGENTS.md "Commands ship with an MCP tool"). This
PR adds `rename_operation` and `reorder_operation`
(`crates/app/src/mcp/order_tools.rs`) over the same undoable commands, and
closes #45.

**Why.** The commands, their refusals (unique names, no shadowing another
op's key, same-setup only) and the round trip were already pinned
(`crates/app/tests/operation_metadata.rs`); re-implementing any of that at
the MCP door would fork the law. The tools are thin: resolve the operation
by key or display name (`edit::find_op`, as every other operation tool
does), build the command, execute it through the session History.

**Shape choices.**
- `reorder_operation` takes `name`, `target` and `place`
  (`"before"` default, or `"after"`) - the drag-and-drop's own model (drop
  onto a row, above or below its midline) rather than an absolute index,
  so the tool cannot express a cross-setup move the command would refuse
  anyway. It answers with the setup's operations in their new order, so
  an agent does not need a second `get_job`.
- `rename_operation` takes `name` and `new_name`; renaming back to the key
  clears the display name (the command's own rule).
- Pause / Home program entries (#155) are not addressable by these tools:
  the MCP surface does not expose entries at all yet, and `find_op` reads
  operations only.

**Considered and dropped.** An absolute `index` argument (dropped: needs a
second validation path the command does not have, and indexes go stale as
an agent edits); a `move_operation_to_setup` here (that is
`SetOperationSetup`, owned by #731); updating the camcraft-mcp skill's
tool table in this PR (a process file - it would bump PROCESS_VERSION for
a feature PR; the skill names `tools::list()` as the live registry).

**Cost to change later.** Low: two rows in a tool table and one handler
each; no project-file or post-output change.
