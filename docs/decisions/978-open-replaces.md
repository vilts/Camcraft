## #978 - Open replaces the MCP/script document

**Decided:** Load into a fresh app world, and only after a successful load replace the headless/script session's world, clear its undo/redo history and forget its previous posted-file path. Keep the current document, history and save destination when loading fails.

**Why:** Applying a saved project to the live world appends operations, so the posted program cuts the same job twice. A fresh world also drops entities absent from the file. The existing project loader remains responsible for parsing, migration and validation; the session owns replacement. Loading before swapping preserves the existing failed-open behavior.

**Considered and dropped:** Clearing the current world first would lose a usable document on a missing/corrupt file. Changing the project loader's global semantics would affect callers that already manage their document lifecycle. No format, schema, units, post dialect or GUI/live-MCP policy change is needed; live MCP continues refusing open_project rather than bypassing the GUI's unsaved-changes prompt.

**Cost to change later:** This adds one temporary world during open. A future shared document-open API could reuse the same success-only replacement contract. The regression checks nonempty/repeated opens, one Face post section, empty-file replacement, undo/redo reset, failed-open preservation and script posted-file metadata.
