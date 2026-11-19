# CamCraft Example Catalog

Reproducible, regression-tested CAM projects organized by operation family (`examples/catalog/<family>/<case>/`).

## Case layout

Every case folder `<family>/<case>/` contains:

- `model/` — clean-room geometry generator script and generated mesh (`.obj`, `.stl`, or `.step`).
- `build.camscript` — headless CamCraft script (`camcraft-cli script build.camscript` from the case directory) that creates the document via undoable commands, generates, verifies, saves `<case>.camproj`, and posts `<case>.nc`.
- `<case>.camproj` — committed project file.
- `<case>.nc` — committed NC program.
- `README.md` — what the case shows, its operations/tools, and what to inspect.

## Adding a new case

1. Copy the seed case folder `examples/catalog/2d/first-part/` to `examples/catalog/<family>/<case>/`.
2. Replace `model/` with your clean-room model generator and mesh.
3. Update `build.camscript` and `README.md`, naming `<case>.camproj` and `<case>.nc`.
4. Rebuild the case's `.camproj` and `.nc` from `build.camscript`:
   ```sh
   cargo xtask catalog --case <family>/<case>
   ```
5. Verify the catalog test suite passes:
   ```sh
   cargo test -p camcraft-app --test example_catalog
   ```

## Catalog commands

- `cargo xtask catalog` — rebuild every case's `<case>.camproj` and `<case>.nc` from its `build.camscript`.
- `cargo xtask catalog --check` — verify all case layouts and check that every `build.camscript` reproduces the committed `<case>.nc` without modifying tracked files.
- `cargo xtask catalog --screenshots` — render off-screen PNG screenshots via `camcraft-ui` into an images-only branch (`catalog-screenshots` by default, never into the product tree).
- `cargo xtask catalog --pdf` — build per-case PDFs into `target/catalog-pdf/` using `pandoc` and headless Chrome.
