## #1089 - G-code window: selectable text, cursor highlight, click to trace

**Decided**
- **A click pins the cursor to exactly the clicked row.** The cursor is
  still the scroll's middle row (the one law the viewer is tested
  against), except that a click scrolls the row to the middle instantly
  and records a pin, (clicked row, the middle row the scroll settled on).
  While the middle row stays that one, the cursor IS the clicked row, so
  rows in the first and last half page - which cannot reach the middle -
  still get the cursor, the band and the amber segment. Any scroll that
  changes the middle row (wheel, drag, scrollbar) releases the pin and the
  middle row rules again. The pin lives in egui temp memory (ephemeral, no
  command, no new `PostView` field). Trace-pane block clicks use the same
  path.
- **The cursor row gets a background band** (theme selection fill at 45%),
  drawn in both the plain and the trace view.
- **Line numbers are a separate, non-selectable label**; the code is
  selectable. Drag-select and Ctrl+C copy the G-code without the gutter
  numbers (pasting numbered lines into a machine would be wrong). The
  copy path is egui's own label selection; no custom right-click menu.
- **Window width:** the trace view used egui's `Sides`, which gave the
  listing an unbounded width in an auto-sized window (5000+ px in a
  headless test). Replaced with two panes in fixed rects cut from the
  window's own free rect; the window keeps its size.
- **Trace default is unchanged.** `PostView::trace` is created `false`
  (`post_file.rs`); what the reporter saw as "tracing on" is the amber
  viewport segment of the listing's cursor, which is always shown while the
  viewer is open (M1.7d). Intended, not flipped.

**Considered and dropped**
- Padding the scroll area so every row can reach the middle: it would
  change the cursor at scroll 0 and the existing cursor-law tests.
- A stored picked-line field on `PostView`.
- Clicking a line with no matching move: it still moves the cursor; the
  viewport segment is simply absent for it (`line_pos` is `None`), as for
  scrolling onto such a line.

**Cost to change later:** low - all in `crates/ui/src/egui_app/gcode_view.rs`.
