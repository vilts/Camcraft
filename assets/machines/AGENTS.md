# AGENTS.md — how to write a `.mch` machine definition

Companion to the root AGENTS.md and to `assets/posts/AGENTS.md` (the
post side of the same contract). Provenance: original JSON, authored
from public machine-tool spec sheets, RS-274/ISO 6983 axis-lettering
convention, and machine-tool handbooks — for CAPABILITY NUMBERS and
behavior only. No vendor definition file was read, adapted, or
paraphrased; the layout was specced from Autodesk's published
post-processor documentation (behavior only) — the grammar and every
key name are ours (`crates/app/src/machine.rs`, `post/machine.rs`).

## What a `.mch` is

Plain JSON, parsed twice on purpose. The CHECKER (`crates/app/src/machine.rs`)
reads the file form for limits and collision (travel bands, spindle
envelopes, toolchanger counts, traverse timing); the POST MIRROR
(`crates/app/src/post/machine.rs`) restates it, and the script's
`machine` global is the flattened form of that. Law throughout:
ABSENT IS NOT ZERO — a limit that is unstated, null, zero, or
non-numeric reads as "not stated", the mirror states nothing, and
posts' `> 0` guards keep their silence. The app never judges an
unstated capability into existence.

## The file-form grammar

    {
      "general":    { "model": "...", "vendor": "Camcraft", "kind": "mill" },
      "kinematics": { "default": { "units": {...}, "parts": [ ... ] } },
      "controller": { "synced_configuration": { "parts": { "X": {...} } } },
      "tooling":    { "number_of_tools": N, "spindle_mount": "BT30", "default_holder": "BT30 ER20" }
    }

- `general.kind` ∈ `mill` | `lathe` | `mill-turn` | `plasma` | `wire_edm` | `sinker_edm`. The DECLARATION
  outranks the axis inventory (`kind()` checks the word first; an
  unknown word REFUSES the file at the door — never a silent demotion
  to inference). The inference, when no kind is stated: Y + a TURNING
  rotary is mill-turn, Y alone is mill, X+Z without Y is lathe — note a
  turning center with a C axis and no Y is a LATHE, not a mill-turn. A
  rotary is turning unless its stated direction leans (z² < 0.99·|d|²
  — the same tilt test the RTCP judgment and `can_tilt` read, one law
  at three sites): an XYZ machine with A/B trunnions or a tilt head is
  a MILL whose axes can lean, not a mill-turn.
- `wire_edm` / `sinker_edm` (#530, part of #290) are DECLARED only: no
  axis inventory infers an EDM. A wire machine needs X and Y (U and V,
  the upper guide, add taper); a sinker needs Z. Neither has a spindle
  or coolant - the head is the guides or the ram. No milling or turning
  op is admitted on either (`Family::admits`), and since #530 the post
  refuses by name an op whose setup's machine cannot run it, even when
  the machine was mounted after the op was made. The EDM section:

      "edm": {
        "lower_guide_z": 0.0, "upper_guide_z": 150.0,  // wire, machine Z, length unit
        "max_taper_deg": 15.0,                         // wire, (0, 90)
        "wire_diameters": [0.25, 0.20],                // wire, length unit
        "awt": true,                                   // wire, automatic threading
        "orbits": [ { "kind": "circular", "word": "..." } ],  // sinker: circular|vector|planetary + the control's word
        "max_orbit_radius": 2.0,                       // sinker, length unit
        "pickup": "...",                               // sinker, the holder system's name
        "conditions": [ { "id": "C110", "gap": 0.02, "wire_diameter": 0.25, "role": "rough" } ],  // either kind (#531)
        "codes": { "condition": "...{id}...", ... }   // either kind: the control's EDM words (#533, #542)
      }

  `edm.codes` (#533, #542, #290 D5) holds the control's EDM words as
  templates, name -> text, filled by the post (`Machine.edm_codes`,
  read by `post::machine::edm_codes`): a name is a lowercase word
  (a-z, 0-9, `_`), a text is printable ASCII of at most 64 characters
  with no `(` `)`, or the file is refused by the name. A sinker post
  reads `metric` / `inch` (the unit word), `pickup` (`{tool}`),
  `reference` (`{offset}`, the electrode's pickup offset), `condition`
  (`{id}`), `depth` (the servo burn, `{z}` and optionally `{f}`) and
  `end` (`SINKER_CODES`); the wire post (#533) reads its own names. A
  code the post needs and the file does not state refuses the post by
  name. No vendor codes ship - fill them from the control's manual.

  Every key is optional and absent means NOT STATED (an op that needs
  it refuses by name). A garbled value refuses the file. The section is
  read strictly, every refusal BY NAME (#551 review): only a `wire_edm`
  or `sinker_edm` file states `edm` at all (on any other kind, declared
  or inferred, it refuses); a key outside the list above refuses
  (`edm.max_taper is not an edm key (...)`, never read as unstated); a
  wire file states only the wire keys and a sinker file only the sinker
  keys (`edm.orbits is a sinker_edm key; this is a wire_edm machine`);
  an `orbits` entry has only `kind` and `word`. Lengths are scaled to
  mm first and must still be finite (an inch `1e308` refuses by its
  key). Like plasma, an EDM head states no `spindle`, `live_tool` or
  coolant list (`post::machine::edm_refusals`, read by the reader and
  the Machines pane's validator alike); a rotary is allowed. No vendor
  values are shipped. No EDM machine is in the built-in library yet:
  every library machine must have a post (`machine_library`). The
  sinker's first post shipped with #542 (`sinker-iso.cps`), the wire's
  with #533 (`wire-iso.cps`); the empty sinker template joining the
  library is #1135. Until then they are test fixtures
  (`crates/app/tests/edm/`, with `sinker-iso-dialect.mch` the synthetic
  codes the sinker post's goldens are posted against).

- `edm.conditions[]` (#531): the generator conditions the user states
  from their machine's manual. None ship. CamCraft reads only the
  geometry a path needs; power and flushing stay the control's, picked
  by `id`. Each entry is an object of these keys only (any other key
  refuses by name: `edm condition "C1" gapp is not a condition key`):

  | key | unit | rule |
  |---|---|---|
  | `id` | - | required. A word with no surrounding spaces, or a whole number ≥ 0 (`330` reads as `"330"`; `1.0`, `1.5`, `-3` refuse). Unique ignoring case and leading zeros: `"C1"`/`"c1"` and `"01"`/`1` are one selector and refuse as a pair. A lookup matches the id exactly as the table writes it. |
  | `name`, `notes` | - | words, free text |
  | `role` | - | `rough`, `skim` or `finish` |
  | `gap` | length | spark gap per side, in (0, 0.5] mm. On a wire, smaller than the wire's radius when both are stated; on a sinker, no larger than the undersize when both are stated. |
  | `wire_diameter` | length | WIRE only (refused on a sinker). In [0.02, 1] mm. When the machine states `edm.wire_diameters`, it must be one of them within 0.001 mm; when it states none, any wire in range passes. |
  | `undersize` | length | SINKER only (refused on a wire): the electrode undersize per side, in (0, 1] mm |

  Lengths are in the file's length unit, scaled to mm on the way in;
  the bounds and the finite check apply to the mm value. Every
  refusal names the condition id, the key and the unit (`edm condition
  "C1" wire_diameter: 50 mm is outside [0.02, 1] mm`). Every field but
  `id` is optional (absent = not stated; the op that needs it refuses
  by name, `Edm::condition`). An absent `conditions` and `[]` read the
  same: no table, and every EDM op refuses. `Machine::is_sane` applies
  the same rules to a hand-edited project file.

- `plasma` (#669) is DECLARED only, never inferred (its X/Y/Z reads as
  a mill), and the word carries rules both doors refuse BY NAME
  (`post::machine::plasma_refusals` - the reader the first, the
  Machines pane's validator all): no `spindle` on any head (each stated
  key named: `spindle.max_speed`, `spindle.power`, ...), no
  `live_tool`, no coolant list (station or tooling root), no rotary
  axis. X, Y and Z are required; Z is the TORCH LIFTER - the control
  owns pierce height, cut height and THC - so the machine check judges
  only X/Y travel and feed, and skips Z depth and rpm with a named note
  (`MachineCheck.notes`). The torch is Tool 1 (`tooling.number_of_tools:
  1`); its KERF is the torch tool's diameter, not a machine number
  (docs/decisions/669-plasma-kind.md). A plasma table states no
  `safe_retract` (its post retracts with G30). `tormach-1300pl.mch` is
  the shipped one.
- `kinematics.default.units`: `"length": "mm"` (or `"inch"`/`"inches"`
  — the factor multiplies EVERY distance and distance-per-minute at
  load; the checker stays mm-only), `"angle": "degrees"`.
- `kinematics.default.parts` is the physical tree — `table` and `head`
  parts frame it; `linear` and `rotary` parts are the driven axes
  (`collect_axes` walks it depth-first: the nest chain is the
  kinematics, the limit table is flat). Each axis part:
  - `name` (or `id`) — the RS-274 LETTER for driven parts (see
    convention below); `X`/`Y`/`Z` map to their direction, other
    linear names are dropped by the mirror.
  - `travel`: `{ "min": a, "max": b }` — a stroke in PROGRAM
    coordinates, absolute world, not joint-local (the checker's
    traverse bands and collision sweep read these directly). Linear:
    mm (unit-scaled); rotary: DEGREES, never unit-scaled. Must be
    finite with `max > min` or the field is dropped. The file cannot
    know the work offset, so a linear slide of stroke L is written
    `-L..L` - the FULL stroke each way, never the machine-frame band
    (`-500..0` for a 500 mm column refuses every retract above the
    work origin, #457; the Tormach files follow the same rule).
  - linear only: `direction` `1` | `-1` (#373) - which way the slide
    moves for a + command; absent or null is `1`, and anything else
    refuses the file NAMING the axis. The program frame stays standard
    (ISO 841); the stated `travel` is the machine's own stroke, so an
    inverted axis is judged as `-v` against it. The post (#374) writes
    an inverted X / Y mirrored (sign, I/J/K, G2/G3, G41/G42) through a
    script that declares `axisInversion = true` (probing too only when it
    also declares `probeAxisInversion = true`, #392); an inverted Z or any
    other letter refuses posting by name. A rotary part's numeric `-1` refuses (a reversed
    rotary is not supported).
  - linear Z only: `safe_retract` (#495) - the Z, in MACHINE
    coordinates (the file's length unit, scaled like travel), a post
    rapids to with `G53 G0 Z..` before a tool change and at program
    end. Zero is a statement here, not "unstated": `0.0` says machine
    Z0 is the top of travel (every shipped Tormach file, `vmc.mch`,
    `tilthead.mch`; the mirror-form default `3axis-vmc.mch` states
    `"safeRetractZ": 0.0`). Absent or null is unstated, and the
    Tormach post refuses a non-Tormach machine that states none (the
    Siemens 840D post, #503, any machine that states none); a
    non-number, or the key on any other axis, refuses the file naming
    the axis. The mirror carries it as `safeRetractZ` (mm).
  - rotary only: `axis` `[x,y,z]` — the rotation direction; three
    finite numbers or the file refuses NAMING the axis (a garbled
    tilt vector must never read as "no tilt": the RTCP judgment rides
    on it). A stated zero vector refuses too. The vector's SIGN is the
    rotary's direction sense (#785): a 4th axis about X states
    `[1, 0, 0]` when a + A turns the tool right-handed about +X relative
    to the part (RS-274 / ISO 841 - the table turns the part the other
    way), `[-1, 0, 0]` when it turns the other way; the Tormach post
    negates its A words for the second. `tormach-770mx-microarc4.mch` is
    the shipped 4th-axis mill (an A about X, travel -360..360 degrees,
    rates unstated: TD10749 states none). `resolution` is the
    positioning step (same scaling law as travel).
  - rotary that turns the WORK only (#804): `nose_to_axis` - how far
    the spindle nose sits above the rotary's centerline with Z at
    `safe_retract` (the file's length unit, scaled like travel). A
    rotary turns the work when no `head` part hangs below it (a table
    bolted to the bed); one that carries the head tilts the tool and
    may not state it. With WCS Y0 Z0 on the centerline, the program
    gate puts the retracted tool tip at `nose_to_axis` minus the tool
    length and refuses an op on a work-turning A about X whose tip
    there is inside the stock / part / turning fixtures' swept radius
    plus 5 mm - or whose machine states no `nose_to_axis` at all. A
    work-turning rotary with no `axis` vector takes its RS-274 letter's
    line (an A is about X and is judged, #813); one whose name is no
    RS-274 rotary letter refuses every op by name - state its `axis`.
    The
    microARC 4 file states 10.6 in: the 770MX's 13.75 in spindle nose
    to table (Tormach 770MX spec page) less the microARC 4's 3.15 in
    table to centerline (4th-axis comparison sheet 02/24).
    The same rotary states `axis_to_table` (#1107): its centerline's
    height over the face it is bolted to (same unit and scaling). The
    gate refuses an op whose stock / part / turning fixtures sweep
    within 5 mm of that face, and judges every fixed fixture over the
    work's X span by its box's distance from the line; unstated, or
    steel it cannot read (no facets, a box across the line), is named
    in the gate's not-judged line. The microARC 4 states 3.15 in.
  - `type: "head"` parts carry the spindle and station, first match
    wins: `spindle { min_speed, max_speed }` rpm, `power` kW (stated,
    not judged — the sheet renders it, no model consumes it),
    `body_diameter`/`body_length` for the envelope checks,
    `rigid_tapping` (#786: `true` | `false`, anything else refuses the
    file by name; absent is "not limited" - a stated `false` makes the
    program gate refuse every tap cycle by name, whatever the post; the
    Tormach mills and routers state it); and
    `tool_station { max_tool_length, max_tool_diameter, coolants }`.
    Separately from first-match, the checker also COUNTS (M5.3 C):
    `Machine.spindles` is how many head parts anywhere in the tree
    state a spindle — the handoff gate reads it (`PART TRANSFER` to a
    second station on a one-spindle machine is a named refusal at the
    program gate). A file stating no spindle keeps `None`: the
    unstated-limit law again, never a guessed `Some(1)`.
  - `live_tool` (issue #33), on the tool station: `{ "c_axis_on",
    "c_axis_off", "spindle_on", "spindle_off" }` - the builder's words
    that engage the spindle as a positioned C axis, release it, start
    the live tool (the post appends `S<rpm>`) and stop it. No public
    standard fixes them, so a post never guesses: all four non-empty,
    or the machine states none and a C-axis live-tool section is
    refused by name. The mirror carries them as
    `head.liveTool { cAxisOn, cAxisOff, spindleOn, spindleOff }`.
  - `coolants`: entries are `{ "name": "Flood", "mCode": "M8" }` (the
    M-code is the machine's own word — string `"M8"` or bare number
    `8`, both normalized to `M8`; `mcode` spelling reads too) or a
    bare NAME string: a named option with no code rides CODELESS, and
    a post that cannot find the code stays silent rather than invent
    `M8`. The stop pair is the fixed M9, not a per-machine number.
    The names the posts read are `Flood`, `Mist`, `Through-spindle`
    and `Through-tool` (`machine::COOLANT_KINDS`, the machine editor's
    pick list; #150) - compared case-blind, spaces and underscores
    reading as hyphens, so `Through spindle` reads. A Flood op takes
    the first Flood entry, else the first coded entry of no other known
    kind (a shop's own `Brine`); a list stating only other kinds has no
    flood code and a Flood op on it is refused by name. A file that
    keeps its list at `tooling.coolants` instead of on the station also
    reads.
- `controller.synced_configuration.parts` is keyed BY LETTER:
  `{ "coordinate": i, "max_normal_speed": r, "max_rapid_speed": r }`.
  The kinematics tree decides WHICH axes exist; this table says how
  fast they go (mm/min for linear — deg/min for a rotary, and those
  deg/min NEVER mirror into a prismatic field: the unit-lie law) and
  `coordinate` is the display/sort order (0=X, 1=Y, 2=Z, rotaries
  from 3 up). A letter absent here simply has unstated rates.
- `tooling.number_of_tools`: DISTINCT tool positions the program may
  call (a double-tier of the same 40 stations is 40, not 80); a
  program asking for a tool number above it is a checker refusal.
- `tooling.spindle_mount` / `tooling.default_holder` (#828, both
  optional): the holder system the spindle takes (`BT30`, `BT40`,
  `CAT40` or `TTS` - the R8 spindle of a Tormach PCNC/M mill takes TTS
  holders) and the built-in holder-library entry a NEW tool on the
  machine starts with (`"BT30 ER20"`, the tool editor's own name). Both
  refuse BY NAME at the reader and in the Machines pane: an unknown
  mount or holder, a non-word, and a holder whose taper is not the
  stated mount (a straight-shank chuck names no mount). The default is
  applied only when a tool is added (GUI, MCP `add_tool`), in the add's
  own undo step and named in its status/reply; existing tools never
  change, and a holder that cannot take the tool's shank is left off
  with a named note. `tooling.mount_source` is free text the reader
  never reads: the public page the mount was taken from.

## What the script sees

`mirror()` restates the checker form in the static grammar:
`kinematics.static.axes[] { letter, direction, speeds{rapid,normal} }`
(linear only), `kinematics.static.rotaries[] { letter, type,
direction, travel[min,max] }` (geometry only — no speeds), and
`kinematics.static.head.spindle.max` + `head.toolStation.coolants`.
`script_props()` then flattens it into the `machine` global:
`name`, `capabilities`, `rapidRate` and `feedRateMax` (the MAXIMA over
stated LINEAR rates — absent everywhere → 0 → silence, never a fake
ceiling), `spindleMax` rpm, `coolants[]`, `rotaries[]
{name, direction, travel}`, and `rtcp`.

`rtcp` is DERIVED GEOMETRICALLY, never a hand-typed flag: true iff
some rotary states a direction that is not the Z column
(z² < 0.99·(x²+y²+z²)). A C table ([0,0,1]) is not RTCP; a trunnion
or tilted axis is. A post may output the tool-center-point promise
only when this reads true — `machine.rtcp` says so.

## The axis-lettering convention

RS-274: rotation about X is `A`, about Y is `B`, about Z is `C`; the
letters in a `rotaries` list are these, not shop names. A slide is a
slide even if the shop calls it a fourth axis; the stream's `A/B/C`
PARAM words (docs/CNC-INTERMEDIATE.md) address the same letters — a
machine with no `C` part must not make a post emit one; the tilted
record is refused instead.

## Registering a library machine

1. Write the `.mch` here with its provenance header comment
   (JSON `//`-style notes ride the loader's tolerance; the shipped
   files carry theirs in `vendor`/`model` and this style section).
2. One `("<Name>", include_str!(...))` line in `MACHINES`
   (`crates/app/src/posting.rs`; `vmc.mch` stays DEFAULT_MCH — entry
   order is not law, the names are what the picker reads).
3. The library test parses every entry BOTH WAYS (file form and
   mirror) and asserts declared kind == inferred kind, so a
   half-registered or self-contradictory definition fails the suite,
   not the shop.
4. Gate: `cargo test -p camcraft-app --test machine --test posting
   --test machine_library` (the last runs the Machines pane's validator,
   `app::machine::validate`, over every library entry - #287),
   plus a CLI smoke — `post.exe --machine <name>.mch <post>.cps
   <file>.cnc out.nc` over a committed golden stream (a 5-axis
   definition should drive the refusal path in fanuc.cps's `tilt()`
   on a tilted golden, and the G43.4 path with `--trace`).

## Style

2-space JSON, keys in the order above, numbers plain with a decimal
point (`30.0`, `0.001`). Model names are shop-uppercase words ("VMC
3-AXIS", "MULTITASKING 5-AXIS", "TURNING CENTER 2-AXIS") —
`vendor = "Camcraft"`, per the M4.3 naming law. Nest as the machine
moves: the standard chain nests Y in X in Z (or off the table for a
trunnion); `base`/`table` roots, `head` beside the axes.
