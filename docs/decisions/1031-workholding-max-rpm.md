## #1031 - A setup's workholding (chuck/collet) max rpm caps G96 and refuses G97 above it

**Decided.** A setup can state the rated maximum rpm of its chuck or
collet (`SetupDef::workholding_max_rpm`, `None` = states none). It is
set by the undoable `SetSetupWorkholdingRpm`, which refuses a value that
is not finite, is under 1 rpm or is over 100 000 rpm, and refuses a
no-op. The MCP tool `set_workholding_max_rpm` (`setup` defaults to the
active one, `max_rpm: null` clears) and the Setups page's "Workholding
max rpm" row (Apply rpm / Clear rpm) both run that command. Under
constant surface speed, the section's rpm ceiling that #891 sends as
`PARAM MaxSpindleSpeed` becomes the lower of the tool's own clamp and
the setup's. The lathe posts already take the lower of that and the
machine's maximum, so `G50 S` (Fanuc) and `G96 D` (Tormach) are
min(machine, tool, workholding). Neither post script changed. Under
constant rpm, an operation that turns the work (the spindle law's
`turns`: every turning kind, and a drill on a lathe or a cross drill on
a mill-turn) and runs faster than its setup's limit is refused at the
program gate (`verification_report`). The refusal names the operation,
its rpm, the setup and the limit. The turning suggestion is held to the
limit too, with a note. A datum transfer (`CreateSetup` with
`datum_from`) keeps the source setup's rating. The project file gains an
additive `workholding_max_rpm` key on a setup, written only when it is
stated. A value no command would accept is a corrupt file, refused by
name.

**Why.** The rating belongs to the workholding, which the setup owns.
The tool's own `max_rpm` was the only place to state it before, and the
same insert in another chuck has another limit. Folding the rating into
the existing CSS ceiling keeps one clamp path in both posts, so no
script logic changed and no golden moved. A G97 rpm over the rating is
refused, not capped: the per-rev feed and the cut were planned at that
rpm, and a silent cap would post a program other than the one the user
verified. The refusal sits at the program gate beside the other setup
clauses, so `verify`, the status line and every post route (single,
per-mount, per-op) see the same line. Copying the rating on a datum
transfer is the safe direction, since dropping a limit would quietly
lift a safety ceiling.

**Considered and dropped.** (1) A new `PARAM WorkholdingMaxRpm` with its
own handling in each post. That is more script code for the same
minimum, and a third-party post that reads only `MaxSpindleSpeed` would
miss it. (2) Refusing the command on a setup whose machine is a mill.
The machine can change after the limit is set, and the limit only acts
on operations that turn the work, so a stated limit on a mill setup is
harmless. The page row is shown only on a lathe or mill-turn setup, or
on any setup that already states one. (3) Capping a G97 rpm at post.
That is a silent wrong answer (see Why). (4) Checking live-tool milling
on a mill-turn. The live spindle's rpm is not the chuck's, so those
operations are not judged. (5) A limit on a document without setups.
The rating belongs to a setup, so the tool asks for `add_setup` first,
and the page row lives on a setup's own page.

**Cost to change later.** One optional field on `SetupDef` and one key
in the file. Moving the rating onto a fixture entity (a modelled chuck)
would need a file step that carries the setup's value onto its chuck.
