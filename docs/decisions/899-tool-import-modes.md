## #899 - tool library import modes (add / update / replace)

**Decided.** `ImportTools` takes an `ImportMode`, default `Add` (today's
behaviour, every existing test unchanged).
- **Update**: a same-named tool takes the imported definition in place (same
  entity, controller number kept, so operations keep pointing at it and read
  Stale from the stamp mismatch). An identical tool is skipped. A name the
  document lacks is added (upsert), so Update never loses a library tool.
- **Replace**: Update, and the library becomes the document's whole tool
  set: a document tool the file lacks is removed (`DeleteTool` snapshot, so
  undo restores number, body and cutter). If any operation uses such a tool
  the whole import is refused by tool and operation name, nothing applied.
- Update/Replace run the same gates as `ChangeTool` (torch refusal, retype
  that strands an operation, body/cutter fit); a library name that belongs
  to a non-tool entity refuses. Add keeps its rename-on-clash law.
- One undo unit: steps apply in order and invert in reverse. The report
  gains `updated` / `replaced` / `removed`; GUI has a mode combo beside
  "Import library..."; `cam.importTools(path, mode?)` and the MCP
  `import_tool_library` take `mode`.

**Why.** Re-importing an edited library otherwise piled up `Name (2)` copies
that operations never pointed at. Upsert for Update is the least surprising;
removal is confined to Replace and guarded by the use check (Prime Directive:
never strand an operation silently).

**Considered, dropped.** Replace that keeps unmatched tools (identical to
Update, no point); Replace that reassigns operations of a removed tool (a
hidden edit to operations); Update that resets the number (changes posted
T-words).

**Key.** The tool name, for now. #898's part number is the intended future
key: matching would move to it with the name as fallback. Cost to change:
the match in `ImportTools::with_mode`.
