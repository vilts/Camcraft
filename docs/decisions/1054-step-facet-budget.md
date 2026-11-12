## #1054 - the STEP facet budget is a setting carried with the import

**What was decided.** The STEP importer's facet budget, until now the
constant `import::MAX_FACETS`, is a value an import carries
(`geometry::import::step_budget`):

- *Default and bounds.* The default is today's cap, 30,000,000 facets, so
  no default import changes (no golden moves). A set budget must lie in
  1,000..=60,000,000 facets; outside that, every door refuses by name
  ("STEP facet budget N is outside 1000..=60000000 facets") before any
  parsing. A budget given for an STL or OBJ file is refused too: those
  files are their own mesh, so nothing would honour it.
- *Doors.* `ImportModel::within(path, budget)` (the GUI, the script verb and
  MCP all build that one command); `cam.import(path, name?, facetBudget?)`
  (a null name keeps the file stem); MCP `import_model` takes an optional
  integer `facet_budget`. Script and MCP accept only a whole number: a
  fraction, a negative or a string is refused, never rounded.
- *Recorded with the model.* A new component `schema::StepBudget` holds
  the budget a model's mesh was made under when it is not the default
  (`model_fidelity::budget_of` / `set_budget`). Import, update (re-import),
  repair (keeps it, like the fidelity flag), delete and their undo trade it
  with the facets. The project file stores it as an optional
  `facet_budget` key, written only when not the default, so every default
  document saves byte for byte as before and an older file loads at the
  default. A hand-edited value out of bounds, or on a non-model row, is
  refused by name at load. `get_job` and `cam.import` report it.
- *The reduced-fidelity flag is untouched.* It is still computed from the
  chord error of the kept mesh (#864). A smaller budget flags; a larger one
  meshes finer and clears it on a re-import. The refusal text now names
  the setting: "re-import with a larger STEP facet budget".
- *GUI.* The model page carries a "STEP facet budget (next import)" row
  (typed, Enter or Set; refused by name on the row and the status line,
  keeping the last good value) and says which budget the model was
  imported under. Every STEP import the window starts (Model button,
  recent list, drag and drop) reads it; a same-named import is the
  existing undoable `UpdateModel`.

**Why.** The refusal told the user to "re-import with a larger budget", a
setting that did not exist. The budget has to ride the command so the
history and the project say how the mesh was made.

**Decisions taken unattended.**

- *The ceiling is twice the default (60M).* At 72 bytes a facet that is a
  ~4.3 GiB peak for the facets alone; the default was sized as a 2.16 GiB
  peak on a large machine. A higher ceiling lets one typo (an extra zero)
  exhaust a smaller machine; a ceiling at the default would leave nothing
  to raise. Changing it later is one constant.
- *The GUI setting is an import preference, not a document change.*
  Typing it moves nothing in the world; the import that uses it is the
  undoable command and records it. It lasts for the session (not in the
  per-user prefs file) and resets to the default on restart, so a raised
  budget never silently carries into a later session's imports.
- *The setting lives on the model page,* not in a new step after the file
  picker: the import "dialog" is the native file picker, and a modal after
  every STEP pick would cost a click on every import to serve the rare
  large model. The user meets the reduced-fidelity notice on that page,
  and the hint there now says to raise the budget below and import again.
  Dropped: an options modal after the picker; a ribbon-menu entry (its
  painted rows are pinned by structural tests).
- *A GUI re-import uses the current setting,* not the model's recorded
  budget. The recorded value is shown on the page beside the setting, so
  reproducing an import is setting the shown number; an automatic
  max(recorded, setting) would make the field lie.
- *No project version bump.* An older reader drops the key and keeps the
  stored facets; it loses only the record of how they were made, which no
  verdict reads.
- *The camcraft-mcp skill doc is not edited here:* it is a process file
  (a `PROCESS_VERSION` bump); the tool's own schema and description carry
  the new argument.
