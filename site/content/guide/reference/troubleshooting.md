+++
title = "Troubleshooting"
description = "Find the message you see, learn what it means and how to fix it."
weight = 4
aliases = ["guide/troubleshooting/"]
+++

<!-- Maintainers: every `### ` heading holds the message(s) as backticked
spans. Text between angle brackets and `...` are placeholders. Each
remaining fragment of 8+ characters must still appear in the sources under
crates/ - the test crates/app/tests/docs_troubleshooting.rs fails on a stale
one. Never write a message here that was not copied from the code. -->

This page is organised by what you see. Find the message (or its start; the
parts in angle brackets are filled in with your operation, tool or numbers),
read what it means, then follow the fix. For the rules behind the
refusals, see [Reading a refusal](@/guide/reference/refusals.md).

## The app will not open a window

### `no GPU adapter could render to this window`

**Means.** CamCraft draws with the graphics card, and none of the adapters
it tried could present to the window. The message goes on to list what was
tried and what was skipped.

**Causes.** An old or missing graphics driver; a remote desktop or virtual
machine with no GPU; a driver that works on one graphics backend only.

**Fix.**
1. Update the graphics driver and start the app again.
2. Pick a backend: start CamCraft with `--gpu-backend dx12`, `metal`,
   `vulkan` or `gl`, or set the environment variable `CAMCRAFT_GPU_BACKEND`
   to one of them. The flag wins over the variable.
3. If one card is wrong, name the one you want with `--gpu <part of its
   name>` (or `CAMCRAFT_GPU`).

### `unknown GPU backend '<name>' (expected dx12, metal, vulkan or gl)`

**Means.** The backend you asked for is not one of the four. CamCraft
refuses by name rather than silently picking another.

**Fix.** Correct the spelling in `--gpu-backend` or `CAMCRAFT_GPU_BACKEND`.
A flag with no value (`--gpu-backend needs a value ...`, `--gpu needs an
adapter name ...`) is refused the same way: give it one.

## Posting

All of these appear in the **Post refused** window; nothing was written to
the file.

### `no operations to post`

**Means.** The document has no operation to write.

**Fix.** Add an operation, then generate it and post again.

### `no operations to post: every operation is suppressed`

**Means.** There are operations, but every one is switched off, so the
program would be empty.

**Fix.** Un-suppress the operations that should be in the program.

### `<operation>: no toolpath yet - generate it first`

**Means.** The operation was never generated, so there is nothing to post.

**Fix.** Select the operation and click **Generate**, then post again.

### `<operation>: inputs moved since generation - regenerate before posting`

**Means.** The operation's toolpath is out of date. The model, stock, tool,
work offset or the operation's own settings changed after it was generated.
The operation page shows the same state as `Stale`, with its reason.

**Fix.** Select the operation and click **Generate**. To find every stale
operation, look for `Stale` on the operation pages. A same-named tool
loaded from a tool library file also makes the operations that use it
stale: regenerate them.

### `<operation>'s tool '<tool>' is gone; ...` / `... - fix the tool before posting`

**Means.** The tool the operation was built with was deleted, is no longer
a tool, or cannot cut this kind of operation (the message names what the
operation needs). Regenerating would not help, so the post is blocked by
name.

**Fix.** Open the operation, pick a suitable tool on its Geometry tab (or
re-add the tool), and **Generate** again.

### `<operation>: tool <n> is not in the library`

**Means.** The operation's tool number is not in the document's tool
library.

**Fix.** Add the tool, or pick another one for the operation, then
regenerate.

### `post blocked - <operation>: <n> warning(s): ...`

**Means.** The operation's own checks found something unsafe in its
toolpath. The flagged moves are marked in the viewport. The rows below
explain the common warnings.

**Fix.** Read the warnings, fix the setting or geometry each names, and
regenerate.

### `gouge: program cuts below the model in <n> cell(s), worst ...`

**Means.** Simulating the whole program found material removed from the
part, in a number of grid cells, with the worst depth and where.

**Causes.** A stock-to-leave that is too small or negative; a tool too big
for a corner; stock or model placed wrongly relative to each other; an
earlier operation's toolpath that cuts into the finish surface.

**Fix.**
1. Run **Simulate** and **Deviate** to see where.
2. Fix the operation that cuts there (check its stock to leave, tool and
   depths) and regenerate it.
3. Post again.

### `stock sim did not judge this program: ...`

**Means.** The stock simulation could not check the program, so CamCraft
cannot say it is safe. When no other check covers the operations either,
the post is refused rather than passed unjudged.

**Fix.** Read the reason after the colon. Typical reasons are missing
stock (see the `no stock yet` entry below) or an operation without a fresh result: set the stock, regenerate
the named operations and post again.

### `<operation> carries no generator-time verify verdict`

**Means.** The operation's toolpath was generated without the safety check
that runs at generation, so nothing judged it.

**Fix.** Regenerate the operation.

## Verify warnings (collisions)

These show on the operation page and in the viewport, and block the post.

### `shank/holder at move <n> collides with the uncut stock` (or `... the part or an island`)

**Means.** The tool's shank or holder reaches into material at that move:
the tool is too short for the depth, or the holder is too wide for the
space.

**Fix.** Use a longer tool or more stickout (**Tools**, the tool's
holder and length), a slimmer holder, or a shallower step-down, and
regenerate.

### `holder body at move <n> rubs the <part or uncut stock> - <x> mm short of clearance`

**Means.** The holder comes within less than its clearance of the part or
uncut stock.

**Fix.** The same as above: more stickout or a smaller holder.

### `rapid move <n> travels below the clearance plane inside stock`

**Means.** A fast move would run through stock instead of above it.

**Fix.** Raise the clearance height on the operation's Heights tab so
rapids stay above the stock, and regenerate.

### `<shank|holder|spindle housing|rapid|flute> at move <n> strikes a fixture`

**Means.** The tool, holder or a rapid hits a vise, clamp or other fixture
in the setup.

**Fix.** Move or resize the fixture on the setup's Fixtures page, or change
the operation so it keeps clear (a longer tool, a different approach), and
regenerate.

## Machine limits

### `machine <name>: <n> limit problem(s): ...`

**Means.** The program asks for more than the mounted machine states it can
do. Each problem is named. Two you will see often:

- `<operation>: tool <n> runs <rpm> rpm, above the machine's <max>` (or
  `below`): the spindle speed is outside the machine's range.
- `<operation>: move <i> reaches <v> in <axis>, the stroke is <lo>..<hi>`:
  a move goes past the end of an axis.

**Causes.** The program really is outside the machine's range, or the
machine file's number is wrong.

**Fix.**
1. For rpm, change the operation's spindle speed (or the tool's).
2. For a stroke, move the work offset or the part so the move stays inside
   the stroke, or choose another machine.
3. If the machine file's number is wrong, correct it in the machine
   editor against the machine's manual.

## Stock, setups and fixtures

### `(unresolved: no stock yet - set it up in Setup > Stock)`

**Means.** The operation needs stock to compute against and the setup has
none.

**Fix.** Go to Setup > Stock and define the stock (see
[Setups](@/guide/set-up/setups.md)), then generate again. The vise dialog says
the same in other words: a vise grips the stock, not the imported model.

### `the stock reaches <n> mm into the fixed jaw - place the vise against the stock first`

**Means.** The vise and the stock overlap, so the vise cannot be placed.

**Fix.** Move the vise clear of the stock (or fix the stock's size or
position on the Stock page) and place it again.

## Tools

### `tool '<tool>' states no lathe orientation (1-9) - set it in the tool editor`

**Means.** A lathe tool has no insert orientation, so CamCraft cannot tell
which corner is touched off.

**Fix.** Open the tool in the tool editor and set its lathe orientation to
the number 1 to 9 that matches the insert.

### `holder not set: <reason>` / `holder profile not set: <reason>`

**Means.** The holder you drew, imported or picked from the library was not
accepted. The reason follows. For a library holder it can be
`a Ø<d> mm shank does not fit - the holder takes <sizes>` or
`there is no holder '<name>' in the holder library`; for a tool body,
`the tool body reaches into the flutes`.

**Fix.** Choose a holder that takes the tool's shank, or correct the
profile so it starts at the end of the shank, and apply it again. The
tool keeps its holder cylinder until a profile is accepted.

## The program is not written

### `cannot write <path>: <reason>`

**Means.** The NC program could not be saved to the place you chose.
Nothing was posted.

**Causes.** The folder does not exist or is read-only, the file is open in
another program, or the disk is full.

**Fix.** Close the file elsewhere, pick a folder you can write to, and post
again.

### `<operation>: no post resolved for its setup`

**Means.** The operation's setup has no post processor.

**Fix.** Open the setup's Post page and choose a built-in post, or load a
`.cps` post script with **Load post (.cps)**.

## Importing a model

### `file is empty`

**Means.** The file you chose has no content.

**Fix.** Re-export the model from your CAD program and import the new file.

### `corrupt STL: <reason>` (also `corrupt OBJ: ...`, `corrupt STEP: ...`)

**Means.** The file could not be read as the format its extension claims.
The reason after the colon says where it went wrong. Nothing was imported.

**Causes.** A file cut short by a failed copy or download; a file renamed
to another extension; an export that wrote something other than a model.

**Fix.** Re-export from the CAD program, and check the file opens there.

### `unsupported file type: <ext>`

**Means.** CamCraft reads STEP, STL and OBJ files; this extension is none
of them.

**Fix.** Export the model as STEP (best, it keeps exact surfaces), STL or
OBJ.

### `'<ext>' import is not supported yet; import a STEP, STL, or OBJ file instead`

**Means.** IGES files (`.iges`, `.igs`) are refused by name: CamCraft has no
reader for them.

**Fix.** Re-export the model from your CAD program as STEP.

### `model exceeds the import limit`

**Means.** The model has more facets than an import may hold.

**Fix.** Export a coarser mesh, or a STEP file (it is tessellated to a
budget on import).

### `imported mesh contains non-finite coordinates`

**Means.** The file holds a coordinate that is not a number, so the model
cannot be trusted.

**Fix.** Repair or re-export the model in the CAD program.

### `the file states two length units (<a> mm and <b> mm per unit)`

**Means.** A STEP file declares more than one length unit and they
disagree. CamCraft refuses rather than guess your part's scale.

**Fix.** Re-export the STEP file with a single length unit. A file in
inches that arrives 25.4 times too small or large was exported without a
stated unit: check the size on the model page and re-export in millimetres.

### `Reduced fidelity: surfaces are approximated to within <tolerance>`

**Means.** The STEP model was tessellated more coarsely than the usual cap
to stay within the facet budget, and the model page says so.

**Fix.** If you need tighter checks, raise the **STEP facet budget** on the
model page and import again.

### `<operation>: model '<model>' was imported at reduced fidelity (...) and this kind of operation cannot be verified against it; ...`

**Means.** The operation's safety checks need a finer model than the one
imported, so the post is blocked rather than passed unjudged. A related
form ends `imported too coarsely to verify; re-import with a larger STEP
facet budget`.

**Fix.** Re-import the model with a larger **STEP facet budget**, then
regenerate.

## Turning

### `Facing needs a turning insert, '<tool>' is a <kind>`

**Means.** A turning operation was given a tool that is not a turning
insert. The same wording starts for Profile Rough, Profile Finish,
Chamfering, Taper Turning and Multi-Pass Turning.

**Fix.** Pick a turning insert on the operation's Geometry tab.

### `the part's axis of revolution is not the lathe spindle: ...`

**Means.** The part leans, or sits off the spindle centre line, so turned
as posted it would cut a taper or an eccentric. The message gives the
lean and the offset.

**Fix.** Align the part's axis with world Z and move the work offset onto
the spindle, as the message says, then regenerate.

### `a Plasma Profile operation cannot be posted through a lathe post` (also Probe)

**Means.** A lathe post turns the work at the tool's speed; an operation
that is not turned would be written wrongly. A message starting
`a <kind> operation would be posted on the work spindle` is the same
refusal for a milling operation.

**Fix.** Post it through the post for its own machine, cut it as C-axis
live tooling where the machine has it, or suppress the operation.

## Plasma

### `a plasma profile mounts a plasma torch, not a <kind> - its diameter is the kerf width`

**Means.** The operation's tool is not a torch.

**Fix.** Pick a torch tool; its diameter is the kerf width.

### `<operation>: the plasma laws have no toolpath to judge - generate it first`

**Means.** A plasma operation has no toolpath, so the cut-order and kerf
checks had nothing to judge.

**Fix.** Select the operation, click **Generate**, then post again.

### `the 3D stock sim does not run for plasma`

**Means.** Not a failure: the report says a torch is judged by the plasma
laws (cut order, kerf crossing and the like) and not by the 3D stock
simulation.

**Fix.** None; if the plasma laws refuse the program the post names the
problem.

## Not covered yet

Wire and sinker EDM refusals are not on this page yet. If you meet a
message that is not here, [report it](https://github.com/NeoTech/Camcraft/issues) with the text
and, if you can, the project file.
