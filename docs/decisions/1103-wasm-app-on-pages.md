## #1103 - the browser build on the Pages site

**Path layout.** The browser shell is served at `<pages>/<repo>/app/`
(`https://neotech.github.io/Camcraft/app/`): the folder
`web/build-app-wasm.sh` writes (`index.html`, `wasi_shim.js`,
`camcraft-ui.js`, `camcraft-ui_bg.wasm`) is copied as is into the Zola
output's `app/`. Every URL the page loads was already relative
(`./wasi_shim.js` in the import map and the module script, `./camcraft-ui.js`,
and wasm-bindgen's `new URL('camcraft-ui_bg.wasm', import.meta.url)`), so
no file changed; `web/check-app-page.sh` now pins it, in the wasm job and
again on the Pages artifact's `app/`, and runs locally on `target/web`.
Considered: a `<base href>` in the page, or rewriting paths in the workflow
- dropped, both add a second truth for a path that is already relative.

**Failure behaviour.** The `site` job needs `wasm` with `if: !cancelled()`,
the pattern `publish` uses for the platforms. A failed wasm build skips
the artifact download; the site deploys anyway, the run carries a warning
naming the missing browser build, and `app/` is a real page instead of a
404: `site/content/app.md`, a Zola page at `app/` that says the browser
build is not part of this copy of the site and why that happens. When the
wasm build succeeded, the workflow replaces that page's folder with the
app. The same page is what a local `zola build` shows (no wasm there), so
it is never a dead link. Considered: a separate stand-in HTML written by
the workflow (dropped: not visible in local previews, and a second place
holding site text), and failing the site on a missing app (dropped: the
issue asks that the docs never wait on the browser build).

**The front page claims only what was deployed** (review of 612a5d89). The
`site` job appends `app = true|false` to the gitignored `site/version.toml`
before `zola build`; `true` only when the wasm job succeeded, the artifact
downloaded (that download is `continue-on-error`) and
`web/check-app-page.sh` passes on it. The front page then shows "Open
CamCraft X in your browser", or "The browser version of CamCraft X is not
available" (same link, to the page that says why), or, with no version
file, the local-preview line. After the copy into `public/app/` the check
runs again where the page is served; a failure there is not fatal either:
the copy is dropped, the flag flips to `false` and Zola builds again (half
a second), with the same warning annotation. Considered: running the check
only in the wasm job (dropped: it would not prove the Pages artifact holds
the files, which the issue's "Done means" asks for), and a second data file
(dropped: version.toml is already the one file the workflow writes for the
templates). The first-run page is Markdown and cannot read the flag, so its
text says the browser version exists "when this site's release has one"
and points at the front page.

**The front page's note** sits in `templates/index.html`, not in
`_index.md`, because it names the version (`site/version.toml`), which only
the templates read. The list of what does not work comes from #31's last
comment: no file picker, no playback without threads, no WebGPU.

**#949 (`Cargo.lock` rewritten by the build).** Not stacked on draft PR
#1009. The wasm build runs on its own runner and only `target/web/` leaves
it (the `wasm-app` artifact), so a rewritten lock cannot reach any package
or the site. The script on main already restores the lock with an EXIT
trap around its cargo build; a cheap `if: always()` step warns if the lock
is still modified, so #1009 can be judged from a real run. #1009 stays
independent and can merge in any order.

**No new trigger.** The job rides `release.yml`'s existing triggers (a
`vX.Y.0` tag, or a hand-run `workflow_dispatch`, which builds and deploys
nothing). AGENTS.md's Actions budget forbids `push` / `pull_request` /
`schedule`; a per-PR browser build would also cost more minutes than the
whole release.

**Pinned downloads.** The wasi sysroot is pinned by version and sha256
inside the script; the wasm-bindgen CLI is installed by
`cargo install --version <lock's version> --locked` (crates.io checksums
via its own lock); clang-18 / llvm-18 come from Ubuntu's signed archive
like the linux job's apt packages; the runner is pinned to `ubuntu-24.04`
so the apt package names hold. No `wasm-opt`: binaryen has no pinned,
checksummed download in this repo, and the size is well inside Pages'
limits (100 MB per file). No rust-cache on the job: tags come weeks apart
and GitHub evicts a cache unused for 7 days, so it would only cost the
save.
