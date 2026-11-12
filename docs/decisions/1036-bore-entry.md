## #1036 - The milled Bore's entry into an undrilled hole is already judged

**What was decided**

1. **No new law; the existing one is pinned.** The issue suspected the
   `OpParams::Bore` arm plunges a cutter that is not centre-cutting into
   an undrilled hole unjudged. Reproduced on main (v0.18.0 + #911): it
   does not. The Bore arm builds its ride on the planar ladder
   (`machining2d::levels::run_levels`), and `operation::generate` lays the
   tool's `center_cutting` over the generation's links
   (`lk.approach.center_cutting = tool.center_cutting`, M11.3 B-D,
   874507a8). Every descent of that ladder asks the centre-cutting law
   (`descent::plan_core` and `plunge_law::guard` in `feed_down`, 515525c4):
   a plunge into the level's solid takes a helix, else a ramp along the
   first cut, else the operation is refused by name ("entry refused: the
   tool is not centre-cutting - ...").
2. **What the ride does.** On a solid 20 mm block, a 20 mm hole and an
   8 mm end mill marked not centre-cutting: each rung is entered by a
   zig-zag ramp along the radial sweep (the note "entry: the tool is not
   centre-cutting and may not plunge into solid (the plunge arm) - ramp
   along the first cut instead"); the ring's re-entry plunges land on
   columns the sweep already cut. `plunge_law::first_solid_plunge` finds no
   axial plunge into solid on that path, and does find one on the
   centre-cutting cutter's path. A 9 mm hole (0.5 mm centre disk, no
   helix, no ramp run) is refused by name at Z 15.
3. **Pinned by `crates/app/tests/bore_entry.rs`** (three tests), and the
   issue is closed as already done, naming 874507a8 and 515525c4.

**Why**

The issue's sketch (refuse unless the program ahead opened the hole, as
#992 does for Thread) answers a gap that is not there for Bore. A thread
mill cannot ramp or helix into solid - its ride feeds it down the axis
and the thread form cannot cut its way in - so #992 needs the prior-stock
read. An end mill without centre teeth can ramp or helix in, which the
M11.3 law already plans for every planar ladder (pockets, slots, bore);
refusing it on an undrilled hole would forbid the entry the law proves
safe and break a common workflow (boring a hole from solid with a
ramping end mill).

**Considered and dropped**

- *Mirroring #992's `thread_entry::judge` for Bore*: would refuse a
  legal ramp entry, and needs #1045's module (stacked on an unmerged
  `feat`) for no safety gain.
- *A new verify/post-gate check*: the generator already substitutes or
  refuses; the after-the-fact audit (`first_solid_plunge`) is used in
  the test to show the result, not added to the arm.

**What it would cost to change later**

- A Bore that must start in a pre-drilled hole (e.g. a ramp-angle law
  for the cutter's centre gap) would add one read in the Bore arm; the
  test file is where it would be pinned.
- Stock standing above the hole's mouth (the ladder's top of cut is the
  hole's `entry_z`) is the generic 2D top-of-cut question, not this one.
