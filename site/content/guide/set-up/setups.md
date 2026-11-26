+++
title = "Setups"
description = "Stock, work offsets, machines, posts and fixtures, per setup."
weight = 1
aliases = ["guide/setups/"]
+++

A setup is one way the part is held on one machine: its stock, its work
offset, its machine and post, its fixtures and the tools it uses. A new
document has no setups yet: its one stock and one work offset act as the
first setup. Click **Create first setup** (Setup > Setups) to make it
explicit, and **New setup** to add more, for example to flip the part and
machine the other side.

## The setup in the browser

Each setup appears in the browser with its own pages:

- {{ <icon name="tree-setup" /> }} **Settings:** whether it is the active setup, where its stock comes from,
  how the part is aligned, and the WCS label the post writes.
- {{ <icon name="tree-machine" /> }} **Machine:** the setup's machine. It can follow the document's machine
  or carry its own. **Load machine (.mch)** loads a machine file.
- {{ <icon name="act-post" /> }} **Post:** the post processor the setup writes with: a built-in post, or
  **Load post (.cps)** for a script of your own.
- {{ <icon name="tree-fixture" /> }} **Fixtures:** vises, clamps and other workholding, imported or placed
  from the fixture library. See [Workholding and fixtures](@/guide/set-up/workholding.md).
- {{ <icon name="tools" /> }} **Tools:** the tools this setup's operations use, in change order.

Below the pages, {{ <icon name="tree-op-group" /> }} **Operations** lists the setup's operations.

![CamCraft setup overview showing browser tree, part, stock outline, work offset, and generated 2D toolpaths](../../../images/guide/setup-overview.png)

## Place a DXF profile

A DXF drawing is a profile on the stock of one setup, not a solid model.
Create the stock first. On the **Stock** page, open **DXF profiles** and
choose **Add DXF profile…**. In the desktop app you can also drop one
`.dxf` file onto the window to import it onto the active setup's stock.
A drop without stock is refused: "make a stock first (Setup > Stock)".
Browser drops are refused; use the desktop app for this workflow.

The automation commands `import_dxf` (MCP) and
`cam.importDxf(path, { setup, units, join_tol, name })` (script) place it
on the stock top at the stock origin. For box and cylinder stock, that
origin is the XY centre. A moved or resized stock carries its profile.

Each profile row has **Profile X**, **Profile Y** and **Profile angle
(deg)**. Choose **Place profile** to apply the placement, **Hide profile**
or **Show profile** to change visibility, or **Delete profile** to remove
it. For example, X 10 mm, Y 5 mm and angle 90 place the drawing 10 mm
across and 5 mm along the stock face, turned a quarter turn.
The automation equivalent is `move_dxf_profile`. The profile stays on
that face; it has no independent Z move. Import, placement and deletion
are undoable; hiding is view state.

A drawing without units waits at **DXF units**, showing its width and
height under both readings. Choose **Millimeters** or **Inches** from
the drawing's intended units. **Cancel** places nothing; CamCraft never
guesses the units. Automation refuses the import until units are stated.
Read the import summary for skipped DXF items.

**Current limit:** a 2D operation can name a profile, but generation from
the chains is not available yet. Generate refuses by name and writes no
toolpath. A 3D operation needs a solid model. Model-top and model-bottom
heights have no value for a profile; use stock or fixed references.

**Breaking file change:** profiles require project format version 71 or
later. A build that reads only version 70 or earlier refuses a newer file
instead of opening the setup without its profiles. Keep an older copy
when you need to return to an older build.

## Typed text as outline shapes

Outline text makes closed letter shapes in a **TrueType** font. It is
different from the single-line strokes used for engraving text. A letter's
inner loop, such as the hole in O, stays a hole. The shapes are stored in
the project as a profile on the setup's stock top; the font is not needed
to reopen that profile.

In 0.26.0 there is no desktop outline-font dialog or font picker. Create
the stock first, then use an [MCP agent](@/guide/automate/mcp.md) connected
to the app to call `insert_outline_text`. For example, the tool arguments
below place the left end of the baseline 10 mm across and 5 mm along the
stock top from its origin. Replace the font path with a file on the
computer running CamCraft:

```json
{
  "font": "/path/to/font.ttf",
  "text": "PART 01",
  "height": 7,
  "position": [10, 5],
  "angle": 0,
  "letter_spacing": 0,
  "name": "Part label"
}
```

`height` is the font's cap height in mm, from 0.5 to 1000. `angle` turns
the baseline counterclockwise in degrees when seen from above.
`letter_spacing` adds a gap in mm between letters. Use a `.ttf` file or
an `.otf` file with TrueType outlines. CFF outlines, font collections,
missing characters, crossing or touching outlines, and empty text are
refused by name. The setup must have stock.

The result appears with the other **DXF profiles** on the Stock page.
Use **Profile X**, **Profile Y**, **Profile angle (deg)** and
**Place profile** to move it as described above. Insertion is one undo
step. Saving the project keeps the closed shapes. The current profile
generation limit above also applies to outline text: an operation can
name it as its source, but generation from its chains is not available
in this release. It does not produce a machinable text toolpath yet.

## Linked models

A desktop model import from a file records a link to that source file.
When that file changes, CamCraft imports it again through the normal
import command. This keeps the model
identity and makes dependent toolpaths stale. Regenerate and verify the
job after a source change. Nothing generates or posts automatically.
The import is one undo step. Undo keeps the old model until the file
changes again. A missing or unreadable file keeps the model and reports
the problem on the status line.

To stop updates, right-click the model in the browser and choose
**Unlink from** followed by its filename. Undo restores the link. MCP `set_model_link` can
link to another path or clear the link. A moved source file needs a new
link; there is no Link to file control in the window yet. See
[opening a file from the command line](@/guide/automate/cli.md#open-a-file-in-the-window).

## Work offset

Each setup owns one {{ <icon name="tree-wcs" /> }} work coordinate system. G54 to G59 is only the label
the post writes for it. Heights that are measured from the work offset read
the setup's offset.

On the WCS page, **Pick markers in viewport** accepts the box markers,
part corners and round-edge centres. A box marker takes priority when
points overlap. A click on a part corner places the origin there; a round
edge supplies its centre.

On a milling setup, the WCS page disables **Tilt about X (roll)**, **Tilt
about Y (pitch)** and the **Turn X+**, **Turn X-**, **Turn Y+** and **Turn
Y-** buttons, because milling cuts along machine Z and generation refuses a
work offset tilted off Z. A hint points to **Part alignment** on the Setup
page, which turns the part instead. **X origin**, **Y origin**, **Z
origin**, **Turn about Z (yaw)**, **Turn Z+**, **Turn Z-** and **Level** stay
live. **Set origin** in the page footer, beside **Close**, applies the typed
origin. On a lathe the tilt fields and all six turn
buttons stay live. If an existing milling work offset is already tilted,
the page warns that generation will refuse it: click **Level**, then turn
the part with Part alignment.

The setup's alignment controls can align a part from faces, axes, three
points or a cylinder wall. For a cylinder, choose **Pick wall**, select
its curved wall, choose the target axis and optional angle or flip, then
choose **Apply cylinder**. **Place origin** can use a stock box, model box
or picked vertex while retaining the WCS rotation. These changes are
undoable. MCP exposes the same cylinder and origin commands.

Face, Contour, Pocket and 2D Adaptive can generate picked features in an
aligned milling setup. **3D Pocket**, **3D Adaptive** and **3D Finish**
also work there, including area controls and picked boundaries in the
setup frame. Extra area controls on the 2D kinds, tilt combined with
picks and unverified operation kinds still receive named refusals. Keep
each pick with its intended setup; changing the alignment makes affected
paths stale.

## Carrying stock to the next setup

**New setup** with **Carry stock and WCS over** ticked starts the new
setup from a copy of the active setup's stock and work-offset placement;
re-place the copy to describe how the part is re-clamped.

On the Settings page of any setup after the first, the stock source can
instead be what the setup before it leaves behind. Verification, rest
machining and the fixture checks of that setup then see the material the
earlier setup left, not the raw stock. CamCraft refuses that choice by
name when it cannot verify the carry, for example between setups on
different machines.

A side-on setup can carry the earlier stock through a quarter turn
(90 degrees about X or Y). Turns must be quarter turns about X, Y or Z;
a skew turn is refused with both setup names. For a side-on carry the
column simulation keeps a conservative hull: it can retain extra material,
but never removes material merely because the setup turned. Read the
carry note in the verification report.

A box stock that keeps its cut history also keeps the machined walls and
cavities through a side-on quarter turn, not only the column heights. The
carry keeps the swept-tool geometry and reads separate material intervals
along the new columns, and later cuts, stock clips and further turns keep
them. Fixture checks use those intervals, so a fixture can touch a finished
wall or sit in a cavity without a false collision. A later cut through the
part is still caught.

The verdict retains a gouge cut before the turn, even if that hull hides
it in the new view. It credits the departing setup's declared chamfers,
deburrs and marks before retaining gouges, including when the next setup
starts from a new block. A cut deeper than the declared finish remains
a gouge; a later setup cannot erase it.

## Machines

The workspace button on the Setup group mounts a built-in machine for
Milling, Turning or Mill-turning, and the ribbon shows only the operations
that machine can run. Mill-turning is a lathe with live tooling, not
turning on a mill: see [Mill-turn](@/guide/check-and-post/machines.md#mill-turn). CamCraft also ships machine files for several
Tormach mills, and **View > Panels > Machines** browses, validates and
edits `.mch` machine files. The pane's kind filter offers **All** and every
machine kind: Mill, Lathe, Mill-turn, Plasma, Waterjet, Wire EDM and Sinker
EDM. New blank takes its kind from the same list, and is off under **All**. See [Machine types](@/guide/check-and-post/machines.md) for what each kind allows.

A machine file states facts such as axis travel, spindle speed range and
rigid-tapping support. CamCraft checks a program against the facts the
mounted machine states before it posts. A machine file is not a guarantee
that a real machine is safe to run: check its numbers against your
machine's manual.

## Setups on different machines

Setups can sit on different machines or posts. Each such group generates on
its own machine, and Post writes one NC file per machine and post, each
checked against its own machine.

## Setup sheets

The Setups page lists each setup's tools in change order, with cut and
rapid lengths and the estimated time. **Export setup sheets…** writes one
text page per setup. Posting does not write them.

For which unit and zero every number uses, see
[Units and coordinates](@/guide/reference/units-and-coordinates.md).
