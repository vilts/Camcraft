+++
title = "Commands and undo"
description = "Every change to the document is an undoable command."
weight = 2
+++

## Everything is a command

A change to the document is never a direct field write from the UI. It is
a **command**: a value that knows how to apply itself to the ECS world and
how to invert itself exactly.

```rust
pub trait Command {
    /// Forward effect.
    fn apply(&self, world: &World);
    /// Exact inverse of `apply`.
    fn invert(&self, world: &World);
}
```

The history (`History` in `crates/app/src/commands/mod.rs`) is a linear
undo/redo stack of executed commands, and it is the single source of
truth for what undo (Ctrl+Z, Cmd+Z on macOS) and redo (Ctrl+Y, Cmd+Y on
macOS) can do. Executing a new
command clears the redo stack. The history also tracks the depth at the
last save, so undoing back to the saved point makes the document clean
again.

Examples: `CreateOperation`, `DeleteOperation`, `ChangeTool`,
`EditOperationParams`, `SetStock`, `SelectEntity`, `ActivateSetup`,
`LoadMachine`, `SetUnits`. Ephemeral view state (hover, camera, drag
previews, the simulation's playback position) is immediate-mode and not
undoable.

## Validate at construction

A command checks its input when it is built, not when it runs. A malformed
stock, an empty setup name or a no-op change is refused with a
`CommandError` that names the problem, so bad input never enters the
history, and `apply` / `invert` are plain writes that cannot fail halfway.

## Commands ship with an MCP tool

`camcraft-cli mcp` runs a Model Context Protocol server that lets an AI
agent drive CamCraft; its tools (in `crates/app/src/mcp/`) call the same
undoable commands the GUI uses. How to run and use it is in the
[AI agents (MCP) guide](@/guide/automate/mcp.md). A pull request that adds a
user-facing command also adds its MCP tool, or files a follow-up issue for
it.

## Deleting frees everything

Deleting an entity frees every resource attached to it, for example the
cached toolpath and rest-material meshes of a deleted operation. A leak is
treated as a critical bug.
