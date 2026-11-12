## #904 - reduced-fidelity models: turning, the rotary kinds and the multi-axis ball kinds are judged, not only refused; the collision laws read the excess

**What was decided.** #864 flagged a STEP model imported coarser than
promised, tightened the upright mill kinds' gouge laws by the model's excess
(chord error less the 0.005 mm promise) and refused every other kind outright.
This slice takes three families off the refused list and makes the collision
laws read the excess. The rule is the one #864 set, unchanged: a tolerance
gives the excess back while a positive tolerance is left
(`verify::excess_floor`); past that the law reads untightened and the program
gate refuses the operation by name.

1. *Turning.* The band kinds (groove rough and finish, part, threading, form)
   are judged by the meridian scan. Its gouge clause now answers to the
   tolerance less the excess (`turning::gouge_tol_on`, the `_excess` twins of
   `blade_warnings`, `thread_warnings`, `form_warnings` and
   `blade_program_warnings`). The rest clause and the silhouette's chord
   budget keep the operation's tolerance. The nose kinds (facing, profile,
   chamfer, taper, multi-pass, boring) are judged by the envelope law (f),
   which #864 already tightened. The meridian rapid, bar-feed and holder scans
   take a tightened margin.
2. *Rotary.* Law (f) again, plus the radial field law: its band allowance and
   its rapid allowance each give the excess back (`rotary_law::rotary_into`).
3. *Multi-axis ball kinds* (5-axis contour and parallel, flow, multi-axis
   contour and rough). Their whole gouge verdict is law (f): a posed ball is
   the same ball about the same centre, so the envelope scan does not see the
   pose. Nothing new had to be tightened; they were refused only because the
   generator's stamp and ramp proofs do not read the excess, and those proofs
   are re-measured by (f).
4. *Collision laws* (the P1 raise from PR #911's review). In `verify`: the
   reach law (c) and its mid-turn sweep (c3) grow each shank and holder band
   by the excess and shrink the kerf that excuses the flutes' own cut; the
   rapid law (d) gives the excess back out of its footprint's slack, and its
   posed second reading out of the depth it lets a leaning stack stand in the
   part.
5. *The floor is per kind* (`fidelity_law::floor_of`): the smallest of the
   0.01 mm laws, the envelope tolerance, the meridian scan's tolerance and a
   Part's inspection ask. A groove held to 0.004 mm is refused on a metre-unit
   file (0.005 mm of excess) while a 0.05 mm groove on the same file is judged.
6. *The setup's stock counts.* The collision laws read clearance against the
   stock. The excess an operation is judged with is now the worse of its
   source and its setup's stock (`material::judged_material`), and the gate
   reads the same pair (`model_fidelity::op_coarse`).

**Why.** A metre-unit STEP file is always reduced fidelity, so before this no
lathe or 4/5-axis job could post from one. A refusal was the safe stand-in;
the laws themselves are simple to tighten because each is a comparison of a
measured depth with a tolerance. The collision laws were named by the
high-tier review: while an operation still posts they could read a clearance
up to 0.01 mm too generous on a convex wall.

**Decisions taken unattended.**

- *Swarf and advanced swarf stay refused.* The flank that does the cutting is
  proved clear only inside the generator (`multiaxis::stamp_pose`,
  `flank_clear` at the plan tolerance). No verify law re-measures it, so
  there is no verdict to tighten. Making the proof read the excess changes
  generated paths; it is filed as #1052.
- *The cone and engrave rides, probe, the cross drill and plasma stay
  refused,* as #864 left them: construction laws or their own modules.
  `rapids_radial` (the cross drill's rapid law) is not tightened either; it
  only ever judges a kind that is refused. Filed as #1053.
- *The excess is taken out along the law's own measure.* The radial field law
  measures along a ray out of the axis, the posed rapid reading across the
  lean. On a surface oblique to that measure the mesh can stand further off
  than its chord error says. The field law's stated resolution is its 0.25 mm
  cell, so this is inside what the law already declares it cannot see; #864's
  laws make the same reading. Not corrected for curvature or obliquity.
- *The stock's excess tightens the gouge laws too.* One number for every law
  is the worse of source and stock. In the common cases (a box or cylinder
  stock, or a stock copied from the part) it changes nothing. A fine part in
  a stock copied from a different, coarser model is now tightened and can be
  refused by that model's name, where #864 judged it plainly. Two numbers
  (one for gouge laws, one for collision laws) were considered and dropped:
  more plumbing through the job for a rare case, and the single number can
  only be stricter.
- *Each law tightens against its own tolerance, as #864's laws do.* A
  threading scan held to 0.05 mm still has 0.025 mm left on a 0.030 mm mesh
  and keeps convicting with it, although the 0.01 mm laws have nothing left
  and the gate refuses the operation. The posed rapid reading (gouge plus
  finish tolerance) behaves the same way. A law is never looser than at full
  fidelity; "reads untightened" applies to each law past its own tolerance.
- *Twins, not changed signatures.* `blade_warnings` and its siblings keep
  their arguments and forward to `*_excess` with 0, so the 25 existing call
  sites in the turning suites are untouched (#864 made the same choice with
  `VerifyContext::default`).
- *Three files were split to stay under the 400-line law:* the blade
  judgments out of `revolve.rs` (`blade_law.rs`), the threading verdicts out
  of `thread.rs` (`thread_law.rs`), and the excess readings out of
  `verify/types.rs` (`verify/excess.rs`). Structure only.

- *The containment law's pin uses a model flagged 0.0075 mm, not 0.012 mm.*
  Pinning it showed that #864's section law (b) convicts a matched drill
  against its own polygon rim once the excess passes about 0.0055 mm: the
  0.01 mm tolerance was sized to absorb our own regions' chord error, and the
  excess takes it away. That is in the merged code, not in this slice, and
  is a false gouge on the safe side; it is filed as #1057 and not fixed here.
  The turning, rotary and groove fixtures of this slice were run at 0.0105,
  0.012, 0.0135 and 0.0149 mm and stay clean.
- *What #904 still listed is filed, so the umbrella can close:* the facet
  budget as a setting (#1054), metre-unit STEP scaling (#1055), and the flag
  gaps and display tolerances from #911's review (#1056).

**Considered and dropped.**
- Threading the excess into the multi-axis generators (stamp, ramp, link
  proofs) for the ball kinds: not needed for soundness, because law (f)
  re-measures every ball they place, and it would change generated paths.
- Growing collision bands without a bound: past the tolerance the gate
  refuses, and an untightened reading keeps the "no misleading verdict off a
  mesh that cannot carry one" rule #864 set.
- A document-level fixture for the turning margin and the rotary allowances:
  no generator emits a marginal graze on demand. They are pinned at the law
  with the value generation hands it; threading and the probe ask are pinned
  through the document.

**Cost to change later.** Low. Each tightening is one call to
`excess_floor` at the law's own tolerance; the per-kind table is
`operation::fidelity_law` (its header says which law judges which kind).
Taking swarf off the refused list is threading the excess into `flank_clear`
and deleting its line in `excess_aware`.
