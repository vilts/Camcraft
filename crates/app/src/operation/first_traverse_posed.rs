//! #3084: generate's seam to the posed first-traverse law (`verify::first_traverse_low_posed`): which operations, on which mounted machines, read their work through every pose of the machine's work-turning rotaries; our own reading of the shop rule "never traverse through uncut stock or steel", no vendor material consulted.
//!
//! The post sends Z, then X/Y at that height, then the move with the
//! rotary words (`traverseZ`, `xyFirst`). Before that move the
//! work-turning rotaries stand where the previous program left them.
//! The #2965 laws read one fitted axis (the rotary kinds) or the part
//! frame (the multi-axis tip law), so they miss:
//! 1. a rotary kind on a machine where two rotaries turn the work;
//! 2. a multi-axis kind on a machine whose table tilts the work;
//! 3. (#3100) a 3+2 plane (`Tilt`) and a cross drill on such a machine:
//!    the #2987 law reads them at the operation's own pose.
//!
//! Each line of a work-turning rotary passes through the WCS origin
//! (the indexed post's setup law, as the index-swing clause reads it).
//! A stated non-zero `pivot` or `offset` moves it off: refused (#3127).
//! Decisions #3084 and #3100 have the premises and the limits.

use crate::cl::{TiltedFrame, Toolpath};
use crate::fixture::{FixtureBox, MACHINE_TABLE};
use crate::geometry::Point3;
use crate::machine::AxisLimits;
use crate::posting::index_swing::{RotaryLine, off_origin, rotary_line};
use crate::verify::{self, PosedTurn, Warning};

use super::OpParams;
use super::kinds::Section;
use super::material::Z_TOL;

/// What the posed law reads: the pose set, the work's points (world),
/// the setup's steel and its names (`fixture::named_boxes_for`), the
/// operation's clearance height in the path's frame, and the machine's
/// rotaries (#3100: whether the table stays fixed under the work).
pub(super) struct Posed<'a> {
    pub turn: Option<PosedTurn>,
    pub rides: &'a [Point3],
    pub fixtures: &'a [FixtureBox],
    pub names: &'a [String],
    pub clearance: f64,
    pub rotaries: Option<&'a [AxisLimits]>,
}

/// A rotary whose line can lean the work: a stated axis with an X or Y
/// part, or no stated axis (the rule of `first_traverse::tilting_all`).
fn tilts(a: &AxisLimits) -> bool {
    a.axis.is_none_or(|d| d[0].abs() > 1e-9 || d[1].abs() > 1e-9)
}

fn multi_axis(params: &OpParams) -> bool {
    let info = params.kind().info();
    info.section == Section::MultiAxis || info.mode == verify::Mode::Rotary
}

/// #3100: the operations whose rotary words the post writes only on the
/// move after the X/Y traverse, and that the posed law reads: the
/// multi-axis and rotary kinds (#3084), each 3+2 plane (a kind that
/// carries a `Tilt`), and the cross drill.
fn posed_kind(params: &OpParams) -> bool {
    multi_axis(params) || params.tilt().is_some() || matches!(params, OpParams::Drill { cross, .. } if !cross.is_empty())
}

/// The machine's work-turning rotaries (`turns_part == Some(true)`).
fn work_turning(rotaries: &[AxisLimits]) -> Vec<&AxisLimits> {
    rotaries.iter().filter(|a| a.rotary && a.turns_part == Some(true)).collect()
}

/// The pose set the posed law reads for this operation on a machine that
/// posts the mill order (`mill`), or `Ok(None)` when the law does not
/// apply: not a [`posed_kind`], no machine, no work-turning
/// rotary that tilts, or a rotary kind on a machine where one rotary
/// turns the work (#2965's swept law reads it). Two or more
/// work-turning rotaries give the sphere about the WCS origin; one gives
/// its line through it. A lone tilting rotary whose line is not known
/// refuses by name: the work it turns cannot be bounded.
pub(super) fn turn_of(params: &OpParams, mill: bool, rotaries: Option<&[AxisLimits]>, wcs_origin: [f64; 3]) -> Result<Option<PosedTurn>, String> {
    if !mill || !posed_kind(params) {
        return Ok(None);
    }
    let turning = rotaries.map(work_turning).unwrap_or_default();
    if !turning.iter().any(|a| tilts(a)) {
        return Ok(None);
    }
    let origin = Point3::new(wcs_origin[0], wcs_origin[1], wcs_origin[2]);
    // #3127: the sphere and the line both pass through the WCS origin.
    let off = |a: &AxisLimits| {
        format!(
            "{}: the work-turning rotary {} states a pivot or offset that moves its line off the WCS origin, so the first traverse over the work it turns cannot be judged",
            params.kind().info().label,
            a.name
        )
    };
    if turning.len() >= 2 {
        if let Some(a) = turning.iter().find(|a| off_origin(a)) {
            return Err(off(a));
        }
        return Ok(Some(PosedTurn::About { origin }));
    }
    if params.kind().info().mode == verify::Mode::Rotary {
        return Ok(None);
    }
    let Some(a) = turning.first() else { return Ok(None) };
    if off_origin(a) && rotary_line(a) != RotaryLine::Vertical {
        return Err(off(a));
    }
    let stated = a.axis.filter(|d| d.iter().all(|c| c.is_finite()) && d.iter().any(|c| c.abs() > 1e-12));
    match (rotary_line(a), stated) {
        (RotaryLine::Vertical, _) => Ok(None),
        (RotaryLine::Horizontal(dir), _) | (RotaryLine::Leaning, Some(dir)) => Ok(Some(PosedTurn::Line { origin, dir })),
        (RotaryLine::Leaning | RotaryLine::Unknown, _) => Err(format!(
            "{}: the work-turning rotary {} states no usable axis line, so the first traverse over the work it turns cannot be judged",
            params.kind().info().label,
            a.name
        )),
    }
}

/// #3084: whether the posed law reads this operation on its setup's
/// machine (a refusal counts: the law reads it too). Its op stamp then
/// takes one more word, so a cache judged without the law reads Stale
/// once; no other stamp moves. The stamp knows neither the tool nor the
/// path, and a mill-turn posts an unstamped rotary kind in the mill order
/// too (#3096, `rotary_traverse::mill_post_rotary`). So it reads each mill
/// and mill-turn setup as the mill order: a word too many only reads a
/// cache Stale once, a word too few would keep an unjudged cache fresh.
///
/// #3100: a post that declares `rotariesBeforeTraverse` takes the word
/// away ([`post_order`]): the law then does not read the operation, so
/// a change of the setup's post, or of its declaration, moves the stamp.
pub(super) fn newly_judged(world: &flecs_ecs::prelude::World, setup: Option<&str>, params: &OpParams) -> bool {
    reads(world, setup, params) && !post_order(world, setup, params).first
}

/// [`turn_of`] on the setup's machine, as the stamp reads it (above).
fn reads(world: &flecs_ecs::prelude::World, setup: Option<&str>, params: &OpParams) -> bool {
    use crate::machine::Kind;
    let mill = matches!(crate::machine::kind_of_setup(world, setup), Kind::Mill | Kind::MillTurn);
    let rotaries: Vec<AxisLimits> = crate::machine::of_setup(world, setup).1.map(|m| m.axes).unwrap_or_default();
    !matches!(turn_of(params, mill, Some(&rotaries), [0.0; 3]), Ok(None))
}

/// #3100: what the setup's post declares about its first traverse.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PostOrder {
    /// The script declares `rotariesBeforeTraverse = true`: it turns the
    /// rotaries to the section's pose at the safe Z, then traverses X/Y
    /// (`post::dialect::turns_rotaries_first`). The X/Y traverse then runs
    /// at the operation's own pose, which the #2987 law reads.
    pub first: bool,
    /// The declaring script is the user's own (`PostRef::File`): taken at
    /// its word, never silently ([`declared_note`]). Each shipped post
    /// that declares it has a posting test of the real order.
    pub custom: bool,
}

/// The note on the path of an operation that a user's own post script
/// takes out of the posed law by its declaration.
pub(super) const DECLARED_NOTE: &str = "First traverse: judged at the operation's own pose, because the post script declares rotariesBeforeTraverse (it turns the rotaries at the safe Z before the first X/Y traverse) - this script's order is taken at its word";

/// #3100: the setup's post's declaration, read only for an operation
/// that the posed law would read ([`reads`]). An unknown post, a script
/// that does not load, or one that declares nothing reads the strict
/// way: `first == false`, never looser.
pub(super) fn post_order(world: &flecs_ecs::prelude::World, setup: Option<&str>, params: &OpParams) -> PostOrder {
    if !reads(world, setup, params) {
        return PostOrder::default();
    }
    let post = crate::setup_mount::post_of(world, setup);
    let first = crate::posting::script_of(&post).ok().is_some_and(|(_, text)| declares_first(&text));
    PostOrder { first, custom: first && matches!(post, crate::schema::PostRef::File(_)) }
}

/// The most script texts [`declares_first`] keeps.
const SEEN_MAX: usize = 32;

/// The declaration of one script text, loaded once per text: the stamp
/// asks for it on every read, and a script load runs the whole script.
fn declares_first(text: &str) -> bool {
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    use std::sync::{Mutex, OnceLock};
    static SEEN: OnceLock<Mutex<HashMap<u64, bool>>> = OnceLock::new();
    let mut h = std::hash::DefaultHasher::new();
    text.hash(&mut h);
    let key = h.finish();
    let seen = SEEN.get_or_init(Mutex::default);
    if let Some(v) = seen.lock().ok().and_then(|m| m.get(&key).copied()) {
        return v;
    }
    let v = crate::post::dialect::turns_rotaries_first(text).unwrap_or(false);
    if let Ok(mut m) = seen.lock() {
        // Bounded: each edited user script is one more text, so the map
        // starts again past a handful (a re-read only loads the script).
        if m.len() >= SEEN_MAX {
            m.clear();
        }
        m.insert(key, v);
    }
    v
}

/// The height the post traverses X/Y at, in the path's frame: the higher
/// of the clearance and the first move's own height (`traverseZ`). A
/// rotary kind's first move is a radial pose, not that height, so it
/// reads the clearance alone (the post never traverses under it).
fn traverse_height(path: &Toolpath, params: &OpParams, clearance: f64) -> Option<f64> {
    let z = path.moves.first()?.from.z;
    let rotary = params.kind().info().mode == verify::Mode::Rotary;
    Some(match (clearance.is_finite(), rotary) {
        (true, true) => clearance,
        (true, false) => clearance.max(z),
        (false, _) => z,
    })
}

/// #3100: whether every tilting rotary of the machine turns the work.
/// The machine table then never turns (`fixture::workholding_for`): it
/// stays fixed under the work, and the #2987 table law does not read it
/// (`first_traverse::work_tilts`). A head that tilts keeps that law.
fn table_stays(rotaries: Option<&[AxisLimits]>) -> bool {
    let mut tilting = rotaries.unwrap_or_default().iter().filter(|a| a.rotary && tilts(a)).peekable();
    tilting.peek().is_some() && tilting.all(|a| a.turns_part == Some(true))
}

/// #3100: the machine table's top in the path's height scale (along unit
/// `up` from `base`), read where it stands: fixed in the machine. Every
/// pose turns the work about the WCS origin `o` (on each line), and the
/// operation's pose stands the path's `up` on the machine's +Z. So a
/// height `h` of the path is the machine height `h + (base - o) . up`
/// over `o`, and the table top is the machine height `top - o.z` over
/// it. `None` when the setup has no table box.
fn table_top(turn: &PosedTurn, fixtures: &[FixtureBox], names: &[String], up: [f64; 3], base: Point3) -> Option<f64> {
    let o = match *turn {
        PosedTurn::Line { origin, .. } | PosedTurn::About { origin } => origin,
    };
    let n = (up[0] * up[0] + up[1] * up[1] + up[2] * up[2]).sqrt();
    let up = if n.is_finite() && n > 1e-12 { [up[0] / n, up[1] / n, up[2] / n] } else { [0.0, 0.0, 1.0] };
    let lift = (o.x - base.x) * up[0] + (o.y - base.y) * up[1] + (o.z - base.z) * up[2];
    fixtures
        .iter()
        .zip(names)
        .filter(|(b, n)| n.as_str() == MACHINE_TABLE && b.max.z.is_finite())
        .map(|(b, _)| lift + b.max.z - o.z)
        .reduce(f64::max)
}

/// Add the posed verdict to `warns` when the traverse height stands
/// under the work or the tagged steel read through every pose. On a
/// machine where only the work tilts, the machine table is read too,
/// fixed where it stands (#3100, [`table_top`]); where a head tilts, the
/// #2987 law reads it. A verdict replaces each first-traverse verdict at
/// a lower top: one verdict names the highest top that the clearance
/// must clear.
pub(super) fn judge(path: &Toolpath, params: &OpParams, frame: Option<&TiltedFrame>, posed: &Posed<'_>, warns: &mut Vec<Warning>) {
    let Some(turn) = posed.turn else { return };
    let Some(height) = traverse_height(path, params, posed.clearance) else { return };
    let steel: Vec<FixtureBox> = posed.fixtures.iter().zip(posed.names).filter(|(_, n)| n.as_str() != MACHINE_TABLE).map(|(b, _)| *b).collect();
    let work = verify::PosedWork { turn, rides: posed.rides, steel: &steel };
    let (up, base) = frame.map_or(([0.0, 0.0, 1.0], Point3::new(0.0, 0.0, 0.0)), |f| (f.axis, f.origin));
    let worked = verify::first_traverse_low_posed(height, &work, up, base, Z_TOL);
    let table = table_top(&turn, posed.fixtures, posed.names, up, base)
        .filter(|t| table_stays(posed.rotaries) && height.is_finite() && height < t - Z_TOL)
        .map(|top_mm| Warning::FirstTraverseLow { at: 0, height_mm: height, top_mm, fixture: true, tilted: false });
    let top_of = |w: &Option<Warning>| match w {
        Some(Warning::FirstTraverseLow { top_mm, .. }) => *top_mm,
        _ => f64::NEG_INFINITY,
    };
    let w = if top_of(&table) > top_of(&worked) { table } else { worked };
    let Some(w) = w else { return };
    // #2991: both posed reads are along the frame's axis from its origin.
    // In the user's frame and `tilted`, as the first-traverse verdicts
    // that this one replaces are, so the tops compare in one frame.
    let w = frame.map_or(w, |f| super::first_traverse::in_user_frame(w, f));
    let Warning::FirstTraverseLow { top_mm, .. } = w else { return };
    warns.retain(|o| !matches!(o, Warning::FirstTraverseLow { top_mm: t, .. } if *t <= top_mm));
    warns.push(w);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixed_table_reads_its_top_in_the_paths_height_scale() {
        // The table top at Z0, the lines through (0, 0, 5): upright the
        // top is Z0; along a B 30 axis from the plane origin (0, 0, 20)
        // the lines stand 15 cos 30 under it, the table 5 under them.
        let table = [FixtureBox { min: Point3::new(-200.0, -200.0, -1e6), max: Point3::new(200.0, 200.0, 0.0) }];
        let names = [MACHINE_TABLE.to_string()];
        let turn = PosedTurn::About { origin: Point3::new(0.0, 0.0, 5.0) };
        let upright = table_top(&turn, &table, &names, [0.0, 0.0, 1.0], Point3::new(0.0, 0.0, 0.0));
        assert_eq!(upright, Some(0.0));
        let b = 30f64.to_radians();
        let tilted = table_top(&turn, &table, &names, [b.sin(), 0.0, b.cos()], Point3::new(0.0, 0.0, 20.0)).unwrap_or(f64::NAN);
        assert!((tilted - (-15.0 * b.cos() - 5.0)).abs() < 1e-12, "{tilted}");
        // A tagged box is no table.
        assert_eq!(table_top(&turn, &table, &["vise".to_string()], [0.0, 0.0, 1.0], Point3::new(0.0, 0.0, 0.0)), None);
    }
}
