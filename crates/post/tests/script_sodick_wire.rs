//! The `sodick-wire.cps` script suite (#617, the per-script-suite law of
//! #926): the post binary on every recorded edge-case stream
//! (`recorded/streams/*.cnc`), its program or its named refusal compared
//! with the golden in `recorded/golden/sodick-wire/`. The recorded
//! streams are cutting streams posted machine-less (or on a mill), so the
//! Sodick wire post refuses each by name - it posts only wire EDM work on
//! a machine declared `wire_edm`; its wire program is pinned by app's
//! `sodick_wire_post`.
//!
//! Regenerate after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-post --test script_sodick_wire`
//!
//! Gate map (M4.4): covers assets/posts/sodick-wire.cps, camcraft_post::post::engine,
//! camcraft_post::post::cnc, the post binary.

#[macro_use]
mod recorded;

post_suite!("sodick-wire.cps");
