# Decisions for #727

## #727 - viewport origin gizmos wear their names

**What was decided:** every origin gizmo in the viewport labels itself. Each
work-offset triad draws its entity name - exactly what the Setup pane
shows, `G54` for the first setup, `Setup2/WCS` for the second - just under
its origin, in a bright axis color at normal label size. The dimmed world
axes draw `World` at theirs, neutral gray dimmed by `WORLD_DIM` like the
axes it stands for. Labels ride `gizmos()`: always visible, and an offset
hidden from the pane loses its label with its lines. Only the `X`/`Y`/`Z`
letters keep the big `is_axis_label` font; names draw at the ordinary
overlay-label size.

**Why:** with several setups on screen, the triads are otherwise
indistinguishable - the issue's ask was to tell whose origin is whose
without hovering, and a hover label cannot be read at a glance or
screenshotted into a setup sheet. The name at the origin is the same
identity the rest of the UI already uses, so the viewport needs no new
vocabulary.

**Considered and dropped:** hover-only labels (the issue asked for always
visible; hover answers "what is this" only for one gizmo at a time);
offset letters (`G54 triad X` per tip - six-plus glyphs per gizmo, clutter
that buries the axes the triad is for); coloring each offset's whole
triad by setup (the axes' RGB channels carry axis meaning already,
mirrored by the Setup pane's colorless rows).

**Cost to change later:** low. The label text is produced in `gizmos()`
and placed by `overlay_labels`; swapping always-visible for hover would
mean gating on `state.hover` in the shell's draw loop and leaving the
producer untouched. `setup_chip_tests` filters plane chips by `" · "`,
which never collides with an entity name - re-scope that only if names
start carrying separators.

## #645 - the names get a persisted switch

**What was decided:** the review on the gizmo-labels PR asked for the
names to be switchable, so `interact::overlay_labels` emits the name
rows only while `State.show_gizmo_names` is on - a browser-pane
checkbox, "Show origin names", right under "Show world origin axes".
The `X`/`Y`/`Z` letters and every gizmo line and stroke ignore the
switch: it quiets names and nothing else. The flag is view state like
the camera (never a command, never on the undo history), but unlike
the camera it is kept: a per-user `view.json` in the same config
folder as the recents file (#662's folder resolver), written the
moment the checkbox flips and read once at startup. Default ON -
shipping the switch must not change a viewport a user already has.

**Why:** names are what make several triads tell apart, but on a busy
model they are clutter some users will want gone; the review asked for
the toggle in view settings, persisted, default on. Persistence rides
the recents' machinery (same folder, atomic write, corrupt-file
tolerance) rather than inventing a second way to keep a preference.

**Considered and dropped:** putting the flag in the project file (it
is how *this user* likes the viewport, not part of the document - the
camera is not saved either); a ribbon View menu entry (the browser
pane is where the other viewport furniture already lives). A separate
`view.json` sibling vs. widening `recent.json`: one file per concern
keeps each tolerant read simple; if the maintainer prefers one config
file, folding the `view` section into `recent.json` is a mechanical
move of the same serde structs.

**Cost to change later:** low. A new view preference is one
`#[serde(default = ...)]` field on `app::view::ViewPrefs`, one State
field, one checkbox; the old files keep reading and unknown keys stay
ignored.
