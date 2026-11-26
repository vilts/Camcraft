//! #2598: a hole operation's feed is its tool's. A Drill feeds the whole
//! end of the tool axially into the work, and a Thread op states its feed
//! at the cutting edge (`motion::machining2d::thread_mill`), so for these
//! two kinds the feed the tool states (its cutting-data row for the
//! material in force, else its running feed; never above a running feed
//! the user lowered for that material, #2724) is the rating:
//!
//! - a new Drill or Thread op, and one given a new tool, takes that feed
//!   (`cutting_rows::seed_feed_for_tool`, `cutting_rows::retargeted_params`,
//!   inside the creating or retargeting command, one undo step);
//! - a program whose Drill or Thread op feeds above it is refused at the
//!   post by name ([`problems`]), in the per-move form of the machine's
//!   feed ceiling. The clause reads the op as it stands, so a project
//!   saved before #2598 with the old 1200 / 300 seed loads unchanged and
//!   is refused at its first post, never silently re-fed.
//!
//! The rating stays the user's: raising the tool's feed (its maker allows
//! more) or lowering the op's clears the refusal. Milling kinds are not
//! judged here: their tool feed is a starting point that the stepover and
//! the chip thinning move (`cutting::suggest`).
//!
//! Provenance: general drilling and thread-milling practice (the maker's
//! feed for the tool and the work material); our own law, no vendor
//! material consulted.

use flecs_ecs::prelude::World;

use crate::cl::{MoveKind, Toolpath};
use crate::drilling::CycleChoice;
use crate::model;
use crate::operation::OpParams;
use crate::tools::{Tool, ToolKind, WorkMaterial};
use crate::units::{self, Unit};

/// Float slack on the comparison: a seeded feed is a copy of the rating.
const SLACK: f64 = 1e-6;

/// The feed `tool` states for `material` (the one in force on the op's
/// setup): its row in force, else its running feed. `None` when that is
/// not a positive, finite number (no rating to seed from or to judge
/// against).
///
/// #2724: the running feed follows the `document` material (`ChangeTool`,
/// `SetWorkMaterial`), so when `material` is that one, a running feed set
/// below the row is the user's limit too: the lower of the two is the
/// rating. A setup whose stock states another material is rated by that
/// material's row alone - the running feed speaks for a different
/// material there.
#[must_use]
pub fn rated_feed(tool: &Tool, material: Option<WorkMaterial>, document: Option<WorkMaterial>) -> Option<f64> {
    let usable = |f: f64| (f.is_finite() && f > 0.0).then_some(f);
    let Some(row) = crate::cutting_rows::row_in(tool, material) else {
        return usable(tool.feed);
    };
    let running = if material == document { usable(tool.feed) } else { None };
    match (usable(row.feed), running) {
        (Some(r), Some(t)) => Some(r.min(t)),
        (r, t) => r.or(t),
    }
}

/// Seed a Drill's or a Thread's feeds from `tool`'s rating for `material`
/// ([`rated_feed`]): the Drill's cutting and plunge feed both take it; the
/// Thread's edge feed takes it and its plunge feed is held to it. `true`
/// when anything moved; every other kind is left alone.
pub fn seed_into(
    params: &mut OpParams,
    tool: &Tool,
    material: Option<WorkMaterial>,
    document: Option<WorkMaterial>,
) -> bool {
    let Some(rated) = rated_feed(tool, material, document) else {
        return false;
    };
    let set = |field: &mut f64, v: f64| {
        let moved = *field != v;
        *field = v;
        moved
    };
    match params {
        OpParams::Drill { feed, plunge_feed, .. } => set(feed, rated) | set(plunge_feed, rated),
        OpParams::Thread { feed, plunge_feed, .. } => {
            let plunge = if plunge_feed.is_finite() { plunge_feed.min(rated) } else { rated };
            set(feed, rated) | set(plunge_feed, plunge)
        }
        _ => false,
    }
}

/// The highest feed the op runs its tool at, as the rating judges it:
/// a Drill's posted cycle feeds (its cached path; a tapping cycle feeds
/// at the tool's pitch x rpm and is not judged here), a Thread's stated
/// edge and plunge feeds (its posted arcs are scaled to the tool centre).
fn judged_feed(tool: &Tool, params: &OpParams, path: Option<&Toolpath>) -> Option<f64> {
    match params {
        OpParams::Drill { cycle, .. } => {
            let taps = matches!(cycle, CycleChoice::Tap | CycleChoice::FloatingTap)
                || (matches!(cycle, CycleChoice::Auto) && tool.kind == ToolKind::Tap);
            if taps {
                return None;
            }
            path?
                .moves
                .iter()
                .filter(|m| m.kind != MoveKind::Rapid)
                .filter_map(|m| m.feed)
                .filter(|f| f.is_finite())
                .reduce(f64::max)
        }
        OpParams::Thread { feed, plunge_feed, .. } => Some(feed.max(*plunge_feed)),
        _ => None,
    }
}

fn per_minute(unit: Unit) -> &'static str {
    match unit {
        Unit::Millimeters => "mm/min",
        Unit::Inches => "in/min",
    }
}

/// The post clause: one blocking line for each program op (Drill or
/// Thread) that feeds above its tool's rating, naming the op, the feed,
/// the tool and its rating in the document's unit.
#[must_use]
pub fn problems(world: &World) -> Vec<String> {
    // An unreadable op list is named by the gate's other clauses.
    let Ok(ops) = crate::posting::program_ops(world) else {
        return Vec::new();
    };
    let unit = units::of(world);
    let u = per_minute(unit);
    let document = crate::cutting::material_of(world);
    ops.iter()
        .filter_map(|(name, e)| {
            let op = model::operation_of(world, e)?;
            if !matches!(op.params, OpParams::Drill { .. } | OpParams::Thread { .. }) {
                return None;
            }
            let tool = model::find_by_name(world, &op.tool).and_then(|t| model::tool_of(world, &t))?;
            let material = crate::stock_material::in_force(world, op.setup.as_deref());
            let rated = rated_feed(&tool, material, document)?;
            let cache = model::toolpath_of(world, e);
            let feed = judged_feed(&tool, &op.params, cache.as_ref().map(|c| &c.path))?;
            (feed > rated + SLACK).then(|| {
                format!(
                    "{name}: feeds {} {u}, above the {} {u} that tool {} states - lower the operation's feed, \
                     or raise the tool's feed if its maker allows more",
                    units::feed(feed, unit),
                    units::feed(rated, unit),
                    op.tool,
                )
            })
        })
        .collect()
}
