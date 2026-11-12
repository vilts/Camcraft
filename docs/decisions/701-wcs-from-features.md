## #701 - Set the WCS from any set of feature picks

**Decided.** The app gains `pick::align_roles(world, picks) -> Result<Alignment, String>`:
each pick is a `(GeoSel, Role)` pair and `Role` is `Origin | X | Y | Z`.
It returns the same `Alignment` struct `pick::align`'s 3-2-1 already
returns, so the fit law and its tests sit together. The Setups page gets
a "Set WCS from features" row: arming it keeps the toolbar's pick mode
(face, edge, vertex or wall), stamps every viewport pick with the role
shown in the row's role box, and Apply queues the existing undoable
`CommandKind::SetWcs` - no new command kind.

**The frame law.** Among the picks present, axis roles are read in the
priority order Z, X, Y: the first exact, the second projected
perpendicular to it, the third derived to close the right-handed frame -
so a third axis pick is overridden wholesale, not blended. An origin
pick sets the origin; without one the origin stays the model's own.
Refusals come back as words, in order: fewer than two picks, a duplicated
role, fewer than two axis roles, an unresolvable datum, a tilt the pick
set cannot support, a stale feature id.

**Cylinder walls.** A wall pick fits its axis by covariance null-space
(Jacobi) and its circle by Kåsa, both on the wall's *local* facets -
the facets are transformed by the feature's `setup_align::view_pose`
first, the same law the plane fit uses. Edge axis picks store the raw
`dir = sub(b, a)` with the ends, normalised only at use.

**Why.** The issue's ask is any feature or set of features naming an
axis; reusing `align_roles`'s already-fitting types keeps one fit law,
and reusing `SetWcs` keeps one undoable placement for every voice - the
anchor grid, the numeric fields and this row all land the same command
the history already knows how to reverse.

**Details chosen.**
- One draft at a time: arming the wcs row disarms the plane-pick and
  axis-pick drafts, and each of those arms disarms this one, because
  all three want the same viewport clicks. The wcs draft also pauses
  (takes nothing) while its setup row is not shown, mirroring #664's
  disarm law.
- Re-picking a role replaces that role's pick in place - the row is a
  set of slots, not a log.
- The Apply gate runs `align_roles` live each frame and shows *its*
  refusal words, inline and on the disabled button, so the UI's why is
  the command's own words, never a paraphrase.
- The picked list wraps (`horizontal_wrapped`): a long list pushed
  Apply past the pane's edge - a real layout defect, not just a test
  one.

**Considered and dropped.**
- A separate pick mode per feature kind for this row: the row's whole
  point is that faces, edges, corners and walls all feed one draft; a
  mode per kind would make the user switch modes mid-set.
- Storing the picks in the open operation's `geo` list: these are
  draft state for a placement, not operation geometry, and mixing them
  would leak into operation edits.
- A new `CommandKind` for the fitted frame: `SetWcs` already carries
  pos and yaw-pitch-roll, posts nothing special, and its undo/redo path
  is proven.

**Cost to change later.** Small: the frame law lives in one function
with a golden test; a future "blend the third axis" variant would be a
new role-order option beside it, not a rewrite.
