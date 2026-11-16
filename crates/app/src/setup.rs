//! Multiple setups: who machines against which stock and which WCS
//! (M3.2 stage A; one WCS per setup since #342).
//!
//! Provenance: our own design; the setup/WCS concept is standard
//! machine-shop practice (a setup is one clamping of the part against one
//! datum system; a flip is an unclamp, a re-clamp with the parts' faces
//! exchanged, and a datum transfer, not an axis rotation). No vendor
//! implementation inspected.
//!
//! ## The laws here
//!
//! - **The implicit-setup law.** A world with no `SetupDef` entity
//!   resolves exactly the way it did before this module existed: stock
//!   is the entity named `STOCK_NAME`, the WCS is the entity `G54`. The
//!   first `CreateSetup` on such a world MATERIALIZES `Setup1` pointing
//!   at those same existing entities - nothing is renamed, so every
//!   path, stamp and saved file predating setups keeps working
//!   untouched. `None` anywhere a setup is expected means "the first
//!   setup by name", which collapses to that same legacy behavior.
//! - **One order law, shared by call.** [`operation_order`] is THE
//!   program sequence: setup rank first, explicit operation position
//!   within a setup (name order for legacy operations without a position)
//!   (`Setup1` before `Setup2`, always). The post, re-machining, the
//!   document sim and batch regenerate all call this one function,
//!   upgrading M3.1 D's "shared by reference" to "shared by call" -
//!   re-machining and the machine can still not disagree, now including
//!   across setups.
//! - **Placement is the flip.** Setup space IS model/world space in a
//!   Z-up 2.5D engine: what changes between setups is where the stock
//!   sits (its `Location`, quaternion included) and where the datum
//!   sits - never a rotated machine axis. The axis-rotation form of a
//!   flip belongs to M3.3's orientation law; until then a flipped
//!   setup is a re-placed (or copied-and-flipped) stock, which is what
//!   the datum-transfer command builds.
//! - **Inert until named** (AGENTS.md M1.6 law). The active-setup
//!   singleton is read only at command creation (what setup new
//!   furniture and new operations join); nothing digests it, so making
//!   another setup active staleness-floats no path. An operation's
//!   `setup` field, by contrast, flows into its stamp through the
//!   RESOLVED heights - a setup change that moves the stock or a WCS
//!   marks the operations that machine against it, all by machinery
//!   that already existed.
//! - **Rest masks are same-setup only** (stage A limitation, logged in
//!   RECOMMENDED.md): re-machining looks at operations in the
//!   CONSUMER's setup ahead of it. A cross-setup mask would need to
//!   transform one setup's cut state into the other's placement; until
//!   that engine exists, a later setup re-cuts conservatively (the mask
//!   law means that is wasted air time, never a gouge).

use flecs_ecs::core::{EntityViewGet, WorldGet}; // not in prelude (0.2.2)
use flecs_ecs::prelude::World;

use crate::model;
use crate::schema::{SetupDef, STOCK_NAME, LEGACY_WCS_NAMES};

/// Every setup entity, name-sorted: the program's setup order.
#[must_use]
pub fn setups(world: &World) -> Vec<(String, flecs_ecs::prelude::Entity)> {
    model::named_entities(world)
        .into_iter()
        .filter(|(_, e)| model::is_setup(world, e))
        .collect()
}

/// The setup names in order.
#[must_use]
pub fn names(world: &World) -> Vec<String> {
    setups(world).into_iter().map(|(n, _)| n).collect()
}

/// The first setup by name - what `None` means.
#[must_use]
pub fn first(world: &World) -> Option<String> {
    names(world).into_iter().next()
}

#[must_use]
pub fn exists(world: &World, name: &str) -> bool {
    model::find_by_name(world, name).is_some_and(|e| model::is_setup(world, &e))
}

/// The definition of a named setup, if the name wears one.
#[must_use]
pub fn def_of(world: &World, name: &str) -> Option<SetupDef> {
    model::find_by_name(world, name)
        .and_then(|e| e.entity_view(world).try_get::<&SetupDef>(|d| d.clone()))
}

/// The work offset a setup is LABELLED with (#196, #342, #931), as its
/// 0-based pick; `None` when it names none (offset 1, [`wcs_slot`]) or
/// the name wears no setup. Returned as stored: a pick the setup's post
/// cannot express is NEVER read as no pick (offset 1) - the post-time
/// gate and the command refuse it by setup name
/// ([`crate::setup_offsets`]).
#[must_use]
pub fn work_offset(world: &World, name: &str) -> Option<usize> {
    def_of(world, name)?.work_offset
}

/// THE work-offset law (#196, #329, #342, #931): the 0-based pick every
/// operation of the declared setup context posts at - the setup's label,
/// else 0 (offset 1). The stream carries `pick + 1`
/// ([`crate::setup_offsets::index`]); the post writes its word. It is
/// only the offset: the origin is the setup's one WCS ([`wcs_entity`])
/// whatever the offset, so one setup is one program zero by
/// construction. A document without setups (or a context naming no
/// setup) has no label and reads offset 1.
#[must_use]
pub fn wcs_slot(world: &World, setup: Option<&str>) -> usize {
    effective(world, setup)
        .and_then(|s| work_offset(world, &s))
        .unwrap_or(0)
}

/// The effective setup of an operation or piece of furniture: what the
/// document resolves a declared `Option<String>` to.
#[must_use]
pub fn effective(world: &World, declared: Option<&str>) -> Option<String> {
    declared.map(str::to_owned).or_else(|| first(world))
}

/// The ACTIVE setup - which context new commands join (M3.2 A). Reads
/// the singleton, falling back to the first setup when it names nothing
/// (or nothing that still exists), so a document with setups always
/// has an active one and a document without has `None` (legacy mode).
#[must_use]
pub fn active(world: &World) -> Option<String> {
    let stored = stored_active(world);
    match stored {
        Some(s) if exists(world, &s) => Some(s),
        _ => first(world),
    }
}

/// The singleton exactly as stored, WITHOUT the first-setup fallback -
/// what a save writes, so an untouched document keeps its legacy
/// byte-shape (`active()`'s resolution would make a choice the user
/// never made).
#[must_use]
pub fn stored_active(world: &World) -> Option<String> {
    world
        .try_get::<&crate::schema::ActiveSetup>(|a| a.0.clone())
        .unwrap_or(None)
}

/// Land the active-setup choice. For [`crate::commands::ActivateSetup`]
/// and the project loader only - the UI queues the command, never pokes
/// this (AGENTS.md).
pub fn set_active(world: &World, name: Option<String>) {
    world.set(crate::schema::ActiveSetup(name));
}

/// Program-order rank of a declared setup: 0 for `None` and for the
/// first setup (they mean the same thing), 1+ for each later one in
/// name order. A name that no longer wears a setup ranks 0 - the same
/// fallback [`stock_entity_name`] applies, so order and resolution
/// cannot disagree about a corrupt reference.
#[must_use]
pub fn rank(order: &[String], declared: Option<&str>) -> usize {
    match declared {
        None => 0,
        Some(s) => order.iter().position(|n| n == s).unwrap_or(0),
    }
}

/// Every operation in program order: setup rank, then explicit position,
/// then stable key. Older operations without a position retain name order. THE
/// sequence - see the module header; `posting::program_ops`,
/// `rest::ahead_of`, `stock_sim::simulate_document`,
/// `machine::check_document` and `RegenerateAll` all walk this.
#[must_use]
pub fn operation_order(world: &World) -> Vec<(String, flecs_ecs::prelude::Entity)> {
    let order = names(world);
    let mut ops: Vec<(usize, u32, String, flecs_ecs::prelude::Entity)> = model::named_entities(world)
        .into_iter()
        .filter(|(_, e)| model::is_operation(world, e))
        .map(|(n, e)| {
            let declared = model::operation_of(world, &e).and_then(|o| o.setup);
            let position = model::operation_meta_of(world, &e).order.unwrap_or(u32::MAX);
            (rank(&order, declared.as_deref()), position, n, e)
        })
        .collect();
    ops.sort_by_key(|(r, position, _, _)| (*r, *position)); // stable: key order breaks ties
    ops.into_iter().map(|(_, _, n, e)| (n, e)).collect()
}

/// Does an operation with this declared setup machine in `target`? The
/// `DeleteSetup` refusal predicate - `None` belongs to whichever setup
/// is first, exactly as the resolvers below will read it.
#[must_use]
pub fn belongs(world: &World, declared: Option<&str>, target: &str) -> bool {
    effective(world, declared).is_some_and(|s| s == target)
}

/// The stock entity NAME for a declared setup context.
#[must_use]
pub fn stock_entity_name(world: &World, declared: Option<&str>) -> String {
    match effective(world, declared) {
        Some(s) => def_of(world, &s)
            .map(|d| d.stock)
            .unwrap_or_else(|| format!("{s}/Stock")),
        None => STOCK_NAME.to_string(),
    }
}

/// The suffix a setup's own WCS entity wears (`Setup2/WCS`, #342).
pub const WCS_SUFFIX: &str = "WCS";

/// The WCS entity NAME for a declared setup context (#342): the setup's
/// one work coordinate system. A document without setups keeps the
/// legacy `G54` entity (the implicit-setup law); a setup names its own
/// (`SetupDef::wcs`), `<setup>/WCS` when it names none. The G-code WORD
/// is never this name - it is the setup's label ([`wcs_slot`]).
#[must_use]
pub fn wcs_entity_name(world: &World, declared: Option<&str>) -> String {
    match effective(world, declared) {
        Some(s) => def_of(world, &s)
            .map(|d| d.wcs)
            .filter(|w| !w.is_empty())
            .unwrap_or_else(|| format!("{s}/{WCS_SUFFIX}")),
        None => LEGACY_WCS_NAMES[0].to_string(),
    }
}

/// The WCS offset a setup context posts, for display: `"WCS (G55)"`
/// where the offset matters (#342) - the tree says plain "WCS". The
/// name is the setup's post's (#931, [`wcs_name`]); display only.
#[must_use]
pub fn wcs_label(world: &World, declared: Option<&str>) -> String {
    format!("WCS ({})", wcs_name(world, declared))
}

pub use crate::setup_offsets::{offset_name, wcs_name, work_offset_names};

/// The stock ENTITY for a declared setup, when one exists.
#[must_use]
pub fn stock_entity(world: &World, declared: Option<&str>) -> Option<flecs_ecs::prelude::Entity> {
    let e = model::find_by_name(world, &stock_entity_name(world, declared))?;
    model::is_stock(world, &e).then_some(e)
}

/// The setup's WCS ENTITY, when it has been placed.
#[must_use]
pub fn wcs_entity(world: &World, declared: Option<&str>) -> Option<flecs_ecs::prelude::Entity> {
    let e = model::find_by_name(world, &wcs_entity_name(world, declared))?;
    model::is_wcs(world, &e).then_some(e)
}

/// Does a setup OTHER than `except` name this entity as its stock or
/// its WCS? The `DeleteSetup` sharing test: furniture two setups point
/// at is nobody's to delete with one of them.
#[must_use]
pub fn is_furniture_of_other(world: &World, entity: &str, except: &str) -> bool {
    setups(world).into_iter().any(|(n, _)| {
        n != except
            && def_of(world, &n).is_some_and(|d| {
                d.stock == entity || d.wcs == entity
            })
    })
}

/// Does this name follow the private-furniture convention - some
/// prefix, then `/Stock`, `/WCS` (#342) or a pre-v54 `/G5x` slot (M3.2
/// A)? A shape test, with
/// no world to consult: what the declared setups actually name is
/// [`stock_entity_name`] / [`wcs_entity_name`] per setup. Shared by
/// [`crate::commands::AddTool`]'s refusal and the project file's
/// reserved-name validation - the law once, not twice.
#[must_use]
pub fn is_furniture_shape(name: &str) -> bool {
    name.ends_with("/Stock")
        || name.ends_with(&format!("/{WCS_SUFFIX}"))
        || name
            .rsplit_once('/')
            .is_some_and(|(_, slot)| LEGACY_WCS_NAMES.contains(&slot))
}

/// The next free `SetupN` name (smallest N, up to a sane 99,999, with no
/// entity wearing it - any entity, not just setups, because `Name` is
/// the world's one-name-one-entity law). `None` means the scan found
/// every candidate taken; the caller refuses rather than squatting.
#[must_use]
pub fn next_setup_name(world: &World) -> Option<String> {
    (1..100_000).find_map(|n| {
        let name = format!("Setup{n}");
        model::find_by_name(world, &name).is_none().then_some(name)
    })
}
