## #997 - an absent rotary takes no angle word (fanuc, doosan-dnm)

**What was decided.** The RTCP path of `fanuc.cps` and `doosan-dnm.cps`
writes an angle word only for a letter the machine states a rotary of
(`statesRotary`, read from `machine.rotaries`). On the A+B tilthead the arm
block is now `G00 X0 Y0 Z20 A30 B0`, not `... A30 B0 C0`. A letter that
turns (non-zero) on a machine without that rotary stays refused by name
("tilted move commands C - the machine states no C rotary"): `missingLetter`
runs first, so the only letters dropped are stated zeros.

**Why.** A control with no C axis alarms on the C address, and on a machine
where C is another function it commands that function. The stream's parser
zeros unstated letters, so the zero carries no information the machine can
use. The fix sits in the word writer, not the stream, so the machine check
and simulation (which already read the machine's own rotaries) see no
change.

**Scope across posts.** Checked every post that writes tilt words: haas,
mazak, sinumerik, heidenhain and carvera refuse a tilted stream outright
(no RTCP output), tormach writes A only (4th axis, its own index path), and
fanuc-lathe / tormach-lathe refuse posed moves. Only `doosan-dnm.cps` shared
the bug - its RTCP path is a copy of fanuc's - so it is fixed here the same
way, rather than filed separately.

**Pinning.** The #926 recorded `rotary` golden of fanuc is recorded (its
KNOWN_BUGS line removed); `fanuc_post::an_absent_rotary_takes_no_angle_word`
and `doosan_dnm_post::doosan_dnm_absent_rotary_takes_no_angle_word` assert
no C word on the tilthead (and fanuc's refusal of a turning C).
`rtcp_output_arms_once_and_walks_the_angles` had pinned the bug
(`A40 B0 C0` on an A-only trunnion); its arm line is now `A40`.

**Dropped.** Writing the zero word of an absent letter only when it was
"last commanded" (no meaning - the axis does not exist); filtering the
letters in the stream parser (would hide a turning letter from
`missingLetter`, losing the refusal by name).

**Cost to change.** One helper per post; revert is the same two hunks.
