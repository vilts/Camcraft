//! The `g-edm.cps` script suite (#2700, the per-script-suite law of #926):
//! the post binary on every recorded edge-case stream
//! (`recorded/streams/*.cnc`), its program or its named refusal compared
//! with the golden in `recorded/golden/g-edm/`. The recorded streams are
//! cutting streams posted machine-less (or on a mill), so the G-EDM wire
//! post refuses each by name - it posts only wire EDM work on a machine
//! declared `wire_edm`; its wire program is pinned by app's `g_edm_post`.
//!
//! Regenerate after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-post --test script_g_edm`
//!
//! Gate map (#2700): covers assets/posts/g-edm.cps, camcraft_post::post::engine,
//! camcraft_post::post::cnc, the post binary.

#[macro_use]
mod recorded;

post_suite!("g-edm.cps");
