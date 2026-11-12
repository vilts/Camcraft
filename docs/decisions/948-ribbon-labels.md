## #948 - Ribbon labels whole at 1280 px, captions back

**Decided (unattended).**
- Cell labels shrink to fit (13 px down to 10 px) before they would elide;
  only a label that still does not fit at 10 px truncates.
- The four section doors wear deliberate short words: "Mill 2D...", "Mill 3D...",
  "Drill...", "Multi..." (was "Milling 2D..." etc., which all elided to "Milli...").
  The full section name stays in the hover tip and the dropdown.
- Group captions are painted again (10 px, faint) under the cells, but only when
  the caption fits its group's span: File, Setup, Modify, Inspect and View show.
  The single-door groups (Milling 2D, Milling 3D, Drilling, Multi-axis) skip it:
  each is one ~43 px cell that already names its section, and four adjacent wide
  captions would run under one another.
- The ground grid returns no lines for a non-finite or beyond 1e9 mm camera
  target/step (dist 1e308 made `reach` infinite and the loop walk all of i64).

**Why.** At 1280 px the 25 cells are ~43 px wide (the build badge keeps 75 px);
no honest layout fits full words at 13 px. Shrinking and short door words keep
the cell grid, the badge and the 66 px ribbon height unchanged.

**Considered and dropped.** Dropping the build badge (cells only 46 px, still
too narrow); a wider/taller ribbon or two-line labels (changes the shell layout
and every golden); removing captions for good (the 0.17.0 look had them and the
issue asks for them back); sub-10 px text (unreadable).

**Cost to change.** Labels: `door_label` and `painted::fit_label`'s floor;
captions: the loop in `panes::ribbon`. The PNG goldens are regenerated at the cut.
