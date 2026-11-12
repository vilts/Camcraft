## #889 - tool library files carry the tool body; "Import tool holders" on the import page

**What was decided.** Built to the user's direction (2026-11-11: "make it
an import toggle for the user on the import page").

1. *Format.* A `.camtools` tool's `definition` may carry `body`, the very
   `ToolBodyDto` a `.camproj` tool stores (drawn holder profile, placed
   mesh, or lathe holder with its insert zone). Additive, no version step:
   `camcraft-tools/1` stays, a file without the key reads as before, and an
   older build parses the key into `ToolDto::body` and drops it (it reads
   only the tool record). Export writes `body` for every tool that has one.
2. *Safety.* A body is judged at read exactly as the project loader
   (`project::validate`) and `SetToolBody` judge one: re-derived by this
   build's envelope law (`ToolBodyDto::to_body`) and fitted to its tool
   (`tool_body::fits`); any refusal refuses the whole file naming the tool.
   `ImportTools` judges the fit again at command creation. Never a body the
   holder laws would misread.
3. *Toggle.* "Import tool holders" is a checkbox beside Import library on
   the Tool library page, **ticked by default**: a library that carries
   bodies was exported from a document where someone fitted those holders,
   so taking them is the expected round trip, and a file without bodies
   imports identically either way. It is the `holders` parameter of the one
   undoable `ImportTools::with_holders` command (`ImportTools::new` = on),
   not a field poke. Unticked lands every tool bare (holder cylinder only).
4. *Remembered per user* (the issue's own comment, cf. #877): kept in the
   per-user prefs file as `importToolHolders`, written only when false, so
   every older prefs file reads as on. Not project state, not on history.
5. *Identity for the skip.* A library tool that lands bare (no body, or
   holders off) is "already in the document" when the tool record matches,
   exactly as before - so an old bodyless file never duplicates a tool the
   user has since fitted a holder to. A tool that lands with a body matches
   only when the document tool's body is the same too (compared as the DTO
   a save would write); otherwise it arrives renamed "Name (2)" and the
   document's own tool and holder are never overwritten.
6. *MCP.* `import_tool_library` takes optional `holders` (boolean, default
   true); its schema and description say so; the reply adds `holders` and
   `holders_left_out`. The script verb takes it as a third optional
   argument (item 8).
7. *Counts.* The status line and MCP reply name how many tools landed with
   holders and how many file holders were left out.

8. *With #899's import modes (merged main, 2026-11-12).* The holders
   choice and `ImportMode` are two parameters of the one command,
   `ImportTools::with_options(world, library, mode, holders)`
   (`with_mode` = holders on, `with_holders` = Add). #899's Add / Update /
   Replace semantics, its `Step` plan and its `change_refusal` gates are
   unchanged; bodies ride on top:
   - **Add**: as item 5 - land with the file body (on) or bare (off).
   - **Update / Replace, holders on, file carries a body**: a body that
     differs from the document tool's (compared as the DTO a save writes)
     is a change like any other, even when the tool record is identical.
     It goes through `change_refusal` (retype that strands an operation,
     torch refusal, cutter-profile agreement, and the fit of the body that
     will stand), lands in the same `Step::Change` (same entity, number
     kept), and undo restores the previous body. The operation stamp
     already digests the tool body (#368), so operations using the tool go
     Stale exactly as after `SetToolBody` - pinned against a twin world.
   - **Update / Replace, holders off, or the file carries no body for that
     tool**: the document's body stays as it is - never cleared silently -
     and must fit the incoming record (`change_refusal` checks it); an
     identical record with holders off is "already in the document".
   - Every file body is still judged with `tool_body::fits` at command
     creation, in every mode; a bad one refuses the file by name.
   - MCP `import_tool_library` takes both `mode` and `holders`; the script
     verb is `cam.importTools(path, mode?, holders?)` (variadic; `cam.d.ts`
     regenerated, the reply adds `holders` / `holders_left_out`); the Tool
     library page keeps #899's mode combo and this PR's checkbox.

**Considered and dropped.**
- *A bodyless file entry clearing the document body in Update / Replace.*
  An older or holders-off export says nothing about the holder, and a
  fitted holder is a safety input of the collision laws; clearing it
  would widen nothing but would silently lose it. Clearing stays an
  explicit `SetToolBody` (clear) step.
- *Treating a body-only difference in Update as "identical" (skip).* It
  would leave the user's operations computed against a holder the file
  says is gone - the M3 review of #1100 flagged exactly this.
- *Showing the count before the import.* The toggle stands on the page
  before any file is picked, so the count arrives in the status line after
  the import instead; a pre-import preview dialog would be a new modal for
  little gain.
- *Storing the holder-library entry name.* A landed library holder is a
  drawn profile (`SPUN_SOURCE`), and the project file stores no entry name
  either; the profile itself is what the laws read and round-trips
  bit-identical. Adding a name is a separate format change.
- *The drawn cutter outline (`ToolCutter`).* Display-only data, outside the
  holder question; left for a follow-up if wanted.
- *A version bump of the library format.* Not needed: the key is additive
  and an older reader over-reports nothing (it imports the tool bare).

**PROCESS_VERSION.** The camcraft-mcp skill line for `import_tool_library`
changed (now with #899's `mode` as well), so this PR bumps
`.claude/PROCESS_VERSION` to main's 18 + 1 = 19. Other open process PRs
also claim 19-23; whichever merges later takes the next number when it
merges main.

**Cost to change later.** Low: the default of the toggle is one field
initialiser and one prefs default; the identity rule is one closure in
`commands/tool_import.rs`.
