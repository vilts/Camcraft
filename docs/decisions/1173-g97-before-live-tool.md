## #1173 - the lathe posts cancel G96 at the end of the section that posted it

**Decided.** Both lathe posts (`fanuc-lathe.cps`, `tormach-lathe.cps`) track
whether the open section posted `G96`; its section end writes a bare `G97`
right after the main spindle stop (`M05` / `M5`), before any after-operation
word (`M01`, `M00`, the home). So every section opens under constant rpm
unless it states `G96` itself, and no program ends - or hands the next
program on the control - a constant-surface-speed mode. A live-tool section
after a CSS one now reads `M05` `G97` ... `T0202` `G98` `M1001` `M1003 S3000`.

**Why section end, not "every section start states its mode".** The section
end is the one place every following section passes through, whatever its
kind: a C-axis live-tool section, a drill on centre, a G32/G76 thread, a
tap, a probe, a section at speed 0, a section whose stream states no
spindle at all, a legacy stream - and kinds not written yet. A start-side
rule has to be repeated in every branch of `startPreamble` (the turning
branch already wrote `G97 S.. M03`; the live branch wrote none) and any new
branch that forgets it reopens the hole. It also could not cover the
program boundary: the header (`G21 G18 G40 G80 G99`) states no spindle mode,
so a start-side `G97` in the live branch would have to be written always -
changing every live-tool program that never had a G96 - or only when the
post had seen a G96, which leaves a G96 left behind by a previous program.
The section-end rule changes only programs that post a G96.

**Price, stated.** One bare `G97` line per CSS section. Two CSS sections in a
row post `G96 .. M03` / `M05` / `G97` / `G96 .. M03` - a needless-looking
flip, harmless (the spindle is stopped when it happens) and pinned by
`two_css_sections_in_a_row_each_state_their_own_g96` in both suites. The
only existing golden that changes is `nc/fanuc-lathe/facing_css.nc` (one
`G97` after its `M05`); no recorded per-post golden posted a G96 before.

**Dropped.** Writing `G97` in the live branch only (fixes the report, not
the class); `G97` before `M05` (would switch mode with the spindle still
running at the CSS rpm - it holds that rpm, harmless, but the stop first
is the plainer order); folding `G97` into the `M05` block (one word per
concern keeps the stop line identical to every non-CSS program).

**Other posts.** `git grep G96 assets/posts` finds only the two lathe posts.
Tormach PathPilot lathe has no live branch (it does not declare
`liveTooling`, so the engine refuses a C section), but its other sections
followed a G96 section the same way; it takes the same rule. The G50 /
`D` clamp logic, the per-rev feed words and every program with no G96
section are unchanged.

**Cost to change later.** Low: one flag and one line per post; the pinning
tests name the behaviour.
