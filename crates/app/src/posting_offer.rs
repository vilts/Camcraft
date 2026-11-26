//! The post a setup may be offered instead of its own (M13.5 B, #2374):
//! the Setup page's banner and the MCP `load_machine` / `get_job`
//! replies read the same answer, so a person and an agent are told the
//! same thing. The pane or the agent chooses - a mount never re-points
//! the post by itself, and a `File` post is never second-guessed.
//!
//! Provenance: our own rule (#669, #2374); no vendor material.

use super::{default_post_for, post_name, post_serves, LATHE_POSTS, PLASMA_POSTS, SINKER_POSTS, WIRE_POSTS, BUILTINS};
use crate::schema::PostRef;
use flecs_ecs::prelude::*;

/// Why a post is offered (#2374).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfferWhy {
    /// The mounted machine's file names this post (`controller.post`).
    Machine,
    /// The current post does not serve the mount's kind; the word is
    /// what it serves instead ("mill" / "lathe").
    Kind(&'static str),
}

/// A post a setup may take instead of its own.
#[derive(Clone, Debug, PartialEq)]
pub struct PostOffer {
    pub post: PostRef,
    pub why: OfferWhy,
}

/// The offer for the active setup - what the Setup page's banner shows.
#[must_use]
pub fn post_offer(world: &World) -> Option<PostOffer> {
    post_offer_for(world, crate::setup::active(world).as_deref())
}

/// The offer for a declared setup context (`None`: the implicit first
/// setup): the post its machine names, when that is a registry post that
/// serves the mount and not the setup's post; else, when the setup's
/// registry post does not serve the mount's kind, the registry's default
/// for it.
#[must_use]
pub fn post_offer_for(world: &World, declared: Option<&str>) -> Option<PostOffer> {
    let kind = crate::machine::kind_of_setup(world, declared);
    let current = crate::setup_mount::post_of(world, declared);
    let name = post_name(&current)?;
    let named = crate::machine::of_setup(world, declared).1.and_then(|m| m.post)
        .map(|n| super::agie::current_name(&n).to_owned());
    if let Some(named) = named.filter(|n| BUILTINS.iter().any(|(b, _)| b == n) && post_serves(n, kind)) {
        let why = OfferWhy::Machine;
        return (named != name).then_some(PostOffer { post: PostRef::Named(named), why });
    }
    if post_serves(name, kind) {
        return None;
    }
    let serves = if LATHE_POSTS.contains(&name) {
        "lathe"
    } else if PLASMA_POSTS.contains(&name) {
        "plasma"
    } else if WIRE_POSTS.contains(&name) {
        "wire EDM"
    } else if SINKER_POSTS.contains(&name) {
        "sinker EDM"
    } else {
        "mill"
    };
    // #669: an offer that would not serve the mount either (a plasma
    // table before any plasma post ships) is no offer.
    let offer = default_post_for(kind);
    let serves_mount = post_name(&offer).is_some_and(|n| post_serves(n, kind));
    serves_mount.then_some(PostOffer { post: offer, why: OfferWhy::Kind(serves) })
}
