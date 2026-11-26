+++
title = "First run"
description = "The window, the ribbon, and a first toolpath."
weight = 2
aliases = ["guide/first-run/"]
+++

## Trying it in the browser first

When this site's release has a browser version,
[CamCraft in the browser](@/app.md) opens it with nothing to install (it
needs a browser with WebGL2: a current Chrome, Firefox, Edge or Safari), so
you can look around the window described below; if it does not, that page
says so, and the front page says which it is. The browser version opens a
model or a project with the browser's file picker. Save and Save As download
the project. Post downloads the program when Output is set to one combined
program; one file for each operation is refused. Its other file commands
(recent files, machine, post, theme, fixture and tool library files, and the
setup sheet, tool list and stock exports) are greyed, or refused with the
reason on the status line. For real work, install a
[desktop build](@/guide/getting-started/install.md).

## The window

Shortcuts on this page are written with Ctrl. On macOS, press Cmd in their place
(Cmd+N, Cmd+Shift+O and so on): CamCraft binds them to the command key there.

- **The ribbon** across the top holds the commands, in groups:
  - **File:** {{ <icon name="file-plus" /> }} **New** (Ctrl+N), {{ <icon name="folder-open" /> }} **Model** (open a mesh model, Ctrl+O), {{ <icon name="file-import" /> }} **Open** (a
    project, Ctrl+Shift+O), {{ <icon name="device-floppy" /> }} **Save** (Ctrl+S), {{ <icon name="device-floppy" /> }} **Save As** (Ctrl+Shift+S) and {{ <icon name="act-post" /> }} **Post**
    (Ctrl+P).
  - **Setup:** {{ <icon name="tree-setup" /> }} **Setups**, {{ <icon name="tree-stock" /> }} **Stock**, {{ <icon name="tree-wcs" /> }} **Origin** (the work offset), {{ <icon name="tools" /> }} **Tools** and the
    machine button, which switches the workspace between Milling, Turning
    and Mill-turning (a lathe with live tooling).
  - **Operation groups** for the machine you are on: Milling 2D, Milling
    3D, Drilling and Multi-axis on a mill; Turning on a lathe. Each group
    opens a list of its operations. Right-click (Control-click or a two-finger
    click on a Mac) an operation to pin it to the ribbon (up to four pins).
  - **Modify:** {{ <icon name="arrow-back-up" /> }} **Undo** (Ctrl+Z), {{ <icon name="arrow-forward-up" /> }} **Redo** (Ctrl+Y), {{ <icon name="act-generate" /> }} **Run** (generate the selected
    operation) and {{ <icon name="trash" /> }} **Delete**.
  - **Inspect:** {{ <icon name="act-frame-all" /> }} **Frame** (F), {{ <icon name="act-show-paths" /> }} **Paths** (show every toolpath), {{ <icon name="act-simulate" /> }} **Simulate** and
    {{ <icon name="tree-stock" /> }} **Deviate**.
  - **View:** Panels (show or hide the browser, properties, machines and
    AI Chat panes, or reset the layout) and Theme.
- **The browser** lists the document: {{ <icon name="tree-part" /> }} models, {{ <icon name="tree-setup" /> }} setups with their {{ <icon name="tree-stock" /> }} stock,
  {{ <icon name="tree-machine" /> }} machine, {{ <icon name="act-post" /> }} post, {{ <icon name="tree-fixture" /> }} fixtures and {{ <icon name="tools" /> }} tools, and the {{ <icon name="tree-op-group" /> }} operations.
- **The properties pane** shows the page of whatever you selected. For a
  tool, it shows that tool's settings; browsing the tool library happens in
  the separate **Tool library** pane (see [Tools](@/guide/set-up/tools.md)).
- **The viewport** shows the model, stock, toolpaths and simulation.

Hover any ribbon button to read what it does. A greyed-out operation says
why in its tooltip, for example "No tool yet - add one (Setup > Tools)" or
"No stock yet - set up stock first (Setup > Stock)".

Every keyboard shortcut and mouse gesture, for Windows, Linux and macOS, is
on the [Keyboard and mouse](@/guide/getting-started/keyboard-and-mouse.md) page.

### The start page

When you start CamCraft with no file named, it shows the start page in
place of the panes. The page lists your recent projects, newest first.
Click a project to open it. A project file that is no longer there is
greyed and marked **(missing)**; hover it to see where it was. The page
also has **Open project…**, **New project** and **Back to the project**.

The {{ <icon name="home" /> }} **Home** button at the top right, beside **Donate**, shows the page
again at any time. Press it again, or choose **Back to the project**, to
return to your document. The page closes by itself when the document
changes, for example when a project opens. Opening a project from the
page asks first if you have unsaved changes, as any project open does.
The version number shows at the right end of the ribbon.

### Open an example project

A release package ships example projects in an `examples` folder beside
the program. Choose {{ <icon name="file-import" /> }} **Open** in the File
group, then **Open example**, and pick a project. The list shows the
`.camproj` files in that folder in name order. If the folder holds none,
the entry reads **No example projects**. The entry is absent when the
folder is missing and in the browser build. Opening an example asks first
if you have unsaved changes, as any project open does.

### Open a file from the command line

The desktop executable accepts one file on the command line:

```sh
camcraft-ui part.step
camcraft-ui job.camproj
```

A model file is imported; a project file is opened. Options can accompany
the path. An unreadable file, an unknown extension or multiple file paths
produce an `open refused` status message and leave an empty project.
See [Command line](@/guide/automate/cli.md#open-a-file-in-the-window)
for supported extensions and the distinction from the headless tool.

### What you see after opening a document

Opening a document frames the camera on the opened part and stock, so
orbiting turns about the part and not about the old view's target. Press F
to frame again at any time. On a lathe, the viewport lays the spindle axis
from left to right with the cross-slide up (a mill-turn, which is a lathe with live tooling, keeps
the milling view). This changes only the view,
not the work offset or the posted program.

### Update notices

The desktop app checks GitHub Releases at startup, at most once in
24 hours. When it finds a newer version, **Update available: vX.Y.Z**
appears in the title bar. Select it to open the release notes in your
browser. To turn off startup checks, clear **Check for updates** in
**Settings**.

### Themes and screen readers

The ribbon, its group captions, the operation tabs and the browser tree
take their sizes and colours from the theme, so text and glyphs read
clearly in both Horizon Day and Teal Night. Browser rows, their eye
toggles and context-menu rows carry accessibility labels, so a screen
reader or an automated test can name them.

### Line width and anti-aliasing

The viewport's **Display** button opens a menu whose **Graphics** group
sets how the viewport draws:

- **Line width** (1, 1.5, 2 or 3 pt; 1.5 is the default) sets the width of
  toolpaths, height planes, outlines and axes. These lines always have
  smooth edges.
- **Anti-aliasing** (Off, 2x or 4x; 4x is the default) smooths the edges
  of models and stock. 4x uses the most graphics memory; turn it down on
  a slow integrated graphics chip. A count that the graphics device does
  not offer is shown, but cannot be chosen.

Both choices are kept for the next start.

## A first toolpath

1. **Import a model.** Click {{ <icon name="folder-open" /> }} **Model** and pick an STL, OBJ or STEP file. If
   an imported mesh has holes, the model's page has **Repair mesh**, which
   welds vertices and fills holes and reports what it changed.
   Closed STL and OBJ shells with inward winding are turned outward on
   import, so their solid checks use the outside of the part.
   A STEP closed shell that meshes with missing walls can load as
   **Incomplete** for viewing and setup. Its page names the hole; every
   operation on it is refused at the post. Re-export a sound model or
   import an STL; raising the facet budget does not repair missing walls.
2. **Set the stock.** Click {{ <icon name="tree-stock" /> }} **Stock**. The stock is a box, a cylinder, or a
   copy of a model.
3. **Place the work offset.** Click {{ <icon name="tree-wcs" /> }} **Origin** to set the work coordinate
   system (WCS) the program is written in.
4. **Add a tool.** Click {{ <icon name="tools" /> }} **Tools**, then **New tool…**. Fill in the
   cutter's type and size, then choose **Add tool**. See
   [Tools and tool libraries](@/guide/set-up/tools.md) for holders, tool numbers,
   and import and export of `.camtools` libraries.
5. **Add an operation.** Pick one from an operation group, for example
   {{ <icon name="op-face" /> }} **Face** under Milling 2D. Its page opens with tabs: {{ <icon name="sec-geometry" /> }} Geometry, {{ <icon name="sec-heights" /> }} Heights,
   {{ <icon name="sec-passes" /> }} Passes, {{ <icon name="sec-leads" /> }} Lead in / Lead out, {{ <icon name="sec-feeds" /> }} Feeds (and {{ <icon name="sec-machine" /> }} Machine for a few kinds). A kind
   only shows the tabs it has settings for.
6. **Generate.** Click {{ <icon name="act-generate" /> }} **Generate** at the bottom of the page. **Apply**
   commits edited settings without generating; **Clear toolpath** removes
   the path but keeps the operation.
7. **Check it.** Use {{ <icon name="act-simulate" /> }} **Simulate** to play the program against the stock,
   and {{ <icon name="tree-stock" /> }} **Deviate** to compare the machined stock with the model.
8. {{ <icon name="device-floppy" /> }} **Save** the project (`.camproj`) and {{ <icon name="act-post" /> }} [post](@/guide/check-and-post/posting.md) it.

More tasks: [Common workflows](@/guide/getting-started/workflows.md).

Complete first parts on one machine each:
[Your first router part](@/guide/getting-started/first-router-part.md) (a
sheet-goods router job) and
[Your first 4-axis part](@/guide/getting-started/first-4axis-part.md)
(indexed 3+1 work on a Tormach 770MX with a microARC 4). On the same
machine,
[Your first continuous 4-axis program](@/guide/getting-started/first-continuous-4axis-part.md)
posts and air-cuts a program that turns A during the feed moves.

## Undo

Every change to the document is a command, so Ctrl+Z and Ctrl+Y (Cmd+Z and Cmd+Y on
macOS) step through all of it: imports, stock, tools, operation settings, setups and
theme changes. Moving the view, hovering and the simulation's playback
position are not part of the history.

Before you import a model, read [Units and coordinates](@/guide/reference/units-and-coordinates.md): an
STL or OBJ file has no unit, and a 25.4 times error is easy to miss.
