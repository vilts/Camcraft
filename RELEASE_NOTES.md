# CamCraft 0.27.0 (clean-room implementation)

Released 2026-11-26.

## Breaking
This release has breaking changes. **The project file format goes from
version 78 to 82, and a tool library file can say version 5 or 6 (was 4).**
0.27.0 opens the files of 0.26.0 and older: a file of an older version
steps up and keeps its content. 0.26.0 and older builds refuse a file that
0.27.0 saves.
- **v79: the tip form of a tapered mill** (#3368, PR #3929). A tapered
  mill states its tip form (sharp, flat or ball) and its tip diameter. A
  tool library file with a flat or ball tip says `.camtools` version 6.
  Operations refuse a flat or ball tip by name until they read it.
- **v80: the 2D Chamfer tip offset** (#3153, PR #3251). A 2D Chamfer can
  select a tip offset (`minimum_cone_radius`) and cut through that band
  of the cone.
- **v81: a stated rotation axis on the rotary operations** (#2772,
  PR #3674). Rotary Contour, Rotary Parallel and Rotary Offset read a
  stated axis first. A part whose faces do not agree with the stated axis
  is refused by name.
- **v82: the Rotary Flow operation** (#2826, PR #3999). Every project file
  that 0.27.0 writes says version 82.
- **`.camtools` version 5: a drawn cutter outline** (#3904, PR #3906). A
  tool library file that keeps a drawn cutter outline says version 5.
- **Start and exit points are operation-level picks** (#3339, PR #3871).
  An operation with a start or exit pick must be generated again one time
  after the update.

A minor release headlined by **start and exit points on nearly all
operation kinds** (Face, Slot, Drill, Bore, Thread, Sinker, Wire Contour,
turning and the 5-Axis Contour), **the Rotary Flow operation**, **a tool
library window** (3D preview with helical flutes, holders, stick-out,
Recipes tab, editor), **a form mill tool kind**, **native Agie wire EDM
jobs** (die openings, punch groups, cutoffs, Ra 0.35 quality, inspection
export, true arcs and tangent arc leads), **SVG and DXF spline import**,
**live MCP calls into a running window** (`camcraft-cli --ui`), **GPU
stock playback in Simulate**, and **many 2D Offset, 2D Contour and 3D Pocket
entry and link fixes**. 154 pull requests merged since 0.26.0 (45
features, 76 fixes, 4 performance changes, docs, tests, build and process).

## Upgrade notes
- Save a project in 0.27.0 only when each person who opens it also has
  0.27.0. Older builds refuse format 82 (see "Breaking").
- Generate each operation that has a start or exit point again one time
  (#3339).
- The "Tool library" dock tab is removed. The Tools ribbon cell opens the
  tool library in its own window (#3900). The tool Properties page is now
  a summary with an "Edit in tool library" button (#3930).
- A 2D Contour under control or wear compensation refuses a Ramp, Helix or
  Linear entry by name (#3888, #3891). Use the Perpendicular lead.
- Waterjet Profile operations are refused at posting until the jet records
  exist (#3637).

## Changes to posted G-code
These change the program that an existing project posts, or stop a post that
went through before.
- **Feed words:** the Fanuc, Doosan DNM and Mazak posts keep the decimal
  point in feed words (#3690). The fanuc, doosan-dnm, mazak, sinumerik,
  tormach and linuxcnc posts write the exact G84 tap feed and refuse a tap
  whose written F and S change the pitch (#3699).
- **Rotaries at program end:** the Fanuc and Doosan DNM posts return the
  rotary axes to zero after G49 and the Z retract when the last section is
  tilted (#3540).
- **Five-axis approach:** the rotaries turn at a standstill before the
  first descent, not during it (#3414, PR #3499).
- **Wire EDM:** DXF and model arcs post as true G2 / G3 arcs (#3215,
  #3937). Tangent arcs join with no zero-length `G1` (#3959). A tangent arc
  lead-in and lead-out (#3991). The native Agie job files accept die
  openings, punch groups of up to nine, straight cutoffs, measured QN2
  stock heights and the CCB035 quality.
- **2D Offset "Pass starts at":** one contour-parallel ring set for Auto,
  Inside and Outside (#3744); rings link in the cut plane and lift only
  with a named reason (#3747); the Arc entry plunges in air outside the
  stock box (#3746).
- **2D Contour:** every ring is entered by its ramp or helix (#3830,
  #3917); a dive past a tab pecks one stepdown at a time (#3678).
- **3D Pocket:** stock to leave is kept axially too (#3669); pass restarts
  enter by helix or ramp (#3670); feed moves shorter than 0.001 mm no
  longer post as zero-length `G01` blocks (#3852).
- **Start and exit points:** the picked points change the start, the order
  and the end of the path on the operations that read them.

## Highlights
- Start and exit points on Face, Slot, Drill, Bore, Thread (mill), Sinker
  Burn, Sinker Side Burn, Wire Contour, turning Facing and Profile, and the
  5-Axis Contour.
- Rotary Flow: a 4-axis flow-line finishing operation between picked guide
  chains.
- The tool library window: 3D preview, helical flutes, holder library,
  stick-out, Recipes tab and tool editor. A form mill tool kind with
  presets. Metric tap-drill and clearance drills in the catalog.
- Native Agie Classic Cut 2S wire jobs for more shapes, and a zero-step
  inspection export.
- SVG import, DXF SPLINE and ELLIPSE import, and Engrave and Trace on
  sketch chains.
- Arc turns with a corner radius on Face, 2D Contour and Slot.
- A Waterlines switch on 3D Finish; 2D Chamfer on open edge chains;
  "Do not touch" on 2D Pocket.
- `camcraft-cli --ui` sends MCP calls to a running window; `--skill`
  installs an agent skill; `--read-only` opens a project read-only.
- Simulate plays back linear-move operations on the GPU, with the CPU as
  fallback.
- AI Chat key setup and provider readiness; an update check in the title
  bar.
- The new `camcraft-gcode` reader crate for RS-274/NGC and ISO 6983.

## Not exercised on a real control
The pull requests record no run on a real control for these changes. The
test goldens are regression records. Check the first program on the machine
before you cut.
- The native Agie wire EDM jobs and the wire tangent arc leads.
- The Fanuc, Doosan DNM and Mazak feed decimals and the exact G84 tap feeds.
- The rotary return at program end and the five-axis approach order.

## Known issues
The cut line is 2026-11-25 19:17 UTC. These `P1` and `P2` issues are open
and are not in this cut. Their fixes ship in the next minor. Two `P1`
issues need attention before you post:
- **#4031 (P1): a left-hand tool runs M3.** A tool has no cutter hand yet.
  A left-hand drill or mill is not refused and is not turned M4. Do not
  use a left-hand tool until this is fixed.
- **#4004 (P1): dovetail cutters ignore `corner_r`.** The corner radius is
  accepted but not read, so the flank is too thin. Its fix (PR #4009) was
  removed from this cut.

All open `P1` and `P2` issues:
- #4085 (P1): DIN/ISO Heidenhain post writes no stop or M6 at a tool change
  on a manual-change machine (Picomax).
- #4055 (P2): Path kernel: the camcraft-pathkernel crate skeleton, the stage
  types and the one switch.
- #4038 (P2): Path kernel phase 1: Thread plans through the path kernel on
  side/path-kernel.
- #4037 (P2): Path kernel phase 1: Slot Undercut plans through the path
  kernel on side/path-kernel.
- #4036 (P2): Path kernel phase 1: Trochoidal slot plans through the path
  kernel on side/path-kernel.
- #4035 (P2): Path kernel phase 1: Slot plans through the path kernel on
  side/path-kernel.
- #4034 (P2): Path kernel phase 1: V-Carve plans through the path kernel on
  side/path-kernel.
- #4031 (P1): Tools: a cutter hand on the tool, so a left-hand drill or mill
  is refused or turned M4, not run M3.
- #4027 (P2): Path kernel K7: one test suite that checks each strategy
  against the kernel laws.
- #4026 (P2): Path kernel K3: one load control stage for trochoidal and
  constant engagement passes.
- #4025 (P2): Path kernel K1: one region model for selection, material side
  and bounds.
- #4023 (P2): Path kernel: one motion planning pipeline for all strategies.
- #4004 (P1): Dovetail cutters: read the corner radius (`corner_r` is
  accepted but ignored, so the flank is too thin).
- #3934 (P1): Pass starts at Outside: every strategy clears in straight
  lines, not along the stock and the selected edge.
- #3933 (P2): 2D Pocket offset rings: waterline-like passes with a nominal
  engagement, not a constant-engagement spiral.
- #3849 (P1): 2D Adaptive Trochoidal: the clean-up passes are not checked
  against the engagement limit.
- #3788 (P1): 2D Offset one ring set: full-width cuts where the rings of two
  lobes merge (fig8 waist), and the narrow note does not name them.
- #3759 (P2): Face and the 2D Adaptive Outside plan: air entries through the
  shared entry rule, Adaptive lifts with a named reason (identical output).
- #3758 (P2): Trace, Engrave and 2D Chamfer: loop changes on the shared ring
  linker, with a named reason for each lift (identical output).
- #3757 (P2): 2D Contour: level links and wall entries on the shared ring
  linker and entry rule (identical output).
- #3756 (P1): Bore: rings on the shared circle ring set, as arcs by the
  chord tolerance, linked at depth with no lift between rings.
- #3755 (P1): Circular: cut the rings of a level in one walk at depth; no
  lift and no new entry into material at each ring.
- #3754 (P2): Do not touch, the 2D Adaptive ring fallback and the 3D Pocket
  levels on the shared ring set (identical output).
- #3753 (P2): 2D Pocket rest machining and the Zigzag closing rings on the
  shared ring set: one chain law (identical output).
- #3752 (P1): 2D Pocket: Waterline, OffsetRings, Inside and Outside on the
  shared ring set; rings link in the plane and lift only with a named
  reason.
- #3750 (P1): 2D ring planning step 1: one shared ring set, ring linker and
  entry rule, taken from the 2D Offset path (identical output).
- #3746 (P1): 2D Offset Pass starts at Inside and Outside: enter in air
  outside the stock box, then an arc lead-in in the cut plane with Arc
  radius and Arc sweep.
- #3334 (P2): Start and exit points: Plasma Profile and the future waterjet
  kinds (pierce point and cancel point).
- #3331 (P2): Start and exit points: turning kinds (start in the Z-X plane).
- #3330 (P2): Start and exit points: multi-axis kinds (5-axis, swarf,
  rotary, flow, multi-axis rough and contour).
- #3329 (P2): Start and exit points: 3D kinds (3D Pocket, 3D Finish,
  Deburr).
- #3327 (P2): Start and exit points: 2D Adaptive (entry of the first pass,
  end of the last pass).
- #3325 (P2): Start and exit points: 2D ring and chain kinds (Pocket,
  Offset, Circular, Slot, Trochoidal slot, Trace, Engrave, Chamfer).
- #2629 (P2): GPU stock sim S3: viewport playback cuts on the GPU, with CPU
  fallback.
- #2424 (P2): Windows 11, desktop Ryzen 5 with onboard Radeon: the whole PC
  freezes with screen "snow" while CamCraft v0.23.0 runs.
- #1473 (P2): Start and exit points: exit points, viewport pick, and the
  cases the first contour slice refuses.
- #1448 (P2): Operations: user-set start and exit points, path regenerated
  to them.

## Process changes
`PROCESS_VERSION` went from 145 to 155 (pinned issue #681):
- 146: `gate heavy` narrows its reach for `#[cfg(test)]` modules and
  binaries (PR #3737).
- 147: MCP skill row for the lead-in `approach_distance` (PR #3765).
- 148: the recording prebuild writes cargo's JSON to a file (PR #3738).
- 149, 150, 152: MCP skill rows for the native Agie wire EDM jobs
  (PR #2964, PR #2968, PR #2969, PR #2974).
- 151: the unit sweep covers a package-level link (PR #3552).
- 153: MCP skill rows for the operation-level start and exit picks
  (PR #3871).
- 154: the gate warns when its base branch is behind its remote
  (PR #3954).
- 155: the rule **Side branch** (`side/path-kernel`), and MCP skill rows
  for `camcraft-cli --ui` and the wire lead arc (PR #4043, PR #4049,
  PR #4002).

## Pixel snapshots
The pixel-snapshot lanes were not run for this cut: no Windows machine was
available. The Windows arms of the per-OS pinned readings were not run
either. The structural UI tests assert what the goldens guard.

---

# CamCraft 0.26.0 (clean-room implementation)

Released 2026-11-23.

## Breaking
This release has breaking changes. **The project file format goes from
version 75 to 78.** 0.26.0 opens the files of 0.25.0 and older: a file of
an older version steps up and keeps its content. 0.25.0 and older builds
refuse a file that 0.26.0 saves.
- **v76: 3D Deburr width is the chamfer chord** (#1890, PR #3220). Deburr
  has contact chord width and depth sizing and multiple passes. An older
  project width is converted on load.
- **v77: setups keep their creation order** (#3447, PR #3485). A setup
  that is added later is listed and posted after the earlier setups,
  whatever its name.
- **v78: stored tool reservations** (#2604, PR #3284). Cutting tools skip
  reserved probe pockets and cannot use reserved tool names.
- **Part-off puts the whole kerf on the bar side** (#3452, PR #3472).
  Before, the kerf was centred on the parting plane and cut half a blade
  width into the part. A part-off saved before this change must be
  generated again before it posts.

A minor release headlined by **new operations** (V-Carve, Waterjet Profile,
Multi-Axis Deburr), **rest machining from the previous operations** on 2D
Pocket and 2D Adaptive, **"Pass starts at" (Auto / Inside / Outside)** on
the clearing kinds, **"Do not touch" on Face, Trace, Engrave, 2D Adaptive
and 3D Finish**, **new posts** (Mach3, RichAuto DSP A11, Siemens Sinumerik
810M, G-EDM wire, generic ISO waterjet, native Agie Classic Cut 2S job),
**saved cut recipes**, **typed text as outline shapes**, and **a headless
GPU compute crate**. 184 pull requests merged since 0.25.0 (36 features,
97 fixes, 3 performance changes, 1 refactor, docs, tests and process).

## Upgrade notes
- Save a project in 0.26.0 only when each person who opens it also has
  0.26.0. Older builds refuse format 78 (see "Breaking").
- Generate each part-off operation again before you post it (#3452).
- A STEP file that states no length unit is no longer read as millimetres.
  The desktop app asks for mm or inches. MCP and scripts take `units`
  (PR #3166).
- The native wire EDM post has the new name "Native wire EDM job files".
  The old name still selects the post (#3537, PR #3598).
- Saving a project no longer writes over changes that another program saved
  to the same file, and a failed write keeps the old file (#3608, #3509).

## Changes to posted G-code
These change the program that an existing project posts, or stop a post that
went through before.
- **New posts:** Mach3 Mill (#2482), RichAuto DSP A11 routers (`.mmg`),
  Siemens Sinumerik 810M with the EMCO VMC-200 machine (#2888), G-EDM 2-axis
  wire EDM, a generic ISO abrasive-waterjet post, and a native Agie Classic
  Cut 2S job (#2945).
- **Tap feeds:** the Haas post writes each feed with its decimals and a
  decimal point, so a G84 tap feed gives the exact pitch (#3681). Mach4
  keeps the G84 tap feed precision and refuses a pitch error above 0.01
  percent (#3701).
- **Fanuc options** (off by default): rigid tap M29 before G84 (#2603), AI
  contour control G05.1 (#2603), and a Manual Guide i blank block (#2769).
- **Rotaries after a tilted operation:** the Fanuc and Doosan DNM posts
  return the rotaries to zero before the next operation. A post script that
  does not declare `uprightReturn = true` is refused by name in that case.
  A leaning five-axis path posts A0 B0 on its opening move (#3300).
- **Arcs:** a post never writes an arc whose written end is its start, or
  whose start and end have the same angle about the centre. It writes a
  straight move (#3474).
- The grblHAL, Grbl 1.1 and TinyG posts write no tool change between
  operations that keep the same tool (#3445).
- The Tormach PathPilot mill post reads the operation's coolant.
- Toolpath changes: entry ramps on later 2D Offset passes, Circular ring
  links over clearance (#2898), Slot round ends cut to size (#2906), Rotary
  Parallel cuts the full turn of a bar, 2D Contour tab levels, and 3D
  Finish hops that stay at the pass height.

## Highlights
- V-Carve operation along the medial axis (#1443).
- Waterjet Profile operation on a shared 2D sheet kernel, with sheet cut
  order and material release checks (#2159).
- Multi-Axis Deburr with holder tilt-away.
- 2D Pocket and 2D Adaptive rest machining from the previous operations.
- "Pass starts at" on 2D Offset, 2D Pocket, 2D Adaptive, Circular, Bore,
  3D Pocket and 3D Finish.
- "Do not touch" on Face, Trace, Engrave, 2D Adaptive and 3D Finish.
- Saved cut recipes (MCP `save_cut_recipe`, `list_cut_recipes`,
  `apply_cut_recipe`).
- Typed text in a TrueType outline font as closed shapes
  (`insert_outline_text`), and the `cam.insertText` script verb.
- CL toolpath transforms: translate, XY turn, A/B turn, vertical mirror.
- The simulation steps to the next or previous operation (Page Up /
  Page Down).
- Cutting data for wood, MDF, plywood, acrylic and HDPE; a router machine
  kind; the Ooznest WorkBee 1000x1500 (Estlcam) machine.
- The browser build loads machine and post files from the user's disk.
- macOS packages can be signed and notarized.
- The new `camcraft-gpu` crate and a GPU stock removal kernel (not yet
  used by the viewport, see #2629).

## Not exercised on a real control
The pull requests record no run on a real control for these changes. The
test goldens are regression records. Check the first program on the machine
before you cut.
- The Mach3, RichAuto DSP A11, Sinumerik 810M, G-EDM and waterjet posts.
- The native Agie Classic Cut 2S job.
- The Fanuc rigid tap, AI contour control and Manual Guide i blank options.
- The Haas and Mach4 tap feed decimals (#3681, #3701).

## Known issues
The cut line is 2026-11-23 19:22 UTC. These `P1` and `P2` issues are open
and are not in this cut. Their fixes ship in the next minor.
- #3699 (P1): Mill posts fanuc, doosan-dnm, mazak, sinumerik, tormach,
  linuxcnc: the G84 tap feed is rounded to a whole mm/min.
- #3339 (P2): Start and exit points: move the picks to one operation-level
  project key.
- #3336 (P2): Performance: the stock simulation is very slow - it runs on
  the CPU, the GPU shader path is not built.
- #3334 (P2): Start and exit points: Plasma Profile and the future waterjet
  kinds (pierce point and cancel point).
- #3333 (P2): Start and exit points: Sinker Burn and Sinker Side Burn
  (start and end of the orbit and the side move).
- #3332 (P2): Start and exit points: Wire Contour (threading point into the
  shared pick, exit on open runs).
- #3331 (P2): Start and exit points: turning kinds (start in the Z-X plane).
- #3330 (P2): Start and exit points: multi-axis kinds (5-axis, swarf,
  rotary, flow, multi-axis rough and contour).
- #3329 (P2): Start and exit points: 3D kinds (3D Pocket, 3D Finish,
  Deburr).
- #3328 (P2): Start and exit points: drilling order (Drill, Bore, Thread
  mill - first hole and last hole).
- #3327 (P2): Start and exit points: 2D Adaptive (entry of the first pass,
  end of the last pass).
- #3326 (P2): Start and exit points: Face raster (first row corner, last
  row end).
- #3325 (P2): Start and exit points: 2D ring and chain kinds (Pocket,
  Offset, Circular, Slot, Trochoidal slot, Trace, Engrave, Chamfer).
- #2629 (P2): GPU stock sim S3: viewport playback cuts on the GPU, with CPU
  fallback.
- #1473 (P2): Start and exit points: exit points, viewport pick, and the
  cases the first contour slice refuses.
- #1448 (P2): Operations: user-set start and exit points, path regenerated
  to them.

## Process changes
`PROCESS_VERSION` went from 122 to 145 (pinned issue #681):
- 123, 124: MCP skill rows for router materials and the Waterjet Profile
  workflow.
- 125, 128: `release.yml` inputs `site_only` and `app_run_id`.
- 126: catalog cases can expect a refusal.
- 127: the gate names how a test executable stopped.
- 129: `cut.yml` runs on the `camcraft-cut` runner label.
- 130: `merge-ready` and `needs-rework` are never on the same PR.
- 131: `gate heavy --covers` and the wave form of heavy lanes.
- 132: the gate stall watchdog and the rule "Runner stops".
- 133: the gate knows the new crate `camcraft-gpu`.
- 134: the post-merge batch step stops at 75 minutes.
- 135: targeted tests at review; the full heavy lanes at the cut.
- 136, 138, 139, 140: MCP skill rows for Wire Contour, the native wire EDM
  post name, `insert_text`, `pass_start` and 2D Adaptive rest.
- 137: the quick gate selects the example catalog suites.
- 141, 142: directory suites in the gate; a failed build stops the quick
  plan; submodule test selection.
- 143: new tests join a directory suite.
- 144: a review finding does not make a ready PR a draft again.
- 145: the quick gate runs the whole suite for a trait impl or scoped `pub`
  item.
- PROCESS_VERSION bump for #3737 follows after the tag.

## Pixel snapshots
The pixel-snapshot lanes were not run for this cut, by the maintainer's
standing decision. The Windows arms of the per-OS pinned readings were not
run either. The structural UI tests assert what the goldens guard.

---

# CamCraft 0.25.0 (clean-room implementation)

Released 2026-11-22.

## Breaking
This release has breaking changes. **The project file format goes from
version 69 to 75.** 0.25.0 opens the files of 0.24.0 and older: a file of
an older version steps up and keeps its content. 0.24.0 and older builds
refuse a file that 0.25.0 saves.
- **Sketch selections need format 74** (#1864, PR #2716; #1868, PR #2920).
  2D Contour, Slot and Trochoidal Slot can read picked DXF sketch chains and
  layers, with a side and a direction for each chain. A project with such
  picks is saved as format 74 or later. A v73 file steps up unchanged.
- **Machine installation facts need format 75** (#3181, PR #3204). A machine
  can record its installed control, the exact NC software, the installed
  options and the tool-change procedure. Older readers refuse such a file,
  so that a save in an older build cannot lose these facts. A v74 file steps
  up with these facts not stated.
- The other format steps of this release:
  - v70: a tool's library holder is a reference to the holder library
    entry, not a drawn copy (#2016, PR #2092). Tool library files
    (`.camtools`) go to version 4.
  - v71: a DXF profile can be placed on the stock of a setup (#1862,
    PR #2134).
  - v72: a model can be linked to its source file and is imported again
    when the file changes (#2463, PR #2563).
  - v73: "Do not touch" face and edge protection on clearing operations
    (#2055, PR #2149, merged through PR #2704).
- A stock copy of an incomplete model keeps the name of its source model's
  hole after the source is deleted (#1700, PR #2024). Such a project file
  does not open in 0.21.0 to 0.24.0.

A minor release headlined by **DXF sketches as operation geometry** (2D
Contour, Slot, Trochoidal Slot, Wire Contour, typed engraving text),
**first-traverse checks for every operation kind and machine layout**,
**STEP import through the solidcore kernel only**, **new posts** (Heidenhain
iTNC 530 Klartext, Grbl 1.1, LinuxCNC 3-axis), **a Slot Undercut
operation**, **fine boring (G76)**, **dogbone and T-bone corner relief**,
**AI chat with provider clients and context budgets**, and **a Home page**.
259 pull requests merged since 0.24.0 (50 features, 162 fixes, 3
performance changes, 1 refactor, docs, tests and process).

## Upgrade notes
- Save a project in 0.25.0 only when each person who opens it also has
  0.25.0. Older builds refuse format 75 (see "Breaking").
- STEP files are now read only through the solidcore kernel. The truck STEP
  crates are removed (#917, PR #2904; PR #2939). Faulty STEP files are
  refused by name, and touching solids import as separate bodies. A model
  can import with different facets than in 0.24.0. Generate and verify old
  jobs again before you post them.
- More programs are refused before the post (see "Changes to posted
  G-code"). A first rapid that can hit the stock, a fixture or the table now
  holds the post by name. Raise the clearance height as the message says.
- Drill and thread operations take the lower of the tool feed and the
  material feed. A feed above the tool feed holds the post (#2598, #2724).
- A project that an older build saved after a tilt angle edit on a pre-v65
  operation can keep the old plane basis (#2739, PR #2751). Change Tool axis
  A or B, set it back, and check the preview before you post.
- FreeBSD is no longer a supported platform (#2570).

## Changes to posted G-code
These change the program that an existing project posts, or stop a post that
went through before.
- **New posts:** Heidenhain iTNC 530 Klartext for the Fehlmann Picomax 55
  (#3182), Grbl 1.1 for 3-axis mills (#2481), a generic LinuxCNC 3-axis
  mill post (#2303), and a zero-step Agie punch setup encoder (#2984).
- **The first traverse after a tool change is judged** for 3+2 planes, cross
  drills, rotary, multi-axis and mill kinds, on mills, lathes and mill-turns,
  through every pose of the work-turning rotaries (#2965, #2987, #2991,
  #3077, #3081, #3083, #3084, #3096, #3100, #3127). The opening rapid starts
  over the entry point of the operation, not over the WCS origin (#2901).
- **Drilling:** a fine boring cycle (G76 on Haas; other posts refuse it by
  name) (#1445). Back bore (G89) carries its dwell and feeds out at the
  plunge feed (#1480). LinuxCNC, Tormach and grblHAL refuse a G73 or G85
  dwell by name (#2609). Heidenhain G205 writes Q208 as a number (#3102).
  Posts can write pre- and post-tool-change macros (#2407).
- **Turning:** Fanuc and Tormach lathe G76 refuse a start point above
  material (#2878, #2952). Tormach inch G76 states a J that cuts the
  verified passes (#1619). Threading refuses a helical modelled thread by
  name (#3023). Turning rides drop duplicate samples and keep to the nose
  offset (#2881, #2882).
- **Flipped WCS:** each mill post posts a flipped X or Y as a mirrored
  program (#2475), also with a Z rotation (#2476). A flipped Z refusal names
  the reason (#2474).
- **Wire and sinker EDM:** the generic ISO wire post writes each dimension
  word with a decimal point (#3206), and refuses by name a wire number (U,
  V, taper angle, taper, plane) that is not finite (#3236, PR #3263).
  Sodick skims post on their own LN condition words and state their
  flushing (#2649, #2686), and Sodick comments drop unsupported characters
  (#3213). Work outside the stated
  guide heights is refused (#2923). Sinker Side Burn posts explicit lateral
  feeds (#3001).
- **Probing:** LinuxCNC and Tormach probe programs stop when a work offset
  has an XY rotation (#2683).
- **Toolpath changes that show in the posted program:**
  - 2D Contour: leads stay on the air side at the seam (#3019). A None
    lead-in stands the arc entry down (#3132). Each tab is checked against
    every cut of its level (#2561), and is as tall as Tab height (#2579).
    Whole-model contours cut their lower bands or name the uncut range
    (#2538, #2597, #2677).
  - 2D pockets: dogbone and T-bone corner relief (#2400), kept by rest
    passes (#3146). Zigzag levels close with a ring (#3129). Equal depth
    steps are an option (#2012).
  - Circular: the helix entries of several rings stay apart (#3109). Ramp
    transitions stay at depth (#1807).
  - 3D: Adaptive has a Trochoidal method (#1591). Shallow blind floors are
    cut (#2088). A straight gap link (#1708).
  - Face does not lift over holes below the cut plane (#2437). 2D Chamfer
    with no picks refuses partly bevelled edges by name (#2887, #2944).
  - Rotary Parallel keeps a held bar end (#2343) and the picked faces.
    Rotary Contour and Rotary Offset refuse a face selection (#2848).
    Rotary Contour makes rings about X (#3000).
  - A 3+2 operation from an older project with new angles posts its plane
    as a new operation does (#2599).
- **Machine checks:** the travel check reads only the points that an arc
  sweeps (#2710). The post output budget scales with the input (#3002).

## Highlights
### Operations
- Slot Undercut for T-slot, keyseat, dovetail and lollipop cutters (#1444).
- 2D Contour on DXF sketches with layer and chain picks (#1864). Slot and
  Trochoidal Slot read DXF chains (#1868). Wire Contour on one straight DXF
  chain on stock (#3180). DXF profiles on the stock of a setup (#1862).
- Typed text as single-line engraving strokes (#2679).
- A guided sheet job on a milling setup (#3070).
- 3D Deburr: picked and excluded edges, a minimum edge angle and a policy
  for edges out of reach (#1888, #1889).
- "Do not touch" protection on 2D Offset and 3D Pocket (#2055).

### Machines and tools
- A machine file can name its post (#2374) and record its installed control
  (#3181). New templates: AGIECUT Classic 2S (#2946) and the Fehlmann
  Picomax 55 with iTNC 530 (#3181).
- Tool tables import and export as Tormach PathPilot (.csv) and Hurco
  WinMax (.TCM) (#2566). Generic starter router cutters (#3069). Plasma
  torch cut-data recipes (#1521).
- Thread infeed plans over MCP (#1847).

### Import and models
- STEP through solidcore (#917), with measured tolerances and named
  refusals. Many STEP fidelity fixes (#2600, #2718, #2719, #2754, #2781,
  #2844, #2919, #2973, #2976, #3079).
- Linked models import again when their file changes (#2463). A model of
  unknown fidelity can be vouched for (#1339).
- An optional `cadmpeg` runner for native CAD files (#1873; no format is
  enabled yet). DXF stock-face import accepts empty layout records (#3176).
- A true-shape nesting kernel (#2158).

### Viewport and UI
- A Home button with a start page and the recent projects (#2384).
- Display menu: quality presets, ground shadow, ambient occlusion and "Look
  at a work plane" (#2834), a "Wireframe, see-through" mode (#2636), and
  anti-aliased overlay lines (#2958).
- The tool body and profile editor has its own window (#2397), with one
  tool preview (#2007, #2017).
- The move gizmo has rotate rings, typed values and Shift snapping (#1198,
  #2624). The Stock page sets the stock axis (#2576).
- Saved panel setups (#2339), a Report button (#1696), Delete in the model
  and tool menus (#2777), and hover help on the viewport toolbar (#2776).
- `camcraft-ui <file>` opens the named file (#2462, #2648).

### Simulation
- Cylinder stock along X or Y (#2758). Shared keyframe bricks use much less
  memory (#2194). The playhead waits for the stock removal (#2611).
- A generated machine model posed by axis values (#2584).

### AI chat and MCP
- AI chat reaches cloud providers over https and local servers on loopback.
  Keys come from the environment or the OS keychain, and an "Offline only"
  switch blocks the other hosts (#1945).
- Compact MCP read modes and chat context budgets (#1944).
  `camcraft-cli mcp reference` and `camcraft-cli mcp prompt` print the tool
  reference (#1947).

### Examples
- New catalog cases for slots, contour tabs, edge work, hole work, turning
  kinds, groove and part-off, cone thread, indexed pockets and open pockets,
  with images for each operation (#2855, #2858).

## Not exercised on a real control
The pull requests record no run on a real control for these changes. The
test goldens are regression records. Check the first program on the machine
before you cut.
- The iTNC 530 Klartext, Grbl 1.1 and LinuxCNC posts (#3182, #2481, #2303).
- The fine boring cycle (#1445) and the back bore dwell (#1480).
- The ISO wire decimal points (#3206), the Sodick skim words (#2649, #2686)
  and the Sinker Side Burn feeds (#3001).
- The flipped WCS mirror on the posts other than Fanuc and Haas (#2475,
  #2476).

## Known issues
The cut line is 2026-11-21 22:16 UTC. These `P1` and `P2` issues are open:
- #3264 (P1): Wire ISO post: a non-finite OriginX or OriginY is ignored
  without a warning, so the program is shifted. It was filed after the cut
  line. Its fix ships in the next minor.

Open defects with a workaround:
- #1950 (P3): 3D Pocket Adaptive on a whole model. The stock ring exceeds
  the planner limits, and Offset rings cut the level. The output is safe.
- #2849 (P3): on a solidcore STEP mesh, the 2D pocket corner check meets a
  loop that crosses itself.
- The release gate skips the tests that pin #1950 and #2849, and checks that
  they still fail (known-fail markers).

## Process changes
`PROCESS_VERSION` went from 100 to 122 (pinned issue #681):
- 101: the MCP skill follows the holder reference (v70).
- 102: a known-fail list in the release gate; FreeBSD removed.
- 103-105: MCP skill rows for `import_dxf`, `move_dxf_profile`,
  `break_through` and `set_model_link`.
- 106: known-fail markers in the test source.
- 107: `cargo xtask gate heavy`; the rule "Heavy lanes at review"; the
  quick gate selects the tests of a changed library fixture (PR #2775); an
  MCP skill row (PR #2704).
- 108: the batch tier builds each crate alone and runs parallel lanes.
- 109: the gate job budget follows the CPU count.
- 110: batch steps start the test executables directly.
- 111: MCP skill rows for thread infeed and patch refusals; STEP fixture
  selection in the quick gate.
- 112: batch lanes on separate runners; catalog images.
- 113: `cargo xtask gate light`.
- 114: MCP skill wording for mill-turn; the post-merge report on a red gate.
- 115: LF endings for stl/obj fixtures.
- 116: the rule "Own blocked PRs first".
- 117: the gate seeds the `target/` of a new worktree.
- 118: the gate pins `CARGO_INCREMENTAL` and the profile env.
- 119: the gate removes old test units; catalog metadata; MCP skill rows
  for sketch picks.
- 120: the MCP skill has `set_stock` and the stock axis.
- 121: one independent post-merge run for each merge.
- 122: `release.yml` runs the site tests.

## Pixel snapshots
The pixel-snapshot lanes were not run for this cut, by the maintainer's
earlier decision. The structural UI tests assert what the goldens guard.

---

# CamCraft 0.24.0 (clean-room implementation)

Released 2026-11-18.

## Breaking
Two changes are breaking. **The project file format goes from version 67 to
69.** 0.24.0 opens the files of 0.23.0 and older. 0.23.0 and older builds
refuse a file that 0.24.0 saves.
- **2D Pocket no longer has the Adaptive strategy** (#1517, PR #2253;
  format 69). Use the 2D Adaptive operation. A project from an older build
  that has a Pocket with Adaptive opens with that operation changed to 2D
  Adaptive (Spiral). If a setting could not be carried over (rest machining,
  predrilled holes), the operation is marked "needs review" and its post is
  held until you mark it reviewed (MCP `mark_operation_reviewed`).
- **A through hole breaks through by the full drill diameter by default**
  (#2441, PR #2504; format 68). The drill point length is added to the
  break-through (`Full diameter`; `Tip` keeps the old depth). Holes in a
  project from an older build keep the old `Tip` behaviour. The stock
  simulation now cuts the drill cone, and MCP `list_operation_types` shows
  `break_through`.

A minor release headlined by **WCS Flip X / Flip Y carried into Fanuc and
Haas programs**, **move and rotate handles for parts in the viewport**, **a
Mach4 milling post**, **STEP tool holders converted to DXF profiles**,
**2D Contour tabs that leave a bridge of the width you set**, and **more
dwell words refused by name instead of dropped**. 92 pull requests merged
since 0.23.0 (12 features, 60 fixes, 1 performance change, 2 refactors,
docs, tests and process).

## Upgrade notes
- Save a project in 0.24.0 only when each person who opens it also has
  0.24.0. Older builds refuse format 69 (see "Breaking").
- Projects with 2D Contour tabs get wider gaps in the tool path on the next
  generate, so that the bridge that stays on the part is the tab width
  (#2480, PR #2486). Before, a tab width equal to or less than the tool
  diameter left no tab, and the part came free.
- Tools get stable tool numbers (the lowest unused number) when they are
  added, imported, or loaded from an older project (#2442, PR #2488). Check
  the T numbers of an older project before you post it.
- A program can now be refused where 0.23.0 posted it without a dwell (see
  "Changes to posted G-code"). Set the dwell to 0, or use a cycle that has a
  dwell on that control.

## Changes to posted G-code
These change the program that an existing project posts, or stop a post that
went through before.
- **Flip X / Flip Y is posted** on the Fanuc and Haas posts (#1649,
  PR #2492): the program mirrors the flipped axis, with the arc direction and
  the compensation side kept correct. A flipped Z, a probe operation in a
  flipped setup and the other posts still refuse by name.
- **Dwells that a cycle cannot carry are refused by name, never dropped:**
  - Fanuc, Haas, Mazak and Doosan DNM: G73 and G84 dwells (#2356, PR #2387).
  - Sinumerik: G73 dwell (#2354, PR #2376), G84 and G85 dwells (#2357,
    PR #2415).
- **grblHAL and Tormach write P0** on the opening G86 block when the bore
  dwell is 0 (#2362, PR #2532).
- **Tormach PathPilot refuses a move with no A word** inside an operation
  indexed away from A0 (#2473, PR #2490). Before, the post turned A back to
  A0 in the middle of the operation.
- **Tormach 770MX + microARC 4** no longer has the unverified -360..360
  A-travel limit; continuous multi-turn A programs post (#2471, PR #2513).
- **Fanuc lathe threading** can post optional in-pass chip-break dips as
  continuous G32 segments (#419, PR #468).
- **The opening approach starts over the setup's WCS origin**, not the world
  origin (#2382, PR #2420).
- Toolpath changes that show in the posted program:
  - 2D Contour tab width is the bridge that stays (#2480, PR #2486); picked
    open contour runs join, and inside corners offset correctly (#2277,
    PR #2487).
  - Whole-model 2D Contour continues across lower stepped, chamfered and
    slotted outlines down to Bottom Z, or refuses by name (#2436, PR #2460).
  - A no-pick 2D Chamfer on an edge that is already chamfered rides the wall
    under the bevel; verify blocks a cone tip inside the part (#2435,
    PR #2450).
  - Raster 3D finishes cross a hole or slot narrower than the tool in one
    feed (#2431, PR #2478).
  - Adaptive 3D Pocket cuts the stock ring around a whole model from the air
    outside the stock where that is proved clear (#1950, PR #2035).
  - Outside containment on clearing operations clears from the setup stock
    (#2232, PR #2383). Whole-model and stock-source 2D clearing keeps part
    islands blocked when it extends outside the stock (#2438, PR #2503).
  - Multi-piece pockets keep their pass starts (#2426, PR #2427).
  - A tilted (3+2) operation measures its reference heights along its tool
    axis (#2389, PR #2418).

## Highlights
### Posts and machines
- A clean-room Mach4 milling post with 3-axis G94 and 3+2 AC trunnion
  support (#1968).
- The post engine reports script errors at your own line and column, refuses
  endless loops, deep recursion and oversized output, and has a blank post
  template and preview samples (#2069).
- The machine and post lists follow their folders; the Setup page lists your
  own machines first (#2380, #2358).
- Bake states that it posts table coordinates without a control rotation
  (#2359).

### Operations
- Part alignment: all ten 5-axis and rotary operations (#2286), Deburr
  (#2284), and Lathe, MillTurn, Plasma, Wire EDM and Sinker EDM operations
  (#2287) work in aligned setups.
- A Drill operation names each hole that is wider than its drill (#2496).
- The Heights tab shows clearance advice when the clearance plane is inside
  the stock (#2131).

### Tools
- ISO 13399 STEP tool holders convert to half-profile DXFs; DXF profiles
  import and export in the UI and MCP (#743).
- A refused tool names itself and the rule that it breaks (#1186).

### Viewport and UI
- Move handles drag a part along an axis (#1198); a Rotate mode turns it by
  a ring (#2047). Each drag is one undo step.
- The WCS page has Set origin in its footer, flags unapplied edits, and says
  where the origin is in words (#870).
- Picked geometry has one readable name instead of a hexadecimal id (#2403).
- Tab, Shift+Tab and Enter move between the value fields of a page (#2516).
- Tilted height planes follow the tool axis (#2416). The near plane no
  longer clips while zooming (PR #1689). A lathe without a Y axis hides Y on
  the gizmo (#2019).
- Property labels, dropdowns and Geometry rows fit narrow panels (#2295,
  #2065, #1688).
- A Donate button in the top bar (#2385).

### Simulation
- The simulation draws the stock body again during playback (#2396), and
  plays up to the first operation that has no toolpath (#2280).

### MCP, examples and diagnostics
- MCP can delete a model as an undoable step (#2444). Generate without a
  name regenerates operations that became stale in the same call (#2439).
- An example catalog (`examples/catalog/`), seeded with `2d/first-part`
  (#2393); refreshed first-part and router NC examples (#2422).
- Library and UI diagnostics go through the filtered terminal log (#163).
  Desktop startup keeps GPU diagnostics and backtraces (#2421).
- The User Guide has reproducible screenshots (#1902) and a Windows
  troubleshooting guide (#2423).

## Not exercised on a real control
No real control ran the output of these changes. The test goldens are
regression records. Check the first program on the machine before cutting.
- The Flip X / Flip Y mirror on Fanuc and Haas (#1649).
- The G73, G84 and G85 dwell refusals (#2354, #2356, #2357) and the grblHAL
  and Tormach G86 P0 (#2362).
- The Tormach A-word refusal (#2473) and the microARC 4 multi-turn A travel
  (#2471).
- The Mach4 post (#1968) and the Fanuc lathe chip-break dips (#419).
- The drill point break-through depth (#2441).

## Known issues
No `P1` or `P2` issue was open at the cut line (2026-11-18 17:33 UTC), and
none was filed during the cut. Two open defects have a workaround:
- #2561 (P3): 2D Contour tabs - the bridge check compares only neighbouring
  cuts. Check the bridges in the simulation when tabs stand close together.
- #1950 (P3, blocked): 3D Pocket Adaptive on a whole model. The stock ring
  around the part exceeds the planner limits. The planner then refuses
  Adaptive on that level with a note, and Offset rings cut the level. The
  output is safe. The release gate skips the three tests that pin this issue
  and checks that they still fail (`xtask/src/known_fail.rs`).

## Process changes
`PROCESS_VERSION` went from 89 to 100 (pinned issue #681):
- 90: the cut list and the cut line replace the P1/P2 start condition; a
  gate-map pin for the rotated WCS control suite.
- 91: the FreeBSD backfill workflow.
- 92: `cargo xtask docs-images`.
- 93-94: MCP skill rows for `set_stock_material`, `cutting_data` and
  `delete_model`; `cargo xtask catalog`.
- 95: the release gate runs its suites in a longest-first queue.
- 96: the `set_wcs_flip` row names the refused operations.
- 97: heavy release tests run first; AGENTS.md "Agent roles".
- 98: `merge-plan` warns on a stale review marker; LF endings for shipped
  NC examples.
- 99: the MCP skill drops Pocket Adaptive.
- 100: the MCP skill documents operation notes and corner checks.

---

# CamCraft 0.23.0 (clean-room implementation)

Released 2026-11-17.

## Breaking
One change is breaking: **the project file format goes from version 62 to
67**. 0.23.0 opens the files of 0.22.0 and older. 0.22.0 and older builds
refuse a file that 0.23.0 saves. The steps that have a named cause are:
- version 63: Drill, Bore and Thread store their through-hole break-through
  (#1557, PR #2126);
- version 66: a machine stores its AC trunnion fields and its non-RTCP flag
  (#1962, PR #2162);
- version 67: a setup stores its WCS axis flips (#1720, PR #1738).

A minor release headlined by **canned-cycle dwells that are posted for each
hole or refused by name**, **a setup that holds the part turned now generates
the 2D and hole operations**, **2D Pocket rest machining**, **a Post dialog
with options for each post**, **simulation playback that is calculated before
it plays**, **a viewport that stays fast on large models**, and **Solidcraft
.sc import**. 117 pull requests merged since 0.22.0 (21 features, 71 fixes,
4 performance changes, 21 tests, docs and process).

## Upgrade notes
- Save a project in 0.23.0 only when each person who opens it also has
  0.23.0. Older builds refuse format 67 (see "Breaking").
- A program with a dwell on a deep-peck cycle (G83) can be refused now, where
  0.22.0 posted it without the dwell (see "Changes to posted G-code"). Set
  the dwell to 0, or use a drill cycle that has a dwell on that control.
- The shipped post scripts and machine files are read from the `assets`
  folder beside the program at run time (#2282, PR #2293). An edit there
  takes effect after a restart. A missing file is refused by name.
- The Work offset control and "Set WCS from features" are now on the WCS
  page, not on the Setup page (#1648, #1926).

## Changes to posted G-code
These change the program that an existing project posts, or stop a post that
went through before.
- **Each hole gets its own dwell on G82 and G89** on the Fanuc, Mazak, Doosan
  DNM, grblHAL, Tormach PathPilot (#2360, PR #2363) and Haas (#2350,
  PR #2355) posts. Before, a hole with a different dwell in the same cycle
  group ran the dwell of the hole before it. A changed dwell now ends the
  cycle and starts it again with its own P. On grblHAL and Tormach the same
  applies to G86. A back-bore with dwell 0 posts G85 on the first five
  posts; on Haas it posts G89 with P0.
- **A G83 dwell is posted or refused, never dropped.** Fanuc, Doosan DNM and
  Mazak refuse it by name (#1820, PR #2048). grblHAL and Tormach PathPilot
  refuse it by name (#2348, PR #2351). Haas keeps or clears it correctly
  (#1820). Sinumerik keeps it as P in milliseconds, for a control in ISO
  dialect mode (#2349, PR #2353).
- **A bore with a dwell on G86 is refused by name** on the Fanuc, Haas,
  Mazak, Doosan and Sinumerik posts, because their G86 has no dwell word
  (#1445, PR #1821). A drilling dwell outside 0 to 3600 s is refused when the
  operation is made or edited.
- **A missing or malformed default machine file refuses the post** and names
  the file (#2319, PR #2320). Before, the program had no coolant words and no
  machine name.
- **A document with no machine is judged against the default machine**
  (#2321, PR #2327). An operation over its spindle speed or feed rate is
  refused by name.
- **A stock-source Deburr or 2D Chamfer that cuts into a part does not post**
  (#2313, PR #2314; #2308, PR #2312). The refusal names the operation and the
  part.
- **A Z-rotated work offset is carried through generation and the post**
  (#760, PR #2136), as Bake or as Control (G68 / ROT).
- **A setup with a flipped WCS axis is refused by name** at generation and at
  the post (#1720, PR #1738), until the flip is carried into the program.
- **A 3+2 tilt that is out of range is refused by name** (#2133, PR #2157).
  The A0 B180 plane keeps its X along machine X.
- **R arcs near 180° and 360° are split** into quadrant pieces, and the chord
  is checked after rounding (#2191, PR #2202).
- **Unit words at the program end** are honoured, noted one time or refused
  on each post (#2189, PR #2208).
- **Stored script options are applied to a named post** (#2190, PR #2206).
  A post "for each operation" from MCP and `cam.post` writes one file for
  each operation (#2188, PR #2203).
- **The Tormach lathe post clears the corners of square-bar stock** in its
  G30 moves (#1666, PR #1681).
- **The Sodick wire post writes tank and flushing words** from the machine
  file (#1499, PR #2125).
- Toolpath changes that show in the posted program:
  - Depth-stepping operations start one stepdown under Top Z when no stock
    stands over Top Z (#2004, PR #2013). Before, the first level was at
    Top Z.
  - 2D Adaptive and Pocket enter each separate area by the helix or ramp
    that you set (#2265, PR #2275).
  - 2D operations with Top below the material over them start over that
    material and feed down through it (#1559, PR #2129).
  - The crossing law holds on Offset rings, Waterline pockets and corner
    arcs (#2240, #2317), and on 3D Pocket helices, Adaptive microlifts and
    offset-shell floors (#2237).
  - A Bore with a Ramp entry under carried stock does not rapid into the
    uncut core (#2301, PR #2328).
  - 3D Finish: shallow-edge, overhang and convex-chord gouges are corrected
    (#1840, PR #2167).
  - Closed 2D contours with an exit point trace their fitted arcs again
    (#2195, PR #2205).
  - Drill, Bore and Thread state their break-through (#1557). A program
    that did not change posts the same.

## Highlights
### Posts and machines
- A Post dialog, with the options that each post declares (#258).
- A shared G93 inverse-time feed solver and section mode markers in the post
  streams (#1966). AC trunnion fields in the machine file (#1962).
- Five Haas 3-axis machines in the built-in machine library (#1976).
- A wire EDM machine states its tank and flushing (#1499). A wire tool kind
  is mounted on Wire Contour operations (#2011).

### Operations
- 2D Pocket rest machining: cut only what a larger tool left (#1489, #1815),
  with Zigzag rest boundaries that leave no wall cusps (#1490) and a travel
  budget on rest Adaptive (#2109).
- A setup that holds the part turned now generates 2D Offset, Trace, Engrave,
  Chamfer, the hole kinds, Circular, Slot and Trochoidal Slot, and accepts
  area controls on 2D Adaptive, Pocket, Contour and Face (#2210, #2283).
- 2D Contour: a picked start point with tabs on a closed contour (#1473);
  start and exit points picked in the viewport (PR #2197).
- Sinker EDM: "electrode from faces" accepts a rounded-corner wall (#1808).
- 3D Pocket and Multi-Axis Rough: a re-machining mask stays clear to open
  edges (#1749).

### Gouge, collision and simulation
- Simulation playback calculates the stock removal one time before it plays
  (#2181). A jump on the timeline does not lag.
- The simulation shows and judges a stock held in a vise, and a stock turned
  by a quarter turn (#2177). A drawn holder is drawn with its own profile
  (#2176). The stated shank is judged below a holder foot (#2199).
- Carried side-face and step contact finer than one grid pitch is resolved
  (#1580).

### Fixtures
- Seat the stock on a placed vise (#2173). Turn a fixture by quarter turns
  or by typed angles (#2174).
- Spun and extruded fixture profiles, with DXF and STEP (#744).
- Lathe chucks fit to the stock in the dialog, with inside grip and stepped
  jaws (PR #2083).

### Importers and geometry
- Model > Open, a viewport drop and MCP `import_model` open a Solidcraft
  `.sc` design (#1456). A damaged file is refused by name.
- An edge between two curved faces picks as one edge (#2209).
- 2D sketch primitives chain into loops with stable ids (#1861). It is not on
  a menu yet.

### UI
- The viewport stays fast with chains, welded faces, area previews and picks
  on the screen: derived geometry is computed one time for each input, not
  on each frame (#2235, #2246, #2247, #2248, #2250).
- A progress overlay over the viewport while a toolpath generates, with
  Cancel (#2257).
- The WCS page has the Work offset control, "Set WCS from features" and the
  axis flips (#1648, #1926, #1720).
- The grid and the work envelope stand on the lowest point of the model, the
  stock and the fixtures (#2172).
- AI Chat: one Ctrl+Z step for each turn, `get_selection`, and a live
  confirmation gate (#1942).
- `camcraft-ui --help` lists each start option; `--version` is new (#2082).
- `theme.json.example` lists each theme setting (#2025). More of the UI reads
  the theme (#2064) and has accessibility labels (#1988, #1848).
- Browser build: tracing goes to the console, and dropped model files load
  (PR #2239).

### Packages and docs
- Each release package has the `examples` directory (#2042).
- A "Your first 4-axis part" guide (PR #2135). The docs search opens from
  the URL query (#2329).

## Not exercised on a real control
No real control ran the output of these changes. The test goldens are
regression records. Check the first program on the machine before cutting.
- The dwell for each hole on G82, G89 and G86 (#2360, #2350): no physical
  control. The G85 in place of a G89 with dwell 0 comes from the published
  cycle formats.
- The G83 dwell refusals (#1820, #2348) and the G86 dwell refusal (#1445): no
  physical control.
- The Sinumerik G83 dwell as P in milliseconds (#2349): the maintainer
  confirmed the word on an 840D in ISO dialect mode (G291). The post writes
  no G291 line: the control must be in that mode.
- The Z-rotated work offset (#760) and the 3+2 tilt changes (#2133): no cut
  on a machine.
- The G93 inverse-time solver (#1966): no post uses it for output yet.
- The Tormach lathe G30 clearance (#1666) and the Sodick tank and flushing
  words (#1499): no physical control.

## Known issues
No `P1` or `P2` issue was open at the start of the cut, and none was filed
during it. These defects are open and have a workaround (set the dwell to 0,
or check the posted block):
- #2354: the Sinumerik post drops a G73 dwell without a diagnostic.
- #2356: the Fanuc, Haas, Mazak and Doosan posts drop a G73 dwell without a
  diagnostic.
- #2357: the Sinumerik G84 and G85 dwell words do not agree with the ISO
  dialects manual.
- #2359: Bake rotation mode: the schema text and the posted coordinates do
  not agree.
- #2362: the grblHAL and Tormach posts write G86 with no P when the bore
  dwell is 0.

## Process changes
`PROCESS_VERSION` went from 78 to 89 (pinned issue #681).
- 79: the multi-pass lathe field parity test is in a heavy lane; MCP skill
  rows for the 2D Pocket rest parameters and the post options.
- 80, 81, 83, 84, 86, 88: MCP skill rows for `list_features`
  `outside_unknown`, `edit_tool`, the operation coolant tool, the lathe chuck
  tools, the thread-mill tools, the Zigzag rest behaviour and `set_wcs_flip`.
- 82: `cargo xtask gate` runs one step for each test binary; `--only` and
  `--failed` select one binary.
- 85: `crates/ui/AGENTS.md` has a "Frame cost" section; MCP skill row for
  `turn_fixture`.
- 87: rules for named page layout tests and for behaviour tests without
  fixed positions.
- 89: two new gate-map pins in `xtask`.

A fork syncs its process files at the start of its next session
(`fork-sync` skill).

---

# CamCraft 0.22.0 (clean-room implementation)

Released 2026-11-16.

## Breaking
One change is breaking: **the project file format goes from version 61 to
62** (#1453, PR #2026). A tap now stores its hand (right or left). 0.22.0
opens the files of 0.21.0 and older, and reads each older tap as right-hand.
0.21.0 and older builds refuse a file that 0.22.0 saves, so that they cannot
lose the tap hand without a word. A tool library export that contains a
left-hand tap uses library version 2; a library with right-hand taps only
stays at version 1.

A minor release headlined by **safer rapids and posts** (a clearance plane
inside the stock now holds the post, left-hand taps and unsupported expanded
bores are refused before output), **a more exact 2D Contour** on open stock
edges and picked rings, **waterjet machine kinds**, **4-axis wire kerf
widening for each ruling**, **an AI Chat dock panel**, and **many tool editor
and theme fixes**. 61 pull requests merged since 0.21.0 (5 features, 48
fixes, 1 refactor, 7 tests, docs and process).

## Upgrade notes
- **Regenerate each operation one time after the upgrade.** The post now
  checks each rapid at the clearance plane against the stock (#2121). A
  toolpath that was made before this check reads as out of date, and the post
  refuses it until you generate it again (#2152, PR #2155). Each saved
  operation shows as out of date one time. Generate it again, then post. The
  included example projects show the same state the first time you open them.
- A 4-axis wire operation that was saved before the kerf widening change
  (#1170, PR #1921) also has an old stamp and must be generated again.
- Save a project in 0.22.0 only when each person who opens it also has
  0.22.0. Older builds refuse format 62 (see "Breaking").

## Changes to posted G-code
These change the program that an existing project posts, or stop a post that
went through before.
- **A clearance plane inside the stock holds the post** (#2121, PR #2138). A
  rapid that rides the clearance plane through uncut stock is named, and the
  post stops. Before, the program posted with no warning. MCP `add_operation`
  and `edit_operation` give a warning when the plane is inside the stock.
- **Old toolpaths are refused until you regenerate them** (#2152, PR #2155).
- **Left-hand taps are refused before posting** (#1453, PR #2026) on the eight
  posts that tap: Fanuc, Haas, Siemens 840D, Heidenhain, Mazak, Tormach
  PathPilot, Doosan DNM and Fanuc lathe. Right-hand output does not change.
- **Heidenhain, TinyG and Carvera refuse an expanded bore** (#1819, PR #2040).
  Their plain-move chains do not stop the spindle before the rapid retract.
- **The Sodick wire post refuses a section that leans the wire before its
  first cut** (#1497, PR #2120). Before, it threaded and cut the section
  upright.
- **The Fanuc lathe post negates C on a machine whose C axis turns about -Z**
  (#33, PR #2103). A C axis that is not on Z is refused by name. Arcs on a C
  turn are cut into short lines.
- **4-axis wire cuts stand off the profiles only where the surface twists**
  (#1170, PR #1921). The straight stretches of a punch are no longer
  oversize. "Uniform" keeps the old behaviour.
- Toolpath changes that show in the posted program:
  - A Contour on an open edge of the stock cuts beside the edge, one tool
    radius plus the stock to leave off it, on the side that Side names (#1928,
    PR #1934).
  - 2D Contour Side Auto on a picked ring reads the material at the ring's own
    level: a blind bore rim is cut from inside (#1933, PR #1954).
  - 2D Contour refuses a picked ring whose side is in the part at each level
    (#1927, PR #1935). Before, it cut only the top plane.
- The Explain G-code switch is now on the document and setup post pages
  (#1390, PR #2079). It adds comments only, and it is off by default.

## Highlights
### Posts and machines
- Waterjet machine and nozzle kinds, with checked material and thickness
  recipes and XY machine checks (#1881).
- The empty Sinker EDM template is in the built-in machine library (#1135).
- The Tormach 8L, 15L Slant-PRO and 15LX Slant-PRO state rigid tapping from
  their operator manuals (#795).
- An AC table-table kinematics solver is in the post crate (#1965). No post
  uses it yet.

### Operations
- 2D Contour: Start point and Exit point on the Lead tab (#1448); Side Auto
  reads the ring's own level (#1933); open stock edges and wrong-side rings
  (#1928, #1927).
- Slot: manual A/B fields in a "Manual" section, shown in the viewport while
  you edit them (#1732).
- 2D rest machining: an Adaptive planner on the proved remaining stock
  (#2031). The Pocket operation does not use it yet.
- 4-axis wire: kerf widening for each ruling and a verify tolerance (#1170).

### Gouge, collision and simulation
- Rapids at the clearance plane are checked against the stock (#2121).
- Machined walls and cavities stay through side-on turns of box stock
  (#1805).
- Bounded 3D chamfer, ball and rounded edge-break volumes are credited
  (#1891).

### Tools
- Taps store their hand (#1453).
- One Weldon holder entry for each taper; the bore comes from the shank
  (#2018).
- Properties shows the selected tool's settings; the Tool library is a docked
  panel of its own (#2002).
- The tool editor preview shows the tool body (#2005), keeps its width (#2003)
  and shows the floor of a machine-default holder (#2009).

### Importers and geometry
- A refused machining mesh is tried again one time at a finer chord (#1463).
- Spline and revolved surface deviation is projected and certified (#1029).
- STEP import stops after a few runaway faces, in bounded time (#1599).
- A bounded ASCII DXF reader (#1860). It is not on the import menu yet.

### UI
- An AI Chat dock panel that works offline, with confirmations and one-click
  undo of a turn (#1946). MCP maps its tool schemas for three AI providers
  (#1943).
- A lathe shows its spindle axis from left to right (#1993). The mill work
  envelope stands on the table (#1963). An opened document frames the part
  (#1997).
- The File menu opens the example projects (#2043). The Machines pane filter
  has Waterjet and EDM (#2030).
- The WCS pane disables X and Y tilt on milling setups and points to Part
  alignment (#1542).
- Ribbon, tabs, browser tree and highlights read the theme (#2021, #2066,
  #2063). Browser rows and menus have accessibility labels (#1989).

### Docs
- A "Your first router part" guide with a Sienci AltMill demo project (#1917).
  The user guide is brought up to date (#1939, #2118) and shows the app icons
  (#1901).

## Not exercised on a real control
No real control ran the output of these changes. The test goldens are
regression records. Check the first program on the machine before cutting.
- The clearance-plane rapid check (#2121) and the stale-cache refusal
  (#2152): no cut on a machine.
- The left-hand tap refusal on the eight tapping posts (#1453): no physical
  control.
- The expanded-bore refusal on Heidenhain, TinyG and Carvera (#1819): no
  physical machine.
- The Sodick wire lean refusal (#1497): no Sodick control.
- The -Z C axis on the Fanuc lathe post (#33): no Fanuc control. The composite
  feed check also needs a real machine.
- The 4-axis wire kerf widening and verify tolerance (#1170): no EDM control.
- The Explain G-code switch (#1390): no machine or control run.
- The Tormach lathe rigid tapping flag (#795): no PathPilot lathe run. The
  lathe does not post a tap cycle from it yet.
- The 2D Contour changes (#1928, #1933, #1927): no machine run.
- Stock simulation and verify: side-on turns (#1805) and edge-break credit
  (#1891) ran in tests only.
- The G76 readings that two manuals settle (PR #1897): no Fanuc control or
  simulator. Reading 4 stays open on #1842.

## Process changes
Contributor and agent process only; no effect on the program. PROCESS_VERSION
went from 76 to 78:
- **77** (PR #2098, bump PR #2117): AGENTS.md has the 2D shape-mask verifier
  guidance for motion tests.
- **78** (PR #2060, bump PR #2139): `cargo xtask gate` has one job budget for
  each run (default 6, `CAMCRAFT_GATE_JOBS` changes it). Each lane of a wave
  gets its share. The release tier runs at most half the budget in lanes at
  one time.

A fork syncs the process files at the start of its next session (`fork-sync`
skill).

## Known issues
- The cut started at 2026-11-16T10:18Z. No P1 issue was open. One P2 issue
  was open, and the maintainer released it for this cut:
  - #2004 Depth-stepping operations cut a first level at Top Z that removes
    no material. The first level is an air pass. Its fix is in PR #2013 and
    ships in the next minor.
- Open P3 issues with a risk of wrong output:
  - #1842 Fanuc lathe G76: four schedule readings are unverified; a wrong
    Z-shift or end-Z reading moves a flank thread axially or into a shoulder.
  - #1666 Lathe post stream: state the stock's turning radius, so a box
    stock's rotating corners are inside the G30 envelope.
  - #1619 Tormach lathe G76 (inch): floored J makes LinuxCNC cut ceil(K/J) =
    4 passes where the record verified 3.
- The pixel-snapshot lanes were skipped at this cut.

---

# CamCraft 0.21.0 (clean-room implementation)

Released 2026-11-15.

## Breaking
None. No pull request in this release is marked breaking, and the project file
format stays at version 61, so 0.20.0 opens files that 0.21.0 saves. Some posts
and some toolpaths change for existing projects, though: read "Changes to posted
programs" below before you post an old project again.

A minor release headlined by **more wire and sinker EDM** (an outline
electrode, Side Burn wear compensation, angle-word taper, 4-axis fan spans and
sync picks, the Sodick ALC400G machine), **a 2D Adaptive Trochoidal method that
clears wide and island regions**, **3D operations in part-aligned setups**,
**stock carried into a setup turned side-on**, and **safer entries and setup
changes in the posts**. 106 pull requests merged since 0.20.0 (19 features, 65
fixes, 22 tests, docs and process).

## Changes to posted programs
These change the G-code that an existing project posts.
- **Fanuc-family posts write a decimal point on whole-number values** (#1836,
  PR #1866). Fanuc, Haas, Mazak, Doosan DNM and Fanuc lathe write `Z14.`, not
  `Z14`, for each length, angle and feed-per-revolution word. A control with
  parameter 3401#0 DPI=0 then no longer reads the word in least increments.
- **G76 threading states the passes that were verified** (#417, PR #517). With
  the G76 cycle option, the finish allowance R is posted: the default Fanuc lathe
  thread now posts `R0.4` instead of `R0`. G32 passes stay the default output.
- **Threading can repeat each roughing depth** (#418, PR #518). Off by default;
  the G76 cycles refuse it by name.
- **Seven mill posts retract Z and name the next setup before a setup-change
  stop** (#1647, PR #1859). Heidenhain refuses a setup change without a safe work
  clearance.
- **The Tormach lathe approaches one axis at a time after G30** and leaves the
  stock before each G30 (#1604, PR #1667).
- **The Tormach mill skips a repeated G30/M6** when two sections in a row use the
  same tool (#1626, PR #1741).
- **grblHAL and TinyG** state indexed work offsets G54-G59 (#1385, PR #1824).
  grblHAL, Carvera and TinyG write no motion after a refused first traverse
  (#1682, PR #1707).
- **Sodick wire posts** refuse an angle-word move and a control-side offset
  request by name instead of posting them wrong (#1496, #1498).
- Toolpath changes that show in the posted program:
  - A Contour on an open edge of a part cuts with the tool on the air side, one
    radius off the edge, not on the edge (#1777, PR #1781).
  - Default Ramp entries of Inside Contour, Circular and Bore use 3 degrees, not
    the link ramp angle (#1674). An inside Contour and a Bore with Ramp entry
    descend at the ramp angle, not straight down at the cutting feed (#1664,
    #1665).
  - A whole-model 3D Pocket and Multi-Axis Rough clear out to the stock outline
    and past an open-sided notch (#1774, #1749).
  - Bore clears a wide hole with concentric rings (#1566).

## Highlights
### Posts and G-code
- Optional plain-English Explain comments on the Tormach mill and the
  Fanuc-family posts; the executable words do not change (#1387, #1388).
- MCP lists the Tormach PathPilot Plasma post with its Plasma Profile workflow
  (#1871).

### Wire and sinker EDM
- A sinker electrode can be a body with an outline, made from the cavity faces
  (#1152). Rough and finish burns count the orbit radius (#1355). A Side Burn
  takes a measured electrode wear and is compensated in Z (#1340). The Machine
  window edits each condition's wear rate and notes (#1381).
- Wire EDM: a machine can state an angle-word taper dialect (#1164); explicit
  4-axis fan spans (#1172); pick the 4-axis sync points in the viewport (#1171);
  skim order, holding tabs and captured-piece controls on the Passes tab
  (#1148).
- A wire taper that the machine cannot post is refused when you set it (#1800).
  A user post script's Wire EDM manifest is honoured (#1335).
- New built-in machine: Sodick ALC400G wire EDM, travels and taper from Sodick's
  published specifications (#1501).

### Operations
- 2D Adaptive (Trochoidal) clears wide, branching, island and closed-ring areas
  with parallel lanes and clean-up peels (#1587). Its engagement, return, return
  feed and loop width are parameters (#1588).
- 2D Pocket, 2D Adaptive and 2D Offset take a corner arc radius (#1733), cut the
  band between two opposite open edges out to the stock (#1735), and accept a
  U-shaped blind slot (#1762).
- Slot: "Extend past A" and "Extend past B" (#1731).
- Face, Contour, Pocket and 2D Adaptive take picked faces and edges in aligned
  setups (#763). 3D Pocket, 3D Adaptive and 3D Finish work in part-aligned setups
  (#1743).
- The Boundary group takes edge picks in any pick mode; two edges bound the area
  between them (#1727).
- Tools can be assembled from modular multi-piece bodies from vendor STEP, STL
  or OBJ models (#361).
- Cross drill works on reduced-fidelity models with clearances widened by the
  model's excess (#1053).

### Gouge, collision and simulation
- A setup turned side-on can take the machined stock of the setup before it; a
  gouge cut before the turn stays in the verdict (#874).
- An earlier setup's chamfers, deburrs and marks are credited before gouges are
  kept (#1806). Engrave and Trace marks are credited only on the marked parts
  (#1627).
- 3D Pocket refuses a depth that the shank or holder cannot reach, and names
  tool clearance when the open area is too narrow (#1844, #1709).
- A stock band narrower than the tool is named (#1655). A stock-source 2D Chamfer
  that breaks into a part names the cause and the workaround (#1637).
- Ring ramps keep their planned chords (#1612). Rim crossings need a distance
  band, so near-trace wall wobble stays under check (#1669).

### Importers
- A STEP closed shell that meshes open imports as an incomplete model with its
  hole named; operations on it are refused at the post (#1610).
- Runaway STEP edge and swept-curve subdivisions are refused by name (#1545).
- Closed inward STL and OBJ bodies are turned outward at import (#1672).

### UI
- The workbench rework: denser built-in themes (PR #1639).
- The Browser View shows the machine's work envelope (#1567).
- The WCS marker pick anchors on part corners and circle or slot centres (#1502).
- Hovering a picked feature marks its row in the Geometry tab (#1519). Pick
  buttons share one style with a clear armed state (#1728). The first Ctrl+click
  on a second edge adds it (#1726).
- Machine page inputs carry accessibility labels (#1119). The CamCraft icon shows
  in the Windows taskbar (#1754).

### MCP
- New tools: `get_post_options` and `set_post_options` (#1534), cylinder
  alignment and setup origin placement (#1586).

### Packaging and docs
- The Linux RPM builds again (#1719).
- Each of the 44 operation reference sections has a toolpath picture (#1250).
  The user guide is grouped by job stage (#1903).

## Not exercised on a real control
No real control ran the output of these post changes. The test goldens are
regression records. Check the first program on the machine before cutting.
- The decimal point on Fanuc-family words (#1836): no control ran with DPI=0 or
  DPI=1. The behaviour comes from the published parameter documentation.
- G76 with the finish allowance R (#417): no Fanuc control run. That run is
  tracked in #1842. The threading repeat pass (#418): no controller run.
- Setup-change retract and stop on the seven mill posts (#1647).
- The Tormach lathe G30 approach (#1604): no PathPilot lathe run. The Tormach
  mill repeated-tool skip (#1626): no PathPilot or physical control run.
- grblHAL and TinyG work offsets (#1385).
- Wire and sinker EDM: the angle-word taper (#1164) and the Side Burn wear
  compensation (#1340) use synthetic fixtures. The Sodick ALC400G output (#1501)
  was not run on a Sodick control.

## Process changes
Contributor and agent process only; no effect on the program. PROCESS_VERSION
went from 65 to 76:
- **66** (PR #1693, PR #1704): the MCP skill documents `fit_stock`; the batch
  gate runs the kernel suites; `gate --only` and `gate --failed`.
- **67** (PR #1757): the post-merge gate has its own runner label.
- **68** (PR #1718): the MCP skill documents `detect_holes` skipped holes and the
  Bore refusal.
- **69** (PR #1722, PR #1695): the MCP skill lists the post-options, cylinder
  alignment and setup origin tools.
- **70** (PR #1737): the release gate builds once and runs one heavy step for
  each test binary.
- **71** (PR #1744): the MCP skill covers modular multi-piece tool bodies.
- **72** (PR #1790): the release gate runs independent lanes up to four at a time.
- **73** (PR #1830): AGENTS.md is shorter and in ASD-STE100; procedures moved to
  the `gate`, `runners`, `release-cut`, `fork-sync` and `issue-claims` skills.
- **74** (PR #1823, PR #1835, PR #1878): committed review artifacts are removed
  and refused; the release plan has kernel heavy steps.
- **75** (PR #1899, PR #1900): `merge-ready` needs evidence of an independent
  review; the MCP skill documents the plasma workflow.
- **76** (PR #1919): the MCP skill states the bounded Bore ring rule of
  `detect_holes`.

A fork syncs the process files at the start of its next session (`fork-sync`
skill).

## Known issues
- No P1 or P2 issue was open at the start of the cut (2026-11-15T16:09Z), and
  none was filed after it.
- The pixel-snapshot lanes were skipped at this cut.

---

# CamCraft 0.20.0 (clean-room implementation)

Released 2026-11-14.

## Breaking
- **Climb milling now runs climb on 2D Contour and 2D Chamfer** (#1555, PR #1577).
  With the M3 spindle, an outside profile runs clockwise and an inside profile
  runs counter-clockwise. Under control or wear compensation, climb posts G41
  and conventional posts G42. The posted direction of every existing 2D Contour
  and 2D Chamfer reverses, and G41 and G42 swap. A project that set climb off
  to get climb milling now posts conventional. Check the first program you post
  from an old project.
- **Work offsets above G59 are saved by number** (#931, PR #1113). A setup can
  use the offsets that its own post supports (Tormach: 7 to 9, G59.1 to G59.3).
  Older builds cannot open a project that saves such an offset. A post that
  cannot write an offset refuses it by setup name. It no longer posts G54.
- **Programs stop between setups by default** (#1532, PR #1650). A program that
  runs from one setup into the next now writes M0 with a comment that names the
  next setup. "Between setups" in the post options chooses an optional stop,
  running straight on, or one file per setup.
- **Tormach mill programs end and lift differently** (#1594, #1596, PR #1600,
  PR #1606). A program ends with `G53 G0 Z0` then `M30`, with no trailing G30.
  Each tool change is followed by a `G53 G0 Z<safe>` lift before the first X/Y
  move.

A minor release headlined by **a Z clearance law for the posts**, **wire and
sinker EDM in the UI**, **stricter gouge judgement of 2D operations against the
parts in the stock**, **a hardened STEP importer**, and **a browser build that
opens, saves and posts**. 120 pull requests merged since 0.19.0. Three fixes came in during the cut: #1692 (issue 1691), #1698 (issue 1697, the STEP bench meters truck's faces) and #1712 (issue 1706, test only: the heavy test of `shared_face_bodies` asked for a 3D Pocket that the product correctly refuses; the test now pins the refusal and generates the 3D Finish).

## Highlights
### Posts and G-code
- Fanuc, Haas, Mazak, Doosan DNM, Sinumerik 840D and Heidenhain posts state the
  section's clearance Z before the first X/Y traverse after each tool change.
  grblHAL, TinyG, Carvera and Tormach do the same. A stream that states no
  clearance or retract plane is refused by name (#1633, #1616, #1632).
- The Carvera post lifts to the clearance height after M6, before the X/Y
  traverse (#1616).
- Work offsets are an index; each post writes its own word. The Setups page lists
  the offsets of the setup's post, and MCP `set_work_offset` takes a 1-based
  `index` (#931).
- The Mazak post writes `S.. M3` or `M4`, and `M5`, not MOTON and MOTOFF (#1411).
- The fanuc-lathe C-axis feed is floored, never rounded, over its cap (#1392).
- Explain G-code reads a lathe post's own words: G98, G99, T0101 and G50 S
  (#1399).
- The whole-program post refuses by name when setups differ in End units (#1540).
- New Sodick wire EDM post for 2-axis contours; taper and 4-axis are refused by
  name (#617).
- Drilling has a Ream cycle (G85 or an explicit feed-out chain) (#1445).
- Circular posts rings as arcs, not 512 chords. A 6 mm hole drops from 74,131
  moves to 417 (#1558).
- New built-in machine Makera Z1 (#868). The Doosan DNM 5700 is a 3-axis machine
  with full-stroke travel, and a separate rotary-table variant (#1433, #1410).

### Gouge and collision checks
- 2D Engrave, Trace, Slot, Trochoidal slot, Circular, Contour, Face, Chamfer,
  Drill, Bore and Thread are judged against the parts in the stock, also when
  the stock is their source. A cut into a part blocks the post (#1543, #1550,
  #1593, #1607, #1631, #1635, #1641).
- The post judges the parts beside each operation's source again at post time
  (#1628). A legal stock-source Drill, Bore, Thread or cross drill in the
  part's own modelled hole is no longer refused there (#1691).
- A Slot or a Trochoidal slot that leaves its picks is refused by name (#1503,
  #1504).
- A Bore wider than three tool diameters is refused by name (#1556).
- A milling setup whose WCS Z is tilted off the machine Z is refused (#1510).
- Fixtures are judged against the carried stock's real shape in a flipped setup
  (#1530). Model heights and Fit to Models measure parts, never fixtures
  (#1529, #1538).
- Lathe Facing on a grooved bar is no longer refused wrongly (#1408). Lathe chuck
  collision solids never drop steel (#1470).
- A 2D clearing no longer cuts across a part between two pieces of its area
  (#1620). A Face with a last row on a rim vertex gets no false Gouge (#1659).

### Operations
- 2D Adaptive has a Trochoidal method (#1509). 2D Pocket can enter by plunge
  down proved pre-drilled holes (#1449).
- Contour has a user-set start point and exit point on closed contours (#1448,
  #1473).
- Wire EDM: skim passes with a condition per pass (#538), and 4-axis ruled
  surfaces between an upper and a lower profile (#537).
- Circular ramp entry rides the ring at the ramp angle (#1609).
- Fixtures can generate 3-jaw, 4-jaw and collet lathe chucks (#1442).

### Importers
- STEP import refuses a closed shell that meshes open, a face whose tessellation
  never converges, and any face the kernel cannot mesh, by name (#1573, #1536).
- Three-fillet corner blends import closed (#1548). Rational B-spline thread
  faces mesh in seconds at full fidelity (#1602).
- A closed STEP solid wound inward is turned outward, and the import says so
  (#1617). The per-face evaluation cap has measured headroom (#1568).
- Solidcraft `.sc` bodies are read, and far-off coordinates are refused (#1456,
  #1467).

### UI
- Setup, Stock, Tool list, Fixtures and Machine pages share one layout (#1440).
- Setup alignment composes on the current pose (#738). Two orientation dropdowns
  replace the old section (#771). Circle and arc centres can be picked as points
  (#692).
- Operation picking is a mode; other tabs lock the selection (#1514). Edge picks
  are one edge per click (#1505).
- Sinker EDM machine setup in the Machine window (#548).
- Drop a STEP, STL or OBJ file on the window to import it (#1450). A failed
  project open keeps the current document (#1613).
- The browser build opens files with a picker, saves, and posts as downloads
  (#31, #1431). The logotype is the window icon and the package icon (#1459,
  #1461).

### MCP
- `set_work_offset` takes a 1-based `index` and replies with `index` and `name`.
- New tool `fit_stock`. Chuck fixtures, pick points and the contour start and
  exit points are available through MCP.

### Tooling
- The cut runs on a self-hosted Linux runner, and a post-merge gate runs on
  main (#1413, #1451). PR builds run on `camcraft-pr-test` runners (#1420).
- `cargo xtask` selects and runs directory test suites (#927).

## Not exercised on a real control
No real control ran the output of these post changes. The test goldens are
regression records. Check the first program on the machine before cutting.
- The clearance Z law on the Fanuc family, Haas, Mazak, Doosan DNM, Sinumerik
  840D and Heidenhain (#1636, #1633). A G43.4 stream with an unknown rotary pose
  was not run.
- The clearance Z law on grblHAL and TinyG (#1632).
- The Carvera post (#1632, #1661): after `M6` it writes `G0 Z` to the clearance
  height before the first X/Y traverse. This follows a reading of the public
  firmware source. No real Carvera ran it.
- Setup stops and "Between setups" (#1650); the Fanuc and PathPilot programs are
  checked by text only.
- The Tormach clearance state, partly (#1618): the refusal path was not run on a
  control.
- The Sodick wire EDM post (#617). Its program header says it is not verified.
- The Ream cycle (#1445) and the fanuc-lathe C-axis feed cap (#1392).

## Process changes
Contributor and agent process only; no effect on the program. PROCESS_VERSION
went from 62 to 65:
- **63** (#1488): the merging session is the one writer of `PROCESS_VERSION`;
  process PRs no longer edit it.
- **64** (#1581): the PR test job runs on any `camcraft-pr-test` runner, Linux or
  macOS. A fork pulls the process files.
- **65** (#1645): issues and comments are written in ASD-STE100 Simplified
  Technical English. A fork pulls the process files and reads AGENTS.md again.

## Known issues
- No P1 or P2 issue was open at the start of the cut, and none was filed after it.
- No installer is built on the cut machine; the zip is the release, and the
  published artifact is built by the release workflow from the tag.

---

# CamCraft 0.19.0 (clean-room implementation)

Released 2026-11-13.

## Breaking
- **The document's unit is the unit every post writes** (#902, PR #981).
  Posted programs are in the document's mm or inch (G21/G20, Siemens 840D
  G710/G700), never the machine file's unit. The grblHAL and TinyG posts follow
  it; the Carvera, Heidenhain and Fanuc Lathe posts refuse an inch document by
  name. Older projects open on the unit they posted in. MCP gains `set_units`.
  Check the first program you post on each machine.
- **Thread milling is reworked** (#894, PR #1005). Thread mills are tool kinds
  (multi-form and single-form), and the Thread operation cuts internal threads
  by helical interpolation (radial passes, climb or conventional, right or left
  hand) and posts G2/G3 with Z. A Thread operation no longer accepts a Tap-kind
  tool, so projects that used one need a thread mill.
- **Air coolant and machine default coolant** (#869, PR #961). An operation can
  ask for Air (M7 air blast), resolved from the machine's coolant list or
  refused by name. A new operation starts on its machine's default coolant (Air
  on the Carvera), and the Carvera post posts the operation's own coolant
  instead of the first Flood entry. The machine editor adds coolants with their
  codes and no longer saves a coolant without one.
- **Project file format is v59** (#1056, PR #1341). Projects saved before mesh
  fidelity was recorded (v58 or older) open with every model marked "fidelity
  unknown", and their operations are refused by name until the model is
  re-imported. Files saved by 0.19.0 do not open in 0.18.0.

A minor release headlined by **wire EDM and sinker EDM**, **more turning**
(threading infeed, workholding rpm limits, lathe holder bodies), **new posts**
(grblHAL, TinyG, generic ISO wire and sinker EDM), **a much larger MCP surface**
and **stricter verification** of imported STEP models. 237 pull requests merged
since 0.18.0.

## Highlights
### Wire EDM
- Machine files can declare `wire_edm` and `sinker_edm` machines with an `edm`
  section and an EDM condition table; a program whose operation its setup's
  machine cannot run is refused at post (#530, #560).
- 2-axis Wire Contour: kerf offset to a stated punch or die side, start hole,
  corner refusal, open runs cut in from the stock's edge, tabs with a tab
  cut-off (an unheld slug is refused), no-core burnout by offset rings, and
  constant taper with posed moves and max-angle and U/V refusals.
- Wire verification: kerf, threading and slug laws block the post.
- A tool-less Wire Contour page and machine section in the UI.

### Sinker EDM
- Sinker burn to depth with one electrode, rough and finish with undersize from
  their conditions, orbit patterns (circular, vector, planetary), side burn,
  electrode wear compensation in Z, and an electrode generated from a cavity's
  picked faces (`electrode_from_faces`).

### Turning
- Threading infeed method with a decreasing-depth pass schedule (#516).
- A setup's workholding max rpm caps G96 and refuses G97 above it.
- The index swing of any work-turning rotary is judged about a level line, and
  a rotary subplate raises the A centerline.
- Lathe holder bodies can be placed from the tool editor and over MCP, are read
  on their own side of the axis, and are judged against chuck jaws and standing
  steel. The simulation draws the turning tool's lathe body.

### Milling, threads and finishing
- External threads on a boss at the operation level, and thread-milling pass
  control (decreasing passes, finish and spring passes, depth allowance).
- Finish 3D gains a land model that fills closed openings on the drive surface.
- Part alignment from two perpendicular edge picks (full 3D frame), the WCS from
  any set of feature picks, and the WCS anchored on a picked circle's centre.
- Tools carry part number, vendor, coating, helix angle, notes and cutting data
  per work material; the setup sheet tool list prints them. Tool library import
  has add, update and replace modes, and `.camtools` carries the tool body.
- A self-centering grip vise jaw, and editing and re-fitting a placed mod vise
  pair.

### Posts
- New posts: grblHAL (3-axis mill), Synthetos TinyG (3-axis mill), generic ISO
  wire EDM and generic ISO sinker EDM (every EDM code is machine data). Sienci
  AltMill machines run on the grblHAL post.
- Fanuc Lathe writes inch programs; the document's program number and comment
  are written per dialect. A user's post script states the machine kinds it
  serves. Rigid tapping is refused by name at the program gate when the machine
  does not state it.
- An opt-in **Explain G-code** option writes plain-English comments on blocks
  for posts that declare a comment form; other posts are refused by name.

### MCP tools
`set_tool_holder`, `list_holder_library`, `rename_operation`,
`reorder_operation`, `set_operation_coolant`, `set_setup_post`, `edit_machine`,
`edit_tool`, `set_wcs_from_features`, `set_wcs_circle_centre`, `lay_part_on_face`,
`set_tool_number`, `set_work_material`, `apply_cutting_data`, `delete_tool`,
`set_operation_setup`, `delete_setup`, `electrode_from_faces`, `set_units`, and
`add_operation` for the Wire Contour with no tool. `pick_in_box` takes a
`through` argument. The skill documents every registered tool.

### Verification and collision
- A STEP model imported coarser than 0.005 mm is flagged as reduced fidelity:
  gouge checks are tightened by the excess and its operations are refused at the
  post once it is too coarse. The facet budget is a setting carried with the
  import. A STEP import can go through solidcore behind a seam, and a machining
  mesh with a measured deviation is available from it.
- A from-model stock's source is judged unless marked raw stock only. The rapid
  law reads the stock under the rapid, and the mid-turn sweeps read the whole
  turn rather than five poses.
- Refusal ghosts show where a part-way refusal stopped.

### UI
- The Setups page lists setups only; alignment, work offset, workholding and
  tools live on the setup. A viewport bottom control bar holds the view buttons,
  and a "Pick through" toggle lets picks reach the far side.
- The G-code window has syntax colour, selectable text, a highlighted trace line
  and click-to-trace. Error text is selectable and copyable everywhere.
- Original SVG operation icons, tool kind glyphs and status badges, with an
  SVG-to-PNG icon pipeline. The tool editor edits each milling tool kind by its
  parameters. Viewport origin gizmos wear their names.
- The browser (wasm32) build is published under `/app/` on the project site.

## Fixes
91 fixes landed; the ones most likely to matter when posting:
- Posts: canned-cycle posts traverse between holes at the stream's hop and seat
  an off-seat cycle by the same law; Sinumerik writes work offsets 5 and 6 as
  G505 and G506; an absent rotary takes no angle word; lathe posts state G97
  before a section that is not constant surface speed; a milling operation is
  never posted on a lathe's work spindle; a cut with the spindle stopped, or a
  canned cycle with no feed, is refused in every spindle mill dialect; a touch
  probe section starts no coolant; a post that does not serve its setup's machine
  kind is refused by name; the sinker ISO post writes the document's unit.
- Rotary and A-axis: posts cap the feed of a block that turns a rotary at the
  machine's stated rotary rate and linear feed (Fanuc, Doosan DNM, Fanuc Lathe;
  #885), and the Doosan DNM post writes the first A word of every operation, so
  an A0 operation after a tilted one no longer cuts at the old angle (#950).
- Swarf: the mid-turn sweep no longer refuses a blade-lying link that turns B
  through vertical (#1404).
- Verification: a tool cutting into a flat is a gouge; a necked tool is judged at
  its drawn neck; non-finite or zero tool axes are refused; crossing checks read
  a shared vertex a rounding apart as a touch; the hole-wall excuse and tilted
  band laws were corrected.
- Import and project: a STEP record nested past 64 parentheses is refused by name
  instead of overflowing the stack; STEP parse errors name the entity and line;
  opening a project over MCP replaces the document.
- Adaptive roughing no longer drops a level to Offset rings because the entry
  orbit's closing point sat a rounding error off its start (#980).
- UI: no console window on a Windows double-click, native dialogs are owned by
  the shell window, and the plasma refusal reads plainly.

## Not exercised on a real control
These were checked by simulation, recorded streams and tests only, not on a
machine or a vendor control:
- The generic ISO wire EDM post and the generic ISO sinker EDM post are
  synthetic: their codes are machine data, not taken from a real control.
- Wire taper, tabs, no-core burnout and 4-axis wire as merged, and the sinker
  orbit, side burn and wear compensation moves.
- The turning thread infeed plan (#516).
- The rotary feed cap (#885) and the Doosan DNM A-state change (#950) merged
  without a PathPilot Hub run or a real Doosan control.
- The grblHAL and TinyG posts, and the Sienci AltMill machine files.
- Thread milling by helical interpolation was verified in simulation only.

## Process changes
Contributor and agent process only; no effect on the program. PROCESS_VERSION
went from 16 to 54 (numbers 23 and 27 were not announced):
- **17** (#1117): the `gh-issues` skill is in the repo.
- **18, 20, 22, 25, 28, 34, 35, 40, 41, 43-53**: camcraft-mcp skill rows for new
  MCP tools and arguments.
- **19, 21, 33, 37, 38, 42**: xtask gate maps and the quick gate follow moved
  modules, changed post scripts and machine files, `#[path]` files, and the
  motion generator; probe and arc post suites and twelve other test files moved
  into the crates that own them.
- **24, 26** (#1116, #1179): `cargo xtask icons` rasterises the SVG masters.
- **29** (#1193): the priority scale is `P1`..`P9`.
- **30** (#1214): the gate reports VOID when the checkout moves during a run.
- **31, 32** (#1227, #1321): refresh only at merge time; any collaborator's
  review counts and both may merge.
- **36, 39** (#991, #1225): code placement rules (`app` holds the document, new
  crate dependencies need a decision) and `crates/ui/AGENTS.md`.
- **54** (#1391): the cut ends once the Pages job is green and the site shows the
  new version.

## Known issues
- The browser build is a first rung; the file picker is still owed.
- No installer is built on the cut machine; the zip is the release and the
  published artifact is built by the release workflow from the tag.

---

# CamCraft 0.18.0 (clean-room implementation)

Released 2026-11-12.

Breaking: none. The project file format stays at v58, so files saved by
0.17.0 open unchanged. **Posted
G-code does change for some machines**, so check the first program you post on
each: the Fanuc, Haas, Mazak and Doosan DNM posts now write `G21` and the
Siemens 840D post writes `G710` before any motion (#901); every mill post
writes an explicit spindle stop after a probe's tool change (#574); lathe
posts reset constant surface speed at the end of a section (#772); and the
post gate judges arcs and tool bodies more strictly in places (below), so a
program that posted before may now be refused by name.

A minor release headlined by **CamCraft in a browser** (a first web runner),
**C-axis unwind** for continuous live tooling, **default holders on machine
files**, **a large fixture and holder library** (plates, mod vises, 5-axis
vises, accessories, BT/CAT/ER/TTS holders), **theme as a user preference**,
a Trochoidal slot operation, Tormach microARC 4 and plasma posts, and
two new lathe and mill posts. Thanks to mkinney, vilts, tbrasselle, ahmetv10,
JSwetz and seelesturm for their contributions.

## This cut
The UI snapshot goldens were regenerated for this cut and accepted by the
maintainer without the usual manual look at the diff images (maintainer's
decision, 2026-11-12).

## New
- **Runs in a browser** (#31, #841, #847): the shell builds for wasm32 and runs
  on a web canvas with an awaited GPU start-up and a page built by
  `web/build-app-wasm.sh`. A first rung only, and **not part of the release
  download yet**: the release workflow does not build or attach it, only the
  manual CI workflow compiles it. The file picker is still owed.
- **C-axis unwind** (#33): continuous C-axis live tooling now unwinds C at
  clear links against the machine's stated C travel. Rotary Contour unwinds;
  Rotary Parallel and Rotary Offset refuse by name where C would need a
  rewind. Each rewind is an orbit the verifier and the post both see, and the
  lathe post caps every C block's feed by the machine's stated linear feed and
  C rate.
- **Machine default holders** (#828): a machine file can name a spindle mount
  and a default library holder (`tooling.spindle_mount`,
  `tooling.default_holder`; refused by name when unknown or mismatched). A new
  tool added in the GUI or over MCP starts with that holder in the same undo
  step. The Tormach MX mills default to BT30 ER20, the PCNC/M mills to TTS ER20.
- **Theme is your preference** (#877, vilts): the theme and colour-vision mode
  survive restarts, and New, Open and project files no longer change them. The
  light theme now has a light viewport with toolpath, overlay, view cube, hover
  and title-bar colours that read on it (#698, vilts).
- **Holder library** (#792, #793, #794, #799): BT30, BT40 and CAT40 taper
  holders in ER collet chuck and Weldon versions; standalone ER11-ER40 collet
  chucks and BT40 ER40 from one ER table, with the flange flare and Weldon
  clamp screw enveloped; Tormach TTS holders (ER20, ER16 and set-screw) with
  the collar face as the gauge line. MCP gains `set_tool_holder` and
  `list_holder_library`. A vendor tool STEP can be aligned by its ISO 13399 tool
  coordinate systems (#370, JSwetz).
- **Tool editor and simulation** (#93, #743): draw a milling tool's cutter,
  neck and shank as a spun outline (Profile tab, validated against the stated
  diameter and lengths, undoable, over MCP as `set_tool_cutter_profile` /
  `get_tool_cutter_profile`). The simulation draws the tool and holder as the
  same revolved body the collision laws judge.
- **Fixture library** (#787, #788, #789, #819, #820): parametric milling vises
  (3-8 in, 100-200 mm); 5-axis self-centering vises (52, 75, 80, 125 mm)
  centred on the stock; accessories (parallels, 1-2-3 and step blocks, strap
  clamps, soft jaws); fixture plates (12 x 12 in, 300/400 mm, 770MX table) with
  any fixture placed at a named grid hole or dowel pair; mod vises (fixed and
  adjustable jaw blocks on a plate, MCP `add_mod_vise_pair`). Each is placed in
  one undoable step and judged by the fixture mesh laws. Links and retracts
  that would strike a fixture are raised over the steel instead of only being
  reported; a clamp taller than the machine's Z stroke refuses generation by
  name (#339).
- **Trochoidal slot operation** (#220): an engagement-controlled trochoid along
  a typed centre line, with the measured contact angle held under a limit by
  adapting the step, microlift or retract returns, and named refusals. It also
  refuses by name a forward step past the loop cap, a microlift above the
  clearance plane and a bore past the helix-turn cap (#774).
- **Pocket and slot detection** (#511): the Model page detects pockets and
  slots and creates their operations with one click.
- **Posts and machines**: Tormach microARC 4 and PathPilot A indexing between
  operations on the new 770MX + microARC 4 machine (#785; simultaneous 4-axis,
  unreachable tilts and A past travel are refused by name); a PathPilot plasma
  post for the Tormach 1300PL (#671); a PathPilot lathe post (#612); a Doosan
  DNM + Fanuc post aligned with a real sample program (#614, JSwetz; see
  Known issues, #950, before using it with a 4th axis);
  documented Heidenhain drilling and tapping cycles with explicit floating
  holder intent (#51, seelesturm).
- **Plasma** (#672, #801): the through-cut laws judge every Plasma Profile at
  the post gate (cut order, cut crossing, a malformed record block the post by
  name); a slot or keyway narrower than the kerf is refused at generation; the
  laws warn on a rapid over a freed hole slug.
- **Refused generations show their moves** (#261): the moves built before a
  refusal are drawn as a dashed ghost in the refusal colour (display only;
  never cached, simulated or posted), and the script/MCP error names its move
  count.
- **UI polish** (ahmetv10): larger ribbon cells with readable labels,
  capitalised tab titles, no internal status text, an empty-viewport hint and a
  Z=0 ground grid. The Machines pane's kind dropdown filters the list (#816).
  The starter tool catalog is now a data file, `assets/tools/catalog.json`
  (#741).
- **Documentation** (#688, #291, tbrasselle): a Zola user-guide and developer
  site published to GitHub Pages at each release tag; a first-part example
  project with tutorial images.

## Safety and correctness
- **Posted programs**: the mm unit word (`G21`, `G710` on the 840D) is written
  before any motion on the Fanuc, Haas, Mazak, Doosan DNM and Sinumerik posts,
  so a control left in inch never reads an mm program as inches (#901). Every
  mill post writes an explicit spindle stop (`M5`; `MOTOFF` on Mazak) after a
  probe's tool change and length offset (#574). Lathe posts (Fanuc, PathPilot)
  no longer carry a section's G96 into a later section that states none (#772).
  The Carvera post names a per-operation smoothing ask with a one-time
  "SMOOTHING NOT WRITTEN" note instead of dropping it (#757).
- **STEP chord cap and arc fitting** (#689): a bore at a STEP hole's own size
  posts clean. STEP meshes are capped at 0.005 mm chord error, arc fitting
  keeps vertices within half the tolerance, closed rings fit whole, and the
  gouge law reads arcs at a quarter of its tolerance. A nominal arc round a
  convex wall now passes; an arc bowing past the floor between its old chord
  joints is now caught.
- **Reach and tool-body laws** (#903, #835): reach laws (c)/(c3) no longer
  excuse a leaning shank or holder standing in the part as cutter kerf. A tool
  body (imported or drawn) is judged by the same band stack the simulation
  draws, so a stepped holder whose flange stands clear is no longer warned;
  laws that still use the enclosing cylinder say so; a mid-turn swing through
  the part is no longer hidden behind a stock shank warning on the same move.
- **STEP files with empty parameter lists import** (#887, tbrasselle): files
  exported by HOOPS Exchange, anything with an empty `()`, no longer fail as
  corrupt.
- **4th-axis A tables** (#797, #804, #813): the machine check reads an A-about-X
  table's pose through the tool axis (the underside A0 B180 posts as A180 on the
  770MX + microARC 4). The program gate refuses an A index when the retracted
  tool tip cannot clear the swept stock, part and turning fixtures by 5 mm. The
  swing check also judges a work-turning A whose machine file states no axis
  vector (an A is about X), and refuses one with neither a vector nor an
  RS-274 letter. A machine whose work-turning A states no `nose_to_axis` has
  every operation posted on it refused by name at the program gate until the
  machine file states it.
- **Adaptive** (#850): the growth planner refuses by name a plan that has
  worked heavy ring geometry over a long walk, so an islanded area it cannot
  finish falls back to the named Offset rings in about 22 s instead of about a
  minute. Plans that finish today are unchanged.
- **Gouge law (b) in Boundary mode** (#905): an arc move in contour, trace and
  face is checked along its whole length against wall corners, not only at
  sample points, so a gouge between two samples is caught. Arcs in the XZ and
  YZ planes are still read at their chord joints (#946).
- **Picking over MCP** (#692): `locate_part` refuses a mate id no click could
  pick, like the other pick tools.

## Fixed
- **Fixtures**: a vise or accessory key this build cannot read is dropped with
  a load note and the project still opens; a strap clamp's stud must be
  narrower than its strap; the parallels fit marks the height it sets (#812).
  Dropped plate and mod-vise keys show in the load notes, a located plate's
  riders are clash-checked, a plate whose mesh changes drops its grid, and
  staggered plates with a clamp ear or chamfers are no longer refused as
  turned (#826).
- **Setup page** (#866, #872, #873): it names the turn that stops the stock
  carry and says what to do; the slow-frame log no longer blames time spent in
  a file dialog.
- **Plasma**: the profile refuses a zero-width slit in a picked contour instead
  of dropping it (#801).
- **Build**: the main branch compiles again after a Trochoid/Stock merge slip
  (#844).

## Changed
- The UI says "work offset (G54-G59)" plainly, and refusals say what a setup
  owns in plain words instead of "furniture" (#876, #878).
- The dev build profile is lighter and the gate can use a compile cache
  (#900); the release build is untouched.

## Known issues
- **#950 (P0)**: the new Doosan DNM post, on a 4th axis without TCP, assumes
  the table stands at A0 at program start and after every tool change. An
  operation at A0 that follows one at another angle is then posted with no A
  word and cuts at the previous angle. Until it is fixed, do not post programs
  through the Doosan DNM post that mix A0 with other A angles, or add the A0
  move by hand and check every A word after a tool change.
- **#980 (P1)**: STEP models are now meshed much finer (the chord cap of
  #689), and on some parts the Adaptive (HSM) plan of a level then crosses
  itself and that level is cut with Offset rings instead - named in the
  operation's notes, and the path still verifies clean. On the project's own
  test part one more level falls back than in 0.17.0 and the path has fewer
  arcs (222, was 600). Expect longer, less smooth roughing on imported STEP
  parts until the planner is fixed.
- **#934 (P0)**: the Sinumerik post writes work offsets 5 and 6 as G58 / G59,
  which on the 840D are programmable offsets, not settable frames. Check
  offsets 5 and 6 by hand on that post.
- **#946 (P1)**: gouge law (b) in Boundary mode still reads arcs in the XZ and
  YZ planes only at their chord joints, so a sharp wall corner between two
  joints is not judged there.
- **#864 (P1)**: an over-budget STEP mesh is imported at reduced fidelity with only a
  log line; gouge verdicts on it can pass a real gouge on a convex wall.
- **#906, #885 (P1)**: a machine that states a C travel but no C rate gets an
  uncapped rewind feed, and a combined linear + C move on the Fanuc and Doosan
  posts is posted at the mill feed without a cap from the C rate.
- **#938 (P1)**: a tool tilted by a rounding error (1e-10 degrees) is judged as
  tilted, and a holder wider than the cutter then reports a false collision.

## Process changes
Contributor and agent process only; no effect on the program.
- **PROCESS_VERSION 11** (#765): the xtask test scanner ignores braces in
  strings, chars and comments.
- **12** (#846): the `classify-effort` skill, one way to choose an issue's
  `effort:*` label.
- **13** (#855): the docs-picture goldens move to the release snapshot lane
  (xtask plan gains a step; a fork re-syncs xtask).
- **14** (#881): a contributor's claim must be live; stale after 48 hours with
  no open PR, branch commit or comment, and an ended claim is never mirrored
  back.
- **15** (#913): `cargo xtask gate` uses sccache when present; a lighter dev
  profile; agents must not export `CARGO_TARGET_DIR`.
- **16** (#944): agents prefer `cargo xtask gate`, set `RUSTC_WRAPPER=sccache`
  only when it is installed, and the release build runs without a compiler
  wrapper.

---

# CamCraft 0.17.0 (clean-room implementation)

Not a breaking release: the project file format stays at v58, so files saved
by 0.16.0 open unchanged and 0.16.0 opens 0.17.0 files. **Posted G-code does
change for existing machines**, so check the first program you post on each
machine. Every mill dialect's Face and other clearance-seeded paths now open
with an approach rapid on the clearance plane (#690, #691). Probe sections
start no spindle and descend to the safe plane before searching (#523, #570).
Lathe posts write the tool's control point for its orientation, threads cut
right-hand toward the chuck by default, and grooving and parting blades post
their touched-off corner (#629, #630, #632). Per-operation smoothing (#616) is
opt-in and changes nothing until you switch it on.

A minor release headlined by **plasma cutting** (a plasma machine kind, the
Tormach 1300PL and a Plasma Profile operation), **one picking rule for the
whole app**, new ways to align the part (three points, an edge or two
points, turning about the WCS origin), a fixture library, pocket and slot
recognition, per-operation smoothing in every mill post, and many new
machines. Thanks to mkinney, austin62jake, tbrasselle, vilts and JSwetz for
their contributions.

## New
- **Plasma** (#669, #670, #672): a `plasma` machine kind (X/Y cut, Z torch
  lifter, no spindle) and the Tormach 1300PL machine file; a Plasma Profile
  operation with a plasma torch tool (kerf = diameter): kerf offset computed
  or by G41/G42, outer contours clockwise and holes counter-clockwise
  (overridable), arc leads, the small-hole rule, and holes cut before their
  outer contour. Milling and turning operations are refused by name on a
  plasma setup.
- **One picking rule everywhere** (#692): faces, edges and points are picked
  from one feature graph - whole faces, real feature edges and corner points
  only, the visible one under the cursor first, never a tessellation edge or
  a hidden edge through the part.
- **Part alignment** on the Setups page: lay the part on a plane through three
  picked points (#555); set the in-plane axes by an edge or two points (#557, #554);
  alignment turns about the WCS origin and lays the picked face down (#606).
  The setup page previews the stock it takes over from the previous setup as a
  translucent body (#406).
- **Fixtures**: a fixture library to save, insert and delete your own fixtures
  (#339); moving a part warns when it or its stock would sink into a fixture
  (#582); a square stock stays square when the part is turned (#571).
- **Pocket and slot recognition** (#35): recognised pockets and slots turn
  into operations; an inside-out or open mesh is refused by name (#718).
- **Posting**: opt-in per-operation smoothing in every mill dialect (G64 P,
  G5.1, G187, CYCLE832, G62; #616); per-setup post options and a per-setup
  tool-list export (#389); Tormach programs can end in the operator's units
  (#596); probe programs report the measured deviation on the control (#575); Haas
  probing on an inverted X/Y axis (#392); comment text is escaped per dialect (#594).
- **Machines**: Tormach 8L, 15L and 15LX Slant-PRO lathes (#611, #619), 24R
  and xsTECH Pro routers (#620), 770M+ and 1100M+ (#618), and the Makera
  Carvera and Carvera Air with a Carvera post (#707). The Machines pane's
  "Use for this setup" assigns the selected machine (#384), and the machine overview
  is a readable two-column grid (#745).
- **MCP**: the tool table is served live from inside the GUI (#183); idle
  live-session clients are dropped after a timeout (#567); new `move_model` and
  `repair_model` tools (#716, #717).
- **UI**: recent projects and models on Open and Model (#662); a refused generation
  marks the spot it names, and drilling refusals mark their hole (#261, #677).

## Fixed
- **Safety and the post gate**: mill paths seeded on the clearance plane always
  open with an approach rapid (#690, #691, #697); a step too small to ladder is
  refused by name instead of allocated (#695); MCP `verify` no longer reports
  a job as clean while an operation's own warnings hold the post (#722); the
  program sim sweeps a tap at its thread's minor diameter, so a tap-drill-size
  hole is no longer a false gouge (#266, partial); an edge-break record is
  credited only within its operation's stated width (#529).
- **Probing**: a touch probe starts no spindle on any mill post (#523), is not
  judged against the machine's spindle range (#565), searches a short way past the
  face and feeds below the safe plane (#570); a probe-only program's stock sim
  judges the uncut stock (#592).
- **Turning**: threads cut right-hand toward the chuck by default and state
  their hand (#629); lathe posts write the tool's control point (#630);
  grooving and parting blades post their touched-off corner (#632, #641); facing
  approaches at rapid and lifts off the face before retracting; the lathe
  stroke and thread travel checks judge the posted radius (#631, #633, #647).
- **Setup page**: lay-on-plane controls stay on screen so a picked face is
  turned flat (#330); another setup's WCS triad stays on its spot (#558);
  setup picks disarm when their page is left and the pick sweep runs every
  frame (#664, #667); a hidden Properties pane drops the locate draft (#673).
- **Units**: status lines, refusals and the gate's gouge clause speak the
  document's units (#578).
- **Views**: orthographic depth maps into WebGPU's clip volume (#600), and
  orthographic picking and stock tracing cast parallel rays (#609, #610); view-cube pole
  clicks keep the roll (#622).
- **Machines**: the tool-number clash is judged per machine (#438); a saved
  machine file keeps its key order (#480).

## Changed
- **Smoothing, picking and alignment** are described above; the stock engine
  builds a block stock in closed form (faster, same result).

## Process changes
Contributor and agent process only; no effect on the program.
- **PROCESS_VERSION 1-4** (#682, #683, #685, #687): process files carry a
  version and are announced on the pinned issue #681 so forks pick them up;
  the `/resolve-conflicts` skill and the conflict rule.
- **5** (#742): MCP issues carry `area:mcp` and P1.
- **6** (#747): every issue is weighted `effort:low|medium|high|max` and
  worked by the matching `issue-worker` agent.
- **7-9** (#750, #751, #753): `cargo xtask gate|release|package` replace the
  PowerShell/bash scripts on every OS; the scripts are wrappers.
- **10** (#726): the camcraft-mcp skill's tool signatures match the server.

## Known issues
- **#738**: setup-page alignment re-applies from the import pose instead of
  composing on the current one, and picks are hard to see (fix in review,
  PR #746).
- **#757**: the Carvera post ignores per-operation smoothing without a note.
- **#74**: GPU selection can fail on hybrid Intel/NVIDIA laptops.

---

# CamCraft 0.16.0 (clean-room implementation)

BREAKING: the project file format moved from v57 to v58 (PR #381: a tool can
carry a modelled holder body). Files saved by 0.15.0 or earlier open here and
are migrated when saved, but a v58 file saved by 0.16.0 is refused by older
builds (save a copy first if you still need 0.15.0). Two post behaviours also
tighten and can refuse a program that used to post: the Tormach posts now
retract to the machine file's `safe_retract` and refuse a non-Tormach machine
that states none, the Siemens 840D post refuses any machine that states none
(#495, #503), and a probing post script must declare
`onProbeGuard` or the Probe op is refused by name (#174). The built-in VMC,
tilt-head, Lathe and Mill-turn machines now state their strokes in program
coordinates (#457, #493), so programs they used to refuse now post.

A minor release headlined by fixtures judged against their real mesh, tools
that carry modelled holder bodies, one NC file per mount for multi-setup jobs,
a Machines pane that edits machine files, and a much larger MCP surface. It
also lands a long run of sim, gouge-gate and post fixes. Thanks to mkinney,
seelesturm, austin62jake and tbrasselle for their contributions.

## New
- **One NC file per mount** (#388): setups on different machines or posts
  generate on their own machine, and the post writes one NC file per
  machine/post, each judged against its own machine; the program sim refuses
  by name to carry stock between setups on different machines (#404).
  Pause, optional stop and go-home entries can sit between operations (#155).
- **Fixtures judged against their real mesh** (#339): the box stays the broad
  phase, so a path between vise jaws no longer refuses the post; tilted,
  rotary, spindle-C and XZ/YZ-arc moves are judged too, each arc chord padded
  by its sagitta (#491), and spindle-C moves are judged at the station pose
  (#486). An upright rapid checks the whole tool stack and a ramp each band
  over its true height span (#424, #425). The fixture UX (PR #549, replacing
  #482, mkinney): mark or import a fixture from the browser, see whether each
  is judged by mesh or box and why, and see which fixture a warned move
  strikes. The Fixtures page rows wrap and Place has labelled fields (#509).
- **Tools with holder bodies**: a tool carries a modelled holder (STEP/STL/OBJ)
  placed by axis and tip, judged by its revolved envelope (#368); import,
  place and draw it in the tool editor and sim (#371); draw a milling holder
  as a revolved outline (#93). Lathe tools carry a holder or boring-bar body
  judged against the revolved stock (#369) with an optional insert zone
  (#396). Export and import a `.camtools` library (#300, PR #487 replacing
  #466).
- **MCP server**: pick faces, edge chains and loops for an operation (#180);
  `detect_holes` (#312, PR #456 replacing #314); `render_stock` PNG with rest
  and gouge volume (#182); setups, work offsets, machines and fixtures (#181);
  tool-library export/import (#355, PR #490 replacing #467); stdio hardening
  against null ids, oversize and non-UTF-8 lines (#443).
- **Machines pane** (View > Machines): browse and validate machine files
  (#287), and create, edit and save `.mch` files with per-field validation
  and atomic writes (#383). Machine files for the Tormach PCNC 770 and 1100
  (#170).
- **Probing**: seed a Probe op from a picked face, edge, vertex or round wall
  (#171); positioning moves post as protected stop-on-contact moves (#174);
  Fanuc probing on an inverted X/Y axis (#392).
- **UI**: orthographic (parallel) projection with a Projection row and bare P
  (#434, tbrasselle); warned toolpath moves are marked in the viewport and
  framed from the op page (#261, PR #454 replacing #349); a refused post's
  reason stays in a window (#265, PR #451 replacing #296); the Post dialog
  opens where the document last posted (#258, austin62jake); the G-code
  viewer lists from the left edge (#256, austin62jake); the Stock page and
  Theme button split (#441, #442); macOS Cmd+Q asks about unsaved changes
  (#144); the browser tree reads on the light theme (#506).
- **Licence**: CamCraft is now Apache-2.0 (was MIT), copyright held by its
  contributors (#452).

## Fixed
- **Post gate false gouges and misses**: XY arcs are cut on the true circle,
  not inscribed chords, in the column engine (#402), the field engine and the
  envelope scan (#413); a column within tolerance of a wall is judged against
  the floor beside it (#299); a typed Slot's round ends no longer read as a
  gouge (#510); a 2D Chamfer or Deburr is judged against its declared edge
  break (#303); the scallop check reads to the ball diameter (#212).
- **Crash**: an intermittent SIGSEGV/SIGABRT when threads created their first
  world at once is gone (#379).
- **Alignment** picks refuse by name on an inside-out, open or non-manifold
  body instead of silently flipping the part (#366).
- **Turning**: multi-pass, chamfer and taper turning exit a longer bar safely
  (#323, seelesturm); the rotary wrap turns its seam clear of a D-flat or
  keyway (#226); rotary Adaptive no longer sorts its leave-limit level by
  float noise (#55); swarf raster lines keep their lying angle on rapids
  (#48).
- **Posts**: the Fanuc probe result no longer writes a doubled sign (#377); a
  trailing Go home is written once (#504); the Tormach and Siemens 840D
  retract from the machine file (#495, #503).

## Known issues
- **#554**: face picking on the setup page keeps snapping to an edge.
- **#330**: Align by picking a face or two edges moves the part but does not
  fully rotate it onto the chosen G17/G18/G19 plane.
- **#558**: another setup's WCS can be drawn in the wrong place when the part
  is aligned differently.
- **#523**: posts start the spindle (S0 M3) on a probe tool.
- **#74**: GPU selection can fail on hybrid Intel/NVIDIA laptops.

---
# CamCraft 0.15.0 (clean-room implementation)

BREAKING: the project file format moved from v50 to v57, and two of those
steps change what a project means. Files saved by 0.14.0 or earlier are
migrated when opened, but a file saved by 0.15.0 is refused by older builds
(save a copy first if you still need 0.14.0).
- **One WCS per setup** (#342): a setup now owns a single work coordinate
  system, and G54-G59 is only the label the post writes for it. Placed slots
  that no setup posted are dropped on load, each with a load note naming the
  entity and where it sat; single-WCS projects post byte for byte as before.
  The `SetWcsOrigin` command lost its slot argument.
- **The setup defines the work offset, not each height** (#329): a height
  measured from the work offset reads its setup's offset, and the Heights
  page no longer offers a per-height WCS choice. A project from 0.14.0 or
  earlier whose operations used different G54-G59 slots inside one setup is
  refused at load by name (open it in 0.14.0, make the slots agree, save,
  then open it here).

A minor release headlined by multi-setup machining: a setup tree in the
browser, setups placed relative to one another, the stock carried from one
setup into the next (including rest machining, verification and fixtures),
plus inverted machine axes. Thanks to austin62jake, bherbruck, linuxguy123,
mkinney and seelesturm for their contributions.

## New
- **Setups as a tree** (#343): the browser shows each setup as a node with an
  active-setup radio and its own WCS, Stock, Machine, Post, Fixtures, Tool
  list and Operations; a setup can have its own machine and post.
- **Part alignment per setup** (#326, #330): hold the part flipped or turned
  (presets or typed angles) with the stock following square to the setup
  frame, or lay it on a G17/G18/G19 plane by picking a flat face or two
  straight edges. Face, 2D Contour, 2D Pocket and 2D Adaptive generate and
  post in an aligned setup; every other kind is refused by name.
- **Place a setup relative to another** (#316): same way up, flipped, turned
  or by angles; changing the previous setup's alignment carries into the
  linked setups, and the transform between two setups is derived from their
  alignments.
- **Stock carried between setups**: the program sim carries the machined body
  through the setup transform (#317); rest machining in a later setup reads
  it, and an upstream change marks the later setup Stale (#318); rapids,
  entries and links are judged against it (#319); a setup after the first can
  take its own stock or what the setup before leaves behind, and the setup
  page shows the incoming stock (#320); fixtures read the carried stock, a
  lathe or mill-turn part flip is refused by name, and the carried body is
  cached (#321).
- **Pick the work offset a setup posts at** (#196) on the Setups page; the
  setup sheet names it and conflicting picks are refused by name.
- **Inverted machine axes**: the machine file states a per-axis direction
  (#373); the Fanuc and Haas posts write the program mirrored (#374: signs,
  I/J/K, G2/G3, G41/G42, work-offset origin; an inverted Z, helical arcs,
  threading, rotary, live tooling, probing and every other post are refused by
  name); the axis is shown on the viewport triad, the setup sheet and the NC
  header (#375).
- **Pause or home after an operation, suppress an operation, "Skip pauses
  and homes" post option and one NC file per operation** (#110, #111).
- **G41/G42 cutter compensation on Haas, Mazak and Siemens 840D** (#39).
- **Continuous C-axis live tooling** (#33): rotary kinds on a mill-turn's
  spindle state the C angle per move and post through the Fanuc lathe post.
- **Comments in posted programs** naming the setup, each operation and each
  tool, in every dialect's own syntax (#257, mkinney).
- **Machine files for the other Tormach mills** (#170, mkinney): 770M, 1100MX,
  1500MX and PCNC 440; a .mch can state spindle belt ranges and rigid-tap
  ability.
- **Lathe parts with a large keyway or flat** turn instead of being refused
  (#113, mkinney); grooving, parting and form tools are cut with their true
  blade profile in the lathe stock sim (#21).
- **Probe**: the stylus stem and body are judged against the part (#172,
  mkinney).
- **Stock view and sim**: dual contouring for the machined-stock mesh keeps a
  pocket's sharp edges (#23); the mesh paints deviation classes and lights the
  hovered facet (#25); the hidden-rest scan answers past its old budget (#24).
- **UI**: the Stock page shows stock only and the theme and colour-vision
  picks have their own Theme ribbon button (#441); build version, time and
  commit show in the toolbar and a Debug info window with a Copy button opens
  from the status strip (#352, linuxguy123; #157, bherbruck); spindle speed is
  editable as rpm or surface speed (#225, bherbruck); a refusal on the status
  strip is drawn as an alert (#192, bherbruck); ribbon section and machine
  doors wear glyphs (#207, bherbruck); a new document can reach the Setups page
  to create its first setup (#322).
- **Logging**: the launching terminal logs save, open, post, generation and
  commands (#163, bherbruck; CAMCRAFT_LOG / RUST_LOG).
- **Operation tops below the remaining stock are explained** during upright
  2D/3D generation (#104, seelesturm).
- **Packaging and tooling**: Arch and FreeBSD package targets (#72,
  linuxguy123); the Windows installer's embedded uninstaller is signed when a
  certificate is configured (#29); headless dev binaries per crate (#32); the
  kernel seam gains union/intersection and edge blends, benched only (#27).

## Fixed
- **Cutter compensation** is engaged in G17, never in a stale G18/G19, on the
  Fanuc, Haas, Mazak and Siemens 840D posts (#391).
- **Work offsets**: a rotated work offset (3-2-1 alignment) is refused by name
  instead of posting unrotated (#315); a non-finite or zero-length rotation
  or a non-finite origin is refused by name (#327, mkinney).
- **Turning**: a pass that ends inside a longer bar leaves its cut at feed
  (#229, austin62jake); a plain two-ring bar fits its true axis and wraps
  (#228, austin62jake).
- **Deburr** leaves each edge run straight up before travelling (#304,
  austin62jake).
- **STEP import** reads the file's length unit (#306, austin62jake).
- **CL text dump** keeps a C-axis path's spindle angle (#295, austin62jake).
- **Facing** tells concave returns from narrow gaps between separate holes
  (#260, seelesturm); cross-drill topology is stable under roundoff (#151,
  seelesturm).
- A fresh depth operation bottoms out at the part's floor, not the G54 plane
  (#214, mkinney).
- A loaded stock keeps its geometry revision, so a project resaves
  byte-identically (#340, mkinney).
- A ball nose's corner radius is its ball, shown and saved as half the
  diameter (#263, austin62jake); Fit to Models' cylinder stock is snug (#230,
  bherbruck).

## Known issues
- **#424 and #425 (P0, fixture law)**: an upright rapid checks only the flute
  against fixtures, so the shank, holder and spindle housing can hit a clamp
  unreported; and a ramp is judged at its lowest z, so the top of the tool
  stack at the high end can hit a clamp unreported. Both predate 0.15.0 and
  are being worked. Until they land, check clamp clearance for holder and
  spindle by hand.

---
# CamCraft 0.14.0 (clean-room implementation)

A minor release: a Probe operation, per-operation coolant, G76 threading,
wear-mode cutter compensation, a Tormach post, 3-2-1 work-offset alignment,
an MCP server for AI agents, and a long run of rapid, drilling and lathe
safety fixes. Thanks to bherbruck, mkinney and seelesturm for their
contributions.

## New
- **Probe operation** (#108): face, edge, outside-corner and bore/boss-centre
  finding with a touch probe. The Fanuc post writes G31 skip touches with a
  no-contact alarm, can set G54-G59 or stop for the operator, and stops when a
  result is out of tolerance. Posts that cannot probe refuse it by name.
- **Coolant per operation** (#112): Off, Flood (default), Mist or Through
  tool. Every post writes the machine's own code, with M9 before each tool
  change and at program end. A coolant the machine has no code for blocks the
  post by name.
- **G76 threading** (#34): the Fanuc Lathe post can write a thread as a
  two-block G76 cycle (post option "Threading", default G32 passes).
- **Wear-mode cutter compensation** (#38): the contour's tool-centre path
  posts under G41/G42 Dn / G40, with the register holding only the wear delta
  (Fanuc; other posts refuse by name).
- **Tormach PathPilot post and 770MX machine** (#131, mkinney). A machine
  file in inches now posts inch (G20) programs through posts that read the
  machine's units.
- **Controller tool numbers** (#221, mkinney): a tool can carry its own
  T/H number; two tools posting one number are refused.
- **Align a work offset 3-2-1** (#187, bherbruck) from a picked face, edge
  and vertex or bore.
- **Box stock per-side allowance** (#198, bherbruck), and the Stock page
  shows the models' size and extents (bherbruck).
- **`camcraft-cli mcp`** (#128): a stdio MCP server that lets an AI agent
  open or import a model, set stock and tools, edit operations, generate,
  verify and post - every change undoable.
- **Unsaved changes** (#107): the title shows `*`, and closing, Ctrl+Q, New
  or Open over unsaved work asks Save / Don't Save / Cancel.
- **3D finish scallop ceiling** (#37): a stepover at or past the ball
  diameter, or a tolerance at or past half the ball radius, is refused by
  name.
- **UI**: Create cells darken without stock and open the Stock page (#216);
  a chunkier, readable origin triad (#195); pinning ribbon cells is undoable
  (#143, mkinney).
- **Stock view**: the rotary simulation states (and can enforce) its
  flute-rotation bound (#20); shading reads a true distance gradient (#22).

## Fixed
- **Rapids are judged more strictly**: against the tool's whole footprint
  (#106), along the whole length of sideways rapids (#140), and for turning
  in the meridian plane against the revolved bar (#103). Any rapid into
  uncut stock blocks the post.
- **Ops go Stale when what they read changes**: a stock edit (#156) or an
  earlier op of the setup changing (#109) marks dependent ops Stale, and the
  post refuses them until they are regenerated; one Regenerate All brings a
  job current. Saved projects may show some ops Stale once after upgrading.
- **Drilling** reads the stock earlier operations left (#89). A cross drill
  is judged against the finished part (#117) and enters box stock at the face
  (#85). It no longer gets the axial-hole check on a lathe (#80), reads its
  holder per station (#119) and unwelded STL rounds at their true surface
  (#121), and hops clear between off-centre holes (bherbruck, #81).
- **Lathe**: turning ops approach from, and rough down from, a bar wider
  than the part (#130). A boring bar's body is checked on its cutting feeds
  (#146). Facing faces the stock allowance above a finished part (#122,
  bherbruck).
- **Axis fit**: it no longer leans on one cross hole (#116), reads short bars
  from their facing flats (#152), and refuses normals that only meet at one
  point (#208). Fine-meshed balls (#237) and D-flat bars (#246) read
  correctly again (bherbruck). The rotary wrap names a seam feature or skin
  hole instead of "the mesh is open" (#54).
- New operations post after their setup's existing ones, not in name order
  (#167, bherbruck).
- Project files round-trip every float exactly (#169).
- The Heidenhain DIN/ISO post declares the TNC's `.I` extension (#52,
  mkinney).
- Idle frames no longer redraw, resizing stays responsive, and rotated face
  and edge picks no longer freeze (#164).

## Changed
- With a single part (or its named source model), moving or rotating the
  part now carries a non-model stock along in the same undo step (#164).

Known limits: the probe op is not yet created from a picked face, edge or
cylinder (#171); on CPU-rendered stock previews a paused scrub may stay stale
until the mouse moves.

---

# CamCraft 0.13.0 (clean-room implementation)

A minor release: CamCraft now ships for Linux and macOS as well as Windows,
three collision checks are tighter (issues #84, #91, #96), and a keyed shaft
is no longer refused on the lathe (issue #71).

## New
- **Packages for Linux and macOS** (contributed by linuxguy123): macOS ZIP
  and DMG, AppImage, Snap, Flatpak, and Linux x86_64 TAR.GZ, DEB and RPM,
  next to the Windows ZIP and installer; an Arch Linux source package
  (PKGBUILD) is in `aur/`. This is the first release these packages are
  built for on GitHub - the macOS, Snap and Flatpak builds have not run on a
  real runner before, so report anything that does not install or start. The
  macOS build is not notarized yet - Gatekeeper may call a downloaded DMG
  damaged until its quarantine flag is cleared.

## Fixed
- **Drilling checks the tool can reach** (#84): a hole deeper from the stock
  surface than the drill's flute is refused by name (a bore or tap in an
  existing hole only needs a shank that fits it), and the holder is proved
  clear of the stock (including the cross drill's swing over the end face).
- **A drill R plane below the stock or part top is refused** (#91): the rapid
  down to R used to cut through material and post silently.
- **A rapid straight down into uncut stock blocks the post** (#96), for every
  milling and drilling operation - unless an earlier operation of the setup
  already removed that stock. Turning rapids along the spindle axis keep their
  old reading for now (#103).
- **Keyed and flatted shafts turn** (#71): the lathe axis is read on the
  turned surface, so a correctly placed keyed shaft is no longer refused; a
  note names the share of the surface (keyways, flats, holes) left for a later
  setup.
- **The AppImage starts** (linuxguy123).

Known limits: with stock declared, a hole on an already-machined pocket floor,
or face-then-drill, may be refused by the drilling checks until they learn what
earlier operations removed (#89); a large keyway is still refused as "not
revolved" (#110).

---

# CamCraft 0.12.1 (clean-room implementation)

A patch release: four fixes, one of them a collision (issues #77, #79, #67,
#74).

## Fixed
- **Cross drilling no longer rapids through uncut stock** (#77). On raw stock
  wider than the finished part, a mill-turn cross drill used to hop between
  stations inside the stock at rapid feed, and nothing caught it. Hops and
  approaches now stand off the raw stock (and a shank wider than the drill),
  hole mouths start at the raw surface, every rapid is proved clear or the job
  is refused by name, and resizing the stock marks the drilling stale.
- **The post check now covers cross-drill rapids** (#79). The rapid-in-stock
  check that blocks a post was switched off for radial programs; it now
  judges them against the raw stock (retracts up a drilled hole stay clean).
- **Cross drilling measures about the machine spindle** (#67). On a bar off
  the world origin under a moved work offset, clearance and hops were measured
  about the wrong axis. A mill-turn bar off its work offset, a hole plunging
  away from the spindle, and on a lathe a part that cannot be read as turned
  are refused by name.
- **GPU startup on dual-GPU laptops** (#74, in progress). Every adapter is
  listed and tried in turn (your `--gpu <name>` choice, then the
  high-performance GPU, discrete before integrated, DX12 before Vulkan on
  Windows); each must draw a test frame, and a failure moves on to the next.
  If none works, a dialog names every adapter tried. Frames over 100 ms are
  logged by phase - please send those log lines if you see lag.

---

# CamCraft 0.12.0 (clean-room implementation)

A minor release, breaking first: Heidenhain NC output and the project file
format change (issues #50, #45, #62, #9, #19).

## Breaking
- **Heidenhain DIN/ISO programs take the TNC's own block form** (#50):
  numbered blocks ending `*`, signed words, the `%NAME G71` frame; drilling
  posts as plain moves with the dwell at the bottom, because G73/G80/G54/G55
  mean something else on a TNC. Tapping and any work offset but the first are
  refused by name on this post until the native cycles land (#51).
- **Project files are v50** (#45 part, contributed by linuxguy123): older
  builds refuse them; v49 files load and migrate.

## New
- **Project and operation workflow** (#45 part, PR #58): Save As and kept
  filenames; rename, reorder, copy and paste operations, with playback in
  their order; browser collapse and viewport visibility controls.
- **Adaptive on the rotary wrap** (#9): each developed level clears on the
  adaptive planner under the load and kink laws; a level it refuses is named
  by its radius and cut by Offset rings.
- **Turning programs in the stock simulation** (#19): facing, profile,
  contour and boring simulate about the part's spindle; blade, thread and form
  tools are refused by name until their sweep lands (#21).

## Fixed
- **A turning part whose axis is not the machine spindle is refused** (#62):
  a part leaning even slightly, or off the work offset, used to post a taper
  or an eccentric with no warning. It is now refused by name at generation
  and again at posting (so moving the work offset afterwards is caught), to
  within 0.0005 mm over the whole part.

---

# CamCraft 0.11.0 (clean-room implementation)

A minor release, breaking first: Heidenhain NC output changes for every XY
arc (issues #49, #17, #18).

## Breaking
- **Heidenhain DIN/ISO arcs post their true centre** (#49). The TNC reads
  I/J as the absolute circle centre, entered in its own block before the
  G02/G03 (TNC 640 ISO programming manual). The post wrote the centre
  relative to the arc start, so a real control would cut around the wrong
  centre. Every Heidenhain XY arc's output changes; an arc without a centre
  is now refused by name.

## Fixed
- **Swarf links half as often and never tilts under level** (#17): about
  half the air links on the blade, the lowest pose level instead of 7-9
  degrees under, and a ball tool's re-entry into the cut it just left is no
  longer refused as a plunge.

## Decided
- **Heidenhain vertical arcs and oblique entries stay chord fans** (#18):
  the manuals give no rotation sense for off-plane arcs, so a native arc
  would be a guess; the fan is exact to the output resolution.

---

# CamCraft 0.10.0 (clean-room implementation)

A minor release: one new capability and two fixes (issues #15, #14, #16).

## New
- **Rotary Parallel and Swarf ramp in on their first entry** (#15). Where a
  straight plunge is refused, the first infeed now comes in on a sloped ramp,
  each one proved clear of the part and the stock before it is kept.

## Fixed
- **A lead-in outside a round bore is seated inside it** (#14): one descent
  per level instead of a plunge into material beside the bore.
- **Rotary Offset is judged against the real round bar** (#16). Its plunge
  check now uses the true stock cylinder about the rotary axis instead of a
  box around it, so moves in the air beside the bar are no longer refused.

---

# CamCraft 0.9.0 (clean-room implementation)

A minor release: one new capability and one measurement fix (issues #11, #10).

## New
- **Adaptive arc output covers the growing spiral** (#11). Where a single
  arc could not follow the spiral's changing curvature, the path is now
  fitted with pairs of tangent arcs (biarcs), each proved inside the area,
  crossing-free and kink-free - about a third fewer moves: rectangle
  18,372 → 12,730, L-pocket 51,788 → 34,582, slot 23,382 → 14,748.

## Fixed
- **The load reader reads chord paths at their true engagement** (#10): it
  faces the path's real direction at each corner, not the lagging chord, so
  readings on straight-segment paths dropped (e.g. the L-pocket from 22.2 %
  to 21.4 % of the tool diameter at the coarse grid).

---

# CamCraft 0.8.4 (clean-room implementation)

A patch release: three fixes (issues #13, #36, #46).

## Fixed
- **2D Adaptive no longer hangs on a long thin band** (#13). A channel too
  long and narrow for the adaptive spiral is refused in about a second with
  its size named, and Offset rings cut it - instead of planning for minutes.
- **Finishing passes ride over mesh folds** (#36). On coarse meshes the
  rows no longer skip a strip at every fold, so the scallop stays within
  tolerance on curved parts.
- **Raster passes reach the rim** (#46). Each pass ends with a row where its
  ground ends, so tilted and flat shelves are finished to the edge.

## Also in this build
- The 2D Adaptive trochoid fallback is wired but still refused by name in
  every case (its landings are #47); output is unchanged.

---

# CamCraft 0.8.3 (clean-room implementation)

A patch release: four fixes, three of them safety (issues #12, #42, #43, #40).

## Fixed
- **A whole-part Pocket or 2D Adaptive no longer cuts into the part** (#12).
  With the whole model as the source, it now clears only the stock around
  the part; if the part fills the stock outline, it refuses and says so.
- **The verifier catches a straight cut across a part's corner** (#42).
  Pocket-type operations are now checked along every move, not only at its
  end - a 0.3 mm corner intrusion reads as a gouge and blocks the post.
- **G41/G42 compensation refuses contours the controller cannot offset**
  (#40): inside fillets tighter than the tool, edges too short for the
  offset, and necks narrower than the tool - each named with its position.

- **Contour moves are checked against the part's walls along their whole
  length** (#43), so a straight cut across a convex corner of the wall is a
  gouge; a Face's rows crossing its open rim stay legal.
- The pocket check from #42 now judges only the stretch of a move below the
  stock top: a ramp crossing the wall line in the air is not a gouge.

---

# CamCraft 0.8.2 (clean-room implementation)

A bug-fix release from hand-testing 0.8.0/0.8.1.

## Fixed
- **Closing an edge loop.** In Edges mode, Shift+click takes the whole loop
  an edge lies on, and Ctrl+click adds or removes one edge. The Selection
  group shows the hint, and the "open run" error says how to close the loop.
- **The Bottom height after picking a face.** Switching Bottom's *From* to
  Selection kept the previous reference's offset (e.g. +18.7), which put the
  floor above the picked face. It now starts at offset 0, so the floor is the
  face.
- **Containment Outside on 2D Adaptive/Pocket.** It no longer machines a
  recess inside the part: only the outer edge is grown, holes stay protected,
  and pockets the band past the outline does not reach are left alone. With
  Outside alone the band is exactly one tool wide, so 2D Adaptive refuses it
  by name ("no room for an entry") - add a Boundary extension.
- **Circular follows the selection.** Pick a bore rim, a round pocket floor,
  or a face around one round hole, and the rings are centred and sized from
  it (shown as "From selection: Ø… at (x, y)"). A selection that is not round
  is refused with where it deviates; with no selection the typed centre and
  radius work as before.

## Known
- 2D Adaptive on a wide Outside band can be slow to plan (minutes); 2D
  Offset plans the same area quickly.

---

# CamCraft 0.8.1 (clean-room implementation)

A packaging release on top of 0.8.0 - the program is unchanged.

- The repository and the release now carry the project's MIT `LICENSE`.
- New issue templates on GitHub: **Toolpath / path planning bug** and
  **UI / UX bug**.
- The repository no longer carries local tooling, agent settings or
  research material.

The 0.8.0 notes below still describe everything in this build. Fixes for
the bugs reported against 0.8.0 (edge-loop picking, face heights and
Outside containment in 2D Adaptive, Circular following the selection) are
in progress for the next release.

---

# CamCraft 0.8.0 (clean-room implementation)

Eighth public release, cut early (2026-11-06) to park a stable build. It
carries what was planned for three trains (0.8.0, 0.9.0 and most of 0.10.0),
every piece through the full gate, heavy lanes included. Everything a
generator cannot prove safe is still refused by name, never posted.

This is an independent, clean-room reimplementation of user-visible HSMWorks
behavior, specced from published documentation and general machining knowledge.
"HSMWorks" is a trademark of Autodesk; this project is unaffiliated (see
AGENTS.md). Third-party notices: `assets/ui/LICENSES.md`.

Please start with `TESTING.md`: a short hand-test list for what changed.

## What's new since 0.7.0

### Machining
- **G41/G42 cutter compensation** on Contour: *Compensation: In control* emits
  the part contour with `G41/G42 D<tool>` / `G40` through the Fanuc post. It
  needs a straight lead of at least the tool radius; anything else is refused
  by name, and posts that cannot emit it refuse instead of writing a centre path.
- **Arc output for adaptive clearing**: G2/G3 over the HSM passes, each arc
  proven inside the area, crossing-free and kink-free (a bore went from 6,650
  to 1,200 moves).
- **3D Pocket uses the HSM planner** per level (*Strategy: Adaptive*); a level
  it cannot plan is cut with offset rings and the summary names its Z.
- **Facing goes around holes** (*Holes: Lift*, the default); *Cut across air*
  and *Through* (the old behaviour) are choices.
- **Contours across two models**: picked edges that meet end to end across
  bodies become one ring, and the depth ladder cuts every level of the union.
- **Waterline stepdown reads the slope**, and the finishing march no longer
  drops rows across mesh fold gaps - most scallop readings are now inside the
  tolerance.
- **Boundaries everywhere they apply**: on 3+2-indexed operations (read in the
  tilted plane), on every 3D finish, and on Trace/Engrave; a clipped finish
  ramps back in.
- **Lathe**: axial drilling on the spindle axis, and the first lathe post
  (`Fanuc Lathe`: diameter X, G96/G97/G50, G99, G83/G84/G85, G32). The Setup
  pane offers the matching post when you mount a lathe; it never switches it
  for you. C-axis live tooling is not in this release (refused by name).

### Safety
- **5-axis swarf** no longer drags the tool's flank through the part on its
  in-air links (it retracts along the surface normal) and never falls back to
  an upright pose inside a wall.
- **Fixtures are checked along the whole move**, not just at its ends, for
  every pose change.
- **Scallop is reported** (a non-blocking note) where a finish leaves more
  than its tolerance.

### Simulation and display
- **The machined stock** is drawn in the viewport and can be picked; rotary
  programs now show their stock too.
- **Playback jumps** no longer stall, and the CPU fallback renders off the UI
  thread.

### Models and output
- **Repair mesh** (Model page): weld vertices and fill planar holes, one undo
  step, never automatic.
- **The setup sheet** says whether each fixture is fixed or turns with the part.
- **Windows installer** (preview) beside the zip, and an optional signing step;
  this build is unsigned.

### Known limits
- C-axis live tooling, G41/G42 wear mode, and comp on posts other than Fanuc.
- Lathe operations are not shown in the field stock simulation yet (refused
  by name; the turning verdict at generation still applies).
- The adaptive planner still falls back to offset rings on island pockets.

---

# CamCraft 0.7.0 (clean-room implementation)

Seventh public release: the user-review release. Most of what is new came
straight from hand-testing 0.6.0:
- the operation dialog now reads like a CAM dialog;
- simulation playback no longer locks up;
- lead in / lead out is complete;
- adaptive clearing is a real HSM spiral;
- every boundary says what it does.

Under it, a run of safety work: wherever the verifier had a blind spot, it was
closed. Where a plan could not be made safe, it is refused by name instead of
posted.

This is an independent, clean-room reimplementation of user-visible HSMWorks
behavior, specced from published documentation and general machining knowledge.
"HSMWorks" is a trademark of Autodesk; this project is unaffiliated (see
AGENTS.md). Third-party notices: `assets/ui/LICENSES.md`.

Please start with `TESTING.md`: a short hand-test list for what changed.

## What's new since 0.6.0

### From your review
- **The operation dialog, Fusion-style.**
  - Label and value share a row, units sit beside the value, and fields are
    grouped in collapsible sections.
  - Only the fields the current choice uses are shown.
  - "0 = auto" fields became explicit Auto toggles, and help moved to tooltips.
  - A one-line status banner sits at the top; a fixed action bar at the bottom
    has Generate as the primary action and Delete set apart.
  - The tabs stay.
- **The boundary says what it does.**
  - One plain sentence per setting, a "Machine the other side" button, and a
    notice when a setting has no effect on the kind.
  - The viewport fills the area actually swept; open edges are dashed.
  - Contour now rides an open boundary as one open pass.
- **Simulation that keeps up.** Playback runs on a worker thread: a UI frame
  went from 182 ms to 0.13 ms, and the stock follows the cutter. Each picture
  uploads only the changed bricks (2 MB to 57 KB).
- **Source is the stock or a part.** G54, setups, tools and fixtures are no
  longer offered, and are refused if a file names them.
- **Pick a vertex, set the origin there.** A new "Vertices" pick mode; "Set
  origin" on a picked vertex or face (centroid, or Z only on a horizontal
  face). It is undoable.
- **Panels always come back.** There is a permanent viewport, a Panels… menu
  and Reset layout. Operation-page errors clear, and Clear toolpath removes the
  path, not the operation.

### Machining
- **HSM adaptive.**
  - A constant-load spiral with no kinks past 5° and one entry per level.
  - The optimal load is set in % of D.
  - Narrow slots join their pieces with lifted returns (floor air 11 m to
    0.9 m).
  - A plan that can't be made sane falls back to offset rings by name.
- **Lead in / lead out, complete.** Ramp angle and length, a continuous helix,
  and arc radius and sweep, all in one tab.
  - Every entry has extensions, entry feeds and an engagement limit.
  - Heights can be measured from the selection.
  - A plunge height of 2 mm applies.
- **Plane-aware transitions.** Arcs carry their plane: G17/G18/G19 in the CL
  and every post. Arc entries post as true vertical arcs.
- **Keep-down links** and plunge, ramp, helix or arc level transitions on every
  ladder operation; how levels link is your choice.
- **New entries.**
  - A horizontal (G17) arc entry for pockets.
  - Face entries run out to air for tools that are not centre-cutting.
  - 3D ladders slow only the first diameter of cut.
- **Area control.** Inside, on or outside containment, boundary offset and
  extension, extra include/exclude boundaries, and open boundaries that start
  outside the stock.
- **Smoother paths.** Tangent arc blends at corners and links (line-arc and
  arc-arc too), and facing reversals become single arcs.
- **More finishing.** Flowline, steep-and-shallow, flat area, pencil and corner,
  ramp, morphed spiral, geodesic, blend, ruled loft, project drape, deburr, 3D
  offset shells.
- **2D additions.** Engrave, chamfer, circular, standalone bore, helical thread.
- **Rotary and 5-axis.**
  - Posed ball tools post the tip, so they land on plan (they were 4.2 mm off
    on 5-axis).
  - Rotary programs open on the clearance pose, and Rotary Parallel links run
    radially.
- **Machine limits and cutting data.** Spindle speed and power are checked, and
  speeds and feeds are suggested per material.

### Safety (the Prime Directive)
- The verifier judges the tool against the part's real solid, with the tool's
  real tip. A program it can't judge says so.
- Closed blind spots:
  - stock under overhangs;
  - a positive stock-to-leave that hid a gouge of that depth;
  - cuts that only happen later in the program;
  - tilted holders sweeping through a chuck;
  - the spindle on tilted rapids.
- Tools that are not centre-cutting never plunge into solid, anywhere: 2D, 3D,
  drilling, Face and every multiaxis kind. They ramp, helix, or are refused by
  name.
- No level under a blind floor; 2D pockets and contours see what hangs above
  them.

## Projects
- The project format goes from v42 to v49. Older projects load. New fields are
  written only when used, so a re-saved 0.6.0 project only grows where you
  change something.
- The persistence of picks, open boundaries, vertex picks, fixed fixtures and
  the pocket arc entry is round-trip tested.

## Known limits (tracked in ROADMAP.md)
- Adaptive island pockets fall back to offset rings, by name.
- Rotary Parallel's radial infeeds and Swarf's first raster entry always plunge.
  With a tool that is not centre-cutting they are refused rather than ramped.
- The fixed-fixture flag has no UI toggle yet, and the generate-time fixture
  check still uses the stricter older law; the post gate applies the new one.
- Building is Windows-first. Linux and macOS build from the same source (M4.2);
  code signing and an installer are still owed (M13.3).
