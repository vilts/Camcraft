## #891 - Turning cutting data: surface speed, feed per rev, G96 under a clamp

**Decided.** A lathe tool can state turning data (`Tool::turning`:
surface speed Vc in m/min, feed per rev in mm/rev, and an optional
max-rpm clamp of its own). The turning table
(`camcraft_tools::turning_data::TURN_TABLE`) gives a Vc and a feed-per-rev
range for each work material, for HSS and for carbide. It sits beside
the milling table, which is unchanged. A turning op's "Suggest feeds &
speeds" (`cutting::suggest`, which hands off to `cutting_turn::suggest`)
takes the middle of each range, with two adjustments. A grooving blade
or form gauge takes 0.4 x the feed. A boring bar takes 0.8 x both the
speed and the feed. The suggestion reads the rpm at the stock's swing
diameter about the spindle axis, the program Z through the WCS origin
(the lathe posts' meridian law). That rpm is held to the tool's max rpm
and then to the machine's spindle maximum. It is refused, not raised,
when it falls under the machine's minimum. The feed is f x rpm and the
plunge feed is half of that (both mm/min, not rounded), so the per-rev
F the post divides out is exactly the table's. `ApplyCuttingData` writes
the op's feed and plunge feed and the tool's rpm and turning data in one
undo unit. It keeps the tool's own clamp as stated. At post, a turning
op (any turning kind except Threading) whose tool states turning data
puts `PARAM SurfaceSpeed <Vc>` on the stream, plus
`PARAM MaxSpindleSpeed <rpm>` when the tool states a clamp. The Fanuc
lathe post writes `G50 S<min(machine, tool)>` before the G96 when that
clamp differs from the one in force. Before a later constant-rpm
section it lifts the clamp back to the machine's. The Tormach post uses
that minimum as `G96 D`. When neither the machine nor the tool states a
maximum, both posts refuse the G96 by name. A tool with no turning data
posts exactly as before, with G97 S<rpm>. The project file gains an
additive `turning` key, written only when it is stated. Tool sanity
refuses a zero or out-of-range Vc or feed, and a constant rpm over the
tool's own clamp.

**Why.** On a lathe, speed belongs to the work's diameter, not the
tool's, so the milling table's fz-at-10-mm column cannot speak for an
insert. A G96 with no clamp runs away as the tool nears the centre, and
on a facing or parting op it nears the centre every pass. That is a
hazard, so the post refuses it instead of guessing a cap. The swing
diameter is the largest the work turns. Reading the rpm there means
every smaller diameter runs slower than Vc under the G97 fallback, which
is the safe direction. Taking the plunge at half the feed per rev keeps
the plunge's per-rev feed sane when the rpm changes. Without it the old
mm/min plunge, divided by a lower rpm, would become a heavier per-rev
plunge. Threading stays at a constant rpm because a synchronized pass
must.

**Considered and dropped.** (1) Storing only Vc and f/rev on the tool
and deriving the mm/min feed at post time. That would have changed what
every turning op means by `feed`, and with it every turning golden. The
mm/min op feed plus the tool's rpm already gives the post its per-rev F
(`F = feed / rpm`), so the suggestion writes numbers that make that
quotient the table's, and nothing else moves. (2) A `surfaceSpeed = true`
capability flag on posts with an engine-level refusal. A post that does
not read the PARAM still writes a correct G97 program at the suggested
rpm with the same per-rev feed, so there is nothing to refuse. (3)
Reading the rpm at the toolpath's largest cutting radius. That needs a
generated op, and a suggestion should not need one. (4) Seeding the
starter catalog's inserts with turning data. Every lathe golden would
have turned into G96, so the catalog stays unstated and the data is
stated by the suggestion or the tool editor.

**Cost to change later.** The table's numbers are data in one module.
The blade and bar factors are two constants. Moving to post-side
per-rev feeds would mean a project-file step and regenerating the
turning goldens.

**Review fix, 2026-11-12: the tool's G50 carries into a later G97
when the machine states no maximum.** The Fanuc post lifts a tool's
lower G50 clamp back to the machine's before a constant-rpm section.
When the machine states no spindle maximum there is nothing to lift
back to, so the tool's `G50 S<tool>` stays in force into the G97
section. This is kept on purpose: on a Fanuc lathe `G50 S` caps the
spindle only under G96, so the G97 section's rpm is posted as stated.
The post says so in a comment, and
`with_no_machine_max_the_tools_g50_carries_into_a_later_g97` pins it.
Dropped: posting a `G50` with an invented high value to "clear" the
clamp, since the post never invents a spindle limit; and refusing the
G97 section, since it is correct as written.
