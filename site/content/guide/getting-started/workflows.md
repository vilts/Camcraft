+++
title = "Common workflows"
description = "Task recipes from facing a block to re-posting for another machine."
weight = 4
aliases = ["guide/workflows/"]
+++

Short how-to recipes, one per task. They are not a reference: each links
the page that explains the screen in depth. Every recipe ends the same way:
{{ <icon name="act-simulate" /> }} **Simulate**, {{ <icon name="tree-stock" /> }} **Deviate**, then {{ <icon name="act-post" /> }} **Post**, which refuses by name if
something is unsafe (see [Reading a refusal](@/guide/reference/refusals.md)). A
passing post is not a guarantee; read the program and dry-run it
([Posting](@/guide/check-and-post/posting.md)).

The recipes use one sample part: `testdata/models/Milling-test.step` in the
repository, a small STEP model. Any STEP part of your own works the same.

All recipes start the same way: {{ <icon name="folder-open" /> }} **Model** to import the part, {{ <icon name="tree-stock" /> }} **Stock** to
describe the block, {{ <icon name="tree-wcs" /> }} **Origin** to place the work offset, {{ <icon name="tools" /> }} **Tools** >
**New tool…** to make the cutters
([First run](@/guide/getting-started/first-run.md)).

## Face the stock top

1. On the **Stock** page pick Box, and use **Fit to Models** so the block
   covers the part. Each face of the box has its own allowance: give the top
   the extra material you want faced off.
2. Make a flat or face mill under **Tools** > **New tool…**, larger than the
   pass width you want.
3. Add **Face** (Milling 2D). On **Heights**, set the top at the stock top
   and the bottom at the depth to remove. On **Passes**, set **Stepover**
   and **Stepdown**.
4. Click **Generate**, then **Simulate**: the tool must clear the whole top
   of the stock. **Deviate** shows any material left.

More: [Operations](@/guide/operations/_index.md).

## Square up a block in two setups

1. Set up the first side as above: **Face**, then **Contour** around the
   outside, leaving the stock you want to keep for the final pass.
2. Setup > **Setups** > **New setup**, with **Carry stock and WCS over**
   ticked. The new setup starts from a copy of the first one's stock and
   work offset.
3. Re-place the stock and work offset to describe the flipped block (the
   setup's Settings page has the alignment, including flips such as
   **Flip over X**).
4. On the new setup's Settings page, set the stock source to what the first
   setup leaves behind, so verification sees the faced and profiled block.
5. Add **Face** to the final thickness. **Generate**, **Simulate**.
6. **Post** writes the program for each setup. CamCraft refuses a carried
   stock it cannot verify, such as between setups on different machines.

More: [Setups](@/guide/set-up/setups.md).

## Pocket and contour a 2D part

1. Import the STEP model and set the stock as above.
2. Add **Pocket** (Milling 2D). On **Geometry**, pick the pocket's floor
   face or its loop in the viewport (a click picks the whole face).
3. On **Heights** set the bottom at the pocket floor. On **Passes** set
   **Stepover**, **Stepdown** and **Stock to leave** so the roughing leaves
   material for a finish pass.
4. Click **Generate**. Add a second **Pocket** or a **Contour** on the same
   geometry with **Stock to leave** at zero for the finish pass, and
   **Contour** around the outside the same way.
5. **Simulate** and **Deviate**; the **Frame gouge** and **Frame rest**
   buttons take the view to the worst spots.

## Drill and tap a hole pattern

1. Add **Drill** (Drilling). Click **Detect holes** to find the holes in the
   model, or pick the hole edges yourself.
2. Choose the cycle (**Cycle**): spot, drill, peck, deep or tap. For peck,
   set **Peck depth**.
3. Add one operation per tool: a spot drill first, then the drill, then a
   tap (or a **Thread** operation under Milling 2D to thread-mill).
4. **Generate** each, **Simulate**, and **Post**. Rigid tapping needs a
   machine that states support for it; a machine without it is refused by
   name.

## Chamfer and deburr edges

1. Add **Chamfer** (Milling 2D) for edges you pick, or **Deburr** (Milling
   3D) to break every edge.
2. Use a chamfer tool or a small ball mill; set its size under **Tools**.
3. **Generate**, then **Deviate** to check the edge break against the model.

## 3D rough and finish a surface

1. Add **Adaptive** (Milling 2D) or a 3D roughing strategy under **3D
   Pocket**, with **Stock to leave** on **Passes** for the finish.
2. Add **3D Finish** and choose a strategy (parallel, scallop, waterline,
   pencil and others). Use a ball mill.
3. For rest machining, use the **Rest roughing** setting on the rougher so
   it works on what the earlier operation left.
4. **Generate** each, **Simulate**, **Deviate**: look for gouge first, rest
   second.

## Turn a simple part on a lathe

1. Click the machine button on the Setup group and pick **Turning**. The
   ribbon now shows the Turning group.
2. Set the stock as a cylinder and the work offset at the chuck or the face.
3. Add **Facing**, **Profile Rough**, **Profile Finish**, **Groove
   Rough**, **Threading** and **Part**, in that order, each with its own
   tool.
4. **Generate** each in order, **Simulate**, then **Post** with the Fanuc
   Lathe post.

## Cut a sheet part

On a router: follow the full walkthrough in
[Your first router part](@/guide/getting-started/first-router-part.md)
(Sienci AltMill 4x4, grblHAL, corner `G54`, `Drill`, `2D Pocket`, and `2D Contour`
with holding tabs — **Tab count**, **Tab width** and **Tab height** under **Lead
in / Lead out** — plus RichAuto DSP A11 shop-router notes).

On a plasma table: click the machine button and pick **Plasma cutting**,
add a **Plasma Profile**, and set **Compensation** and **Direction** on its
page. Make the torch tool with **Kerf width**; **Pierce height** is on the
operation's page.

**Simulate** and **Post** as usual.

## Re-post a job for a different machine

1. Open the setup's **Machine** page and **Load machine (.mch)**, or pick
   another machine, then pick its post on the **Post** page.
2. Regenerate the stale operations (**Regenerate all stale**, or
   **Generate** on each).
3. **Post**. Read what is refused: a machine-limit refusal names the axis
   or setting and the limit. Fix the setup, not the limit.

## Use the tool library

**Tools** can export a library (**Export library…**) and **Import
library…** reads one back, so tools are reused across jobs.

## Save a panel arrangement

Open **View > Panels > Panel setups...** after arranging the panels. Enter
a name and choose **Save current setup**. Choose **Load** beside a saved
arrangement to restore it, or **Delete** to remove it. **Reset layout** returns to the
default arrangement.

The saved arrangement keeps open panels, split sizes, tab order, active
tabs, collapsed panels and floating-window positions. **Auto-load** is on
by default and restores the arrangement on the next start. Turn it off
to start with the default layout. A corrupt or older settings file falls
back to the default layout and reports why.

Panel setups are application settings in `panel-setups.json`, separate
from the project. Loading an arrangement changes view state and does not
enter undo history.

## Not yet

Importing a tool library from another system, and a router machine mode
(tabs work today on a mill Contour), are not described here because they
are not on `main` yet.
