## #906 - a C travel with no C rate refuses the rewind by name

**Decided.** A machine whose rotary C states a travel but no rate
(neither its feed nor its rapid, deg/min) gets no C rewind. Two layers
refuse it by name:

- At generation, `rotary_gate::stamp_spindle_c` runs the unwind as
  before. If the unwind changed the path (it inserted an orbit or
  shifted a stretch's C) and the machine's C states no rate, the
  operation is refused: "the path needs a C rewind, but the machine
  file states a C travel but no C rate ...". A path that fits the
  travel as stamped needs no rewind and is not refused.
- In the lathe post (`fanuc-lathe.cps`, `liveMove`), when the machine
  states a C travel and no C rate, a C-section rapid that turns C is
  refused: "a C-axis rapid that turns C, but the machine file states a
  C travel but no C rate ...". This is the backstop for a stream that
  reaches the post without the generation gate (a hand-built stream, a
  script), and it also covers C-turning link rapids that are not
  orbits. Those rapids post at the machine's maximum linear feed scaled
  by the C turn, which is the same unbounded F.

**Why.** The Prime Directive asks for the correct output or a visible
refusal. An uncapped rewind F (about 57 x max feed / radius, F22954 in
the #860 review) is a number the machine file never stated. The control
clamping it is the control's behaviour, not ours to rely on. The post
already refuses builder data it would otherwise have to guess (the
live-tool words), and a C rate is the same kind of data. The fix for
the user is one line in the machine file (the C axis's `speeds`), and
the refusal says so.

**Considered and dropped.** A conservative default C rate named in a
program comment (the issue's second option). Any default is a guess at
a builder's number: too high and it is the same lie, too low and the
cycle time is wrong without notice. A default would also have to be
justified for every machine, and the shipped `millturn.mch` already
states its rates. Refusing only at generation was also dropped: the
post would then still voice an uncapped F for any stream that bypasses
the gate.

**Not changed.** A machine that states no C travel gets no rewinds, so
the unwind question does not arise. Its C-turning rapids are still not
capped when it states no C rate (the existing `c_axis_live` bench).
That case predates #860 and is outside this issue.

**Cost to change later.** Swapping the refusal for a stated default is
two small edits (the gate's `c_rated` check and the post's refusal
branch) plus a decision on the number. The pinning tests name the
refusal text.
