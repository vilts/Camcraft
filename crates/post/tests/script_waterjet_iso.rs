//! The `waterjet-iso.cps` script suite (#926, #1882): the post binary on
//! every recorded edge-case stream (`recorded/streams/*.cnc`), its
//! program or its named refusal compared with the golden in
//! `recorded/golden/waterjet-iso/`. No recorded stream is a waterjet
//! cut, so each is refused by name.
//!
//! Regenerate after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-post --test script_waterjet_iso`
//!
//! Gate map (M4.4): covers assets/posts/waterjet-iso.cps, camcraft_post::post::engine,
//! camcraft_post::post::cnc, the post binary.

#[macro_use]
mod recorded;

post_suite!("waterjet-iso.cps");
