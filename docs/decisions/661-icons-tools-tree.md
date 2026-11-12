## #661 - Tool kind, tree row, status badge and action icons (slice 2)

**Decided - same pipeline, four new families.** 31 new SVG masters in
`assets/ui/svg/`, rasterised by `cargo xtask icons` like slice 1's: 13
`tool-*` (one per `ToolKind` - the issue's 12 plus the plasma torch, which
joined the enum with #670), 7 `tree-*` (setup, part, stock, work offset,
fixture, machine, operations folder), 4 `badge-*` (not generated, OK,
stale, warning) and 7 `act-*` (generate, post, simulate, show, hide, frame
all, debug info). All drawn original; none from Tabler, none from
SolidCraft. The existing `Icon` slots keep their names and texture keys
and now load the new files; 11 PNGs they no longer use (post, sim,
fixture, debug-info, and the Tabler box-model, cube, transform,
player-play, maximize, eye, eye-off) are deleted. Eight Tabler chrome
glyphs stay, so the Tabler licence stays in `assets/ui/LICENSES.md`.

**Tools read as tools, not operations.** Mill tools hang from a solid
holder block; lathe tools run left from a solid tool-post block at the
right (the turning operations have their chuck block at the LEFT). That
frame is what keeps Drill the tool apart from Drill the operation and the
probe tool apart from the Probe operation; a structural test also pins that
no tool kind wears an operation's glyph. `ToolKind::Taper` is drawn as the
chamfer mill it is (the UI names it so).

**Badges are solid.** At 8-10 px a 2-unit stroke is under a pixel, so the
badges are filled shapes with the mark knocked out (`fill-rule="evenodd"`):
a ring (not generated), a disc with a check (OK), a disc with clock hands
(stale), a triangle with an exclamation (warning). This extends slice 1's
"small solid dots/blocks" allowance to whole badge shapes; the lint is
unchanged (it checks stroke weights, not fills).

**The warning badge has a data source - no new state.** The row's text
already says `BLOCKED` (the tool law's `tool_problem_of`) and `n
warnings` (the cached toolpath's warnings); the badge shows the same:
refused -> warning; else `Never` / `Stale` -> their badges; `Ok` with
warnings -> warning; `Ok` -> OK. A stale path's warnings belong to the old
path, so stale outranks them. `OpStatus` is unchanged and no command is
involved (`tree_badges::status_badge`, pinned by
`status_badge_follows_the_row_status`).

**Wiring.** Tool kinds: the browser's tool library rows, the setup's Tool
list table and a glyph beside the operation page's tool picker (the
combo's items stay text: an image per egui combo item would need a custom
popup). The pages have no `IconSet` handed in, so `IconSet::load` leaves a
shared copy (texture handles, not pixels) in the egui context and
`IconSet::from_ctx` reads it; headless there is none and the text alone
shows. Tree rows: Setups, Machine, Operations now wear their own glyphs
(Machine wore the simulate glyph and Operations the generate glyph); the
ribbon's Setups door takes the setup glyph too. Badges paint over the op
row's glyph on a disc of the row's ground (`tree_badges::row_glyph`, which
also absorbed the row-glyph paint from the over-long painted.rs).

**Sheet labels.** `cargo xtask icons --sheet` now prints each stem under
its icon with a 3x5 bitmap font of our own (`xtask/src/icon_font.rs`), so a
review can name what it sees. The masters' shapes-only rule is about the
icons, not the sheet. Process file, so `PROCESS_VERSION` goes to 26.

**Considered and dropped.** Threading an `IconSet` parameter through every
page function (a wide signature change for two call sites); coloured
badges (the tint path stays the row ink, so colour-vision modes and both
themes hold without new tokens); stroke-drawn badges (unreadable at 9 px).
