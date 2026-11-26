+++
title = "Your first router part"
description = "Program a sheet-goods part on the Sienci AltMill 4x4 (grblHAL) and prepare output for RichAuto DSP A11 shop routers."
weight = 5
+++

This guide walks through a sheet-goods CNC router job from stock setup to a
checked `.nc` file using CamCraft's **Sienci AltMill 4x4** machine profile
and the built-in **grblHAL** post (`assets/posts/grblhal.cps`), plus shop notes
for handheld **RichAuto DSP A11** routers.

Companion project in the repository: `examples/first-router-part.camproj`
(`140 × 100 × 12 mm` bracket plate centered on `200 × 160 × 12 mm` sheet stock).
You can open it in the app with **Open**, or run it headlessly via the
[CLI guide](@/guide/automate/cli.md):

```text
camcraft-cli process examples/first-router-part.camproj -o first-router-part.nc
```

## Machine, sheet stock and corner G54

1. **Select the router profile and post.** Open the machine picker and choose
   **Sienci AltMill 4x4** (`X ±1260 mm`, `Y ±1248 mm`, `Z ±170 mm`, spindle
   `1200–24000 RPM`). Set the post processor to **grblHAL** (`grblhal.cps`).
   See [Machines](@/guide/check-and-post/machines.md) for how CamCraft models
   routers under the `mill` machine kind.
2. **Define sheet stock (`Setup > Stock`).** Use a `Box` stock of
   `200 × 160 × 12 mm` around the `140 × 100 × 12 mm` model. That leaves a
   `30 mm` hold-down border on all four sides for perimeter clamps, brass
   screws, or double-sided tape on top of your MDF spoilboard.
3. **Model clamps if they sit near a cut.** CamCraft has no dedicated
   spoilboard, vacuum table or clamp generator for routers yet — model physical
   clamps as fixtures (see [Workholding](@/guide/set-up/workholding.md)) or
   keep retract height safely above fastener heads.
4. **Set a corner work offset (`G54`).** In **Setup > Origin**, put `G54` at
   the **front-left top corner** of the sheet stock (`X = -100`, `Y = -80`,
   `Z = 12` in document coordinates). In posted G-code (`G54`), every X/Y
   coordinate stays positive inside `0..200 mm × 0..160 mm`. Cutting depths
   are negative relative to the sheet top: the contour reaches `-12 mm`, and
   the drill reaches `-14.803 mm` for breakthrough (`1 mm` past full diameter plus `1.803 mm` for the `118°` drill point below the `12 mm` stock).
   This matches a corner Z touch plate or zero puck resting on the sheet. See
   [Units and coordinates](@/guide/reference/units-and-coordinates.md) for
   more on WCS orientation.

## Tool selection (flat end mill and drill)

Open **Setup > Tools** and add two catalog cutters:

- **Flat end mill (`EM-0600-4FL`):** `6 mm` diameter, `0 mm` corner radius,
  `15 mm` flute length, `8 mm` shank diameter, `9500 RPM`, `1800 mm/min` feed.
  Always compare usable flute length to sheet thickness before profiling: here
  `15 mm` flutes clear the `12 mm` sheet with margin, whereas an `18 mm` sheet
  would push the wider `8 mm` shank against uncut stock wall and trip a reach
  refusal.
- **Drill (`DR-0600`):** `6 mm` twist drill (`18 mm` flute length, `7500 RPM`,
  `115 mm/min` feed) for the four `Ø6.2 mm` corner mounting holes.

On dry wood, MDF or plywood, set **Coolant = Off** in each operation so the
post does not switch on flood coolant (`M8`). See
[Tools](@/guide/set-up/tools.md) and [Setups](@/guide/set-up/setups.md).

## Cut order and operations

Order operations from inside-out so the surrounding sheet keeps holding the
part rigid until the very last cut (see
[Order of operations](@/guide/check-and-post/order-of-operations.md) and
[2D operations](@/guide/operations/operations-2d.md)):

1. **`Drill1` (Drilling > Drill).** Click **Detect holes** on `router-part` to
   populate the four through-holes while the sheet is completely intact. Leave
   **Cycle** on `Auto` (`grblhal.cps` posts `G73` chip-break peck when a `4 mm`
   peck is set), **Clearance** at `Top of stock + 15 mm`, **Retract** at
   `Top of stock + 5 mm`, and **Coolant = Off**. The detected through-holes
   include `1 mm` breakthrough past full diameter (plus the `1.803 mm` `118°`
   drill point) below the `12 mm` stock: the posted drill bottom is
   `-14.803 mm` in `G54` (`G73 X53 Y50 Z-14.803 R0 Q4 F115` for the
   first hole). Check this depth before running the drill.
2. **`2D Pocket1` (Milling 2D > 2D Pocket).** Pick the floor face of the
   `50 × 30 × 4 mm` central recess using `EM-0600-4FL`. Use `Waterline`
   strategy, **Entry = Helix** (`1.2 mm` helix radius), `3 mm` stepover,
   `2 mm` stepdown, `400 mm/min` plunge feed, and **Coolant = Off**.
3. **`2D Contour1` (Milling 2D > 2D Contour — last, with holding tabs).** Pick
   the outer bottom edge chains of `router-part` using the same `EM-0600-4FL`
   flat end mill so `Drill` -> `Pocket` -> `Contour` performs only one manual
   tool change (`T1 M6` -> `T2 M6`):
   - Keep **Side = Outside** and cutter compensation **In computer** (`G40`).
   - Set **Top** to `Top of stock (0 mm)`, **Bottom** to `Bottom of stock (0 mm)`,
     and **Stepdown** to `4 mm` (three depth passes: `-4`, `-8`, `-12 mm`).
   - On the **Lead in / Lead out** tab, enable holding tabs: **`Tab count = 4`**,
     **`Tab width = 8.0`**, **`Tab height = 3.0`**. On the final full-depth
     pass, the cutter bridges four `8 mm` tabs on the perimeter so the finished
     bracket stays secured to the sheet instead of tearing loose into the bit.

## Simulate, check clearances and post to `.nc`

Click **Simulate**, scrub through `Drill1` -> `2D Pocket1` -> `2D Contour1`,
and check **Deviate**:

- Verify `0` gouges and confirm the four perimeter holding tabs remain joined to
  the outer sheet waste.
- Small `3 mm` inside corner fillets stay uncut inside sharp `90°` pocket
  corners (excess material left by a `6 mm` round end mill, not a gouge).
- Check clearance for both the contour (`-12 mm` Z in `G54`) and the drill
  breakthrough (`-14.803 mm` Z: `1 mm` past full diameter plus the `1.803 mm`
  drill point below the `12 mm` stock). Confirm the
  spoilboard permits this penetration and all screws and other fasteners stay
  clear of the drill and contour paths through their full cutting depths.

Click **Post** (with **grblHAL** selected) to write `first-router-part.nc`. A
valid program starts with `grblhal.cps`'s metric header, lifts via `G53 G0 Z0`
before each `T<n> M6`, runs `G54`, `G73` pecking for `Drill1`, helical entry
for `2D Pocket1`, tabbed perimeter passes for `2D Contour1`, and ends cleanly
with `G53 G0 Z0` / `M30` (and emits no `G43`, `G49`, or `%` lines):

```gcode
(Sienci AltMill 4x4)
G21 G17 G90 G91.1 G94 G40 G80
(OPERATION: Drill1 - DRILL - G54)
G17 G90 G54 G40 G80
(T1 DR-0600 - DRILL D6)
G53 G0 Z0
T1 M6
S7500 M3
...
```

## Common router refusals

Several safety doors protect router jobs before a `.nc` file is written (see
[Posting and verification](@/guide/check-and-post/posting.md) and
[Refusal reference](@/guide/reference/refusals.md)):

- **`G41` / `G42` cutter compensation:** `grblhal.cps` refuses Control or Wear
  compensation by name (`control cutter compensation (G41/G42) is not supported
  by grblHAL - choose In computer`). Keep **In computer** selected.
- **Rigid tapping (`G84`):** `grblhal.cps` refuses `G84` tapping cycles (`rigid
  tapping (G84) is not in grblHAL's G-code set`).
- **Missing clearance or retract heights / `AfterOperation::Home`:** `grblhal.cps`
  refuses an operation with no stated clearance or retract plane, and refuses a
  `Home` action after an operation.
- **Spindle speed limits:** RPM values above `24000` (or negative/non-finite
  values) are refused against `Sienci AltMill 4x4`'s spindle range.
- **Probing on grblHAL:** probing operations are refused by `grblhal.cps`.
- **Shank reach and table / fixture collisions:** deep cutouts exceeding usable
  `flute_len` with a larger shank, or toolpaths colliding with fixtures or Z
  limits, fail verification and block posting.

## Shop routers with RichAuto DSP A11

CamCraft's dedicated `richauto.cps` post processor and Laguna / Grizzly router
machine profiles are tracked separately (`#1914`, `#1915`, `#1916`) and are not
on `main` yet. Until those ship, follow these controller-preparation practices
from public RichAuto DSP A11 OEM operator manuals when running shop routers
equipped with a RichAuto A11 handheld pendant:

- **USB stick formatting (`FAT16` / `FAT32`) and `8.3` filenames:** Format the
  USB flash drive as **`FAT32`** (or `FAT16`; `2–8 GB` drives are typically
  most reliable — avoid exFAT or NTFS) and name every file using strict **`8.3`**
  uppercase ASCII filenames without spaces (for example, `DRILL1.NC`,
  `MILL1.NC`, `PART01.NC`). Copy programs larger than a few hundred kilobytes
  from `U-Disk` into internal DSP memory (`512 MB`) before pressing start so USB
  disconnects cannot stall motion mid-cut.
- **Verify `G Code Setup` (`MENU -> Auto Pro Setup -> G Code Setup`):**
  - Set **`AbsCntr = Off`** so incremental arc centers (`I`/`J`, matching `G91.1`)
    are used instead of interpreting `I`/`J` as absolute center coordinates,
    which corrupts small arcs into sweeping circles.
  - Check `Read F`, `Read S`, `CodeHead`, `Read/Ign T`, `Read/Ign G54`, `Ign G49`,
    and `Ign G40` against your router wiring before cutting stock.
- **Calibrate your Z touch-off puck (`C.A.D Thickness`):** Measure the aluminum
  or brass touch-off puck height in millimeters with calipers and save that
  exact figure in **`MENU -> Machine Setup -> C.A.D Thickness`**. Jog X and Y
  to the front-left corner of your sheet (`G54` `(0, 0)`) and press **`XY→0`**
  to set the X/Y work origin. **`ORIGIN/OK`** returns to the existing work
  origin; it does not set the origin. See the [RichAuto A11 OEM manual,
  sections 2.3 and 5.4.1](https://www.richautocontroller.com/wp-content/uploads/2023/08/DSP-A11-Manual.pdf).
  Place the puck flat on the sheet surface under the bit (attach the ground
  clip if required), and press **`ON/OFF + MENU`** to auto-zero Z (or use a
  paper feeler touch-off and zero Z manually).
- **Use a single-tool `.nc` file per bit:** Manual-collet DSP routers without an
  ATC often pause unexpectedly or alarm on `T<n> M6` when `Read T` is active.
  Suppress the other tool's operations in CamCraft and post one **single-tool**
  `.nc` file per cutter (`DRILL1.NC` for `DR-0600`, then `MILL1.NC` for
  `EM-0600-4FL`), inspect any remaining `T<n> M6` / `G53` header lines, and run
  a quick high **air cut** above the workpiece before lowering Z zero to stock
  top.

## Screenshot checklist (`#1902`)

Once headless UI capture (`#1902`) lands on `main`, add renders for:

- Stock box (`200 × 160 × 12 mm`), bracket model, and front-left `G54` origin
- `2D Pocket1` helical ramp and concentric waterline passes inside the central recess
- `2D Contour1` perimeter path showing the four `8 mm` holding tabs on the last pass
- `Simulate` + `Deviate` material-removal view of the finished tabbed bracket
