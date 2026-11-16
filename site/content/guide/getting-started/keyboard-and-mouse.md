+++
title = "Keyboard and mouse"
description = "Every keyboard shortcut and mouse action, for Windows, Linux and macOS."
weight = 3
aliases = ["guide/keyboard-and-mouse/"]
+++

This page lists the keys and mouse gestures CamCraft binds today. Each row
was checked against the code. The keys use the platform's command key: Ctrl
on Windows and Linux, Cmd on macOS. Where a row says **Control**, the key is
the physical Control key on every platform, not Cmd. Anything not in a table
is in [Not bound yet](#not-bound-yet).

## File and Edit

| Action | Windows/Linux | macOS |
|---|---|---|
| New document | Ctrl+N | Cmd+N |
| Open a model | Ctrl+O | Cmd+O |
| Open a project | Ctrl+Shift+O | Cmd+Shift+O |
| Save | Ctrl+S | Cmd+S |
| Save As | Ctrl+Shift+S | Cmd+Shift+S |
| Post the program | Ctrl+P | Cmd+P |
| Quit | Ctrl+Q | Cmd+Q (the app menu's Quit does the same) |
| Undo | Ctrl+Z | Cmd+Z |
| Redo | Ctrl+Y or Ctrl+Shift+Y | Cmd+Y or Cmd+Shift+Y |
| Copy the selected operation | Ctrl+C | Cmd+C |
| Paste the copied operation | Ctrl+V | Cmd+V |
| Delete the selected item | Delete | Fn+Delete (the forward-delete key; not checked on hardware) |
| Commit a typed value (parameter and step-budget fields) | Enter | Enter (Return) |

New, open, save, post, quit, copy, paste and delete do nothing while a text
field has the keyboard, so a typed letter never opens a dialog. Undo and redo
work everywhere.

## View

| Action | Windows/Linux | macOS |
|---|---|---|
| Frame the whole scene | F | F |
| Switch perspective and orthographic | P | P |

## Viewport mouse

| Action | Windows/Linux | macOS |
|---|---|---|
| Orbit | Drag with the left button (past a 4 px travel), or drag with the middle button | Drag with the left button (one finger on a trackpad). Middle-drag needs a middle button |
| Pan | Drag with the right button | Drag with the right button (two-finger click and drag on a trackpad; not checked on hardware) |
| Zoom | Mouse wheel, toward the cursor | Two-finger scroll, toward the cursor |
| Pick (select a model, or a face, edge or vertex in those modes) | Left-click without dragging | Left-click without dragging |
| Snap to a view from the ViewCube | Left-click a face, edge or corner of the cube | Same |
| ViewCube navigation buttons | Left-click a button | Same |

A plain click on empty space in model mode clears the selection. A
right-click in the viewport does not open a menu; it starts a pan, so a
right-click with no movement does nothing.

## Picking modifiers

Picks in the viewport add to the open operation's geometry in Face, Edge and
Vertex modes. A plain click replaces the picks.

| Action | Windows/Linux | macOS |
|---|---|---|
| Replace the picks with this one | Click | Click |
| Add a face, vertex or edge to the picks | Ctrl+click or Shift+click | Cmd+click or Shift+click |
| Add one edge, or remove it if it is already picked (Edge mode) | Ctrl+click | Cmd+click |
| Pick the whole face loop the edge lies on (Edge mode) | Shift+click | Shift+click |
| Pick one mesh segment instead of the whole chain (Edge mode, boundaries) | Alt+click | Option+click |
| Remove an edge from the picks | Ctrl+click on the picked edge | Cmd+click on the picked edge |

In a boundary, Ctrl (Cmd on macOS) toggles a pick, Shift closes the picks around their
face loop, and Alt takes one segment, the same keys as above. Ctrl- or Cmd-clicking
a face or vertex that is already picked is refused with a status note;
only edges toggle out.

On macOS the picking modifier is Cmd (the platform command key), so Control
stays free for its secondary-click meaning.

## Right-click menus

| Action | Windows/Linux | macOS |
|---|---|---|
| Operation in the browser: its menu (verbs and the retype list) | Right-click the row | Control-click or two-finger click |
| Model in the browser: use or stop using as a fixture | Right-click the row | Same |
| A setup's Operations row: Paste | Right-click the row | Same |
| A ribbon operation button: pin or unpin it | Right-click the button | Same |
| An error message line in a page or dialog: Copy message | Right-click the line | Same |

## Not bound yet

These have no keyboard or mouse binding in the app today:

- Box-select (drag a rectangle to pick several features).
- A shortcut to run the selected operation (Run is on the ribbon only).
- A shortcut for the ribbon's Paths, Simulate and Deviate, or for the Setup
  groups (Setups, Stock, Origin, Tools).
- Select all (Cmd+A or Ctrl+A) outside a text field.
- Pinch to zoom in the viewport (the wheel and two-finger scroll zoom).
- Pan with Ctrl, Shift or Alt drag, or with the middle button alone.
- Arrow keys to orbit or pan.
- Escape for the side windows (tool editor, G-code view, machine editor and
  similar panels): close them with their buttons.
- A middle-click without a drag (it does nothing).
