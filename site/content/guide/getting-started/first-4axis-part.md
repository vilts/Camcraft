+++
title = "Your first 4-axis part"
description = "Cut four flats and a cross slot on a round bar with the Tormach 770MX and the microARC 4, one indexed face at a time (3+1)."
weight = 6
+++

This guide makes one small part from start to finish on a **Tormach 770MX**
with a **microARC 4** rotary table: a round bar with a flat on each of four
faces and a cross slot on one of them. The work is *indexed*, also called
3+1: the A axis turns the bar to an angle and stops, and then an ordinary
3-axis cut runs. The A axis never turns during a cut.

You need the machine file `tormach-770mx-microarc4.mch` and the post
`tormach.cps`. Both ship with CamCraft, as the machine
`Tormach 770MX + microARC 4` and the post `Tormach PathPilot`.

Companion files in the repository:

- `examples/first-4axis-part.camproj`: the finished project.
- `examples/models/first-4axis-part.obj`: the model of the finished part.
- `examples/first-4axis-part.nc`: the program that the project posts. Each
  G-code excerpt on this page is copied from this file.

Open the project in the app with **Open**, or post it from the
[command line](@/guide/automate/cli.md):

```text
camcraft-cli process examples/first-4axis-part.camproj -o first-4axis-part.nc
```

Continuous 4-axis work, where A turns during the cut, is a different job
and is not walked through here. `tormach.cps` posts it (issue #796). Four
hand-written motion tests in the PathPilot simulator confirm the control
behavior. The posted continuous program still needs a simulator run and a
hardware check. See
[Continuous 4-axis](@/guide/multi-axis/continuous-4-axis.md) and
[What the post refuses](#what-the-post-refuses) below.

## The part and how it is held

The stock is a 3/4 in round bar of 6061 aluminium, a minimum of 2.5 in
long. Hold it in the microARC 4 chuck with 1.5 in out of the jaws. Each cut
is shallow, so a first run has a small risk.

| Operation | Index | Tool | Cut |
|---|---|---|---|
| Flat A-90 | `G0 A-90` | T1, 1/4 in flat end mill | A flat 0.015 in deep, from 1/8 in to 7/8 in from the free end |
| Flat A0 | `G0 A0` | T1 | The same flat on the top face |
| Flat A90 | `G0 A90` | T1 | The same flat, opposite the first one |
| Flat A180 | `G0 A180` | T1 | The same flat, opposite the top one |
| Cross slot A0 | `G0 A0` | T2, 1/8 in flat end mill | A slot 1/8 in wide across the top flat, 1/2 in from the free end, 0.030 in below the bar's skin |

The finished bar measures 0.720 in across each pair of flats
(0.750 - 2 x 0.015). The cross slot marks the face that was up at A0.

Each flat has round ends, because the 1/4 in tool makes it in one pass
along the bar. The flat is 7/8 in to 1/8 in from the free end on its centre
line only, and it is about 0.21 in wide. The model
`first-4axis-part.obj` has the same round ends.

CamCraft does not make one setup for each angle. One setup holds all five
operations, and each operation carries its own index. See
[Indexed work](@/guide/multi-axis/indexed.md) for the rules behind this page.

## Machine, post and unit

1. **Mount the machine.** In the browser, click the **Machine** row. In the
   **Library** list, pick `Tormach 770MX + microARC 4`. The machine is
   mounted at once. The machine file states a rotary named A about X with
   no travel limit (PathPilot treats A as continuous, with no soft limit).
   It also states `nose_to_axis` 10.6 (inches,
   spindle nose to the A centreline at the top of Z). The
   [index swing check](@/guide/multi-axis/checks.md) uses that number.
2. **Pick the post.** In the browser, click the **Post** row and pick
   `Tormach PathPilot` in the list.
3. **Set the unit.** Click {{ <icon name="tree-stock" /> }} **Stock** in the ribbon. In the **Units** row,
   choose **Inches**. The document unit is also the unit of the posted
   program, so this program starts with `G20`.

## Model, stock and work offset

1. **Import the model.** Click {{ <icon name="folder-open" /> }} **Model** and pick
   `examples/models/first-4axis-part.obj`. An OBJ file has no unit and
   CamCraft reads its numbers as millimetres. The Stock page shows the model
   size: 1.5 x 0.75 x 0.75 in. The bar axis is the X axis. The free end
   is at X 0 and the axis goes through Y 0, Z 0.
2. **Fit the stock.** On the Stock page, choose **Box**, set **Fit margin**
   to 0 and click **Fit to Models**. The stock is now the box that the
   round bar fits in: **Length (X)** 1.5, **Width (Y)** 0.75 and
   **Height (Z)** 0.75, with **X center** -0.75, **Y center** 0 and
   **Z bottom** -0.375. A Cylinder stock always stands along Z, so it cannot
   be a bar along X. The box touches the bar on the four faces that the job
   cuts.
3. **Check the work offset.** Click {{ <icon name="tree-wcs" /> }} **Origin**. Keep **X origin**,
   **Y origin** and **Z origin** at 0. The work offset G54 is then at the
   free end of the bar, on the A centreline.

The work offset must be on the A centreline. `tormach.cps` writes each
indexed cut in the frame of the turned part, and that is correct only when
G54 Y0 and Z0 are on the line that the part turns about. On the machine:

- Set G54 X0 at the free end face of the bar.
- Set G54 Y0 on the centreline: indicate the two sides of the bar and take
  the middle.
- Set G54 Z0 on the centreline: touch the top of the bar and subtract the
  radius, 0.375 in. Z0 is the centre of the bar, not its top.
- Mark a line on the top of the bar at A0. The cross slot must cut that
  line.

## Tools

Click {{ <icon name="tools" /> }} **Tools**, then **New tool…**. The editor starts with an end mill.
Make two tools:

| Field | T1 | T2 |
|---|---|---|
| **Name** | EM 1/4 | EM 1/8 |
| **Diameter** | 0.25 | 0.125 |
| **Flute length** | 0.75 | 0.375 |
| **Shank diameter** | 0.25 | 0.125 |
| **Spindle rpm** | 6000 | 8000 |
| Feed (in/min) | 15 | 8 |

Choose **Add tool** for each one. The tools post as T1 and T2, in list
order. If your machine's tool table uses other numbers, set
**Tool number (0 = next free)** on each tool. See
[Tools and tool libraries](@/guide/set-up/tools.md).

## The five operations

Each operation is a {{ <icon name="op-slot" /> }} **Slot** from the **Milling 2D** group: one straight
pass between two typed points. For each row of the tables below:

1. Add a **Slot**. Set its **Tool**, and keep **Source** on Stock.
2. Click **Rename**, type the name and choose **Apply name**.
3. On the **Geometry** tab, type the two ends in the **Manual** section
   (**A X**, **A Y**, **B X**, **B Y**) and the **Slot width**.
4. On the **Heights** tab, set the three heights and the **Stepdown**. For
   each height, set **From** to **Top of stock** and type the **Offset**.
5. On the **Heights** tab, tick **Tilted plane (3+2)** for an indexed face
   and type **Tool axis A** and **Tool axis B**. Keep **Plane origin X**,
   **Plane origin Y** and **Plane origin Z** at 0: the plane origin is then
   on the A centreline. The flat at A0 and the cross slot need no tilt.
6. On the **Feeds** tab, type **Feed** and **Plunge feed**.
7. Click {{ <icon name="act-generate" /> }} **Generate**.

The index of each operation:

| Operation | **Tool** | **Tilted plane (3+2)** | **Tool axis A** | **Tool axis B** |
|---|---|---|---|---|
| Flat A-90 | EM 1/4 | on | -90 | 0 |
| Flat A0 | EM 1/4 | off | | |
| Flat A90 | EM 1/4 | on | 90 | 0 |
| Flat A180 | EM 1/4 | on | 0 | 180 |
| Cross slot A0 | EM 1/8 | off | | |

The cut of each operation (inches):

| Operation | **A X**, **A Y** | **B X**, **B Y** | **Slot width** | **Clearance Z** | **Top Z** | **Bottom Z** | **Stepdown** | **Feed** | **Plunge feed** |
|---|---|---|---|---|---|---|---|---|---|
| Flat A-90 | -0.75, 0 | -0.25, 0 | 0.25 | 0.5 | 0 | -0.015 | 0.015 | 15 | 4 |
| Flat A0 | -0.75, 0 | -0.25, 0 | 0.25 | 0.5 | 0 | -0.015 | 0.015 | 15 | 4 |
| Flat A90 | -0.75, 0 | -0.25, 0 | 0.25 | 0.5 | 0 | -0.015 | 0.015 | 15 | 4 |
| Flat A180 | -0.75, 0 | -0.25, 0 | 0.25 | 0.5 | 0 | -0.015 | 0.015 | 15 | 4 |
| Cross slot A0 | -0.5, -0.16 | -0.5, 0.16 | 0.125 | 0.5 | 0 | -0.03 | 0.01 | 8 | 2 |

Keep the other settings at their defaults. A new operation has **Coolant**
on Flood, so the program writes `M8`.

Three facts explain these numbers:

- **The heights are the same on each face.** A tilted operation measures
  its heights along the tool axis from the plane origin. The plane origin
  is on the A centreline and the stock box is centred on it, so the top of
  the stock is 0.375 in from the centreline on all four faces.
- **The slot ends are typed in the tilted plane.** For A-90, A90 and
  A 0 B 180 the plane's X is the machine's X, along the bar. All four
  flats thus have the same ends (`-0.75, 0` and `-0.25, 0`).
- **The face opposite the top is A 0, B 180.** The page accepts
  **Tool axis A** from -90 to 90 only; an angle outside `-90..90` is
  refused by name and points to A 0, B 180. Type A 0 and B 180 for that
  face. The machine check and the post read this pose as A180.

A tilted operation takes typed geometry or a whole model. It refuses a
picked face or edge: "a tilted plane cannot yet index a picked selection -
pick a whole model instead". That is why this part uses typed slots.

## Check it

Click {{ <icon name="act-show-paths" /> }} **Paths** and look at each toolpath: one pass along each of the
four faces, and one pass across the top.

When you post, CamCraft runs these checks on the indexed program:

- **The machine check.** Each A angle must be in the travel that the
  machine file states.
- **The index swing.** Before each index the program lifts Z to the top of
  travel. The check makes sure that the tool tip clears the radius that the
  stock and the part sweep when A turns, with a 5 mm margin. It uses the
  machine file's `nose_to_axis`. It does not know your chuck or its jaws.
- **Shank and holder.** The tool's shank and holder are read against the
  stock and the part along the tilted moves.

The stock simulation does not judge a tilted program. {{ <icon name="act-simulate" /> }} **Simulate**
and **Deviate** report a refusal for the stock, and the post result says:

```text
stock sim did not judge this program: Flat A-90: move 2: posed moves are not modelled by the column engine (its atoms stand upright on +Z; a tilted tool cut upright is a wrong verdict)
```

This is a statement that the cut was not simulated. It is not a pass. The
program still posts. The proof for this part is the PathPilot simulator and
an air cut, below. See [Checks](@/guide/multi-axis/checks.md).

## Post the program

Click {{ <icon name="act-post" /> }} **Post** and save `first-4axis-part.nc`. The status line reports
the five operations and repeats the "did not judge" warning. To get a
comment on each block, tick
**Explain G-code (add a comment to each block)** on the Post page first.

The program starts in inches, changes to T1 and makes the first index:

```gcode
(Tormach 770MX + microARC 4)
G20 G17 G90 G91.1 G94 G40 G49 G80
G64 P0.001
(OPERATION: Flat A-90 - SLOT - G54)
G17 G90 G54 G40 G49 G80
(T1 EM 1/4 - FLAT END MILL D0.25)
G53 G0 Z0
G30
T1 M6
G43 H1
G53 G0 Z0
(SPINDLE 6000 RPM NEEDS THE HIGH BELT RANGE)
S6000 M3
M8
(4TH AXIS A ABOUT X - G54 Y0 Z0 ON THE A CENTERLINE)
(NO A CLAMP CODE - RELEASE ANY MANUAL ROTARY CLAMP)
G53 G0 Z0
G0 A-90
G0 X-0.75 Y0
G0 X-0.75 Y0 Z0.875
G0 X-0.75 Y0 Z0.4537
G1 X-0.75 Y0 Z0.36 F4
G1 X-0.25 Y0 Z0.36 F15
G0 X-0.25 Y0 Z0.875
M9
M5
```

Read it in this order:

- **The index.** `G53 G0 Z0` lifts Z to the top of travel. Then `G0 A-90`
  turns the table, alone on its line. No A word is on a cutting move.
- **The two notes** before the first index tell the operator where G54
  must be, and that the program writes no clamp code. The microARC 4 has
  no clamp.
- **The cut.** After the index, Z is the distance from the centreline.
  The one pass cuts the flat (Z0.36). The bar's skin (Z0.375) is the top
  of the stock, so no pass runs there: it would cut only air.

The other three flats post the same X, Y and Z lines after their own
index. The fourth flat is the A 0, B 180 pose, and it posts as A180:

```gcode
(OPERATION: Flat A180 - SLOT - G54)
G17 G90 G54 G40 G49 G80
(T1 EM 1/4 - FLAT END MILL D0.25)
G43 H1
G53 G0 Z0
(SPINDLE 6000 RPM NEEDS THE HIGH BELT RANGE)
S6000 M3
M8
G53 G0 Z0
G0 A180
G0 X-0.75 Y0
```

The cross slot changes to T2. The tool change starts with `G30`, and a
`G30` with no axis words moves each axis to the stored G30 position, A
included. Thus the operation states its index again after the tool change:

```gcode
(OPERATION: Cross slot A0 - SLOT - G54)
G17 G90 G54 G40 G49 G80
(T2 EM 1/8 - FLAT END MILL D0.125)
G53 G0 Z0
G30
T2 M6
G43 H2
G53 G0 Z0
(SPINDLE 8000 RPM NEEDS THE HIGH BELT RANGE)
S8000 M3
M8
G53 G0 Z0
G0 A0
G0 Z0.875
G0 X-0.5 Y-0.16
```

The slot goes down in 0.010 in steps to Z0.345, and the program ends with
Z at the top of travel. It writes no `G30` and no A move at the end. The
last operation is at A0, so the table stays at A0:

```gcode
G1 X-0.5 Y-0.16 Z0.345 F2
G1 X-0.5 Y0.16 Z0.345 F8
G0 X-0.5 Y0.16 Z0.875
M9
M5
G53 G0 Z0
M30
```

## Prove it in the PathPilot simulator

CamCraft's stock simulation does not judge this program, so run it in a
PathPilot simulator before the machine. Tormach's PathPilot HUB has a 770MX
simulator.

1. Start a 770MX simulator. In PathPilot's settings, select the microARC 4
   as the 4th axis. Tormach's microARC 4 documents give the steps.
2. Enter T1 and T2 in the tool table, with a length for each.
3. Set G54 so that the program has room: Z0 is the centre of the bar, and
   the cuts are at about Z0.36.
4. Load `first-4axis-part.nc` and run it.

Check these points:

- The program loads with no error and runs to `M30`.
- There are five index lines, in this order: `G0 A-90`, `G0 A0`, `G0 A90`,
  `G0 A180`, `G0 A0`.
- `G53 G0 Z0` is the line before each index, and Z is at the top of travel
  when A turns.
- A turns only on its own line. The A position does not change during a
  `G1` move.
- The four flats show as the same short pass at the same X, Y and Z, each
  at its own A angle.

## On the machine

The simulator does not show your chuck, your tool lengths or the direction
that your table turns. Do these steps in order:

1. **Check the A direction.** In MDI, with the tool well clear, run
   `G0 A90` from A0. The side of the bar that faced you (the front) must
   come to the top. If the back comes up, stop: the direction is reversed
   for this post. Do not run the program.
2. **Check the collet nut against the chuck.** The flats end 7/8 in from
   the free end. With 1.5 in of bar out of the jaws, the nut must clear the
   jaws when the tool is at Z0.36.
3. **Check the swing at the top of Z.** Jog Z to the top (`G53 G0 Z0`) and
   make sure that the tool tip and the nut clear the bar, the chuck and the
   jaws through one full turn of A. The index swing check knows only the
   stock and the part.
4. **Know your G30 position.** Each tool change goes to the stored G30
   position, and A can turn there. Keep the stored G30 Z at the top of
   travel, and make sure that the tool clears the part at the stored A.
5. **Air-cut first.** Move G54 Z up 1 in and run the program at a low feed
   and rapid override, in single block. Watch each `G0 A` line.
6. **Cut.** Then put G54 Z back and cut the part. Keep a hand on feed hold.
   Measure 0.720 in across each pair of flats.

## What the post refuses

`tormach.cps` indexes A between operations. Since issue #796 it also posts an
operation that turns A while it cuts (continuous 4-axis, for example Rotary
Parallel): A keeps counting past 360, and the moves that turn A run in `G93`.
It refuses these cases by name and writes no file:

| Case | The refusal contains |
|---|---|
| A changes inside one operation, but the machine file states no A rate | `simultaneous 4-axis motion` ... `states no A feed rate` |
| A keeps turning one way past the A travel (A counts on, PathPilot does not roll it over) | `A counts on through 360` |
| A tilt that an A about X cannot make (a B or C lean) | `the A axis about X cannot reach it` |
| An A angle outside the machine file's travel | `is beyond the A axis travel` |
| An arc in an indexed operation | `arc on an indexed A` ... `the indexed frame posts lines only` |

Continuous 4-axis output for the microARC 4 is new in issue
[#796](https://github.com/NeoTech/Camcraft/issues/796). Four hand-written
motion tests in the PathPilot simulator confirm the control behavior. The
posted continuous program still needs a simulator run and a hardware check.
Air-cut a continuous program slowly before you cut with it.

Other limits of indexed work today:

- Drill does not take a tilt, so a hole on a side face is not a drilling
  cycle. See [Indexed work](@/guide/multi-axis/indexed.md).
- The stock simulation does not judge a tilted program (above).

## Images to add

When the reproducible UI renders are on `main`, add these images:

- The bar, the stock box and the work offset on the A centreline.
- The Heights tab with **Tilted plane (3+2)** ticked for Flat A90.
- The five toolpaths with **Paths** on.
