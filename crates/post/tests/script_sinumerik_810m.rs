//! The `sinumerik-810m.cps` script suite (#2888): the post binary on every
//! recorded edge-case stream (`recorded/streams/*.cnc`), its program or its
//! named refusal compared with `recorded/golden/sinumerik-810m/`.
//!
//! Regenerate after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-post --test script_sinumerik_810m`

#[macro_use]
mod recorded;

post_suite!("sinumerik-810m.cps");
