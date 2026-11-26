# CamCraft Example Catalog

Reproducible, regression-tested CAM projects organized by operation family (`examples/catalog/<family>/<case>/`).

## Case layout

Every case folder `<family>/<case>/` contains:

- `model/` — clean-room geometry generator script and generated mesh (`.obj`, `.stl`, or `.step`).
- `build.camscript` — headless CamCraft script (`camcraft-cli script build.camscript` from the case directory) that creates the document via undoable commands, generates, verifies, saves `<case>.camproj`, and posts `<case>.nc`.
- `<case>.camproj` — committed project file.
- `<case>.nc` — committed NC program (a case with `expect = "post"`).
- `refusal.txt` — committed refusal lines, in place of `<case>.nc` (a case with `expect = "refusal"`).
- `case.toml` — operation kinds, machine and expected result; the kinds drive the guide's example links.
- `README.md` — what the case shows, its operations/tools, and what to inspect.

## Case metadata

Keep the case's explanation in its README. Put its machine-readable metadata in `case.toml`:

```toml
kinds = ["Face", "2D Pocket", "Drill"]
machine = "3-axis mill"
expect = "post"
```

`kinds` lists the exact display names from `KindInfo::ALL` in
`crates/app/src/operation/kinds.rs`. Each name links the case below that kind's
heading on the operation guide pages. A case can list several kinds; the seed
case appears below Face, 2D Pocket and Drill. The case folder's family does not
choose its kinds: an indexed 2D Pocket is still a 2D Pocket.

`machine` describes the machine used by the case. It is a required, nonempty
description; it does not configure the machine.

`expect` is `"post"` or `"refusal"` (#2856). A post case posts `<case>.nc`, and
the tests compare it with the committed file byte for byte. A refusal case
shows a program that CamCraft refuses to post. It has no `<case>.nc`. Its
script saves `<case>.camproj` and then runs the refused step without a catch,
so the script run ends with an error. `refusal.txt` holds the refusal lines
that the script host records, one line each: the stderr lines of the run that
start with `camcraft-cli: `, without that prefix. The log lines on stderr are
not part of it. The tests and `--check` compare
them with the committed file byte for byte. A refusal case that posts fails.

The supported flat TOML format has quoted strings without escapes and an array
of quoted strings for `kinds`. Single or double quotes, comments and multiline
arrays are supported. Empty fields, duplicate fields or kinds, unknown fields,
unknown kind names and unsupported syntax are refused. Tables and escaped
strings are not supported.

## Adding a new case

1. Copy the seed case folder `examples/catalog/2d/first-part/` to `examples/catalog/<family>/<case>/`.
2. Replace `model/` with your clean-room model generator and mesh.
3. Update `build.camscript`, `case.toml` and `README.md`, naming `<case>.camproj` and `<case>.nc`.
4. Rebuild the case's `.camproj` and `.nc` from `build.camscript`:
   ```sh
   cargo xtask catalog --case <family>/<case>
   ```
5. Verify the catalog test suite passes:
   ```sh
   cargo test -p camcraft-app --test example_catalog
   ```

## Catalog commands

- `cargo xtask catalog` — generate the operation pages' Example cases lists and rebuild every case's `<case>.camproj` and `<case>.nc` (or `refusal.txt`) from its `build.camscript`.
- `cargo xtask catalog --check` — validate metadata and guide links, then verify all case layouts and check that every `build.camscript` reproduces the committed `<case>.nc` (or `refusal.txt`) without modifying tracked files.
- `cargo xtask catalog --screenshots` — render PNG images via `camcraft-ui` and publish the images-only branch to `origin` (`catalog-screenshots` by default).
- `cargo xtask catalog --pdf` — build per-case PDFs into `target/catalog-pdf/` using `pandoc` and headless Chrome.

`--case <family>/<case>` limits rebuilding and NC parity checks to that case.
Metadata and guide links are still validated for the whole catalog because
operation pages can link several cases. A missing or stale generated list and
a link to an unknown case make `--check` fail before the CLI build. Rebuilding
regenerates the marked Example cases blocks and preserves the surrounding guide
prose. Screenshot-only and PDF-only commands do not regenerate these lists.

## Catalog images

The screenshot command captures the model without toolpaths, then each operation separately with the same camera.
It needs a GPU or a software adapter.
It writes PNG files outside the product tree.
It publishes them at `catalog/<family>/<case>/0-blank.png` and `catalog/<family>/<case>/<ordinal>-<operation>.png` on the images branch.
The operation ordinal follows the document order.
The filename replaces each character outside ASCII letters and digits with `-`.
A second run without a source change uses the same paths.
The push uses `origin` and does not force an update.
If the remote branch has other commits, reconcile them before running the command again.

See the [first-part images](2d/first-part/README.md#images) for the seed case.
