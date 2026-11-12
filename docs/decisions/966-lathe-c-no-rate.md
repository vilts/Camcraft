## #966 - no C rate stated refuses a C-turning rapid, with or without a C travel

**Decided.** The lathe post (`fanuc-lathe.cps`, `liveMove`) refuses
by name every C-section rapid that turns C when the machine states no
C rate (neither the C axis's feed nor its rapid, deg/min). #906 did
this only when the machine also states a C travel; #966 drops that
condition. The text names what is missing: "... the machine file
states no C rate - its feed would be uncapped; state the C axis's
speeds" (or "a C travel but no C rate" when a travel is stated, so
#906's pinned text holds). Cut moves are unchanged: their F is the
planned feed along the part-frame chord, a number the program stated.

**Why.** Same reasoning as #906: a C-turning rapid posts at the
machine's maximum linear feed scaled by the C turn, a feed no file
states. Whether or not a C travel is stated changes only whether
rewind orbits can occur, not whether a link rapid turns C. A refusal
keeps one rule for both cases; the fix for the user is one line in the
machine file.

**Considered and dropped.**
- A no-F form (G00 with C, or a G01 with no F word): a G00 with C runs
  each axis on its own and follows no verified path (the post's own
  law), and a G01 without F inherits the previous modal F, which is a
  cut feed at an unrelated chord ratio - neither is the planned motion.
- Refusing at generation as well (#906's `c_rated` gate): with no
  travel the unwind never runs, so there is no rewind to gate; the
  generator's links are ordinary rapids and the post is where their F
  is chosen. Generating stays allowed (the path previews and
  simulates); only posting refuses.
- A no-rate exception for the `c_axis_live` bench. Its contour test
  (`a_rotary_contour_on_the_spindle_states_c_on_every_move_and_posts`)
  posted C-turning link rapids on that bench at exactly the uncapped F
  this issue is about, so the bench (a test fixture, no builder's data)
  now states a C rate - feed 6000, rapid 12000 deg/min, the shipped
  `millturn.mch`'s figures - and keeps posting, with every F checked
  inside the stated rates. The no-rate case is pinned on an `unrated`
  copy of the same bench. The `c_axis_helix.nc` golden does not move
  (its cuts run well under 6000 deg/min and its rapids turn no C). No
  shipped machine changes: the only built-in with live-tool words
  (`millturn.mch`) already states its C rates.

**Cost to change later.** Swapping the refusal for a stated default is
the one branch in `liveMove`; the pinning test names the refusal text.
