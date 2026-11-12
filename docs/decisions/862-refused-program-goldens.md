## #862 - Posted NC goldens only for programs the gate passes

- **Decided: option (a), move the leads.** `contour_disc_hole` and
  `contour_tangential_lead` now use a 2 mm lead (quarter arc and half
  arc) where they used 3 mm. The R6 bore cut by a 6 mm tool puts the
  cutter centre on an r = 3 ring; a 3 mm lead arc lies on that ring's
  own circle, so the leads crossed the ring and the verifier's crossing
  law refused the program. With 2 mm the lead circle sits inside the
  ring, tangent at the seam, and the gate passes both programs (probed
  at 0.5, 1, 1.5, 2 and 2.5 mm: all clean; 2 mm keeps a whole millimetre
  of margin to the 3 mm limit and stays a round number).
- **Why (a) keeps each fixture's purpose.** `contour_disc_hole` is the
  witness that a tessellated round bore fits to arcs; that rides the
  ring, not the lead. `contour_tangential_lead` is the witness that a
  half-arc lead posts as one `S±1` arc with a 2 x radius standoff; the
  new golden still shows it (standoff 4 mm for r = 2). Both CL goldens
  keep 69 moves and the same arc count; only lead coordinates moved, and
  every NC golden kept its line count.
- **The refusal stays pinned on its own fixture.** The 3 mm leads on the
  same plate are now `a_lead_on_the_rings_own_circle_is_refused` in
  `solid_law_goldens`, with the exact crossing pairs the old goldens'
  verdicts carried - generated in the test, no golden kept, since no
  user can post that program.
- **A guard so it cannot recur.** `every_posted_golden_is_a_program_the_gate_passes`
  walks every `.nc` under `tests/golden/nc/` whose stem is a `.cl`
  golden: a depth-law golden must verify clean; a golden no depth law
  judges (slots, trochoid, trace, arc_planes) must be named in
  `UNJUDGED` with its reason; a golden of a refused program must say
  "refused program, posted for syntax conformance" on its first line
  (option (c)'s door, unused today).
- **Dropped:** (b) deleting the posted goldens - it would lose the two
  contour programs from every dialect's conformance matrix for no gain,
  since (a) works; (c) a refused-program header - honest, but it keeps
  output no user can get as a byte reference, which the issue names as
  the problem.
- **Cost to change later:** a lead change on these two fixtures
  regenerates their CL golden and 16 NC goldens (8 dialects x 2), as
  before.
- **Test-file split.** `solid_law_goldens.rs` stood at 405 lines; its
  part builders moved to `tests/common/solid_part.rs` so the new tests
  fit under the 400-line law.
