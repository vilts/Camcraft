## #867 - Copyable error text

**Decided:** one shared helper, `egui_app/error_text.rs`, draws every
error/refusal text as selectable wrapped text with a right-click "Copy
message" entry (the status strip's wording), plus a small visible "Copy"
button when the message is multi-line or over 80 characters. Wired into the
operation/setup/tool page errors, the geometry-pick refusal, the stock-pick
refusal, the tool-clash note, the Post refused window, the machines pane
and machine file editor refusals, and the four fixture dialogs.

**Why:** the user wants to paste errors into issue reports. A single
helper keeps behaviour uniform and future error sites one call.

**Dropped:** a copy button on every short error (clutter; right-click
covers it); a "copy error" command (clipboard copy is ephemeral UI, no
document state changes, so AGENTS.md needs no command); icons for the
button (a text "Copy" is clearer and needs no asset).

**Cost to change:** change the threshold constant or helper body in one file.
