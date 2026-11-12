+++
title = "First run"
description = "The window, the ribbon, and a first toolpath."
weight = 2
+++

## Trying it in the browser first

When this site's release has a browser version,
[CamCraft in the browser](@/app.md) opens it with nothing to install (it
needs a browser with WebGL2: a current Chrome, Firefox, Edge or Safari), so
you can look around the window described below; if it does not, that page
says so, and the front page says which it is. The browser version cannot
open or save files yet and simulation playback does not work in it, so for a
real first toolpath, install a [desktop build](@/guide/install.md).

## The window

- **The ribbon** across the top holds the commands, in groups:
  - **File:** New (Ctrl+N), Model (open a mesh model, Ctrl+O), Open (a
    project, Ctrl+Shift+O), Save (Ctrl+S), Save As (Ctrl+Shift+S) and Post
    (Ctrl+P).
  - **Setup:** Setups, Stock, Origin (the work offset), Tools and the
    machine button, which switches the workspace between Milling, Turning
    and Mill-turning.
  - **Operation groups** for the machine you are on: Milling 2D, Milling
    3D, Drilling and Multi-axis on a mill; Turning on a lathe. Each group
    opens a list of its operations. Right-click an operation to pin it to
    the ribbon (up to four pins).
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

## A first toolpath

1. **Import a model.** Click **Model** and pick an STL, OBJ or STEP file. If
   an imported mesh has holes, the model's page has **Repair mesh**, which
   welds vertices and fills holes and reports what it changed.
2. **Set the stock.** Click **Stock**. The stock is a box, a cylinder, or a
   copy of a model.
3. **Place the work offset.** Click **Origin** to set the work coordinate
   system (WCS) the program is written in.
4. **Add a tool.** Click **Tools**, then **New tool…**. Fill in the
   cutter's type and size. Tool libraries can be exported and imported as
   `.camtools` files.
5. **Add an operation.** Pick one from an operation group, for example
   **Face** under Milling 2D. Its page opens with tabs: Geometry, Heights,
   Passes, Lead in / Lead out, Feeds (and Machine for a few kinds). A kind
   only shows the tabs it has settings for.
6. **Generate.** Click **Generate** at the bottom of the page. **Apply**
   commits edited settings without generating; **Clear toolpath** removes
   the path but keeps the operation.
7. **Check it.** Use **Simulate** to play the program against the stock,
   and **Deviate** to compare the machined stock with the model.
8. **Save** the project (`.camproj`) and [post](@/guide/posting.md) it.

## Undo

Every change to the document is a command, so Ctrl+Z and Ctrl+Y step
through all of it: imports, stock, tools, operation settings, setups and
theme changes. Moving the view, hovering and the simulation's playback
position are not part of the history.
