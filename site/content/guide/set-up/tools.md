+++
title = "Tools and tool libraries"
description = "Create tools, set holders and tool numbers, and exchange CamCraft tool libraries."
weight = 3
aliases = ["guide/tools/"]
+++

## Create and edit a tool

Open **Tools** in the Setup ribbon group. Choose **New tool…**, give the
tool a unique name, and select its type. Enter its cutter, flute, shank,
holder, speed and feed values. Choose **Add tool** to save it.

To change a document tool, select it in the browser and choose **Edit**.
Change the fields and choose **Apply**. **Close** closes the editor.
Edits and imports enter the undo history. A geometry change can make the
operations that use the tool stale. Generate those operations again before
posting.

The **Catalog** on the Tool library page offers built-in tools. Choose
**Add** beside an available entry to copy it into the document. The copy
can then be edited. In the editor, the separate **Catalog** section stores
part number, vendor, coating, helix angle and notes. These facts travel with
the tool in a project and in a library. Import matches tools by name, not
by part number.

**Centre cutting (may plunge into solid)** states whether the cutter can
plunge into material. Set it from the real tool's specification. Turning
tools also have orientation or touched-off corner settings. These describe
the point touched off on the machine; see [Turning](@/guide/operations/operations-turning.md).

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
that the tool shank fits the selected clamp. The applied holder becomes a
custom profile that can be edited.

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

## Import and export

**Export library…** writes all document tools to a `.camtools` file.
If the chosen filename has no extension, CamCraft adds `.camtools`.
The native format is JSON with `format: "camcraft-tools"`, `version: 1`,
`units: "mm"` and a `tools` array of `{name, definition}` records.
Lengths in the file remain millimetres even when the display uses inches.
The export includes attached bodies and profiles. Controller tool numbers
belong to the document and are not part of the exported tool definition.

**Import library…** accepts `.camtools` or `.json` files in this native
format. It also accepts a bare array of the same `{name, definition}`
records, such as the tools listed by MCP `get_job`. A `.json` extension
alone does not make a vendor library compatible. Fusion and HSMWorks
library formats are not supported.

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

## Tool numbering

**Tool number (0 = list position)** sets the machine's controller number.
Zero means the tool's position in the document tool list, starting at one.
An explicit number must be between 1 and 99999. A post can impose a smaller
controller limit. The tool number supplies the posted `T` word and the
length-offset number where that post uses one.

Two different tools cannot use the same explicit number. A later post
check also detects a clash between an explicit number and an automatic
list-position number. The check applies to the tools used on each machine,
including tools used by different setups on that machine. Assign distinct
numbers that match the real machine's tool table before posting.

## Common refusals

| Message or condition | Action |
| --- | --- |
| `the document has no tools to export` | Add a tool before exporting. |
| `not a tool library`, unsupported version or unsupported units | Use the native format above. Do not rename an unsupported vendor file. |
| A tool appears twice, has no name, or has invalid sizes | Correct the named record. Sizes must be finite and valid for the tool type. |
| `nothing to import` | Every imported tool already matches the document. No change is needed. |
| `replace would remove tool` | Keep that tool in the library or assign another tool to the named operation. |
| A retained or imported body does not fit | Correct the tool and body dimensions or placement. The import applies nothing. |
| A mate is absent or ambiguous | Supply the required frames and select the named target and piece coordinate systems. |
| `would both post as T` | Give the two named tools distinct controller numbers. |

These checks do not replace measurement, a correct machine tool table or
[proving out the program](@/guide/fundamentals/proving-out.md).
