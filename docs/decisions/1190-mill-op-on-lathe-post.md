## #1190 - A milling operation is never posted on a lathe's work spindle

**What was decided.** The program gate refuses, by name and before a line
is written, every operation a lathe post would open on the work spindle
without that being how a lathe cuts it. One function decides it -
`app::lathe_voice::voice(params, path, origin)` - and answers one of three:

| The operation | A lathe post writes | Answer |
|---|---|---|
| its path states a C angle (`PARAM LiveTool`): the rotary kinds on the spindle's C | the machine's live-spindle word, work held on C | `LiveTool` - posts |
| a turning-family kind (12 kinds) | `G97 S M03` / `G96` | `WorkSpindle` - posts |
| Drill with every cutting move and cycle on the spindle axis, tool along +Z (drill, centre drill, tap, reamer on centre) | `G97 S M03`, `G83` / `G84` / `G85` or plain moves | `WorkSpindle` - posts |
| Drill with a hole off the axis, or a tilted (cross) drill | would be `G97 S M03` | refused, naming the hole |
| every other kind (28: the 2D, 3D and multi-axis milling kinds - a rotary kind whose path states no C included - the helical Bore, the thread-mill Thread, Probe, Plasma Profile) | would be `G97 S M03` | refused, naming the kind |

The door (`lathe_voice::at_post`) runs wherever a script is known: the
single-file post (`post_script`, so `post_setup` and one-mount
`post_mounts`), the per-mount post and the per-operation post - which is
every entry point (GUI Post and Post per operation, the script verb, the
MCP tool, the CLI through them). It fires only when the script declares
the turning dialect in its manifest (`capabilities` with the turning bit,
which every shipped post already states: 2 for the two lathe posts, 1 for
the rest). A mill post is untouched - there `M03` is the cutter's own
spindle.

**Why this is exact for the on-centre hole.** When the cut is on the
spindle axis and the tool lies along it, tool and work share one axis, so
it makes no difference to the cut which of the two turns: the relative
rotation is the same (and M03 on a lathe's work spindle gives a
right-hand drill the same relative sense as M03 on a mill's). That is how
a lathe drills, reams and taps on centre. Off the axis the equivalence is
gone - a drill standing still 6 mm off a turning work's axis cuts a ring
- so the cutter must itself turn. Hence: Drill on centre yes (a tap on
centre included - it is a Drill with the tap cycle); Bore no (its cutter
orbits the hole); the thread-mill Thread no (the cutter must turn);
everything else no.

**The tolerance** for "on the axis" is `spindle_law::SPINDLE_TOL`
(0.0005 mm), the same half-micron the two lathe scripts use as `EPS` and
the spindle law uses for a turned part. Rapids are not judged: a drill
may come in from the side in air (the seat law of #1026 does exactly
that).

**Why not a new stream parameter** (the first worker's route: an
operation-family or spindle-owner `PARAM` through the app, the CL spec,
the engine and both lathe scripts). The gate already holds everything the
law needs - the operation's kind, its cached path, its work offset - so
the refusal can be made before the stream exists. A stream parameter
would let the *script* refuse, which adds nothing a user sees, while
changing the stream format and moving goldens in every dialect. It
becomes worth it only when a lathe post should *voice* a milling section
(a driven tool with the work held still and no C move - a live-tool face
mill or cross drill): that needs the script to know the section's kind,
and is new behaviour, not this fix.

**One derivation.** `voice` is the only place that decides which spindle
a lathe post turns for an operation; `voice_of(world, entity)` reads it
from the document and `on_work_spindle(world, entity)` is the predicate
"is this section voiced on the work spindle?". The workholding rpm limit
(#1174, PR #1191) should call it instead of keeping a second reading:
`WorkSpindle` is the section whose stated rpm turns the work, `LiveTool`
the one whose work turns on C, a refusal one that never reaches a lathe
post. Note it would newly hold a mill-turn's on-centre Drill to the chuck
limit, which the present reading (`spindle_law::turns`) leaves out. The
"does the path state C?" test is `post::dialect::states_live_tool`, which
the stream's `PARAM LiveTool` is now written from too.

**Considered and dropped.**
- *Refusing by the machine's kind* (any milling kind on a mill-turn):
  wrong - a mill-turn posting through a mill post mills correctly, and
  existing documents do.
- *Reading the dialect from the registry name* (`LATHE_POSTS`): misses a
  user's own `.cps` file and a script handed to `post_script`. The
  manifest is the script's own statement; a unit test pins that the
  registry list and the manifests agree.
- *Trusting the script's own on-centre check for Drill*: the Fanuc lathe
  script refuses a cycle off the axis, but the PathPilot one posts the
  hole as plain moves and has no such check, and neither sees a hole on
  the meridian.
- *Letting a Bore or an end-mill plunge on centre through*: physically
  the same equivalence would hold for a pure on-axis plunge, but those
  kinds exist to cut off the axis; one admitted kind keeps the law short.

**Left open.**
- A user's own lathe script that declares no `capabilities` is not
  recognised as a turning dialect, so the door does not fire for it (it
  posts as before this change). Filed as #1199.
- The door does not make a refused operation postable. A milling pass
  with a driven tool and no C move on a mill-turn has no lathe voicing
  yet; it posts through a mill post or as a C-axis rotary operation.

**Cost to change later.** Admitting another kind is one arm in `voice`
and one row in the unit table (`lathe_voice_tests::want`, an exhaustive
match - a new operation kind does not compile until it is decided).
Moving to a stream parameter later keeps `voice` as the app-side source
of that parameter.
