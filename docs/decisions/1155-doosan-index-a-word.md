## #1155 - Doosan indexing path writes its A word only for a stated A

**What was decided.** `doosan-dnm.cps`'s non-RTCP indexing path (the
clamp/unclamp 4th-axis path in `tilt()`) now writes its rotary word
through #1092's `angleWord("A", ...)` and runs #1092's `missingLetter`
first. A turning letter the machine states no rotary for is refused by
name ("indexed move commands A - the machine states no A rotary") and no
word is posted. A stated C that turns is refused too ("indexed move
commands C - this dialect's indexing path writes only an A rotary"):
this path never wrote C, so before this change a turning C was silently
dropped from the block. A machine that states A posts byte-identically
(`angleWord` is `w()`, which is `"A" + fu(v)` for a finite angle, and
`tilt()` returns early unless the angle is finite).

**Why.** Every other angle-word writer in the script went through
`angleWord`/`missingLetter` after #1092; this one formatted `A` itself,
so a non-RTCP machine with clamp codes and no A rotary posted A words
the control has no axis for. Prime Directive: refuse by name, never post
an address the machine does not have, never drop a turning axis.

**Considered and dropped.**
- Reusing the RTCP message text ("tilted move commands ..."). Dropped: an
  index is not a tilt, and the existing tilt refusal strings are matched
  by tests; a distinct "indexed move" prefix keeps both readable.
- Writing a C word on the indexing path for a stated C. Dropped: that is
  new indexing behaviour (a C table's clamp codes, its sign law) with no
  machine or golden to pin it; refusing is the safe floor until an issue
  asks for C indexing.
- Moving the refusal before the clamp-code check. Dropped: it would
  change which refusal an existing undeclared-clamp case reports (the
  `rotary.refusal` golden and the undeclared-clamp tests).

**Other direct letter writers.** None left that format a rotary letter:
the probe macro's `ax + fu(pos)` writes only X/Y/Z, and `fanuc.cps`'s
non-RTCP path refuses every tilt, so it writes no index word.
