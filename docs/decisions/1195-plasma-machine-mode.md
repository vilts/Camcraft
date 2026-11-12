## #1195 - Plasma cutting machine mode; explicit table; door, menu and tip read the same table

**What was decided.**
1. The mode list stays an explicit table (`MACHINE_MODES` in
   `crates/ui/src/machine_modes.rs`), one reference per kind, with a fourth row:
   `Plasma cutting` mounting the built-in `Tormach 1300PL`. It is not derived
   from `posting::MACHINES`: `machine_mode_tests::every_kind_with_a_shipped_machine_has_a_mode`
   fails when a kind with a shipped machine has no row, and
   `every_mode_mounts_a_shipped_machine_of_its_own_kind` fails when a row names a
   missing machine or one of another kind. So no EDM row appears (no EDM machine
   ships), and a mode that mounts nothing cannot land silently.
2. The door (the narrow ribbon cell) wears the plain caption only: "Milling",
   "Turning", "Mill-turning", "Plasma cutting". Pinned by
   `the_door_wears_the_plain_caption_for_every_mode`.
3. The door tip lists the modes from the table (`modes_phrase`), so it cannot go
   stale. Pinned by `the_door_tip_lists_every_mode`.
4. The menu rows add the sections a mode adds to a mill document, labelled from
   the registry (`shown_for(kind)` minus `shown_for(Milling)`, `Section::label`),
   and only those the caption does not already name (case-sensitive match).
   Result: "Milling", "Turning", "Mill-turning - Turning", "Plasma cutting".
   A mill-turn also keeps every mill section; the row names only what it adds.
5. The menu is as wide as its longest row (the "(current)" tail included),
   measured with the theme's row font, and never narrower than the usual 200 px
   (`machine_menu_width`). Text is never cut.
6. The mode table, captions, row text, tip phrase and `set_machine_mode` moved
   out of `actions.rs` (433 -> 401 lines) into `crates/ui/src/machine_modes.rs`.

**Why.** The review found that the door showed the long row text and that the
tooltip listed three modes. The door's word and the menu's words are separate
concerns, so each has its own function over one table. Building the tip from the
table removes the drift that caused the stale text.

**Considered and dropped.**
- Deriving the reference table from `MACHINES`: needs a reference-pick rule
  (several machines per kind), a product decision; deferred.
- Naming the operation ("Plasma Profile") in a row: a hand-picked list can drift;
  the section label is the registry's word.
- Listing every section a mode shows in its row: too long for a dropdown row.
- Case-insensitive caption match: would drop the suffix from "Mill-turning" (it
  contains "turning"), hiding what a mill-turn adds.

**Known wart.** A mill-turn row says "Turning", which is only what it adds beyond
a mill; the row does not say it keeps the mill sections too.

**Cost to change later.** Adding a kind means one row in `machine_modes.rs` (the
test says so when it is missing). Switching to a derived table is local to that file.
