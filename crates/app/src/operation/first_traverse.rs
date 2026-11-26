//! #2901, #2965: generate's seam to the first-traverse laws (`verify::first_traverse_low`, `verify::first_traverse_low_swept`): which operations answer to them and which steel they read; our own reading of the shop rule "never traverse through uncut stock or steel", no vendor material consulted.

use crate::cl::{TiltedFrame, Toolpath};
use crate::fixture::{FixtureBox, MACHINE_TABLE};
use crate::geometry::Point3;
use crate::machine::AxisLimits;
use crate::verify::{self, Warning};

use super::OpParams;
use super::kinds::Section;
use super::material::Z_TOL;

/// What the first traverse must ride over: the start stock's top in the
/// path's frame, and the setup's steel in the world (`fixture::
/// named_boxes_for`: the tagged fixtures' boxes, then the machine table),
/// `names` beside `fixtures` in the same order. `rotaries` is the mounted
/// machine's rotary inventory (`None`: no machine is mounted).
pub(super) struct Over<'a> {
    pub stock_top: f64,
    pub fixtures: &'a [FixtureBox],
    pub names: &'a [String],
    pub rotaries: Option<&'a [AxisLimits]>,
    /// The Z stroke of the mounted machine, mm: the tool-change height
    /// over the table top (`None`: unstated, read as 0 - a start on the
    /// table top, the stricter premise).
    pub z_stroke: Option<f64>,
    /// #2965: the work's rotary for a rotary kind (`Mode::Rotary`): the
    /// law then reads the work swept about it. `None` for every other
    /// kind.
    pub turned: Option<Turned<'a>>,
    /// The operation's clearance height in the path's frame. The post
    /// rapids Z to the higher of it and the start before the X/Y
    /// traverse (`traverseZ`); the #2965 kinds read that height.
    pub clearance: f64,
}

/// What a rotary kind's first traverse is judged against (#2965): the
/// fitted axis line in the part frame, the start stock's and the part's
/// points, and the setup's steel split by whether it turns
/// (`fixture::workholding_for`).
#[derive(Clone, Copy)]
pub(super) struct Turned<'a> {
    pub axis: verify::RotaryAxis,
    pub rides: &'a [Point3],
    pub turning: &'a [FixtureBox],
    pub fixed: &'a [FixtureBox],
}

/// The points that ride the rotary: each vertex of the start stock and
/// of the part (the part can stand outside a stock that a user drew too
/// small). A box with no facets gives its eight corners.
pub(super) fn rides_of(stock: Option<&[crate::geometry::Triangle]>, aabb: Option<(Point3, Point3)>, part: &[crate::geometry::Triangle]) -> Vec<Point3> {
    let mut out: Vec<Point3> = stock.unwrap_or(&[]).iter().chain(part).flat_map(|t| [t.a, t.b, t.c]).collect();
    if stock.is_none_or(<[_]>::is_empty)
        && let Some((lo, hi)) = aabb
    {
        out.extend((0..8).map(|i| Point3::new([lo.x, hi.x][(i >> 2) & 1], [lo.y, hi.y][(i >> 1) & 1], [lo.z, hi.z][i & 1])));
    }
    out
}

/// The axis a rotary kind (`Mode::Rotary`) turns its work about, on a
/// machine that posts the mill order ([`mill_order`]): the job's (`fixture::rotary_axis_of`), else the same fit of
/// the same facets that the generator ran. `Ok(None)` for every other
/// kind. No axis: refused by name, as the law cannot judge the work.
pub(super) fn turned_axis(params: &OpParams, mill: bool, job: Option<verify::RotaryAxis>, facets: &[crate::geometry::Triangle]) -> Result<Option<verify::RotaryAxis>, String> {
    if !mill || params.kind().info().mode != verify::Mode::Rotary {
        return Ok(None);
    }
    job.or_else(|| crate::multiaxis::axis::fit(facets).ok().map(|a| verify::RotaryAxis { origin: a.origin, dir: [a.dir.x, a.dir.y, a.dir.z] }))
        .map(Some)
        .ok_or_else(|| format!("{}: no rotary axis can be fitted to the part, so its first traverse cannot be judged", params.kind().info().label))
}

/// The rotary inventory of `job.machine_rotaries` as [`Over::rotaries`].
pub(super) fn rotaries_of(m: Option<&(String, Vec<AxisLimits>)>) -> Option<&[AxisLimits]> {
    m.map(|(_, axes)| axes.as_slice())
}

// The 3+2 tilted-table reading (#2987, #3081) lives in its own module,
// split along the domain seam (#3288). Re-exported here so callers keep
// reading them through `first_traverse`.
pub(super) use super::first_traverse_table::{TABLE_UNJUDGED_NOTE, TableRead, table_read, tilting_all, work_tilts};

/// Whether the operation's first traverse comes from an unknown X/Y on
/// a machine that posts the mill order (`mill`, [`mill_order`]): the planar and 3-D milling kinds, the hole kinds (the cross
/// drill included, #2965) and the multi-axis kinds (#2965). The post
/// sends Z, then X/Y at that height, then the rotary words, for each of
/// them. Turning, plasma and EDM read their clearance on other surfaces
/// or axes; the rapid laws judge their moves (decision #2901). A lathe's
/// live-tool cross drill is not a mill's: decision #2965.
fn answers(params: &OpParams, mill: bool) -> bool {
    mill && matches!(params.kind().info().section, Section::Mill2D | Section::Mill3D | Section::Drilling | Section::MultiAxis)
}

/// #3083: whether the machine posts a milling tool's first traverse
/// in the mill order that [`answers`] reads (Z to the clearance, then
/// X/Y at that height, then the rotary words). A mill does. A mill-turn
/// does too: its default post for each kind is the mill post
/// (`posting::default_post_for`), which sends that order (`traverseZ`,
/// `xyFirst`). A lathe's insert, blade or gauge (`lathe_tool`) is no
/// milling tool; the turning laws judge its moves. The premise of the
/// laws stays the mill's: the tool starts at the tool-change height
/// (decision #3083).
///
/// NOT on a mill-turn: a rotary kind (`Mode::Rotary`). There it turns
/// the bar on the spindle C under a radial live tool, whose first rapid
/// comes from the turret's index position, not from an upright tool over
/// an unknown X/Y; the mill premise refuses every such program. That law
/// is #3077's live-tool reading, in `rotary_traverse` (#3096).
pub(super) fn mill_order(kind: crate::machine::Kind, lathe_tool: bool, params: &OpParams) -> bool {
    use crate::machine::Kind;
    !lathe_tool
        && match kind {
            Kind::Mill => true,
            Kind::MillTurn => params.kind().info().mode != verify::Mode::Rotary,
            _ => false,
        }
}

/// #3083: the mill kinds that a mill-turn brought under the law. Their
/// op stamp takes one more word on a mill-turn setup, so a cache judged
/// without the law reads Stale once; no mill's stamp moves.
pub(super) fn newly_judged_on(kind: crate::machine::Kind, params: &OpParams) -> bool {
    kind == crate::machine::Kind::MillTurn && answers(params, mill_order(kind, false, params))
}

/// The kinds that #2965 brought under the law: the multi-axis and
/// rotary kinds and the cross drill. Their op stamp takes one more
/// word, so a cache judged without the law reads Stale once.
pub(super) fn newly_judged(params: &OpParams) -> bool {
    params.kind().info().section == Section::MultiAxis || matches!(params, OpParams::Drill { cross, .. } if !cross.is_empty())
}

/// The rotary kinds' law (#2965): the path's first point and its tool
/// axis, carried to the world when the path is in a tilted frame, read
/// against the work swept about its axis.
fn judge_turned(path: &Toolpath, frame: Option<&TiltedFrame>, clearance: f64, t: &Turned<'_>) -> Option<Warning> {
    let first = path.moves.first()?;
    let (start, axis) = match frame {
        Some(f) => (f.to_world(first.from), f.axis),
        None => (first.from, first.axis.unwrap_or([0.0, 0.0, 1.0])),
    };
    let steel = verify::SweptSteel { turning: t.turning, fixed: t.fixed };
    // A tilted frame's clearance is a local height: carried to the world
    // it is no Z, so only the upright path's clearance lifts the start.
    let clearance = if frame.is_some() { f64::NEG_INFINITY } else { clearance };
    verify::first_traverse_low_swept(start, axis, clearance, &t.axis, t.rides, steel, Z_TOL)
}

/// Add the first-traverse verdict to `warns` when `path` (in `frame`'s
/// local heights when it is tilted) starts under the stock or any steel.
/// The note it returns says what a 3+2 plane could not judge.
///
/// #2991: the verdict names the heights in the frame the user sets them
/// in. Generate shifts each height by `h - origin.z` into the frame, so
/// the verdict adds `origin.z` back: a 3+2 plane at origin Z 20 with a
/// clearance of 30 reads 30, not the local 10. A frame that is not
/// upright marks the verdict `tilted`, and its text says that the heights
/// are measured along the tool axis.
///
/// A 3+2 plane reads the tagged fixtures along its tool axis (they ride
/// the table with the part). It reads the machine table's top face over
/// the stated travel when the head tilts (#2987), only where the move
/// along the tool axis from the tool-change height can carry the tool
/// under the table top (`table_read`). Past a 90-degree head tilt the axis
/// points down toward the table, and that case gets its own verdict that
/// names the plane to lower to (#3081). When only the work tilts, the table stays
/// under the work (`work_tilts`). A mounted machine that states no travel
/// gives no table box, and the note names that. Upright, the table's top
/// is under the stock and never the higher top.
///
/// A rotary kind (`over.turned`) also reads the work swept about its
/// axis (#2965, [`judge_turned`]): the rotary words come after the X/Y
/// traverse, so the work stands at an angle the program does not state.
/// The multi-axis kinds and the cross drill read the upright law on
/// their tip: a TCP post writes the tip in the part frame, so Z then
/// X/Y run on the part's own plane, and the tool leans up from the tip
/// in each pose that cuts from above (decision #2965).
pub(super) fn judge(path: &Toolpath, params: &OpParams, mill: bool, frame: Option<&TiltedFrame>, over: &Over<'_>, warns: &mut Vec<Warning>) -> Option<String> {
    if !answers(params, mill) {
        return None;
    }
    let tilting = |turns: bool| over.rotaries.is_some_and(|r| tilting_all(r, turns));
    // A rotary kind on a machine whose head alone tilts: the work never
    // turns, so only the tip law reads it. On one whose work alone
    // tilts, the post writes machine heights, so only the swept law.
    // An unstated machine, or a mixed one, answers to both.
    let swept = over.turned.as_ref().filter(|_| !tilting(false)).and_then(|t| judge_turned(path, frame, over.clearance, t));
    // The #2965 kinds' start, lifted to the height the post traverses
    // at. A rotary or multi-axis path opens at its own pose (a radial
    // clearance about the axis), which can stand under the plane.
    let lifted;
    let path = match path.moves.first() {
        Some(m) if newly_judged(params) && over.clearance.is_finite() && m.from.z < over.clearance => {
            let mut m = *m;
            m.from.z = over.clearance;
            lifted = Toolpath::new(vec![m]);
            &lifted
        }
        _ => path,
    };
    let tip_judged = over.turned.is_none() || !tilting(true);
    let mut note = None;
    let mut under = None;
    let w = match frame {
        _ if swept.is_some() => swept,
        _ if !tip_judged => None,
        Some(f) => {
            let work_turns = over.rotaries.is_some_and(work_tilts);
            let is_table = |i: usize| over.names.get(i).is_some_and(|n| n == MACHINE_TABLE);
            let start_h = path.start().map(|p| p.z).filter(|z| z.is_finite());
            let stroke = over.z_stroke.filter(|s| s.is_finite() && *s >= 0.0).unwrap_or(0.0);
            let mut steel: Vec<FixtureBox> = Vec::with_capacity(over.fixtures.len());
            for (i, b) in over.fixtures.iter().enumerate() {
                if !is_table(i) {
                    steel.push(*b);
                    continue;
                }
                match start_h.filter(|_| !work_turns).map(|h| (h, table_read(b, f, h, stroke))) {
                    Some((_, TableRead::Counts(face))) => steel.push(face),
                    Some((height_mm, TableRead::Under { limit })) => {
                        under = Some(in_user_frame(Warning::FirstTraverseUnderTable { at: 0, height_mm, limit_mm: limit }, f));
                    }
                    Some((_, TableRead::Clear)) | None => {}
                }
            }
            let has_table = (0..over.fixtures.len()).any(is_table);
            if over.rotaries.is_some() && !work_turns && !has_table {
                note = Some(format!("{TABLE_UNJUDGED_NOTE}: the machine does not state all of the X, Y and Z travel, so the first traverse is judged against the stock and the fixtures only"));
            }
            verify::first_traverse_low(path, over.stock_top, &steel, |p| f.z_local(p), Z_TOL).map(|w| in_user_frame(w, f))
        }
        None => verify::first_traverse_low(path, over.stock_top, over.fixtures, |p| p.z, Z_TOL),
    };
    // #3081: the table verdict of a down-pointing axis stands beside the
    // stock and fixture verdict; their repairs differ (lower, raise).
    for w in [w, under].into_iter().flatten() {
        if !warns.contains(&w) {
            warns.push(w);
        }
    }
    note
}

/// The law's local verdict in the user's heights frame (#2991): both
/// heights shifted back by the frame's `origin.z`, `tilted` set when the
/// frame is not upright. #3148: the table verdict of a down-pointing
/// axis shifts its plane and its limit the same way. Other warnings
/// pass unchanged.
pub(super) fn in_user_frame(w: Warning, f: &TiltedFrame) -> Warning {
    match w {
        Warning::FirstTraverseUnderTable { at, height_mm, limit_mm } => {
            Warning::FirstTraverseUnderTable { at, height_mm: height_mm + f.origin.z, limit_mm: limit_mm + f.origin.z }
        }
        Warning::FirstTraverseLow { at, height_mm, top_mm, fixture, .. } => Warning::FirstTraverseLow {
            at,
            height_mm: height_mm + f.origin.z,
            top_mm: top_mm + f.origin.z,
            fixture,
            tilted: !f.is_upright(),
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::Kind;
    use crate::operation::Height;

    fn face() -> OpParams {
        OpParams::Face {
            holes: Default::default(),
            tilt: None,
            blend_radius: None,
            blend_turns: false,
            stepover: 3.0,
            clearance_z: Height::fixed(30.0),
            cut_z: Height::fixed(9.0),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
            picks: Default::default(),
        }
    }

    fn rotary_contour() -> OpParams {
        OpParams::RotaryContour {
            tolerance: 0.05,
            clearance_z: Height::fixed(25.0),
            retract_z: None,
            top_z: Height::fixed(6.0),
            bottom_z: Height::fixed(-8.0),
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            lead_deg: 0.0,
            lag_deg: 0.0,
            max_tilt_deg: 60.0,
            stated_axis: None,
            picks: Default::default(),
        }
    }

    /// #3083: a mill-turn posts a milling tool's first traverse in the
    /// mill order, so its mill kinds answer as a mill's do; a lathe tool,
    /// a lathe and the radial rotary kinds of a mill-turn do not.
    #[test]
    fn a_mill_turn_judges_its_milling_kinds_in_the_mill_order() {
        let (f, r) = (face(), rotary_contour());
        assert!(mill_order(Kind::Mill, false, &f) && mill_order(Kind::Mill, false, &r));
        assert!(mill_order(Kind::MillTurn, false, &f));
        assert!(!mill_order(Kind::MillTurn, true, &f), "a lathe tool");
        assert!(!mill_order(Kind::MillTurn, false, &r), "the radial live tool of a mill-turn's C");
        assert!(!mill_order(Kind::Lathe, false, &f));
        assert!(newly_judged_on(Kind::MillTurn, &f));
        assert!(!newly_judged_on(Kind::Mill, &f), "no mill's stamp moves");
        assert!(!newly_judged_on(Kind::MillTurn, &r), "an unjudged kind's stamp stays");
    }
}
