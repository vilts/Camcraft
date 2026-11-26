+++
title = "Your first continuous 4-axis program"
description = "Set up, post and air-cut a Rotary Parallel program on the Tormach 770MX and the microARC 4, with the A axis turning during the feed moves."
weight = 7
+++

This guide takes one small job from start to finish on a **Tormach 770MX**
with a **microARC 4** rotary table: a Rotary Parallel pass along a round
bar. The work is *continuous* 4-axis: the A axis turns inside the
operation, and on some feed moves it turns together with X, Y and Z. For
*indexed* work, where A stops before each cut, see
[Your first 4-axis part](@/guide/getting-started/first-4axis-part.md).

**Air-cut this program before you cut a bar with it.** The guide goes
through a run in the PathPilot simulator and an air cut with no bar in the
chuck, and only then a cut. Those two runs prove what a continuous program
must prove first: that the control reads the A words, the `G93` feeds and
the `G94` return as the post wrote them.

The bar goes on past the model into the jaws of the chuck. Rotary Parallel
thus leaves the chuck-side end face of the model: a pass down that face
would be a cut through the bar (issue
[#2343](https://github.com/NeoTech/Camcraft/issues/2343)). See
[The chuck-side end face](#the-chuck-side-end-face).

The continuous output of `tormach.cps` is new (issue
[#796](https://github.com/NeoTech/Camcraft/issues/796)). It was written
from four test moves in the PathPilot simulator. No complete program has
been run in the simulator or on a machine yet.

You need the machine file `tormach-770mx-microarc4.mch` and the post
`tormach.cps`. Both ship with CamCraft, as the machine
`Tormach 770MX + microARC 4` and the post `Tormach PathPilot`.

Companion files in the repository:

- `examples/first-continuous-4axis-part.camproj`: the finished project.
- `examples/models/first-continuous-4axis-part.obj`: the model.
- `examples/first-continuous-4axis-part.nc`: the program that the project
  posts. Each G-code excerpt on this page is copied from this file.

Open the project in the app with **Open**, or post it from the
[command line](@/guide/automate/cli.md):

```text
camcraft-cli process examples/first-continuous-4axis-part.camproj -o first-continuous-4axis-part.nc
```

## The job and how the bar is held

The stock is a 3/4 in round bar in the microARC 4 chuck, with 2 in out of
the jaws. The model is a plain cylinder, 0.680 in across and 1.25 in long,
at the free end of the bar. One operation, a Rotary Parallel pass with a
1/8 in ball tool, finishes the model.

![Side and end views of the bar in the chuck: G54 X0 at the free end face, Y0 and Z0 on the A centreline, and the model as a dashed outline.](../../../guide-pictures/first-continuous-4axis-setup.svg)

The bar axis is the X axis, because the microARC 4 turns about X. The work
offset G54 is at the free end of the bar, on the A centreline.

## Machine, post and unit

1. **Mount the machine.** In the browser, click the **Machine** row. In the
   **Library** list, pick `Tormach 770MX + microARC 4`.
2. **Pick the post.** In the browser, click the **Post** row and pick
   `Tormach PathPilot` in the list.
3. **Set the unit.** Click {{ <icon name="tree-stock" /> }} **Stock** in the ribbon. In the **Units** row,
   choose **Inches**. This program starts with `G20`.

The machine file states four things about the A axis. Compare each one with
your machine before you trust a program:

| In the machine file | Value | What uses it |
|---|---|---|
| A rotary about X | `axis` 1, 0, 0 | The operation's fitted axis must be on this line. The posted letter is A. |
| A travel | Not stated | The shipped file permits continuous A counts. It supplies no A position limit for the machine check or the post. |
| `nose_to_axis` | 10.6 (inches) | The [index swing check](@/guide/multi-axis/checks.md). It is the catalogue distance from the spindle nose to the A centreline at the top of Z. |
| A `max_normal_speed` | 360 (degrees per minute) | The post holds each turn of A to this rate. A machine file with no A rate is refused. |

The A rate of 360 degrees per minute is the rate that the PathPilot
simulator ran in the test moves. It is not a figure from Tormach. The A
travel is unstated in both the shipped file and this example project. This
does not verify PathPilot's settings or the machine's cables and fixtures. If your machine has
other values, edit a copy of the machine file: see
[Machine types](@/guide/check-and-post/machines.md).

## Model, stock and work offset

1. **Import the model.** Click {{ <icon name="folder-open" /> }} **Model** and pick
   `examples/models/first-continuous-4axis-part.obj`. An OBJ file has no
   unit and CamCraft reads its numbers as millimetres. The model's axis is
   the X axis. Its free end is at X 0 and the axis goes through Y 0, Z 0.
2. **Set the stock.** On the Stock page, choose **Box**. Type
   **Length (X)** 2, **Width (Y)** 0.75 and **Height (Z)** 0.75, then
   **X center** -1, **Y center** 0 and **Z bottom** -0.375. Click
   **Apply**. The box is the 2 in of bar that is out of the jaws. A
   Cylinder stock always stands along Z, so it cannot be a bar along X.
3. **Check the work offset.** Click {{ <icon name="tree-wcs" /> }} **Origin**. Keep **X origin**,
   **Y origin** and **Z origin** at 0.

You do not tell CamCraft where the rotary is. A rotary operation fits the
axis of revolution from the model, and that axis must be on the line of the
machine's A axis. The post then turns each point about the work offset. Thus
G54 Y0 and Z0 must be on the line that the table turns about. On the
machine:

- Set G54 X0 at the free end face of the bar.
- Find the A centreline in Y: indicate the two sides of a round bar in the
  chuck and take the middle. Set G54 Y0 there.
- Find the A centreline in Z: touch the top of the bar and subtract its
  radius, 0.375 in. Set G54 Z0 there. Z0 is the centre of the bar, not its
  top.
- Check it: turn A by 180 degrees and indicate the bar again. If the two
  readings are different, the bar does not run true in the chuck, and half
  of the difference is the error of your centreline.
- Set A to 0 where you want it. This job has no feature that needs one
  angle, but the program's first A move is absolute.

## The tool

Click {{ <icon name="tools" /> }} **Tools**, then **New tool…**. Set **Type** to **Ball nose** and
type these values:

| Field | T1 |
|---|---|
| **Name** | BN 1/8 |
| **Diameter** | 0.125 |
| **Flute length** | 0.625 |
| **Shank diameter** | 0.125 |
| **Spindle rpm** | 8000 |
| Feed (in/min) | 8 |

Choose **Add tool**. The speed and the feeds in this project are the
example's numbers. They are not a recommendation for your machine, tool or
material.

The flute is longer than the radius of the bar, 0.375 in, because the tool
goes down the free end face to the A centreline. See
[Check it](#check-it).

## The operation

Add a {{ <icon name="op-rotary-parallel" /> }} **Rotary Parallel** from the **Multi-axis** group. Set its
**Tool** to BN 1/8 and its **Source** to the model,
`first-continuous-4axis-part`. Click **Rename**, type `Journal` and choose
**Apply name**. Then set these values:

| Tab | Setting | Value |
|---|---|---|
| **Heights** | **Clearance Z** | **From** Fixed value, **Offset** 0.875 |
| **Heights** | **Top Z** | **From** Fixed value, **Offset** 0.375 |
| **Heights** | **Bottom Z** | **From** Fixed value, **Offset** -0.375 |
| **Passes** | **Stepover** | 0.1 (turn **Auto** off) |
| **Passes** | **Tool lead angle** | 0 |
| **Passes** | **Tool lag angle** | 0 |
| **Passes** | **Max tilt** | 60 |
| **Passes** | **Tolerance** | 0.02 |
| **Feeds** | **Feed** | 8 |
| **Feeds** | **Plunge feed** | 2 |
| **Feeds** | **Climb milling** | on |

Click {{ <icon name="act-generate" /> }} **Generate**.

These points explain the numbers:

- **The clearance is a radius.** Rotary Parallel reads **Clearance Z** as a
  distance from the bar's axis. 0.875 in is 1/2 in outside the bar's skin.
  The tool goes out to this radius before A turns between two passes.
- **Top and bottom keep the whole bar.** **Bottom Z** is the lowest point
  where the tool touches the bar as it lies. At -0.375 the passes go all
  the way round the bar (issue
  [#2970](https://github.com/NeoTech/Camcraft/issues/2970)). A Bottom Z
  above the axis keeps the cut to the upper arc of the bar.
- **The tolerance and the stepover are coarse.** They keep the program
  short (2866 lines) for a first run. They do not give a good surface.

These points distinguish the other continuous operations:

- **Rotary Contour** cuts rings about a bar that stands on its axis. It
  refuses a bar that lies along X and points to Rotary Parallel.
- **Rotary Offset** roughs in rings round the bar. Each level that cuts
  takes two turns of A. The shipped machine file now permits continuous
  A counts, so the former -360..360 travel refusal no longer applies
  (issue [#2344](https://github.com/NeoTech/Camcraft/issues/2344)). This
  guide uses Rotary Parallel for its finishing pass. No Rotary Offset
  machine run is established here.
- No operation wraps a drawing (text or a slot pattern) round a cylinder.
  See [Continuous 4-axis](@/guide/multi-axis/continuous-4-axis.md).

## Check it

Click {{ <icon name="act-show-paths" /> }} **Paths** and look at the toolpath. The picture below shows each
move of the posted program, with X along the bar and the A angle up the
page.

![Each move of the program, unrolled: feed passes that run along the bar at one A angle, turn A across the free end face, and run back at another angle, with rapids at the chuck-side end.](../../../guide-pictures/first-continuous-4axis-paths.svg)

The feed moves make passes along the bar. A pass runs along the bar at one
A angle. At the free end it goes past the end of the model, turns A across
the end face, and runs back at another angle. At the chuck-side end the
tool stops at X-1.25, goes out to the clearance radius, turns A there by
rapids in small steps, and comes back in.

**The free end face.** At the free end the passes go 1/16 in (the tool
radius) past the model, and there the tool goes down the end face to the A
centreline: the program has feed moves at X0.0625 with Z below 0. This is
air, because the stock ends at X0.

### The chuck-side end face

At the chuck end the stock goes on 3/4 in past the model, into the jaws. A
pass down that end face would be a groove 1/8 in wide from the skin of the
bar to its centre, and the part would come off the bar. Rotary Parallel
reads the stock and leaves that end face. The program has no feed move
past X-1.25, and at X-1.25 the tool stays at the radius of the model. The
operation says so in a note, in millimetres:

```text
Rotary Parallel: the stock goes on 19.050 mm past the end of the model at (-31.750, 0.000, 0.000) - that end face is left, and the tool stops at its plane
```

The rule is this. Where the stock goes on past an end of the model by more
than one tool diameter, the centre of the ball does not go past that end
of the model. Half of the ball goes past it, and there it cuts no deeper
than the surface of the model. Where the stock ends with the model, or
goes on by one tool diameter or less, the end face is cut to the
centreline as at the free end.

The operation has a second note. It says that the passes go all the way
round the bar, so the machine must turn the part through a full turn of A:

```text
Rotary Parallel: the passes go all the way round the bar - the machine must turn the part on a rotary axis through the whole turn; set Bottom above the axis to cut only the upper arc
```

CamCraft knows only the stock that you set. Make the stock as long as the
bar that is out of the jaws. If the stock ends where the model ends,
CamCraft reads air there and the program cuts through the bar.

When you post, CamCraft also runs these checks:

- **The machine check.** If a machine file states A travel, each A angle
  must be in that travel. The shipped file and this example state no A
  travel, so this check supplies no A position limit.
- **The radial check.** The cutter is checked against the model for
  gouges when the operation is generated.

The stock simulation does not judge a rotary program. {{ <icon name="act-simulate" /> }} **Simulate**
and **Deviate** report a refusal for the stock, and the post result starts:

```text
stock sim did not judge this program: Journal: a rotary program turns the part under a posed tool
```

This is a statement that the cut was not simulated. It is not a pass. The
program still posts. See [Checks](@/guide/multi-axis/checks.md).

## Post the program

Click {{ <icon name="act-post" /> }} **Post** and save `first-continuous-4axis-part.nc`. The program
starts as an indexed program does: Z goes to the top of travel, and A goes
to its first angle on its own line.

```gcode
(Tormach 770MX + microARC 4)
G20 G17 G90 G91.1 G94 G40 G49 G80
G64 P0.001
(OPERATION: Journal - ROTARY PARALLEL - G54)
G17 G90 G54 G40 G49 G80
(T1 BN 1/8 - BALL-NOSE D0.125 R0.0625)
G53 G0 Z0
G30
T1 M6
G43 H1
G53 G0 Z0
(SPINDLE 8000 RPM NEEDS THE HIGH BELT RANGE)
S8000 M3
M8
(4TH AXIS A ABOUT X - G54 Y0 Z0 ON THE A CENTERLINE)
(NO A CLAMP CODE - RELEASE ANY MANUAL ROTARY CLAMP)
G53 G0 Z0
G0 A44.604
G0 X-1.25 Y0
G0 X-1.25 Y0 Z0.875
G1 X-1.25 Y0 Z0.3399 F2
G1 X-1.1793 Y0 Z0.3399 F8
```

After the first A move, Z is the distance from the A centreline. The first
pass is at Z0.3399, the radius of the model. A move that does not turn A is
an ordinary `G94` move: its F word is inches per minute.

### A move that turns A with X, Y and Z: G93

```gcode
G0 X-1.0892 Y0 Z0.875 A8.196
G1 X-1.0892 Y0 Z0.3399 F2
G93 G1 X-0.0005 Y0 Z0.3399 A8.194 F7.347
G1 X0.0067 Y0 Z0.3395 A7.751 F812.641
G1 X0.0128 Y0 Z0.3388 A7.038 F504.908
```

The post writes `G93` on the first feed move that turns A together with X,
Y or Z. `G93` is inverse time feed: the F word is not a speed. It is 1
divided by the time of the move in minutes, and each `G93` line has its own
F word.

- `F7.347` is 1/7.347 minute, about 8.2 seconds, for a move of 1.089 in.
  That is the 8 in/min feed of the operation.
- A short move has a large F word. `F812.641` is a move of 0.07 second. A
  large F word in `G93` is not a high speed.

The post uses `G93` because of what the PathPilot simulator did with the
other mode. In `G94`, `G1 X4 A180 F20` took 12 seconds: the control applied
F to the X, Y, Z distance only, and A went along in the same time (Hub
reading 2 on issue #796). A move with a small X, Y, Z distance and a large
A turn would then run A as fast as the short distance allows. With `G93`
the post states the time of each move. The simulator accepted `G93` and
ran `G93 G1 X4 A180 F5` in 12 seconds, which is 1/5 minute (Hub reading
3).

### The A rate cap, and a move that turns A only

The post gives each move the longer of two times: the time for the tool to
go along its path at the operation's feed, and the time for A to turn at
the machine file's A rate. The path length bound includes the curved
trajectory relative to the part between the rounded XYZ and A words.
Rounding makes a move slower, not faster.

```gcode
G1 X0.0625 Y0 Z-0.032 A187.5 F153.977
G1 X0.0625 Y0 Z-0.0321 A186.086 F254.596
G1 X0.0625 Y0 Z-0.0322 A183.75 F154.109
G94
G1 A182.334 F360
G1 A180 F360
G1 A178.581 F360
G1 A176.25 F360
G93 G1 X0.0625 Y0 Z-0.0321 A174.829 F253.342
G1 X0.0625 Y0 Z-0.032 A172.5 F154.572
```

- These moves are on the free end face, where the path is short and the
  turn of A is large. The A rate sets the time: from A186.086 to A183.75
  is 2.336 degrees, and 2.336 x 154.109 is 360 degrees per minute.
- `G94` is on its own line before a move that is not a `G93` feed.
- `G1 A182.334 F360` turns A only. It stays in `G94`, and its F word is
  degrees per minute: in the simulator `G1 A180 F360` took 30 seconds
  (Hub reading 4).
- `G93` is on the next move that turns A with X, Y or Z.

### A counts on

The same excerpt goes through 180 degrees: A182.334, A180, A178.581. The
post does not write A-177.666 for the angle before 180. In the simulator,
from A350 the move `G1 A10 F360` turned A back through 340 degrees and
stopped at 10 (Hub reading 1): PathPilot reads A as a plain number of
degrees. It does not roll over at 360 and it does not take the short way.
So each A word is the angle nearest to the last one, and the count goes on
past 180 and past 360.

![The A word of each line of the program: it stays between -85.708 and 265.687 degrees; the chart scale is not a machine limit, and the lines in G93 are marked below.](../../../guide-pictures/first-continuous-4axis-a-count.svg)

In this program A stays between -85.708 and 265.687 degrees. A program
that turns past a limit in a custom machine file is refused. The shipped
file has no such A limit. The post does not write an unwind inside an
operation.

### The end of the operation

```gcode
G1 X0 Y0 Z0.3399 A265.687 F2603.14
G1 X0.0031 Y0 Z0.3399 A265.686 F2444.083
G94
G0 X0.0031 Y0 Z0.875
M9
M5
G53 G0 Z0
M30
```

`G94` comes back before the last rapid, so no operation ends in `G93`.
The program ends with Z at the top of travel and A at its last angle,
265.686. The next operation of a longer program lifts Z and states its own
first A between -180 and 180.

## What the post refuses

`tormach.cps` refuses these cases by name and writes no file:

| Case | The refusal contains |
|---|---|
| A changes inside one operation, but the machine file states no A rate | `simultaneous 4-axis motion` ... `states no A feed rate` |
| A turns past the A travel stated in a custom machine file | `A counts on through 360 (PathPilot does not roll A over)` |
| An arc, a canned cycle or a probe move that would turn A | `the post writes a turning A on straight G0 / G1 moves only` |
| A move that turns A from a start point the post does not know | `its path and time cannot be judged` |
| A move that turns A and cannot be split fine enough to stay on its path | `still bows` |
| A tilt that an A about X cannot make (a B or C lean) | `the A axis about X cannot reach it` |

The last but one row is about the path between two points. The machine
moves X, Y and Z in a straight line while A turns, so the tool does not go
straight across the part. The post splits each such move into shorter
moves until the error is less than 0.0002 in. This is why the program has
many short `G93` lines.

A refusal is not an error in your project. It says that the post cannot
write a safe program for the case. See
[Reading a refusal](@/guide/reference/refusals.md).

## Prove it in the PathPilot simulator

Run the program in a PathPilot simulator before the machine. Tormach's
PathPilot HUB has a 770MX simulator.

1. Start a 770MX simulator. In PathPilot's settings, turn the 4th axis on.
   The test moves of issue #796 were run with PathPilot v2.14.4 and the 4th
   axis type `4" 4th Axis`. Tormach's microARC 4 documents give the steps
   for the machine.
2. Enter T1 in the tool table, with a length.
3. Set G54 so that the program has room: Z0 is the A centreline, and the
   tool goes from Z0.875 down to a little below Z0.
4. Load `first-continuous-4axis-part.nc` and run it.

Check these points, and record the result on issue
[#1904](https://github.com/NeoTech/Camcraft/issues/1904):

- The program loads with no error on a `G93` or `G94` line, and runs to
  `M30`.
- At line 2685 the A position is 180. On the lines before it, A is
  183.75 and 182.334. On the next lines it goes down to 178.581 and
  176.25. It does not go through 0.
- The A position stays between -85.708 and 265.687.
- Line 254, `G93 G1 X-0.0005 Y0 Z0.3399 A8.194 F7.347`, takes about 8
  seconds.
- After the program, the control is in `G94`.

## On the machine: single block, then an air cut

The simulator does not show your chuck, your tool length or the direction
that your table turns. Set the work offset with the bar in the chuck, as
above. Then take the bar out, and do these steps in order.

1. **Check the A direction.** Put a bar or a pointer in the chuck for this
   step only. In MDI, with the tool well clear, run `G0 A90` from A0. The
   side that faced you (the front) must come to the top. If the back comes
   up, stop: the direction is reversed for this post. Do not run the
   program.
2. **Empty the chuck.** The program goes to X-1.25, and the jaws are at
   X-2. Make sure that the collet nut clears the chuck body and the jaws
   at that X with the tool at the radius of the bar.
3. **Know your G30 position.** The tool change goes to the stored G30
   position, and A can turn there. Keep the stored G30 Z at the top of
   travel.
4. **Run the start in single block.** Set the feed and rapid overrides
   low. Step through the lines to the first `G93` line (line 254). Watch
   that `G53 G0 Z0` lifts Z before `G0 A44.604`, and that each rapid
   between two passes is at Z0.875.
5. **Air-cut the whole program.** Keep the chuck empty and the overrides
   low, with a hand on feed hold. Watch the A position go through 180 near
   line 2685 without a jump.
6. **Cut only after a good air cut.** If one step above was not as
   described, stop: do not put a bar in the chuck for this program.

If each step is as described, your machine reads continuous A, `G93` and
`G94` as the post writes them. Please record the PathPilot version and the
result on issue #1904.

## Before you cut the bar

The program leaves the chuck-side end face, so it does not cut through the
bar. That removes the limit that this guide had before (issue
[#2343](https://github.com/NeoTech/Camcraft/issues/2343)). No complete
continuous program has been cut on a machine yet. These points stay with
you:

- The stock in the project must be the bar that is in the chuck: 3/4 in
  across, 2 in out of the jaws, with its free end at G54 X0. With a longer
  or thicker bar, change the stock and generate again.
- The pass takes 0.035 in from the radius of the bar in one cut. A pass
  that starts at X-1.25 starts with a plunge into the skin of the bar.
- The speed and the feeds are the example's numbers. They are not a
  recommendation.
- Run the first cut with low overrides and a hand on feed hold.
