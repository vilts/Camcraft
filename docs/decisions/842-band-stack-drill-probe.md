## #842 - drill reach, the cross drill and the probe stylus read a body's band stack

**Decided.** The three laws #835 left on the enclosing cylinder now read
the same band stack every other stack law reads and the simulation draws
(`Cutter::with_body`, built once at the top of `run_job_arms` from
`tool_body::stack` with #957's neck, and handed to each law):

- **Axial holder face** (`drill_reach::axial_reach`): each band's face
  disc - `bottom + band.above`, at that band's own radius - against the
  material top over that disc (`top_over`, the prior grid read at that
  radius). Bottom-up, first offender named: "tool T's body band (dia. D,
  A mm above the tip) sinks to Z ...".
- **Cross-drill holder** (`drill_reach::cross_holder`): the same per-move
  law (the on-station tube and the swing's swept rectangle) run once per
  band at its face, radius and length. On a station each band's tube is
  unbounded outward, as the holder's always was, so a narrow band under
  a wider one is read at least as wide as the stack - never narrower.
- **Cross-drill part law** (`cross_part::part_clear`): takes the
  body-carrying cutter, re-judged against the tool's geometry
  (`Cutter::judged`, so a stale stack falls to the square stack).
- **Probe stylus** (`probe_op::stylus_cutter(tool, stack)`): ball, stem,
  then the probe body (the record's shank band) and the body's own bands
  above it, checked as a `BodyBands` of the stylus stack.

The enclosure wording ("judged here by the cylinder enclosing it")
stays only where the cylinder is still what is read: a body whose stack
does not fit the record (`body_judged::falls_back`) for the generator
refusals, and a body that does not fit the stylus for the probe note.

**Why.** #743/#835: what is drawn is what is checked. A stepped body
(narrow nose, wide flange high up) was refused for a flange that stood
above the wall or past the ear. The bands are never smaller than the
body (`HolderProfile`'s law, pinned in `tool_body_judged`).

**Safety rules taken.**
- No body (or a stack equal to the holder cylinder) carries nothing: the
  laws read exactly the `holder()` cylinder they read before, with the
  same refusal text (pinned to the byte in `drill_reach::body_tests`).
- The drill laws trust a carried band only if it is finite, starts at or
  above the shank's top and is no wider than the record's enclosing
  holder (or shank); otherwise the cylinder stands in.
- A tool with no holder modelled (`holder_d` or `holder_len` 0) sweeps
  nothing, body or not, as before; a non-finite holder field is still
  refused by name.
- The probe's stack is passed only when the tool carries a body; the
  record's own holder cylinder is never rebuilt into two bands (that
  would read the probe body narrower than today's folded band).

**Considered and dropped.**
- Dropping `name_enclosure` / `probe_note` entirely: the fallback still
  judges the cylinder, and the user should be told.
- A new argument type bundling tool and bands for the drill laws: one
  tuple argument (`(tool, body)`) keeps clippy's argument limit without
  a new type.
- The multi-axis shank-avoidance dodge (`motion::multiaxis::avoid`): a
  generator choice, not a verdict, and #835 already listed it as kept;
  out of scope here.

**Moved goldens.** None: no golden or snapshot carries a tool body.

**Cost to change.** Small: `stack_bands` in `drill_reach`, the
`cutter` argument of `cross_spindle::cross_drill` / `cross_part::
part_clear`, and `stylus_body` in `probe_op`. Passing `None` restores the
cylinder everywhere.
