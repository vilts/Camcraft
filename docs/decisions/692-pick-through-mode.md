## #692 - pick-through mode is view state, not a command

**What was decided.** The viewport gets a "Pick through" button in its nav
column (after Axes), off by default and drawn in its off state. While on,
every viewport feature pick sends `through: true`, so a click can reach an
edge or point behind the near surface. It is a plain view field,
`State::pick_through`, flipped directly by the nav button - the same way
the Wire, Paths and Axes buttons flip theirs. It is not an undoable
command and is not saved with the project.

**Why.** AGENTS.md "Everything Is a Command" lists ephemeral view state as
immediate-mode, not undoable. The neighbouring viewport toggles on main
(`viewcube::act` for Wire, Paths, Axes; the Solids radios) write their
fields directly, and `viewcube.rs` says so in its header. A pick mode
changes what a click means, but it changes no model data: undoing it would
change what the user sees in the viewport, not any part or operation, so
it belongs with the camera and display toggles rather than the history.

**Considered and dropped.**
- A command (`SetPickThrough`) with undo: the history would fill with
  view flips and undo would toggle the viewport, not the model. Rejected.
- A modifier key (hold Alt): invisible, cannot be shown as on or off, and
  Alt already selects a single segment of an edge run. Rejected.
- A control in the browser pane beside Solids: far from the cursor, and
  the viewport's own toggles are where a user looks for pick behaviour.
- Placing the button in the bottom bar of #1203: that PR is unmerged; this
  change stays on main's nav column to keep the later merge small.

**Cost to change later.** Moving the button is a `NAVS` edit plus the
click arm; a command would mean routing the flip through `CommandKind`,
an MCP tool and an undo entry. Changing the mode's meaning (for example,
through only for the Setups page) is a one-line gate in
`feature_click::query_at`.
