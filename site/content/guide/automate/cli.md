+++
title = "Command line"
description = "Run CamCraft without the window: process a project, run a script, and read the exit codes."
weight = 1
aliases = ["guide/cli/"]
+++

`camcraft-cli` is the headless lane: the same commands and the same post
gate as the window, with no UI. Build it as shown in [Install](@/guide/getting-started/install.md)
(a debug build is `target/debug/camcraft-cli`; a release build is
`target/release/camcraft-cli`). Every example below was run against the
0.18.0 build from this repository, from the repository root, and the output
is trimmed where noted. Log lines (timestamps) are shortened to their
message.

## Help and version

There are three commands, `script`, `process` and `mcp`. There is no
per-command `--help`: `help` (or running with no arguments) prints the one
screen that covers all three.

```sh
$ camcraft-cli version
camcraft-cli 0.18.0 (boa_engine)
$ echo $?
0

$ camcraft-cli help        # prints the usage screen, exit 0
$ camcraft-cli             # the same screen, but exit 2 (no command given)
camcraft-cli - CamCraft headless batch lane (clean-room)

USAGE:
    camcraft-cli script   <job.js>
    camcraft-cli process  <job.camproj> [-o <out.nc>] [--cps <name|file.cps>]
    camcraft-cli mcp      [--connect <host:port>]
    camcraft-cli help
    camcraft-cli version
...
A clean run prints its logs, then the last verb's JSON answer on
stdout; refusals print on stderr. Exit: 0 stood, 1 refused, 2 usage.
$ echo $?
2
```

## Generate and post a project

`process` loads a `.camproj`, generates its toolpaths, runs the same checks
as Post, and, with `-o`, writes the NC program.

```sh
$ camcraft-cli process examples/first-part.camproj -o scratch/first.nc
INFO camcraft_app::project::wire: project opened file=first-part.camproj bytes=157729
INFO camcraft_app::posting: NC program written file=first.nc bytes=5945 ops=3 lines=298 ms=137
{"gate":{"ok":true,"problems":[]},"held_by":[],"ok":true,"operations":[{"key":"Face1","kind":"Face","name":"Face1",...,"status":"ok","warnings":[]}, ...]}
$ echo $?
0
$ head -3 scratch/first.nc
%
O0001 (VMC 3-AXIS)
G21
```

Without `-o` it only generates and verifies, writing nothing (exit 0 here
too).

Pick the post with `--cps`: a built-in name (case-sensitive, as listed in
the error below) or the path of a `.cps` file.

```sh
$ camcraft-cli process examples/first-part.camproj -o scratch/h.nc --cps Haas
...NC program written file=h.nc bytes=5690 ops=3 lines=298...
$ echo $?
0
$ camcraft-cli process examples/first-part.camproj -o scratch/h2.nc --cps assets/posts/haas.cps
...NC program written file=h2.nc bytes=5690 ops=3 lines=298...
$ echo $?
0
```

## Run a script

`script` runs a JavaScript job through the `cam` verbs (typed in
`assets/scripts/cam.d.ts`; a runnable shape is `assets/scripts/example-batch.js`).
Every world change goes through the same undoable commands as the window.
`cam.log` writes to the log, and the script's last verb answer is printed as
JSON on stdout.

Import a model and a tool library (`scratch/import.js`):

```js
var m = cam.import("testdata/models/box.step", "box");
cam.log("imported", m.model, m.facets, "facets");
var t = cam.importTools("assets/tools/catalog.json", "add");
cam.log(t.summary);
cam.exportTools("scratch/tools-out.json");
```

```sh
$ camcraft-cli script scratch/import.js
imported
box
12
facets
imported 29 tools
{"path":"scratch/tools-out.json","tools":29}
$ echo $?
0
```

(`cam.log` prints each argument on its own line.) `importTools` takes the
mode `"add"`, `"update"` or `"replace"`.
Read [Tools and tool libraries](@/guide/set-up/tools.md) before replacing a
library. That page explains the supported formats, tool numbers and
import refusals. Export a copy before a replacement when you need to
keep the current library.

Generate and post a project (`scratch/job.js`):

```js
var job = cam.load("examples/first-part.camproj");
cam.generate();
var out = cam.post({ to: "scratch/job.nc" });
cam.log("posted", out.lines, "lines");
cam.report();
```

```sh
$ camcraft-cli script scratch/job.js
INFO camcraft_app::posting: NC program written file=job.nc bytes=5945 ops=3 lines=298 ms=134
posted
298
lines
{"cut_mm":4515.31...,"document":"examples/first-part.camproj","minutes":5.93...,"operations":[...]}
$ echo $?
0
```

## Exit codes

| Code | Meaning | Observed |
|---|---|---|
| 0 | stood: the run finished and, for `process`, verify passed | all runs above |
| 1 | refused or failed: a refused post, a missing file, an unknown post name, a script error | below |
| 2 | usage: no or unknown command, a missing argument, an unreadable script file | below |

Refusals print on stderr, and a refused post writes no file.

```sh
# posting a document with no operations (scratch/refuse.js: import box.step, then cam.post)
$ camcraft-cli script scratch/refuse.js
WARN camcraft_app::posting: NC program not written: no operations to post file=none.nc
camcraft-cli: post: no operations to post
$ echo $?
1
$ ls scratch/none.nc
ls: cannot access 'scratch/none.nc': No such file or directory

$ camcraft-cli process examples/first-part.camproj -o scratch/h.nc --cps haas
camcraft-cli: post: unknown built-in post 'haas' - names: Fanuc, Haas, Siemens 840D, Heidenhain, ...; or give a .cps file path
$ echo $?
1

$ camcraft-cli process missing.camproj
camcraft-cli: load: I/O error: No such file or directory (os error 2)
$ echo $?
1

# a JavaScript throw, or a call to a verb that does not exist
$ camcraft-cli script scratch/throw.js
camcraft-cli: Error: x (unknown at :1:7)
$ echo $?
1

$ camcraft-cli bogus
camcraft-cli: unknown command "bogus" (try help)
$ echo $?
2
$ camcraft-cli script scratch/nofile.js
camcraft-cli: cannot read scratch/nofile.js: No such file or directory (os error 2)
$ echo $?
2
$ camcraft-cli script
camcraft-cli: script takes exactly one .js file
$ echo $?
2
$ camcraft-cli process examples/first-part.camproj -o
camcraft-cli: -o needs a path
$ echo $?
2
```

In a shell script, test the code, not the text: `camcraft-cli process job.camproj -o out.nc || exit 1`.

## Agent server

`camcraft-cli mcp` starts a Model Context Protocol server on stdio for an AI
agent; `--connect <host:port>` bridges to a running window started with
`--mcp-port`. It is not covered by the examples above.
