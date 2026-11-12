## #163 - tracing for the app's ad-hoc eprintln! diagnostics (second slice)

**Decided.** The 12 library-side `eprintln!` diagnostics in `camcraft-app`
(Top/stock, carried stock, drill prior, rapid-law job, stock view, rotary
verdict, playback worker, kinds table, STEP import, model transform, MCP
bridge) became `tracing` events: `warn!` for a degraded-but-continuing
fallback, `error!` for a thread death, a kernel panic caught, or an
impossible table index. Budget/limit refusals were already logged at warn at
the generate seam with the generator's own message (which names the limit and
value); a test now pins that. The subscriber was checked: it writes to
stderr, so the MCP stdio JSON-RPC stream on stdout is never touched.

**Left as println/eprintln on purpose.** `camcraft-cli` command results and
usage errors (user-facing output, MCP protocol), tests and harness prints,
`xtask`, `build.rs`, the standalone `bin/` tools of the sub-crates.

**Still owed (Refs, not Fixes).** The `camcraft-motion`, `-cl`, `-sim`,
`-geometry`, `-tools` library eprintlns (~25) need `tracing` as a dependency of
those crates; and the `camcraft-ui` GPU/shell prints go through `ui::diag`.
Separate follow-up so each stays a small, reviewable change.

**Why.** One logging path, filterable with `CAMCRAFT_LOG`, with the op name
as a field. Considered: `log` facade (dropped, tracing is already in).
