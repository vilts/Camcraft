+++
title = "Units and coordinates"
description = "Which unit each number is in, and which frame each coordinate is measured from."
weight = 1
aliases = ["guide/units-and-coordinates/"]
+++

Most unit and coordinate mistakes come from assuming that two numbers share
a unit or a zero when they do not. This page lists, for each kind of number
in CamCraft, what it is measured in and from where.

## Document unit and program unit

Inside CamCraft every length is millimetres, always. The **document unit**
(millimetres or inches, chosen with the **Units** setting on the Setup > Stock page) is only a lens:
it changes how lengths and feeds are shown and typed, and how messages
quote them. A value typed in inches is converted to millimetres when it is
entered, and the stored geometry and parameters do not change when you
switch the lens back.

The **program's unit** is the one the post writes. On main today the
document unit does not decide it:

- The Fanuc, Haas, Mazak, Doosan, Fanuc Lathe and Carvera posts write
  `G21` and millimetre numbers; the Siemens 840D post writes `G710`
  (metric) and the Heidenhain post a `G71` (mm) program header. The Carvera post states that its firmware
  runs in millimetres only and asks for a metric machine file.
- The Tormach mill, lathe and plasma posts follow the **machine file**: a
  machine whose `kinematics.default.units.length` is `inch` gets `G20`, and
  every length and feed in the program is divided by 25.4. Any other
  machine gets `G21`.
- The **Post** page's end-units choice ("Program's unit", "Inch (G20)",
  "mm (G21)") only sets which unit word a post that supports it (Tormach)
  writes before the program end, so the control is not left in the other
  unit. The default leaves the program's own unit in force.

Making the document unit the unit every post writes is open work (pull
request #981, not on main when this page was written); until it lands,
switching the document to inches does not change what a Fanuc-style post
writes. Check the `G20`/`G21` line and a few coordinates of your first
program before you run it.

## Imported models

- **STEP** declares its length unit in the file. CamCraft reads it before
  any geometry is built: millimetres, metres and other SI prefixes, and
  conversion-based units such as an inch, are scaled to millimetres on
  import. The import summary on the status line names the unit, for
  example `length unit: in (stated by the file)`. A file whose contexts
  disagree, or whose unit CamCraft cannot convert, is refused by name
  instead of guessed.
- **A STEP file that states no length unit** is never read in a guessed
  unit. The desktop app asks for the unit the model was drawn in:
  **Millimeters (mm)** or **Inches (in)**. It then imports the file in
  that unit, and the summary says `stated at import`. **Cancel** imports
  nothing. The question comes again at the next import of that file. A
  script or an MCP client states the unit with `units` (`"mm"` or `"in"`);
  without it the import is refused. A tool holder or a fixture from such a
  file is refused: re-export the file with its unit.
- **STL** and **OBJ** carry no unit. The readers apply no scale, so their
  numbers are taken as they stand, that is as millimetres. A model
  exported in inches therefore arrives 25.4 times too small.
- **DXF and SVG profiles** are 2D drawings that lie on the top face of a
  setup's stock (`import_dxf` and `import_svg` over MCP, `cam.importDxf`
  and `cam.importSvg` in scripts). A DXF states its unit in `$INSUNITS`.
  An SVG states its size in the `width` and `height` of the `<svg>`
  element: mm, cm, in, pt, pc or px (96 px to the inch), with the
  `viewBox` for the drawing units. A drawing that states neither is
  refused by name until you give the unit of one drawing unit (mm or in);
  it is never guessed. SVG's Y axis points down, so CamCraft flips it:
  the drawing looks as it does in Inkscape or Illustrator. Circular arcs
  stay arcs; Beziers and other ellipses become short lines within
  0.005 mm. Import applies group transforms and uses top-level group
  labels or IDs as sketch layers. For an SVG without a physical size,
  pass `units: "mm"` or `units: "in"` in the import options. For a
  sized SVG, omit `units`. Import skips text, images, clip paths, masks
  and markers and counts them in the reply. Convert text to paths in
  the editor before you export. Fill and stroke are not read: a shape with a stroke imports as
  its centre line, not as the outline of the stroke. To cut the outline,
  convert the stroke to a path in the editor. A style sheet can hold
  paint and text rules only. A file with a rule that hides, moves or
  resizes a shape, with an animation element, or with a reference to a
  DOCTYPE entity is refused by name: the import never cuts a shape that
  the editor shows at a different size or does not show.
- **Check the size after every import.** The Stock page reports the
  models' size and extents. Compare one dimension you know, for example
  a 2 in plate that should read 50.8 mm (or 2.0000 in with the document
  in inches). A result 25.4 times off means the file's unit was not what
  was assumed.
- **The STEP facet budget** is the most triangles one STEP import may
  produce while tessellating (field on the model page; the `facetBudget`
  argument of `cam.import` in scripts). It is a quality limit, not a
  unit, but a budget set too low gives a coarse model. A value outside the
  allowed range is refused by name, and the budget is recorded with the
  model only when it differs from the default.

## Tools, tool libraries, machines and holders

- A **tool library** file (`format: camcraft-tools`) has an optional
  top-level `units`. The readers accept only `mm` (or no `units`) and
  refuse anything else ("lengths are mm"). Every number in the file is
  millimetres, whatever the document unit.
- A **machine file** (`.mch`) states its own length unit in
  `kinematics.default.units.length`: `mm`, or `inch` / `inches`. When it
  says inch, the reader multiplies every distance and distance-per-minute
  by 25.4 at load, so travels, safe retract and similar numbers are
  stated in the file's unit and used in millimetres. The machine property
  sheet shows the numbers as the file states them. `units.angle` is
  degrees. A length unit other than mm or inch refuses the file.
- **Tool holders and fixtures** are meshes, so they follow the model rule:
  the numbers in the mesh are millimetres unless a STEP file declares
  otherwise. A STEP file that declares no unit is refused there. Lengths given to tool and MCP commands are millimetres, not
  the document unit.

## Frames and coordinates

1. **Model frame.** The coordinates of the imported geometry (in
   millimetres, after the STEP unit is applied).
2. **Setup frame.** Each setup can hold the part differently: its part
   alignment is the rigid transform from the model frame into the setup,
   for which face is on the table or which edge runs along X. The setup's
   stock, WCS and fixtures are placed in that frame.
3. **WCS (work coordinate system).** Each setup owns exactly one: an
   origin and axes. The picks that set it (origin, X, Y, Z roles from
   viewport features, or the anchor grid) all queue the same undoable
   command. Heights "from the work offset" read this same WCS.
4. **Work offset word.** `G54` to `G59` is only the label the post writes
   for the setup's WCS. Changing it moves nothing; it selects no stored
   origin. An unlabelled setup writes `G54`. A one-file program in which
   two setups post the same word with different placements is refused,
   because the control holds one register per word.
5. **Machine frame.** The control's own coordinates. The program is in
   WCS terms: the post subtracts the WCS origin from world coordinates,
   and the control adds its stored `G54` value back at run time. The
   machine zero is whatever you set in the control by touching off.

The post carries translation only. A WCS that carries a rotation is
refused by name (it would post a program turned against the part); rotate
the part with the setup's part alignment instead.

**Why Z zero matters.** Every posted Z is relative to the WCS origin. If
the origin is on the stock top, a depth of 5 mm posts as `Z-5`; if it
is on the table, the same cut posts as a positive Z. Touch off the control
at the place you put the WCS origin, or the program is shifted by the
difference.

## Stock

Stock is a box, cylinder or from-model solid placed in the setup's frame,
like the WCS. The WCS origin is the zero the program is written from, and
the stock is only the material; one does not define the other. For a box or cylinder that stands square to the setup,
turning the part keeps the stock's allowance on each side, so top stays top.
A second setup can instead take the material the previous setup leaves.

## Rotary axes and lathes

- Angles are degrees everywhere: machine files, rotary travels and
  program words.
- A rotary axis's `axis` vector gives its direction, and the vector's
  sign is its sense: `[1, 0, 0]` means a positive A turns the tool
  right-handed about +X relative to the part. A machine that turns the
  other way states `[-1, 0, 0]`, and the Tormach post negates its A words
  for it.
- Lathe posts (Fanuc Lathe, Tormach lathe) use **diameter** X: the X word
  is twice the radius from the spindle axis, in the G18 (ZX) plane.

## Common mistakes

- A schema or script telling an agent the wrong unit for a holder offset
  would have placed the holder 25.4 times off: tool and MCP numbers are
  millimetres, whatever the document shows.
- An inch STL or OBJ imported without a size check (25.4 times too small).
- Expecting the document's inch unit to produce a `G20` program from a
  Fanuc-style post (see above).
- Measuring Z zero from the wrong place, or labelling a setup `G55` while
  the control's `G55` was never touched off.
- Reading a machine file's numbers as millimetres when the file says
  `inch`.
- Reading a lathe X word as a radius.
- Reading generator refusals in millimetres in an inch document: some
  refusal messages still print millimetres (tracked as issue #740).

See also [Setups](@/guide/set-up/setups.md) and [Posting](@/guide/check-and-post/posting.md).
