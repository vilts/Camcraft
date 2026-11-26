+++
title = "Tools and tool libraries"
description = "Create tools, set holders and tool numbers, and exchange CamCraft tool libraries."
weight = 3
aliases = ["guide/tools/"]
+++

## Create and edit a tool

Open {{ <icon name="tools" /> }} **Tools** in the Setup ribbon group. Choose **New tool…**, give the
tool a unique name, and select its type. Enter its cutter, flute, shank,
holder, speed and feed values. Choose **Add tool** to save it.

To change a document tool, select it in the browser. Its settings show
directly in the Properties pane: geometry, holder, cutting data, tool
numbers and identification. The setup's Tool list opens
the same form. Change the fields and choose **Apply**. The Tool list's
**Edit** button opens the wider editor, and **Close** closes it. Browsing the catalog, **Add**,
**New tool…** and import and export live in the separate **Tool library**
pane. Open it from **Tools** or from **Tool library…** in the browser; it
does not replace the tool in Properties.
Edits and imports enter the undo history. A geometry change can make the
operations that use the tool stale. Generate those operations again before
posting.

The catalog in the Tool library pane offers built-in tools. Choose
**Add** beside an available entry to copy it into the document. The copy
can then be edited. In the editor, the separate **Tool information** section stores
part number, vendor, coating, helix angle and notes. These facts travel with
the tool in a project and in a library. Import matches tools by name, not
by part number.

A tap stores its hand. The tap editor has a **Left-hand tap (posting
unavailable)** checkbox; older tools are right-hand. No built-in post
writes a left-hand tapping cycle, so CamCraft refuses a left-hand tap at
posting, before any NC file is written. The message names the operation and
the tool: `<operation>: left-hand tap <tool> is not supported by posting`.
Right-hand output is unchanged. The hand travels with the tool in
projects, tool libraries and undo.

Choose **Edit tool shape** to open `Tool shape: <name>`. The holder
library comes first, followed by the **Body** and **Profile** tabs. Body
imports and drawn profiles are edited there; Properties keeps the tool's
settings. **Close** closes the shape window and keeps the tool.

The side preview draws the holder bands used by the collision check.
For a machine-default holder you have not changed, it includes the
conservative parameter-cylinder floor that the checks use. The 3D body
view appears while a Pick button is armed for a tip or axis pick.

A **Waterjet nozzle** is a tool kind for waterjet machines (see
[Machine types](@/guide/check-and-post/machines.md)).

**Centre cutting (may plunge into solid)** states whether the cutter can
plunge into material. Set it from the real tool's specification. Turning
tools also have orientation or touched-off corner settings. These describe
the point touched off on the machine; see [Turning](@/guide/operations/operations-turning.md).

## Cutting data for wood and plastics

On an operation's **Feeds** tab, open **Cutting data** and select
**Workpiece material**. The choices include **Softwood**, **Hardwood**,
**MDF**, **Plywood**, **Acrylic (PMMA)** and **HDPE**. State the tool's
**Tool material** and **Flutes** too. The suggestion shows cutting speed,
chip load, spindle rpm and feed. **Apply suggestion** writes the spindle
and feed as one undoable change; first apply or revert any other edits
on the operation page.

For example, for a plywood pocket with a carbide end mill, select
Plywood, Carbide and the cutter's actual flute count. Read the suggested
rpm and feed and any machine-limit notes before applying. The router
material tables use the cutter diameter to choose chip load. A spindle
limit changes the suggested rpm and recomputes feed at that rpm to keep
the chip load; the readout says when it does so. These are starting
suggestions: check them against the cutter and machine specifications.

A missing material, tool material or flute count produces **No
suggestion** with the reason. Supply the named input; CamCraft does not
guess it. The tool editor's **Cutting data** group also has **Add material
row** for storing your own rpm and feed for each material. **Apply** saves
those rows. A repeated material or a non-positive rpm or feed is refused
by the tool's name. Rows and the material choice travel with the project.

## Holders and profiles

A milling tool without a custom body uses the holder cylinder stated by its
diameter and length. The editor's **Profile** tab offers **Draw holder**
and **Draw cutter**. A custom holder profile is a table of axial positions,
radii and optional arc radii. Choose **Apply profile** to store it.
**Draw cutter** edits the cutter, neck and shank outline; choose
**Apply cutter** to store that outline.

The **Holder library** below the holder profile offers built-in BT30,
BT40, CAT40, Tormach TTS and straight-shank holders. Select **Library
holder**, set the gauge length and projection, and choose **Apply holder**.
Gauge length runs from the spindle gauge line to the holder nose.
Projection runs from the tool tip to the holder nose. The picker checks
that the tool shank fits the selected clamp. The tool stores the applied
holder by its library name, and the Properties show the holder diameter
and length of that entry. The profile table shows its outline. If you
edit a point and choose **Apply profile**, the holder becomes a custom
profile.

The library holder is a reference: its entry name, gauge length and
projection travel with the tool. A corrected library entry therefore
reaches tools that name it. An unknown entry or an invalid fit is refused
by name; CamCraft does not substitute another holder. Applying an
unchanged outline keeps the reference. Editing a point makes it custom.

**Breaking file change:** holder references require project format
version 70 or later, and `.camtools` version 4 or later. Older builds
that support only project version 69 or tool-library version 3 refuse
these newer files. An older drawn copy loads as a custom profile with
the same outline. An older holder cylinder keeps its own dimensions;
neither is silently converted into a library reference.

Check the dimensions against the real assembly. A library entry does not
measure the holder installed on your machine.

## Imported and modular tool bodies

Add the tool before importing a body. On **Body**, choose **Import body…**
and select a STEP, STL or OBJ holder or assembly model. For a spindle tool,
set **Tool axis (model direction, tip to spindle)** and **Tip offset
(model origin above the tip)**. **Pick tip** and **Pick axis** can place
these from the preview. Choose **Apply body** to save the placement.
Orbit, zoom and picks change the preview until applied.

A turning holder uses radial and spindle directions and a tip point in the
turning frame. A boring bar can choose its body frame. Check the preview
and any insert-zone settings before applying the body.

An imported body can contain multiple modular pieces. The **Assembly**
section lists each piece and its named coordinate systems. Choose
**Attach piece…** to add a STEP, STL or OBJ adapter or clamping unit.
Blank **Target CSYS** and **Piece CSYS** fields request an automatic mate
between a unique ISO 13399 `MCS` and `CSW` pair. To resolve an ambiguous
mate, choose **Target piece** and enter the two coordinate-system names.
The piece list uses indexes starting at zero. **Remove** removes one piece
as an undoable change; it cannot remove the last piece.

STL and OBJ do not carry named STEP coordinate systems. If automatic
assembly has no suitable frames, use suitable named STEP models or the
explicit placement options exposed through [MCP](@/guide/automate/mcp.md).
CamCraft refuses missing or ambiguous mates instead of guessing.
The combined body is saved with the project and tool library.

**Clear body** removes an applied body. A milling tool then returns to its
holder cylinder. A turning tool without a body does not have a judged
holder. Read the operation's warnings and [refusals](@/guide/reference/refusals.md).

## Plasma torch recipes

A plasma torch can carry cutting-data recipes for a material and an exact
plate thickness. A recipe stores cut feed, kerf and optional pierce and
height data. Projects, tool libraries and MCP tool definitions preserve
these rows. The tool page preserves them but does not show them as plain
material rows or offer a recipe editor yet.

The stored recipe does not change the operation or posted program yet.
See [Stored torch recipes](@/guide/operations/operations-plasma.md#stored-torch-recipes)
for the fields, validation and current limits.

## Import and export

**Export library…** writes all document tools to a `.camtools` file, a
Tormach PathPilot `.csv` tool table, or a Hurco WinMax `.TCM` library
(selected by the save filter or extension).
If the chosen filename has no extension, CamCraft adds `.camtools`.
The native format is JSON with `format: "camcraft-tools"`, a `version`,
`units: "mm"` and a `tools` array of `{name, definition}` records.
The writer selects the version required by the stored tool data; a
library with holder references requires at least version 4.
Lengths in `.camtools` remain millimetres even when the display uses
inches. Controller tool numbers belong to the document and are not stored
in `.camtools` definitions.

**Import library…** accepts `.camtools` or `.json` files in this native
format, Tormach PathPilot `.csv` mill tool tables, and Hurco WinMax
`.TCM` tool libraries. It also accepts a bare JSON array of `{name,
definition}` records, such as the tools listed by MCP `get_job`. A
`.json` extension alone does not make a vendor library compatible. Fusion
and HSMWorks library formats are not supported.

Choose the import mode before opening the file:

| Mode | Effect |
| --- | --- |
| **Add (rename duplicates)** | Keep existing tools. Skip an identical tool. Rename a different tool with a used name to `Name (2)`, then the next free suffix. |
| **Update same-named** | Update tools with matching names in place. Keep their controller numbers and operation references. Add new names. |
| **Replace library** | Update matching names and add new names. Remove document tools absent from the file, unless an operation uses one. |

**Import tool holders** controls whether imported bodies accompany the
tools. It is checked by default and remembered as a user preference.
In Add mode, clearing it adds bare tool records. In Update or Replace
mode, clearing it keeps an existing document holder; a file with no body
also keeps that holder. The retained holder must fit the incoming tool.
An imported body change can make operations stale.

The whole file and the planned import are checked before any change.
An import is one undo step. The result states which tools were added,
renamed, skipped, updated, replaced or removed, and which holders arrived.
See [CLI](@/guide/automate/cli.md) and [MCP](@/guide/automate/mcp.md) for headless exchange.

## Machine tool tables (Tormach PathPilot and Hurco WinMax)

CamCraft imports and exports controller tool tables so the tool numbers
and diameters in your job match your machine or simulator without re-typing
every row.

### Tormach PathPilot (.csv)

**Bring tools from PathPilot into CamCraft:**

1. On the PathPilot controller (or PathPilot Hub), open the **Offsets** tab
   and choose **Export**.
2. Type a filename and save the `.csv` file. On a machine, copy the file to
   a USB drive from the **File** tab.
3. In CamCraft, match the document units (**mm** or **Inches**) to your
   PathPilot machine units (or pass `"units"` in MCP/script calls).
4. Open **Tool library**, choose an import mode (use **Replace library** if
   replacing an existing tool set that shares `T` numbers), and choose
   **Import library…** to pick the `.csv` file.

CamCraft reads each populated row's tool number (1–1000), diameter,
description, and optional tokens (`R:` corner radius, `loc:` flute length,
`<n>FL` flute count, material, and Tormach `[part]` number). It keeps the
PathPilot `Description` as the tool name and skips empty tool slots.
PathPilot `Z offset` values are machine placeholders and are not treated
as tool stick-out; negative `Max RPM` values (such as `-2.0` on a probe)
are markers, not spindle speeds. Complete missing fields such as flutes,
flute length, speeds/feeds, and holder geometry in CamCraft before
generating toolpaths.

**Send CamCraft tools to PathPilot Hub or the machine:**

1. Assign each tool the controller number you want to post (`1`–`1000`).
   When posting, CamCraft writes those controller tool numbers as `T<n>`
   (and `H<n>` length offsets).
2. In **Tool library**, choose **Export library…** and save with a `.csv`
   extension (using the document units that match your PathPilot machine
   or Hub profile). CamCraft writes the required `Mill Tool Table` header
   row and `Tool number,Description,Z offset,Diameter,...` columns with
   zeroed offsets.
3. In PathPilot or PathPilot Hub, open the **Offsets** tab, choose
   **Import**, and select the exported `.csv` file so the simulator or
   machine holds the same tool numbers and diameters as your posted `.nc`
   program. Touch off or enter tool Z offsets on the controller before
   cutting.

### Hurco WinMax (.TCM)

**Import library…** also reads Hurco WinMax `.TCM` XML tool libraries,
preserving tool numbers, names, diameters, flute counts, flute/tool
lengths, angles, tip radii, thread pitches, manufacturer names, and
`AbsoluteGaugeLength` (as the holder gauge length). Hurco `ToolType` codes
are mapped strictly to CamCraft tool kinds; any WinMax tool type that
CamCraft has no matching kind for (such as keyseat, dovetail, corner
round, or back spotface cutters) is refused by name rather than guessed.

**Export library…** with a `.TCM` extension writes UTF-8 XML with a BOM
and CRLF line endings containing only the tool geometry fields CamCraft
owns. Control state (`UsedTime`, `Flags`, `GeometryFlags`, and probe
offset indexes) is omitted so importing into WinMax does not reset tool
life or probe data. Acceptance of exported partial `.TCM` files on a
WinMax 9.01 control still requires verification on the machine.

## Tool numbering

**Tool number** sets the machine's controller number.
Zero means the tool's position in the document tool list, starting at one.
An explicit number must be between 1 and 99999. A post can impose a smaller
controller limit. The tool number supplies the posted `T` word and the
length-offset number where that post uses one. Tools imported from a
PathPilot `.csv` or Hurco `.TCM` table keep their file's controller
numbers (`T<n>`), and importing is one undoable step.

Two different tools cannot use the same explicit number. A later post
check also detects a clash between an explicit number and an automatic
list-position number. The check applies to the tools used on each machine,
including tools used by different setups on that machine. Assign distinct
numbers that match the real machine's tool table before posting.

## Common refusals

| Message or condition | Action |
| --- | --- |
| `the document has no tools to export` | Add a tool before exporting. |
| `not a tool library`, `not a PathPilot mill tool table`, `not a Hurco WinMax .TCM file`, unsupported version or unsupported units | Use a valid `.camtools`, PathPilot `.csv`, or Hurco `.TCM` file. Keep PathPilot row 1 intact. |
| A tool appears twice, has no name, or has invalid sizes | Correct the named record. Sizes must be finite and valid for the tool type. |
| Hurco tool type has no CamCraft tool kind | Remove or convert the named unsupported Hurco tool type (for example keyseat, dovetail, or back spotface). |
| `tool number T<n> is already '<other>'s` | Use **Replace library** mode or renumber the conflicting document tool first. |
| `nothing to import` | Every imported tool already matches the document. No change is needed. |
| `replace would remove tool` | Keep that tool in the library or assign another tool to the named operation. |
| A retained or imported body does not fit | Correct the tool and body dimensions or placement. The import applies nothing. |
| A mate is absent or ambiguous | Supply the required frames and select the named target and piece coordinate systems. |
| `<operation>: left-hand tap <tool> is not supported by posting` | Use a right-hand tap, or thread another way. No post writes a left-hand cycle. |
| `would both post as T` | Give the two named tools distinct controller numbers. |

These checks do not replace measurement, a correct machine tool table or
[proving out the program](@/guide/fundamentals/proving-out.md).
