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
  - **File:** New (Ctrl+N), Model (open a mesh model, Ctrl+O), Open (a
    project, Ctrl+Shift+O), Save (Ctrl+S), Save As (Ctrl+Shift+S) and Post
    (Ctrl+P).
  - **Setup:** Setups, Stock, Origin (the work offset), Tools and the
    machine button, which switches the workspace between Milling, Turning
    and Mill-turning.
  - **Operation groups** for the machine you are on: Milling 2D, Milling
    3D, Drilling and Multi-axis on a mill; Turning on a lathe. Each group
    opens a list of its operations. Right-click (Control-click or a two-finger
    click on a Mac) an operation to pin it to the ribbon (up to four pins).
  - **Modify:** Undo (Ctrl+Z), Redo (Ctrl+Y), Run (generate the selected
    operation) and Delete.
  - **Inspect:** Frame (F), Paths (show every toolpath), Simulate and
    Deviate.
  - **View:** Panels (show or hide the browser, properties and machines
    panes, or reset the layout) and Theme.
- **The browser** lists the document: models, setups with their stock,
  machine, post, fixtures and tools, and the operations.
- **The properties pane** shows the page of whatever you selected.
- **The viewport** shows the model, stock, toolpaths and simulation.

Hover any ribbon button to read what it does. A greyed-out operation says
why in its tooltip, for example "No tool yet - add one (Setup > Tools)" or
"No stock yet - set up stock first (Setup > Stock)".

Every keyboard shortcut and mouse gesture, for Windows, Linux and macOS, is
on the [Keyboard and mouse](@/guide/getting-started/keyboard-and-mouse.md) page.

## A first toolpath

1. **Import a model.** Click **Model** and pick an STL, OBJ or STEP file. If
   an imported mesh has holes, the model's page has **Repair mesh**, which
   welds vertices and fills holes and reports what it changed.
   Closed STL and OBJ shells with inward winding are turned outward on
   import, so their solid checks use the outside of the part.
2. **Set the stock.** Click **Stock**. The stock is a box, a cylinder, or a
   copy of a model.
3. **Place the work offset.** Click **Origin** to set the work coordinate
   system (WCS) the program is written in.
4. **Add a tool.** Click **Tools**, then **New tool…**. Fill in the
   cutter's type and size, then choose **Add tool**. See
   [Tools and tool libraries](@/guide/set-up/tools.md) for holders, tool numbers,
   and import and export of `.camtools` libraries.
5. **Add an operation.** Pick one from an operation group, for example
   **Face** under Milling 2D. Its page opens with tabs: Geometry, Heights,
   Passes, Lead in / Lead out, Feeds (and Machine for a few kinds). A kind
   only shows the tabs it has settings for.
6. **Generate.** Click **Generate** at the bottom of the page. **Apply**
   commits edited settings without generating; **Clear toolpath** removes
   the path but keeps the operation.
7. **Check it.** Use **Simulate** to play the program against the stock,
   and **Deviate** to compare the machined stock with the model.
8. **Save** the project (`.camproj`) and [post](@/guide/check-and-post/posting.md) it.

More tasks: [Common workflows](@/guide/getting-started/workflows.md).

## Undo

Every change to the document is a command, so Ctrl+Z and Ctrl+Y (Cmd+Z and Cmd+Y on
macOS) step through all of it: imports, stock, tools, operation settings, setups and
theme changes. Moving the view, hovering and the simulation's playback
position are not part of the history.

Before you import a model, read [Units and coordinates](@/guide/reference/units-and-coordinates.md): an
STL or OBJ file has no unit, and a 25.4 times error is easy to miss.
