+++
title = "Coming from another CAM system"
description = "What you can bring into CamCraft from your current CAM system, how to bring it, and what you redo."
weight = 5
aliases = ["guide/migrating/"]
+++

CamCraft reads its own formats and neutral ones. It does not read the tool
library files, project files or post processors of any other CAM system, and
nothing here asks it to. You bring a part and a tool list across in plain
formats, and you re-create the rest. This page walks through that, tools
first.

## Tools

The route is: export your tool list from your current CAM system in a neutral
form (most systems can write a CSV or spreadsheet tool list), map its columns
onto CamCraft's tool fields, write a CamCraft tool library file, and import
that.

### The tool library file

A tool library is a JSON file in CamCraft's own `camcraft-tools` format,
version 1. The app suggests the `.camtools` extension; a `.json` name works
too. It looks like this:

```json
{
  "format": "camcraft-tools",
  "version": 1,
  "units": "mm",
  "tools": [
    { "name": "name shown in the tool list", "definition": { "...": "..." } }
  ]
}
```

- `format` and `version` must be exactly as shown. `units` is optional, and
  if present must be `"mm"`: every length in the file is in millimetres,
  whatever units the app displays. Convert inch tool sheets before writing
  the file.
- Each tool needs a non-empty, unique `name`. A repeated name, a missing
  definition or a definition that fails CamCraft's sanity check refuses the
  whole file by name; nothing is half-imported.
- A bare list of `{name, definition}` objects without the envelope is also
  accepted.

A tool's `definition` holds these fields. Every field is required unless marked
optional; use `0` where a field does not apply to the tool kind:

| Field | Meaning |
|---|---|
| `kind` | `end_mill`, `ball_nose`, `bull_nose`, `drill`, `tap`, `bore`, the turning kinds `turning`, `groove`, `thread` and `form`, and more; for a kind not listed here, copy the word from an export of a tool of that kind (see below). |
| `diameter` | Cutting diameter, mm. Must be above 0 and at most 500. |
| `corner_r` | Corner radius, mm. `0` for a flat end mill; at most half the diameter. |
| `tip_angle` | Included point angle in degrees, read by drills (for example `118`). Optional, defaults to `0`. |
| `pitch` | Thread pitch in mm per revolution, read by taps. Optional, defaults to `0`. |
| `flute_len` | Length of the cutting flutes, mm. |
| `shank_d`, `shank_len` | Shank diameter and length, mm. |
| `holder_d`, `holder_len` | Holder cylinder diameter and length, mm: the simple holder body the collision checks use. |
| `material_group` | Text label of the material the suggested speeds are for. |
| `rpm`, `feed` | Suggested spindle speed (rpm) and feed (mm/min). |

Optional fields include `flutes` (the number of cutting edges, which the
feed-per-tooth suggestion reads) and `number` (the controller tool number;
without it a tool posts as its position in the list).

The fastest way to see every field for a kind you need is to add one such tool
in the app, run **Export library...** on the **Tool library** page and open the
file; the export is pretty-printed JSON you can read and hand-edit.

### A worked example: CSV to `camcraft-tools`

Suppose your tool list export looks like this (lengths in mm; the column names
are whatever your system writes, you only map them once):

```csv
number,name,kind,diameter,corner_r,flutes,flute_len,shank_d,stick_out,rpm,feed
1,10 mm flat,end_mill,10,0,4,22,10,40,8000,1600
2,6 mm ball,ball_nose,6,3,2,13,6,30,10000,1200
3,8.5 mm drill,drill,8.5,0,2,75,8.5,100,3500,350
```

The matching CamCraft file holds the same three tools:

```json
{
  "format": "camcraft-tools",
  "version": 1,
  "units": "mm",
  "tools": [
    { "name": "10 mm flat", "definition": {
      "kind": "end_mill", "diameter": 10, "corner_r": 0, "tip_angle": 0,
      "pitch": 0, "flute_len": 22, "shank_d": 10, "shank_len": 40,
      "holder_d": 40, "holder_len": 50, "material_group": "aluminium",
      "rpm": 8000, "feed": 1600, "flutes": 4, "number": 1 } },
    { "name": "6 mm ball", "definition": {
      "kind": "ball_nose", "diameter": 6, "corner_r": 3, "tip_angle": 0,
      "pitch": 0, "flute_len": 13, "shank_d": 6, "shank_len": 30,
      "holder_d": 40, "holder_len": 50, "material_group": "aluminium",
      "rpm": 10000, "feed": 1200, "flutes": 2, "number": 2 } },
    { "name": "8.5 mm drill", "definition": {
      "kind": "drill", "diameter": 8.5, "corner_r": 0, "tip_angle": 118,
      "pitch": 0, "flute_len": 75, "shank_d": 8.5, "shank_len": 100,
      "holder_d": 40, "holder_len": 50, "material_group": "aluminium",
      "rpm": 3500, "feed": 350, "flutes": 2, "number": 3 } }
  ]
}
```

Notice what the conversion had to decide. The `stick_out` column became
`shank_len`; the holder size is not in the CSV, so the script puts in one
default holder cylinder (40 mm by 50 mm here) that you replace with your real
holders afterwards; `tip_angle` is a drill's point angle, which the CSV did not
carry, so the script states 118 degrees. Check each of these against your tool
sheet.

A short script does the conversion. This is Python 3, with no extra packages:

```python
import csv, json, sys

HOLDER_D, HOLDER_LEN = 40.0, 50.0   # placeholder holder; replace later
tools = []
with open(sys.argv[1], newline="") as f:
    for r in csv.DictReader(f):
        kind = r["kind"]
        tools.append({"name": r["name"], "definition": {
            "kind": kind,
            "diameter": float(r["diameter"]),
            "corner_r": float(r["corner_r"]),
            "tip_angle": 118.0 if kind == "drill" else 0.0,
            "pitch": 0.0,
            "flute_len": float(r["flute_len"]),
            "shank_d": float(r["shank_d"]),
            "shank_len": float(r["stick_out"]),
            "holder_d": HOLDER_D, "holder_len": HOLDER_LEN,
            "material_group": "aluminium",
            "rpm": float(r["rpm"]), "feed": float(r["feed"]),
            "flutes": int(r["flutes"]), "number": int(r["number"]),
        }})
json.dump({"format": "camcraft-tools", "version": 1, "units": "mm",
           "tools": tools}, open(sys.argv[2], "w"), indent=2)
```

Run it as `python convert.py tools.csv shop.camtools`. If your tool sheet is in
inches, multiply the lengths by 25.4 and the feeds (inches per minute) by 25.4
in the script. Check the script's output by importing it: CamCraft refuses a
file it does not understand and names the tool and field at fault.

### Importing the library

On the **Tool library** page choose **Import library...** and pick the file.
Beside the button, a mode choice says what to do with names the document
already has:

- **Add (rename duplicates)**: never overwrites. A tool identical to one you
  already have is skipped; a same-named tool that differs comes in under a
  new name such as `Name (2)`.
- **Update same-named**: a same-named tool takes the file's definition in
  place, keeping its tool number, so operations that use it stay attached
  and show as stale until you regenerate them. Names the document lacks are
  added.
- **Replace library**: the file becomes the document's whole tool set. Tools
  the file lacks are removed; if an operation uses one of them the whole
  import is refused and nothing changes.

The import reports what it did (added, renamed, skipped, updated, and for
Replace, removed), and each import is one undo step. The same import is
available from a script as `cam.importTools(path, "add" | "update" |
"replace")`, and `cam.exportTools(path)` writes the document's tools back out
as a library file, which is also a convenient backup. Run a script with
`camcraft-cli script`; the verbs are listed in `assets/scripts/cam.d.ts`.

### Holders

A tool starts with the plain holder cylinder from `holder_d` and `holder_len`.
A tool can also carry a drawn holder profile or a placed holder model, set on
the tool editor's Body section (**Import body...**). A library file carries
such bodies under a `body` key and imports them by default (`holders` is true
in `cam.importTools`; pass `false` to leave them out, and the summary then
counts the holders left out). A holder model imported from a vendor STEP file
follows the same unit and placement checks described in the
[vendor fixture guide](https://github.com/NeoTech/Camcraft/blob/main/docs/user-guide/vendor-fixtures.md).

### Check before you cut

After importing, compare a handful of tools with your tool sheet: diameter,
flute length, stick-out and tool number. Tool numbers matter most. A tool
without a `number` posts as its position in the list, and CamCraft refuses to
post a program in which two tools would post the same number.

## Models

Bring the part in with **File > Model**. STEP is the preferred format; STL and
OBJ meshes are also accepted. The model is stored in millimetres. A STEP file
that declares inches is read in inches and shown in millimetres, so check one
known dimension against your drawing: if it is off by 25.4 times, fix the file
before going on.

A STEP model is turned into a triangle mesh. The limit on that mesh is the
STEP facet budget, 30 million triangles by default. A model too large for
the budget is imported reduced, and CamCraft says how closely its surfaces
are approximated: checks against it are stricter, and operations on a model
that is too coarse to check are refused at the post. Import it again with a
larger budget to clear that: set **STEP facet budget (next import)** on the
model page before importing, or pass it from a script,
`cam.import(path, name, facetBudget)`. Either way it accepts 1 000 to
60 000 000.

## Machines and posts

Pick the built-in machine and post closest to your machine in **Setup**; see
[Setups](@/guide/set-up/setups.md) and [Posting](@/guide/check-and-post/posting.md). CamCraft ships
machine files (`.mch`) for generic mills, a lathe and a mill-turn, and for
several Tormach and other machines, and posts (`.cps` scripts) for Fanuc, Haas,
Siemens 840D, Heidenhain, Mazak, Tormach PathPilot, Makera Carvera and Fanuc
Lathe. For a machine that has none, the contributor guide
[Machine onboarding](https://github.com/NeoTech/Camcraft/blob/main/docs/MACHINE-ONBOARDING.md)
says what a machine file records and where its facts come from.

Your current system's post processor does not carry over, and its output
should not be copied across. Re-create the machine in a `.mch` file from your
machine's own manual, and use or write a CamCraft post. Nothing is imported
from another system's machine or post files.

The first program CamCraft posts for a machine is proved like any new post:
read the G-code, dry-run it with the spindle off and Z raised, and run it
single-block before it cuts anything. Passing CamCraft's checks is not a
certificate that your machine is safe.

## Concepts

Your current system has its own names for these; CamCraft's are:

| Idea | In CamCraft |
|---|---|
| A fixturing and orientation of the part | A **setup**, with its own machine and post if you want |
| The work coordinate system | The setup's **work offset**, set from **Setup > Origin** |
| The raw material | **Stock**, which can carry on from an earlier setup |
| A cutting step | An **operation**, in the setup's list |
| A cutter | A **tool** in the tool library, with a controller number |
| The NC file writer | A **post** (`.cps`), run with the machine's `.mch` facts |

CamCraft refuses to post when it cannot show a program is safe against the
model, stock, fixtures and machine facts it knows. That can feel stricter than
you are used to; see [When CamCraft refuses](@/guide/reference/refusals.md).

## What does not migrate

Existing toolpaths and NC programs are not imported. Operations are
regenerated in CamCraft from the part, and the program is posted again.
Feeds, speeds and every operation parameter are entered again; keep your old
setup sheet beside the screen while you do it. Machines and posts are
re-created as described above.

## A first-job checklist

1. Install and check the app ([Install](@/guide/getting-started/install.md),
   [First run](@/guide/getting-started/first-run.md)).
2. Import your tool list and compare it with your tool sheet.
3. Import the part and check one dimension, including its unit.
4. Set the machine, post, stock and work offset ([Setups](@/guide/set-up/setups.md)).
5. Add operations and generate them; read the diagnostics.
6. Post ([Posting](@/guide/check-and-post/posting.md)). A refusal is a result, not a fault:
   read it ([Refusals](@/guide/reference/refusals.md)).
7. Read the G-code. Dry-run with the spindle off and Z raised, then run
   single-block on the control before cutting material.
