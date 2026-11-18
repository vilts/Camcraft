# AGENTS.md — how to write a `.cps` post

Companion to the root AGENTS.md; it inherits every law there (Prime
Directive, clean-room protocol, everything-is-a-command). Provenance of
this folder: original scripts, specced from public G-code standards
(RS-274 / ISO 6983) and published control programming manuals for
BEHAVIOR only — no vendor `.cps` text, template, or post file was read,
adapted, or paraphrased. Each file starts with a provenance comment
naming its sources; a new post must not ship without one.

## What a `.cps` is

Plain JavaScript, evaluated once with the engine's PRELUDE prepended
(`crates/app/src/post/js.rs`). No module system, no host calls beyond
the prelude's functions. The consumer is `crates/app/src/post/` — the
in-app post gate (`app::posting`) and `post.exe`. The record stream it
consumes is specced in `docs/CNC-INTERMEDIATE.md` — read that first;
the property table there is the vocabulary, this file is the grammar.

Manifest: top-level assignments only — `description`, `vendor`, `legal`,
`certificationLevel`, `minimumRevision`, `extension`, `setCodePage`,
`capabilities`, `serves`, `coolantModes`, `programUnits`, `endUnits`, `liveTooling`, `afterModes`, `axisInversion`, `probeAxisInversion`, `threadHands`, `workOffsetIndex`, `workOffsetMax`. All but `serves` and the last nine are
text the engine reads none of; `serves = ["plasma"];` (#1266; one word or a list of kind words: mill, lathe, mill-turn, plasma, wire_edm, sinker_edm) names the machine kinds a script of the user's own posts to - the app refuses it by name on any other kind, and a script that states none is accepted only on a mill (non-turning dialect), a lathe (turning) or a mill-turn, never on a plasma table or an EDM; shipped posts are judged by registry name and need no `serves`; `programUnits = "mm inch"` (#902) says
the script writes the program in the document's unit -
`postOptions.Units`, `"mm"` or `"inch"`, absent = mm: the dialect's
unit word (G21 / G20, the 840D's G710 / G700) before any motion, every
length / 25.4 at four decimals and every feed in in/min, angles,
seconds and rpm as they come - and an inch document on a script whose
`programUnits` lacks `inch` (Carvera and Heidenhain say `"mm"`) is
refused by name before any hook runs; `endUnits = "G20 G21"` (or
`"G700 G710"`, or `"note"` when a dialect cannot switch units at program
end and notes the ask once instead, #2189) states how the script handles
`postOptions.EndUnits` (`"inch"` / `"mm"`) - a script without `endUnits`
is refused by name when `EndUnits` is set, and the UI offers the picker
only when two unit words are declared; `explains = "paren"` or `"semi"`
(#1386, optionally `commentMax`, the longest comment the control keeps,
delimiters included, 60 when unstated) says the script's `block()`
appends `explain(words)` after its words - the PRELUDE helper that
returns `""` unless the document's Explain G-code option is on, and
applies the comment guards itself (printable ASCII, no delimiters, the
active-keyword lead, the length cap) - and Explain on a script that
states no form is refused by name before any hook runs; a control that
reads a word differently from the mill table states it (#1399):
`explainWords = { G99: "FEED PER REVOLUTION", G50: { said: "SPINDLE
SPEED LIMIT", takes: "S" } }` (a text, `""` to leave the word
unexplained, or a word explained only beside the value word it folds
in) and `explainTool = "station-offset"` (a lathe T0101 is tool 1,
offset 1) - a malformed table is refused by name when Explain is on;
never read
`machine.units` for the program's unit, it is only the unit the
machine file was written in; `workOffsetIndex = true` (#931) says the
script reads `PARAM WorkOffset` as the 1-based offset number and writes
its own word for it, up to `workOffsetMax` (6 when unstated) - the
engine refuses a higher offset by setup name; without the flag the
script receives the word `G54`..`G59` for offsets 1-6 and the engine
refuses any offset above 6. A flagged script also defines
`workOffsetName(n)`, its control's name for offset n (display only).
Every shipped post sets it. An extended word (`G59.1`, `G505`...) is
written only where the control's public manual is cited (tormach.cps
and tormach-lathe.cps: the LinuxCNC G-code reference's G59.1-G59.3);
`threadHands = true` (#629) says the script
reads `PARAM SpindleDirection` and states a thread's hand (M04 when the
helix needs the spindle reversed) - without it the engine refuses a
reverse-spindle thread section by name (fanuc-lathe.cps and tormach-lathe.cps set it); `liveTooling = true` (issue #33) says the
script voices a `PARAM LiveTool` section - C-axis live-tool work, the
stream's X Y Z the part-frame tip and C the stated spindle angle - and
without it the engine refuses such a section by name (only
fanuc-lathe.cps sets it); `coolantModes = true` says the script reads `PARAM
Coolant` (#150) - without it the engine refuses a section asking Off,
Mist or Through tool by name, since the script would post its own
flood. Every shipped post sets it. `afterModes = true` says the script
reads `PARAM AfterOperation` (#110) - without it the engine refuses a
section asking Optional stop, Program stop or Go home by name, since
the script would run straight on into the next tool. Every post
that writes it sets it. `axisInversion = true` (#374) says the script's
output on the engine's mirrored stream (an inverted X / Y: negated
coordinates, centres and origin, CW/CCW reversed in a plane holding one
inverted axis, the comp side swapped) is verified by goldens and the
controller-interpretation test - without it a machine with an inverted
axis is refused by name. Fanuc and Haas set it. `probeAxisInversion =
true` (#392) says the script's probing blocks on that mirrored stream
(negated touch target and trip, back-off delta, found expected values)
are verified by the macro-B interpreter on a simulated inverted machine
(`crates/app/tests/inverted_probe.rs`) - without it probing on an
inverted axis is refused by name. Fanuc and Haas set it. The file-viewer's about box reads
the text ones, and every shipped post carries them.
New files: `vendor = "Camcraft"` (the M4.3 naming law — the shipped
five still carry the old label; do not copy that line's value).

## The hook contract

The engine calls, in this order, only the hooks you define (full
argument semantics in `docs/CNC-INTERMEDIATE.md`):

| hook | fires |
|---|---|
| `onOpen` | once, before all records (write the header here; the global `program` is the program's identity (#258): `program.number` - the document's number, 1 when it states none, counting up per file when posting one file per operation (#111; `programNumber` holds it too) - `program.numbered` (the document stated it: a dialect whose default header writes no number keeps writing none) and `program.comment` (the document's program comment, `""` for none - write the machine's name then, as before). Write them in the dialect's own form (O-word, program name, header comment), clean the comment to the dialect's comment-legal characters, and refuse by name what the dialect cannot hold) |
| `onParameter(name, value)` | per `PARAM`; `onSpindleSpeed` follows the `SpindleSpeed` one |
| `onToolChange(number, name)` | per `TOOL` |
| `onSection` / `onSectionEnd` | per operation span (preamble bookends) |
| `onCommand(name)` | before every motion/cycle; returning literal `false` swallows the record — legal filtering, counted, never silent |
| `onRapid` / `onLinear` / `onCircular` / `onPlunge` | the motion kinds (`onPlunge` only if defined, else `onLinear`) |
| `onCycle` → `onCyclePoint` → `onDwell(s)` → `onCycleEnd` | per canned cycle |
| `onComment(text)` | per `WARN` (the PRELUDE default echoes it verbatim) |
| `onRadiusCompensation(side)` | per `COMP` (M13.11): `"left"` / `"right"` engage G41 / G42 on the NEXT motion record (a straight lead at least a radius long, by the generator's law; `properties.CompensationRadius` is the register's radius), `"off"` cancels (G40) on the next one. The records between are the PART contour - or, when `properties.CompensationType` is `"wear"` (#38), the cutter-centre path with the register holding only the wear delta - so a post that cannot write the words must `message()` a refusal — the engine refuses a script that lacks the hook outright |
| `onPlasma` / `onTorchOn` / `onTorchOff` | per `PLASMA` header / `TORCH ON` / `TORCH OFF` (#671): a plasma section's kerf and stated heights (`PlasmaKerf`, `PierceHeight`, ... - absent = the controller's AutoFS), then the torch struck before the next motion record and put out after the previous. The engine refuses a script lacking either torch hook when a header arrives, and keeps the torch law (no torch without a header, no ON while lit, no rapid while lit, nothing left lit) - only `tormach-plasma.cps` defines them |
| `onThreadCycle` / `onThreadCycleEnd` | per `THREAD` / `THREAD END` (issue #34): a threading cycle's numbers bracket its explicit passes; a script that writes the cycle (G76) skips the passes itself, one that does not define the hooks posts them. `postOptions` (global) carries the document's post options plus resolved `declaredOptions` |
| `onClose` | once, after all records (retract + `%`-end here) |

### Declared options (`declaredOptions`) and `programTools` (#258)

A script may define a top-level `declaredOptions` map so the host renders typed per-post properties and validates CLI / MCP / script overrides by name and range. Each entry declares `type` (`"boolean"`, `"integer"`, `"number"`, `"string"`, `"enum"`), required matching `default`, optional `title`, `description`, `group`, bounds (`min`/`max`), and non-empty `choices` (`string` or `{ id, title }`) for `"enum"`. Built-in host keys (`Units`, `Threading`, `Smoothing`, `EndUnits`, `Explain`, `BetweenSetups`) are reserved. Defaults must preserve existing golden output byte-for-byte. Before `onOpen`, the engine also publishes `programTools`, an array of `{ number, name, diameter, kind }` for tools in the stream, so header tool-list comments can summarize every tool up front.

Output is only what `writeln`/`write`/`writeBlock` push into `OUT`;
`message(s)` is a COUNTED WARNING, not text — in-app it blocks the
write (the gate), so the released program never carries one.

## The laws every post states

- **Determinism.** No `Date`, no randomness, no path-dependent state
  beyond the record stream — the same input posts byte-identical text
  (goldens depend on it, `the_dialects_actually_differ` and
  `every_builtin_posts_every_golden_byte_exact` enforce the shape).
- **The machine is truth.** Read `machine.spindleMax` (refuse rpm above
  it by name), `machine.coolants` (the program's coolant word is the
  machine's own first-Flood `mCode`, or stay silent; M9 is the fixed
  stop pair; a section's `PARAM Coolant` - `off`, `mist`,
  `through-tool`, flood when unstated (#112) - picks the machine's
  entry of that family instead, refused by `message()` when the
  machine states no code for it; the ask resets to flood at section
  end; a flood ask on a machine whose list states only other kinds is
  refused the same way, and every coolant refusal names the section's
  `OperationName` (#150); names compare case-blind, spaces and
  underscores read as hyphens; a section's `PARAM AfterOperation` -
  `optional-stop`, `stop`, `home` (#110) - is written after its
  spindle stop in the dialect's own words, a home the dialect cannot
  trust is refused by `message()` naming the operation, and the ask
  resets there too; a script that writes it declares
  `afterModes = true`, and the engine refuses any stop or home ask,
  by operation name, for a script that does not), `machine.rtcp` and `machine.rotaries` (RTCP may be
  OUTPUT only when `rtcp === true`; a tilted record on a non-RTCP
  machine is a `message()` refusal — never a reinterpretation), and
  the rate ceilings for anything you pace.
- **Up is the machine's word.** A retract in machine coordinates
  (`G53 G0 Z..`) reads `machine.safeRetractZ` (#495); machine Z0 is the
  top of travel only by convention (Tormach's), and a machine homed
  with Z0 at the bottom would rapid down into the part. Unstated and
  not a known-convention machine is a `message()` refusal by name
  (`sinumerik.cps`, #503: no name is a known convention there - only
  "no machine at all" keeps the old `G53 Z0`). A dialect whose machines
  often run unhomed (`grblhal.cps`, #710: with `$22=0` machine Z0 is the
  power-on height) reads an unstated `safe_retract` as "not homed" and
  lifts in the work offset to the section's `ClearancePlane` instead,
  refused by name when that height is unknown or not above `StockZTop`.
- **A length word carries its decimal point (#1836).** A Fanuc-family
  control with parameter 3401#0 DPI=0 reads a word with no point in least
  increments (`Z14` is 0.014 mm). The Fanuc, Fanuc Lathe, Haas, Mazak and
  Doosan posts write every length, angle, lead and feed-per-rev word with
  its point (`Z14.`); only the words the control documents as "no decimal
  point" (lathe G76 P Q, G83 Q) stay in whole least increments.
- **Refuse, never lie.** A record you cannot honor in this dialect
  gets a counted `message()` naming why (Fanuc's `tilt()` returns null
  after one refusal per program — a per-line refusal is noise, a
  silent skip is the silent wrong answer).
- **World coordinates, origin-relative.** The stream speaks WORLD mm;
  programs are written from the work offset. Subtract
  `OriginX/Y/Z` (PARAMs, 0 for legacy `.cl`) from every position word;
  NEVER from increments (`I/J`, cycle `Q`) — a shared translation
  cancels there.
- **Preamble completeness.** A posted program without an offset, a
  tool, or a spindle is not a posted program: close the preamble over
  everything the first raw motion line needs (`startPreamble`
  pattern), and serve legacy streams that carry no `SECTION` by
  firing it from the first motion too.
- **The first traverse states its height (#1614).** After a tool
  change (or a G30, or at the program's start) the top of Z travel
  is not proof of clearance: a long tool on a high work offset can
  stand below the part there. `tormach.cps` writes `G0 Z<ClearancePlane>`
  (else the RetractPlane) before the first X/Y move, so a setup that
  cannot clear is stopped by the control's Z limit instead of rapiding
  through the part; a stream stating neither plane is refused by name,
  and the refused move (with every later one while X/Y stay unknown) is
  not written.
- **A cycle states its feed; a cut needs a turning spindle (#1357).**
  A canned cycle whose `FeedRate` is missing or not positive is a
  `message()` refusal by name ("canned cycle G73 has no feed rate"),
  never a G7x/G8x with no F that drills at the last modal feed or
  alarms on zero. `tormach.cps` also refuses a feed, arc or cycle with
  the spindle stopped (no `S.. M3` since the last `M5`); a touch
  probe's 0 rpm section is exempt, and spindleless dialects (plasma,
  wire, sinker) never apply it.
- **Modal discipline.** G43.4 / retract modes once per program
  or per arm; tool change resets the arm and the angle memory
  (stage D). The cycle group suppresses its expanded chain — one
  cycle block per hole, bare X Y continuations when the signature is
  unchanged, `G80` before any cycle-signature change or section end.
  **The hop law (#994):** the chain a cycle replaces is only what
  stays on its hole (its XY, at or below R). Each hole returns to R
  (`G99` in the block; a dialect with no return word is read with the
  tool seated on R, so either return mode lands there), and the first
  rapid leaving the hole - the hop, the traverse, the exit - ends the
  group with `G80` and posts as the stream says. The program never
  moves between holes lower than the verified path, and a cut that
  leaves the hole inside a group is refused by name
  (`crates/post/tests/cycle_hop.rs` reads every cycle post back).
  A cycle read away from its hole with no rapid to it (two cycle
  records back to back, or a cycle straight after probing) is seated
  at the section's own `RetractPlane`: up to it, over the hole, down
  to R. A section that states no `RetractPlane` is refused by name
  (#1028). Probe moves seat the next cycle like any other motion.
  A dialect whose documented cycles this post was not specced from
  (Heidenhain DIN/ISO: G200-G209 with Q parameters, #50; TinyG, which
  has no canned cycles at all) posts the
  expanded chain as plain moves instead and refuses what plain moves
  cannot do (tapping) - never a cycle word the manual does not list.
  The arc plane (M11.2 A) is modal too: `CircularPlane` XY/XZ/YZ maps
  to G17/G18/G19, written only on change (an XY-only program writes
  nothing new), with the plane's own center words (`IJKIncrement`
  carries I J / I K / J K) - and G17 comes back before any canned
  cycle and at section end. A dialect that cannot state a plane
  without touching the tool axis (Heidenhain DIN/ISO) posts the arc
  as its chord fan instead - never a word it would misread.
- **Legacy tolerance.** It must run on a `.cl`-only stream: guard
  absent properties (`has(properties.A)` is the tilt test), default
  everything, and let the modal feed stand where `FeedRate` is null.

## The shipped five, and what actually differs

Fanuc (entry 0 — the default, goldens at `crates/app/tests/golden/nc/`),
Haas, Siemens 840D, Heidenhain (DIN/ISO mode), Mazak, Tormach PathPilot,
and the lathe posts Fanuc Lathe and Tormach PathPilot Lathe (#612:
LinuxCNC G7 / G33 / one-block G76, drilling as plain moves) — each
addressed BY NAME through `BUILTINS` in `crates/app/src/posting.rs`;
their dialect goldens ride under `nc/<dialect>/`. The knobs that
legitimately differ: header delimiters (`%` vs `;` vs bare), the `O`
word, spindle words (`S.. M3` vs Heidenhain's own `M3` block), tool-change words
(`T.. M6`, +`D..` on Sinumerik), and whether a G98 retract word
exists to write at all (DIN families retract to R by documented
default — inventing a word is the same lying as omitting one).

## The plasma post

`tormach-plasma.cps` (#671, "Tormach PathPilot Plasma") is the one
`posting::PLASMA_POSTS` entry: it serves a plasma table only, and no
milling post serves one. It writes XY only - the control owns Z (G15
probe, G16 pierce, THC) - and refuses by name every op that carries
no `PLASMA` header, any rpm, coolant, rotary, cycle or Z cutting move,
and any machine not declared `kind: plasma` (`machine.kind`). Its
goldens ride under `nc/tormach-plasma/` (`plasma_post.rs`), not the
milling matrix.

## The wire EDM post

`wire-iso.cps` (#533, "Generic ISO Wire EDM") is the one
`posting::WIRE_POSTS` entry: it serves `kind: wire_edm` machines only
(`machine.kind`) and declares `wireEdm = true` - the engine refuses a
wire section (`PARAM EdmCondition`) through any script without it. It
writes only RS-274 / ISO 6983 motion and program words; every EDM code
(condition select, compensation cancel, AWT thread / cut, wire-tip
disposal) is the machine's `edm.codes` text (`machine.edm.codes`, #290
D5), and a missing one is refused by name. The path is the wire-centre
path, so compensation is cancelled, never engaged. Its golden rides
under `nc/wire-iso/` (`wire_post.rs`); the decision record is
`docs/decisions/533-wire-iso-post.md`.

`sodick-wire.cps` (#617, "Sodick Wire EDM (Mark 21)") is the second
`WIRE_POSTS` entry, never the default. Its words are Sodick's own, from
the 1998 "Wire EDM Machine Operation" manual (Mark 21 / EX21 control):
`T91` / `T90` thread and cut the wire (`edm.awt`), the operation's
condition id is the `C` word itself (C and three digits, else refused;
the header and each section tell the operator it must exist in the
control's condition table), `G40` rides the `G00` to the threading point, a coordinate always carries its decimal point,
`M02` ends. It reads no `edm.codes`. It writes a vertical 2-axis cut
only: taper, 4-axis, a missing G54-G59 and an arc as the first cut are
refused by name; G92 and G41 / G42 are never written. The machine's
`edm.flushing` / `edm.tank` (#1499), when stated, post T84 / T85 and
T94 / T89 after each threading, before the C word (the manual's Program
Example 1 order); unstated, nothing is written and the header says so.
The tank fill / drain, flushing pump and wire-drive T codes are never
written. It was not run on a control; what is unconfirmed is in
`docs/decisions/617-sodick-wire-post.md`. Its golden rides under
`nc/sodick-wire/` (`sodick_wire_post.rs`).

## Adding a post to the app

1. Write the `.cps` here (copy `mazak.cps` as the structural skeleton
   — it is the shortest honest path), with its provenance header.
2. Register it: one `("<Name>", include_str!(...))` line in `BUILTINS`
   (`crates/app/src/posting.rs`). Entry 0 stays Fanuc forever; the
   picker lists by name automatically after that.
3. Produce its goldens (`crates/app/tests/golden/nc/<name>/`) and add
   the dialect to `conformance.rs`'s list — its
   `every_builtin_posts_every_golden_byte_exact` runs over every
   `BUILTINS` entry, so an unregistered/unpinned post fails the suite,
   not the shop.
4. Gate: `cargo test -p camcraft-app --test conformance --test
   fanuc_post --test posting`, plus a CLI smoke —
   `post.exe --machine ../machines/vmc.mch <name>.cps <file>.cnc out.nc`
   against a committed golden `.cl` (every golden is a legal input).

A USER post needs none of that: `SelectPost` → *Load post script…*
mounts any `.cps` by path; unreadable or unparsable refuses by name —
the same law `Script::load`'s clean refusal gives the CLI.
