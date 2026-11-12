## #980 - the entry orbit closes exactly on its start (the STEP bore's HSM plan back, part 2)

**What was decided.** `adaptive::rings::circle` (the entry orbit the HSM
plan walks first, and the growth/trochoid circles) now closes exactly: its
last point is its first point, and where the start lies on the circle
(within `ON_CIRCLE` = 1e-9 mm) both are the start itself. The plan's
crossing law (`adaptive::guard`) is unchanged.

**Why.** Measured, not guessed: the Z 10 level's center area and plan were
dumped from the heavy test on main. The plan is one star-morph walk; its
"crossing" at (-1.99, -0.13) is segment 0 (the orbit's first chord, out
of the start) against segment 96 (the spiral's first chord, out of the
orbit's closing vertex). The closing vertex, computed at `a0 + TAU`, stood
7.7e-16 mm off the start, and the orientation predicate read that ulp as a
proper crossing. The Z 20 fallback, in the pin since #850, is the same
seam (segments 0 and 96, endpoints 8.9e-16 apart). Which level the noise
hit depended on the outline - #839's finer mesh only reshuffled it; no
near-duplicate vertex, sliver or arc-fit defect of the outline is involved.
With the orbit closed exactly the two chords share an exact vertex, the
predicate reads 0 and no crossing; a real crossing still reads as one. The
snap moves a point by at most 1e-9 mm, six orders under the 0.005 mm
verify tolerance, so no wall moves.

Result on the heavy pin
`pocket3d_hsm::the_users_bore_roughs_adaptive_level_by_level_and_verifies_clean`:
no level falls back to Offset rings any more (was {Z 20, Z 10}; before
#839 {Z 20}); the path verifies clean; counts re-measured in the test.
The fast pin is `crates/motion/tests/adaptive_orbit_seam.rs`, the Z 10
center outline (287 vertices, dumped) grown back into a 2-D pocket - it
read "crosses itself at (-2.09, -0.13)" before the fix - plus a unit pin
in `rings.rs` sweeping 400 orbit seeds.

The reviewer's open point from PR #1003 (does a rings level clear its stock
as fully as the HSM plan?) is moot for this part: no level of the bore
falls back now.

**The heavy pin per platform (review round on PR #1032).** The planner's
output differs slightly between platforms: on macOS Apple silicon the same
head plans the bore as 16160 / 15432 / 1571 against Windows x86_64's
16197 / 15458 / 1581 (the old pin had the same split, issue #1010, which
owns making the planner bit-stable). The `(moves, planar, arcs)` triple is
therefore exact per measured platform (`cfg`: Windows x86_64, macOS
aarch64), so a real change still trips it on the release platform; any
unmeasured target gets a band covering both (moves 16100..=16260, planar
15380..=15520, arcs 1560..=1600) and a comment asking for its exact
triple. Every platform-independent assertion stays exact: no warnings, no
level falls back, no budget note. Dropped: one band for all platforms
(fork PR #1014's form) - a +-55 band on Windows hides small changes.

**Considered and dropped.**
- Cleaning the level outline before planning (merge near-duplicate
  vertices): the outline was not the cause; it would have changed every
  level's walls for nothing.
- Making the guard treat segments whose endpoints meet within an epsilon
  as touching: it would fix this seam too, but loosens the law everywhere;
  the brief and the safety floor say the check stays as it is. The fix
  sits at the source of the noise instead.

**Cost to change later.** None in behaviour; any circle-closing change
re-measures the heavy pin.
