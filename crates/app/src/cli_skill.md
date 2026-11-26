# Working with camcraft-cli

`camcraft-cli` is the command-line door of CamCraft, a CAM program that turns
3D models into NC programs (G-code) for CNC milling machines. Use it to run
CAM jobs without a window, to drive a document through MCP tools, or to send
commands to a CamCraft window that is open on this machine.

## Pick the lane

- A whole project, start to end: `camcraft-cli process <job.camproj> -o <out.nc>`
  loads the project, generates every toolpath, posts the program and checks
  the result. Without `-o` it only generates and verifies.
- A scripted job: `camcraft-cli script <job.js>` runs a job script through the
  `cam` verbs (`cam.load`, `cam.generate`, `cam.post`, `cam.verify`,
  `cam.call`). Each change to the document goes through the undoable commands.
- An agent session on a headless document: `camcraft-cli mcp` is an MCP server
  on stdio. `camcraft-cli mcp reference` prints the full tool table.
- A window that the user has open: `camcraft-cli --ui windows` lists the
  running windows. `camcraft-cli --ui call <tool> '<json-args>'` runs one MCP
  tool in that window. The change shows at once and Ctrl+Z in the window undoes
  it. Use `--window <pid|project>` when more than one window runs.

## Read the exit code

The exit code is the verdict. Read it before you read the output.

- `0`: the job stood. The last answer (JSON) is on stdout.
- `1`: a verb, a tool or the verify report refused. The reason is on stderr.
- `2`: the command line was wrong, or a file could not be read.
- `3` (`--ui` only): no CamCraft window runs. The CLI never falls back to a
  headless document, so start the app or drop `--ui`.
- `4` (`--ui` only): the window is busy with another client. Try again later.

A refusal is a result, not a crash. CamCraft refuses a toolpath that gouges,
collides or cannot be computed in full. Report the refusal text to the user.
Do not work around it, and never edit a posted program by hand to hide it.

## Examples

```sh
# Bring a project current and post it with a post processor that you name
# (a built-in post name, or the path of a .cps file).
camcraft-cli process part.camproj -o part.nc --cps my-post.cps

# Check a project without posting it: the exit code says if it is clean.
camcraft-cli process part.camproj

# Ask which MCP tools exist, then call one of them in the open window.
camcraft-cli mcp reference
camcraft-cli --ui call <tool> '{"key": "value"}'
```

## Rules for an agent

- Ask `camcraft-cli <command> --help` when you are not sure of a flag. It prints
  the usage and runs nothing.
- Prefer `process` or `script` for repeatable work. Use `--ui` only when the
  user wants to see the change in the window.
- Never guess a tool name or its arguments: read `camcraft-cli mcp reference`.
- Keep the user's files: write a posted program to a new path unless the user
  names the file to replace.

The command reference below comes from the CLI's own help text.
