## #661 - Operation icons: original SVG masters and the SVG-to-PNG pipeline (slice 1)

**Decided - the pipeline.** Every operation glyph has an SVG master in
`assets/ui/svg/<stem>.svg`. A developer command, `cargo xtask icons`,
rasterises each master to the 32x32 PNG the UI already loads
(`assets/ui/<stem>.png`, embedded by `crates/ui/src/egui_app/icons.rs`,
drawn at 16-20 px), and the PNGs are committed. `cargo xtask icons --check`
and the xtask unit test `committed_icon_pngs_match_their_svg_masters` fail
when a committed PNG differs from a fresh render of its master (by more
than 8 per channel - resvg's anti-aliasing can move a step or two across
CPUs) or when a master breaks the house rules. `cargo xtask icons --sheet
<out.png> [--px 16] [--zoom 4]` draws a contact sheet of every master on a
dark and a light band, for a human look. The structural UI test
`every_op_kind_has_its_own_svg_mastered_icon` pins that every `OpType` maps
to its own `op-*` glyph with a master, and that no two kinds share one.

**Why.** The SVG is the durable asset (a later kit or theme can draw from
it at any size); the PNGs keep the running app exactly as it loads icons
today, with no new runtime dependency and no `build.rs`. A test that
compares pixels, not mtimes, is what git can actually check.

**New dependency.** `resvg =0.45.1` (MIT OR Apache-2.0), with no default
features (no text, fonts or raster images - the masters are shapes only),
in `xtask` ONLY: never a dependency of the app or ui crates, so it is not
in any shipped binary. Pinned exactly; 0.45 shares the `tiny-skia 0.11`
already in the lock, so `Cargo.lock` gains only resvg/usvg and their
parsers.

**House rules (lint in `xtask/src/icons.rs`).** `viewBox="0 0 24 24"`, no
intrinsic width/height; round caps and joins; stroke weights 2 (the
object) and 1.5 (a toolpath or detail line over it) only; `currentColor`
ink, no fills except small solid dots/blocks; no `<image>`, `<script>`,
`<text>`, `<style>`, `<foreignObject>`, `href`, `url(` or font - shapes
only. The loader repaints every pixel white and lets alpha draw, so the
masters' colour never reaches the screen and one PNG serves both themes.

**Licence finding - nothing from SolidCraft.** The issue's comment that
about 30 icons could be borrowed from tangency-labs/solidcraft (then MIT
OR Apache-2.0) is out of date: upstream commit 312454f899 (2026-11-10,
PR #14578) added LICENSE-BRAND ("SolidCraft Brand and Interface
Licence"), covering crates/ui-kit, crates/look (the icon masters, glyphs,
themes) and crates/theme; reuse in another program needs the SolidCraft
maintainers' written permission. So the set takes nothing from SolidCraft:
no SVG, no path data, no glyph traced or adapted, no style-guide text, and
its licence is not in `assets/ui/LICENSES.md`. The masters follow Tabler's
conventions instead (24-unit grid, 2-unit stroke, round caps and joins) so
they sit with the Tabler glyphs we already ship. (During authoring the
worker read SolidCraft's style guide and one of its SVGs - `chamfer`, an
isometric cube - through the GitHub API, before the licence change was
known; nothing was saved or copied, the first draft used a 64-unit grid
from that guide, and the whole set was then redrawn on the 24-unit Tabler
grid, the 2D Chamfer glyph recomposed. Whether CamCraft later adopts
SolidCraft's look is the maintainer's call, with upstream's permission;
these masters are ours either way.)

**Every glyph is drawn original.** 41 operation masters, one per `OpType`
(the issue's sections 1-3, plus Trochoidal Slot and Plasma Profile, which
have since joined the enum): a cutter and its path, drawn from what the
operation does - nothing adapted from HSMWorks, Fusion or any other CAM
product. Families share a frame so they read as families and differ by
the cut: 2D milling in plan or section, the multi-axis kinds on a dome with
the ball leaning on its normal, the rotary kinds on a bar with the tool
riding the radial, and every turning kind with a solid chuck block at the
left. The old reuse law (Facing and every turning blade wearing the face
mill's pixels; Engrave wearing Trace's; Bore wearing Drill's) is retired:
`op_icon` (now `crates/ui/src/egui_app/op_icons.rs`, out of the oversized
panes.rs) maps every kind to its own slot. The turning section door wears
the Facing glyph. The 18 PNGs the operations no longer use (13 earlier
originals and 5 Tabler stand-ins) are deleted.

**Considered and dropped.** A `build.rs` in the ui crate rasterising at
build time (a new build dependency on every build, and the reviewers' law
holds build scripts for the owner); loading SVG at runtime in the ui crate
(a new runtime dependency); copying SolidCraft's masters (licence, above);
the 64-unit grid with 2.55/4.25 weights (dropped with SolidCraft's style);
an mtime check (git does not keep mtimes).

**Cost to change later.** Swapping the rasteriser or grid is one xtask
module and one regeneration; moving the UI to load the SVGs directly
(e.g. through a kit's icon set) reads the same masters and drops the PNGs.
Still owed on #661: tools (4), tree rows (6), status badges (7), actions
(8); tabs (5) land with #659.
