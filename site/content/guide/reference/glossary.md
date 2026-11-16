+++
title = "Glossary"
description = "What CamCraft's terms mean, in the words the app uses, with the guide page that covers each."
weight = 2
aliases = ["guide/glossary/"]
+++

Each entry says how CamCraft uses the term and links to the guide page that
covers it in depth. Terms are listed alphabetically.

**Adaptive.** A Milling 2D operation kind that roughs an area in adaptive
passes, and also one of the 3D Pocket strategies. See [Operations](@/guide/operations/_index.md).

**Capabilities.** The mask a `.cps` post declares in its manifest: 1 for
milling and 2 for turning. A post that declares no mask is read as not
turning, so CamCraft never guesses what a dialect speaks. See [Posting](@/guide/check-and-post/posting.md#post-processors).

**Carried stock.** The material a setup leaves behind. A later setup can
take it as its stock source instead of the raw stock, so verification and
rest machining see the material already removed. New setups can also carry
stock and the work offset over from the active setup. See [Setups](@/guide/set-up/setups.md#carrying-stock-to-the-next-setup).

**Collision.** A move that strikes a fixture. The post checks the whole
program for collisions and refuses to write it when a move hits one. See
[Posting](@/guide/check-and-post/posting.md#what-is-checked-before-anything-is-written).

**Dialect.** The controller's own flavour of G-code, which a post script
writes. The Post page offers the dialects by name. See [Posting](@/guide/check-and-post/posting.md#post-processors).

**Generate.** Computes an operation's toolpath and checks it. A generate
that cannot make a safe path stops with a message that names the setting or
geometry at fault, and never leaves a partial path. See [Operations](@/guide/operations/_index.md#generating).

**Gouge.** Material removed from the part where the program cuts into the
model. The post refuses a program with a gouge, and Deviate shows where it
is. See [Reading a refusal](@/guide/reference/refusals.md#common-post-refusals).

**Holder.** The tool holder that clamps the cutter. A holder profile is
drawn in the tool editor or taken from the holder library, and a holder
that hits the part is flagged as a warning on the operation. The guide has
details in [Tools and tool libraries](@/guide/set-up/tools.md#holders-and-profiles).

**Holder body.** The holder's body, the section above its nose that the
tool editor's Body tab draws. It is checked against the part like the rest
of the holder. See [Tools and tool libraries](@/guide/set-up/tools.md#imported-and-modular-tool-bodies).

**Live tooling.** A driven tool in a lathe or mill-turn turret. When a path
states spindle C angles it is live-tool work: the S word drives the live
tool, and the work turns only on C. See [Setups](@/guide/set-up/setups.md#machines).

**MCP.** The Model Context Protocol. `camcraft-cli mcp` serves CamCraft as
an MCP server, so an AI agent can drive it headlessly through the same
undoable commands the window uses. See [Posting](@/guide/check-and-post/posting.md#headless-posting).

**Mill-turn.** A machine that mills and turns in one setup. Mill-turning on
the Setup group shows both the milling and the turning operation groups.
See [Setups](@/guide/set-up/setups.md#machines).

**Model.** An imported 3D mesh or STEP file (STL, OBJ or STEP) that
describes the part. Import it with **Model** on the File group. See [First run](@/guide/getting-started/first-run.md#a-first-toolpath).

**Operation.** One machining step: a tool, an area to cut, and how to cut
it, set on its Geometry, Heights, Passes, Lead in / Lead out, Feeds and
(on a few kinds) Machine tabs. See [Operations](@/guide/operations/_index.md).

**Part.** The physical piece being machined. The models describe it, the
stock is cut into it, and Deviate compares the machined stock with it. See
[First run](@/guide/getting-started/first-run.md#a-first-toolpath).

**Post.** A post processor: a `.cps` script that turns the operations into
one controller's NC program. A setup picks its post on the Post page, or
loads its own with **Load post (.cps)**. See [Posting](@/guide/check-and-post/posting.md#post-processors).

**Reduced fidelity.** A STEP model imported with a coarser mesh than the
importer promises. The model page flags it, and the program gate refuses to
post on verdicts it cannot honour until the model is imported again at a
higher step budget. See [First run](@/guide/getting-started/first-run.md#a-first-toolpath).

**Refusal.** CamCraft declining an action it cannot make safe. A refused
post, change or generation writes nothing and names its reason; it is not a
crash. See [Reading a refusal](@/guide/reference/refusals.md).

**Rest material.** The material still uncut after the earlier operations,
which later operations and rest machining work from. **Frame rest** takes
the view to the worst uncut spot. See [Operations](@/guide/operations/_index.md#checking).

**Setup.** One way the part is held on one machine: its stock, work offset,
machine, post, fixtures, tools and operations. See [Setups](@/guide/set-up/setups.md).

**Setup sheet.** A text page for one setup listing its tools in change
order, with cut and rapid lengths and the estimated time. **Export setup
sheets…** writes them; posting does not. See [Setups](@/guide/set-up/setups.md#setup-sheets).

**Stale.** An operation whose toolpath no longer matches its inputs: the
model, stock, tool, work offset or its own settings changed after it was
generated. A stale operation blocks posting until it is generated again. See
[Operations](@/guide/operations/_index.md#generating).

**Stepdown.** The depth of cut per pass along the tool axis, a setting on
the Passes tab. See [Operations](@/guide/operations/_index.md#the-settings-tabs).

**Stepover.** The sideways distance between adjacent passes, a setting on
the Passes tab. Auto applies the scallop law where the kind offers it. See
[Operations](@/guide/operations/_index.md#the-settings-tabs).

**Stick-out.** The length from the tool tip to the holder's nose face: the
flute plus the shank, shown on the tool's holder profile. See [Tools and tool libraries](@/guide/set-up/tools.md#holders-and-profiles).

**Stock.** The raw block the part is cut from: a box, a cylinder or a copy
of a model, set with **Stock** on the Setup group. Verify and simulation cut
into it. See [Setups](@/guide/set-up/setups.md).

**Stock to leave.** Material left on the part for a later operation to
remove, set as **Stock to leave** on the Passes tab. See [Operations](@/guide/operations/_index.md#the-settings-tabs).

**Tool spindle.** The spindle that turns the cutter, as opposed to the
work spindle that holds the chuck. A milling post's S drives it. See [Setups](@/guide/set-up/setups.md#machines).

**Toolpath.** The cutter moves an operation generates and caches on the
operation. **Paths** shows every cached toolpath at once. See [Operations](@/guide/operations/_index.md#generating).

**Verify.** The check on the whole program before it is written: the
simulated stock has no gouge, no move strikes a fixture, and no move
exceeds a limit the mounted machine states. See [Posting](@/guide/check-and-post/posting.md#what-is-checked-before-anything-is-written).

**WCS.** The work coordinate system the program is written in. Each setup
owns one, and the post labels it G54 to G59. See [Setups](@/guide/set-up/setups.md#work-offset).

**Work offset.** The placement of the WCS on the stock, set with **Origin**
on the Setup group. Heights measured from the work offset read the setup's
offset. See [Setups](@/guide/set-up/setups.md#work-offset).

**Work spindle.** The spindle that holds the chuck on a lathe or mill-turn.
A turning section opens on it, and a constant-rpm (G97) speed over the
setup's chuck rating is refused rather than capped. See [Setups](@/guide/set-up/setups.md#machines).
