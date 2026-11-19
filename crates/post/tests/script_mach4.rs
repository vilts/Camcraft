//! The `mach4.cps` script suite (#1968): the post binary on every recorded
//! edge-case stream (`recorded/streams/*.cnc`), its program or its named
//! refusal compared with the golden in `recorded/golden/mach4/`. No app, no
//! toolpath generation: the inputs are recorded, and their generators keep
//! them current (`recorded_streams.rs`, app's `recorded_post_streams.rs`).
//!
//! Regenerate after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-post --test script_mach4`
//!
//! Gate map (M4.4): covers assets/posts/mach4.cps, camcraft_post::post::engine,
//! camcraft_post::post::cnc, the post binary.

#[macro_use]
mod recorded;

post_suite!("mach4.cps");
