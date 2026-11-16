# Tormach microARC 4 kit: `microarc_test_part.nc`

**Machine:** Tormach 770MX (PathPilot v2.7.2 or later) with the microARC 4 as the A axis (Settings > 4th Axis Rotary > "4" 4th Axis").
**Tests:** issue #785. The PathPilot post indexes A between operations: Z goes straight up (`G53 G0 Z0`), then A turns on its own line (`G0 A..`), then an ordinary 3-axis cut. No A word ever rides a cutting move.
**Built by CamCraft:** `microarc_test_part.camproj` is the project, and `microarc_test_part.nc` is exactly what it posts (pinned by `crates/app/tests/microarc_test_part.rs`).

## The part
3/4" round bar, 6061 aluminium or 360 brass. Every cut is shallow, and the whole program runs in a few minutes, mostly tool changes and indexes.

| Op | Index | Tool | Cut |
|---|---|---|---|
| FLAT A-90 | `G0 A-90` | T1, 1/4" flat end mill | a flat 0.015" deep, one 1/4" pass from X-0.125 to X-0.875 (the tool centre runs X-0.25 to X-0.75), on the side that faced the back (+Y) |
| FLAT A90 | `G0 A90` | T1 | the same flat on the side that faced you (front, -Y), opposite the first |
| CROSS SLOT A0 | `G0 A0` | T2, 1/8" flat end mill | a 1/8" groove across the top at X-0.5, 0.020" deep in two 0.010" steps (it runs out both sides of the bar) |

Speeds are 6000 rpm (T1) and 8000 rpm (T2), both in the high belt range (the program says so before each spindle start). Feeds are 15 ipm and 8 ipm, with flood coolant (M8). The program doesn't number your tools for you: set T1 and T2 in the tool table, or edit the `T.. M6` / `G43 H..` lines to your own numbers (for example your T31 1/4" 3-flute).

**The continuous op is not in this program.** A wrapped pass around the diameter turns A during the cut. PathPilot's handling of that (rollover, combined feed) isn't confirmed yet, so CamCraft refuses it, naming the operation, and writes no file. (The PathPilot post refuses it as "simultaneous 4-axis motion ... not confirmed on PathPilot". The program check passes it: since #797 it reads the pass under the bar, A0 B180, as the A180 this table reaches.) Its test joins this kit with the follow-up, #796.

## The setup
- **Stock:** 3/4" bar, at least 2.5" long, in the microARC chuck with **1.5" sticking out** (1.25" minimum: the cuts reach X-0.875).
- **Units:** inch (`G20`).
- **Work offset G54:**
  - **X0 at the free end face** of the bar.
  - **Y0 and Z0 on the A axis centerline.** Z0 is the centre of the bar, not its top: the top of the bar is Z0.375. Find Y0 by indicating both sides of the bar, and Z0 by touching the top and subtracting 0.375". The program's turns are only right if Y0 Z0 lie exactly on the centerline. The program says so in a comment at the first index.
- **A0:** mark a line on the top of the bar at A0. The cross slot lands on that line, and the flats land 90 degrees either side of it.
- **No clamp code:** the microARC 4 has no clamp, and the program writes none.

## Safety first
1. **Check the A direction before anything else.** In MDI, with the tool well clear, run `G0 A90` from A0. The side of the bar that faced you (the front) must come to the **top**. If the back comes up instead, **stop**: the direction sense is reversed for this post, so tell us on the issue and don't run the program.
2. **Check the collet nut clears the chuck.** At X-0.875 (the inner end of the flats) the ER20 nut must clear the chuck jaws with the tool at Z0.36. Keep at least 1" of tool stick-out from the nut.
3. **Check the swept radius at the safe Z.** Before every `G0 A..` the program lifts Z to the top of travel (`G53 G0 Z0`), and that is the only clearance it gives. CamCraft's program gate (#804) checks the tool tip there against the radius the stock, the part and any fixture modelled as turning with them sweep, with a 5 mm margin, from the machine file's 10.6" nose-to-centerline figure (Tormach's 13.75" nose-to-table less the microARC's 3.15" centre height). This kit doesn't model the chuck or its jaws, and the figure is the spec sheet's, not your machine's. Before the first A move, jog Z to the top (`G53 G0 Z0`) and confirm the tool tip (and the collet nut) clears the radius the part, chuck and jaws sweep as A turns a full revolution. On this 3/4" bar it does with room to spare; a bigger part, a bigger chuck or a long tool may not. If it doesn't clear, don't run the program.
4. **Know your G30 position's A.** Every tool change starts with `G30` (the program end does not: it stops at the top of Z, #1594). A `G30` with no axis words takes **every** axis to the stored G30 position, A included, so A may turn there, together with X and Y (Z is already at the top of travel when it starts). The program never trusts A after a `G30`: every operation states its own `G0 A..` again after its `T.. M6`. Check the A your G30 position holds (set it with A0 if you can), and make sure the tool clears the part there too. **Keep the stored G30 Z at the top of travel.** The swing clearance in step 3, and CamCraft's check of it, assume Z stays at the top while a `G30` turns A; a G30 Z below the top brings the tool tip closer to the part as A turns.
5. **Air-cut first.** Shift G54 Z up 1" (so Z0 sits 1" above the centerline), and run at 25% feed and rapid override with single-block on. Watch every `G0 A..` line: Z must already be at the top (`G53 G0 Z0` on the line before), and A must turn only on its own line.
6. Only then cut, aluminium or brass, with a hand on feed hold and E-stop. This is a test program, not a production part.

## Checklist
1. The program loads in PathPilot with no error. If it doesn't, send the line and the message.
2. Before the first index there are two comments (WCS on the A centerline, no clamp code), and then `G53 G0 Z0`, `G0 A-90`.
3. Every `G0 A..` line is preceded by `G53 G0 Z0`, and A turns with Z at the top. Every operation has its own `G0 A..` after its `T.. M6`, even when the A is the same as the last one (after the `G30`, A is wherever the G30 position put it).
4. **FLAT A-90:** the flat is cut on the side that faced the back, about 0.21" wide and 3/4" long, from 1/8" to 7/8" in from the end.
5. **FLAT A90:** the second flat is exactly opposite the first. Measure across the flats: 0.720" (0.750 - 2 x 0.015).
6. **CROSS SLOT A0:** the groove crosses your A0 line 1/2" from the end, 1/8" wide and 0.020" deep, square to the bar.
7. The program ends with Z up (`G53 G0 Z0`) and M30, with no `G30` after it (#1594) and no A return: the last operation runs at A0, so A is left at 0 and Z stays at the top of travel.
8. Total run time is a few minutes.

Send back the PathPilot version, the result of each step, the across-flats measurement, and a photo of the part.
