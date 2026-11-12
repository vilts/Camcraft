## #845 - adaptive growth: the overload fallback compares whole revolutions

**What was decided.** Where no growth step holds the load, `Planner::step_search`
(`crates/motion/src/machining2d/adaptive/grow.rs`) now chooses the least
overload by each tried revolution's WHOLE walk on the stock (`least_whole`),
not by the walks the tries cut short at the first reading past the bound.
The tries are walked again from the planner's stock, largest step first;
a later (smaller) step replaces the kept one only when its whole reading
is more than 2 % less (`SLACK`, the old fallback's own tie tolerance), and
each re-walk is abandoned at the first reading that can no longer win.
The kept revolution's stock and reading are those of its whole walk, so
the revolution is not walked a third time.

**Why.** A truncated walk stops before the front wherever the overload
does not grow with the step (a corner the ring rounds before it reaches
the new front): every step reads the same, the first and largest won the
tie, and it was then walked whole far past the reading it was chosen on
(island NE corner, #7 round 10: 59.5 deg chosen, 125 deg cut).

**The #13 growth budget is kept honest by not charging the re-walks.**
Law 2 (`WORK_MAX`) sums the length of every revolution *tried*, and each
trial charges its revolution's whole length (`add_len`) before its walk
is cut short. The re-walk tries no new revolution and rebuilds no
geometry (law 3's vertices): it re-reads revolutions already paid for in
full, at most once each, and usually only part way. So `WORK_MAX`,
`GEOM_MAX` and `GEOM_LEN` and the band refusal's timing are unchanged.
The planner's runtime per overloaded revolution rises by at most
`STEP_TRIES` raster walks of an already-charged length.

**Considered and dropped.**
- The held fix on `issue/7-r10-corner-reading` (e4039ef0): call the
  trial again with no limit for every tried step. It rebuilt each ring's
  offsets and charged its length and vertices a second time, which is
  part of why the island's growth work read 61,971 r against the 30,000 r
  budget; it also dropped the 2 % preference for the larger step.
- Recalibrating `WORK_MAX` and re-timing the band refusal: not needed
  once the re-walks are not charged, and a raised budget would let a band
  plan longer before it is refused.
- Keeping each try's stock instead of re-walking: the truncated stock is
  partial (`walk_until`), so it cannot be reused; holding up to seven
  full stock clones per revolution also costs memory for nothing.

**Cost to change later.** Local to `step_search` / `least_whole`; the
tolerance is one constant. Smaller steps chosen in overloaded corners
can still add revolutions (real, charged growth work) - if a fixture
crosses the budget because of it, that is the budget law doing its job,
named by its refusal.
