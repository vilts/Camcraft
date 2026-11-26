//! What the Boundary settings DO, in plain words (the user's hand-test,
//! 2026-11-06: "unclear what open boundary affects .. unclear what
//! boundary actually does"): the summary at the top of the Boundary
//! group, computed from the draft - which area is machined, where the
//! tool may stand, how far the area reaches - and the notices for
//! settings that have no effect on this kind (said, never silently
//! ignored). Egui-free: the page draws it, the tests read it.
//!
//! The sentences follow the area law in `camcraft_app::operation`
//! (`boundary.rs`): the open boundary picks the side, the offset grows
//! or shrinks, the extension reaches past exterior edges, includes and
//! excludes add and cut, and containment is an offset relative to what
//! the kind does natively with its region.
//!
//! Provenance: our own wording of our own area law; no vendor material.

use camcraft_app::geo2d::Point2;
use camcraft_app::model;
use camcraft_app::operation::{
    BoundaryMode, BoundaryShape, ChainSide, Containment, CornerStyle, OpParams, OpType, PickShape, SrcRef,
};
use camcraft_app::machining2d::Side;
use camcraft_app::units;

use crate::boundary_draft::{self, Role};
use crate::state::State;

/// The group's plain-language account.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    /// What the settings do, one sentence each, most important first.
    pub lines: Vec<String>,
    /// Settings this kind ignores or refuses, each with why.
    pub notices: Vec<String>,
}

/// The kind's family, as the area law groups it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    Face,
    Contour,
    Clearing,
    /// The 3D finishing family: the whole-model path clipped to the area.
    Finish,
    /// The outline rides, Trace and Engrave (M13.10): tool centre ON
    /// the chain, so containment moves the ride by the whole side.
    Ride,
}

fn family(kind: OpType) -> Family {
    match kind {
        OpType::Face => Family::Face,
        OpType::Contour => Family::Contour,
        OpType::Finish3D => Family::Finish,
        OpType::Trace | OpType::Engrave => Family::Ride,
        _ => Family::Clearing,
    }
}

/// The compass word of a machined-side direction in world XY: the
/// dominant axis, or both when the side runs diagonal.
#[must_use]
pub fn side_word(d: Point2) -> Option<&'static str> {
    let (x, y) = (d.x, d.y);
    if !(x.is_finite() && y.is_finite()) || x.hypot(y) < 1e-9 {
        return None;
    }
    let (ax, ay) = (x.abs(), y.abs());
    Some(if ay > 2.0 * ax {
        if y > 0.0 { "+Y" } else { "−Y" }
    } else if ax > 2.0 * ay {
        if x > 0.0 { "+X" } else { "−X" }
    } else {
        match (x > 0.0, y > 0.0) {
            (true, true) => "+X +Y",
            (true, false) => "+X −Y",
            (false, true) => "−X +Y",
            (false, false) => "−X −Y",
        }
    })
}

/// Which way the open boundary's machined side lies, read off the
/// draft's preview area - the arrows' own law, so words and arrows agree.
#[must_use]
pub fn open_side_word(state: &State) -> Option<&'static str> {
    let region = state.boundary_view.preview.as_ref()?.region.as_ref()?;
    let chain = boundary_draft::entries(state).into_iter().find_map(|e| match (e.role.is_open(), e.shape) {
        (true, Some(PickShape::Open { chain, .. })) => Some(chain),
        _ => None,
    })?;
    let arrows = crate::boundary_viz::side_arrows(&chain, region);
    let (sx, sy) = arrows.iter().fold((0.0, 0.0), |(x, y), (_, d)| (x + d.x, y + d.y));
    side_word(Point2::new(sx, sy))
}

fn what(state: &State, fam: Family) -> String {
    match (state.op_ui.src(), fam) {
        (SrcRef::Stock, Family::Face) => "the stock top".to_owned(),
        (SrcRef::Stock, _) => "the stock".to_owned(),
        (SrcRef::Entity(n), _) => format!("'{n}'"),
        (SrcRef::Selection(_), _) => "the selected area".to_owned(),
    }
}

fn verb(fam: Family) -> &'static str {
    match fam {
        Family::Face => "Faces",
        Family::Contour => "Contours",
        Family::Clearing => "Clears",
        Family::Finish => "Finishes",
        Family::Ride => "Rides",
    }
}

fn tool_radius(state: &State) -> Option<f64> {
    model::find_by_name(&state.world, &state.op_ui.tool)
        .and_then(|t| model::tool_of(&state.world, &t))
        .map(|t| t.diameter * 0.5)
        .filter(|r| r.is_finite() && *r > 0.0)
}

/// Whether the kind's generator natively keeps the whole tool inside its
/// region (so Inside is the identity) - `None` when a contour's Auto side
/// cannot be read yet.
fn native_inside(state: &State, fam: Family) -> Option<bool> {
    match (&state.op_ui.params, fam) {
        (OpParams::Contour { side: Side::Inside, .. }, _) => Some(true),
        (OpParams::Contour { side: Side::Outside, .. }, _) => Some(false),
        (OpParams::Contour { .. }, _) => {
            let setup = model::find_by_name(&state.world, &state.op_ui.name)
                .and_then(|e| model::operation_of(&state.world, &e))
                .and_then(|o| o.setup);
            camcraft_app::operation::contour_auto_side(&state.world, setup.as_deref(), &state.op_ui.src(), &state.op_ui.params)
        }
        // The finish's rings reach a radius past the silhouette.
        (_, Family::Finish) => Some(false),
        // The ride's centre is ON the chain: neither side is its own,
        // so no containment is the identity and no notice says so.
        (_, Family::Ride) => None,
        _ => Some(true),
    }
}

/// The summary of the open operation's Boundary draft.
#[must_use]
pub fn summarize(state: &State) -> Summary {
    let kind = state.op_ui.params.kind();
    let fam = family(kind);
    let a = &state.op_ui.area;
    let u = state.units();
    let mut out = Summary::default();
    let what = what(state, fam);
    let len = |v: f64| format!("{} {}", units::len(v, u, 3), u.suffix());
    let signed = |v: f64| format!("{} {}", units::len_signed(v, u, 3), u.suffix());

    // 1. The area: the open boundary's side, or the selection's area with
    //    its includes and excludes.
    let includes = a.extra.iter().filter(|b| b.mode == BoundaryMode::Include).count();
    let excludes = a.extra.len() - includes;
    match &a.open {
        Some(o) if fam == Family::Contour => {
            let side = if o.side == ChainSide::Inside { "side A, the side holding more of" } else { "side B, the side holding less of" };
            let ext = if a.extension > 0.0 { format!(", run on {} past both ends", len(a.extension)) } else { String::new() };
            out.lines.push(format!(
                "Contours along the picked open edge as one open pass on {side} {what}: the tool rides beside the edge{ext}, entering and leaving past its ends."
            ));
        }
        Some(o) => {
            let ab = if o.side == ChainSide::Inside { "side A" } else { "side B" };
            let side = match open_side_word(state) {
                Some(w) => format!("the {w} side ({ab})"),
                None if o.side == ChainSide::Inside => format!("{ab}, the side holding more of {what},"),
                None => format!("{ab}, the side holding less of {what},"),
            };
            let chain = boundary_draft::entries(state)
                .iter()
                .any(|e| e.role.is_open() && matches!(e.shape, Some(PickShape::Open { .. })));
            let edge = if chain { "the picked open edge" } else { "the picked loop" };
            out.lines.push(match fam {
                Family::Face => format!(
                    "{} {what} on {side} of {edge}, up to that edge. Rows run parallel to the edge, starting from the far side and working in toward it.",
                    verb(fam)
                ),
                _ => format!(
                    "{} {what} on {side} of {edge}, up to that edge; the outer passes start in the air past the stock.",
                    verb(fam)
                ),
            });
        }
        None => {
            let mut s = format!("{} {what}", verb(fam));
            if includes > 0 {
                s.push_str(&format!(" plus {includes} included boundar{}", if includes == 1 { "y" } else { "ies" }));
            }
            if excludes > 0 {
                s.push_str(&format!(", keeping the whole tool out of {excludes} excluded boundar{}", if excludes == 1 { "y" } else { "ies" }));
            }
            s.push('.');
            out.lines.push(s);
        }
    }

    // 2. Where the tool stands, with the offset folded in.
    let single_loop = a.open.is_none()
        && a.extra.len() == 1
        && a.extra.iter().all(|b| b.mode == BoundaryMode::Include && matches!(b.shape, BoundaryShape::Picks(_)));
    let noun = if single_loop { "the picked loop" } else { "the boundary" };
    let off = if a.offset != 0.0 { format!(", offset {}", signed(a.offset)) } else { String::new() };
    let r = tool_radius(state).map_or_else(|| "a tool radius".to_owned(), |r| format!("one tool radius, {}", len(r)));
    match a.containment {
        Some(Containment::Inside) => {
            out.lines.push(format!("Tool stays inside {noun}{off}: its edge touches it, its centre runs {r} in."));
            if native_inside(state, fam) == Some(true) && fam != Family::Contour {
                out.notices.push(format!(
                    "{kind} already keeps the whole tool inside its area, so Inside changes nothing here - pick On or Outside to reach further."
                ));
            }
        }
        Some(Containment::On) => out.lines.push(format!("Tool centre rides on {noun}{off}: half the tool passes over it.")),
        Some(Containment::Outside) => out.lines.push(format!(
            "Tool passes {noun}{off} until its edge touches it from outside: its centre runs {r} out, so the edge itself is cut clean."
        )),
        None => {
            if a.offset != 0.0 {
                let how = if a.offset > 0.0 { "grows outward" } else { "shrinks inward" };
                let round = if a.open.is_some() && a.outer_offset != a.offset { " at the picked edge" } else { ", all round" };
                out.lines.push(format!("The area {how} by {}{round}.", len(a.offset.abs())));
            }
            out.lines.push(match fam {
                Family::Face => "Tool as Face cuts: the rows clear the area's whole bounding band, each run overrunning both ends.".to_owned(),
                Family::Contour if a.open.is_some() => format!(
                    "Tool edge on the picked edge: its centre runs {r} off it, plus the stock to leave."
                ),
                Family::Contour => "Tool as Contour cuts: it rides the ring on its Side (Passes tab).".to_owned(),
                Family::Clearing => format!("Tool as {kind} cuts: the whole tool stays inside the area."),
                Family::Finish => format!(
                    "Tool as {kind} cuts: the whole-model path is clipped to the area, the centre reaching {r} past its edge; it leaves and re-enters through the clearance plane."
                ),
                Family::Ride => format!(
                    "Tool as {kind} cuts: its centre rides the outline itself; Inside or Outside moves the ride {r} in or out, the offset by its own amount."
                ),
            });
        }
    }
    if a.containment == Some(Containment::Outside) && fam == Family::Contour && a.open.is_none() && native_inside(state, fam) == Some(false) {
        out.notices.push("This contour already rides its ring from outside, so Outside changes nothing here.".to_owned());
    }
    if a.containment == Some(Containment::Inside) && fam == Family::Contour && a.open.is_none() && native_inside(state, fam) == Some(true) {
        out.notices.push("This contour already rides its ring from inside, so Inside changes nothing here.".to_owned());
    }

    // 3. How far the area reaches past its exterior edges - or, for the
    //    open pass, how far it runs past the chain's ends.
    if a.extension > 0.0 {
        let open = if a.open.is_some() { ", never across the picked open boundary" } else { "" };
        out.lines.push(match fam {
            Family::Contour if a.open.is_some() => format!(
                "The pass runs {} past each end of the picked edge's chain, straight on along it.",
                len(a.extension)
            ),
            Family::Face => format!("Runs out to the stock outline and {} past it over the exterior edges{open}.", len(a.extension)),
            _ => format!("Extends {} past the exterior edges - those with no part material beyond them{open}.", len(a.extension)),
        });
    }

    // 4. What this kind ignores.
    if a.outer_offset != 0.0 {
        if a.open.is_some() && fam != Family::Contour {
            let how = if a.outer_offset > 0.0 { "out" } else { "in" };
            out.lines.push(format!("The outer side, toward the stock, moves {how} by {}; the picked edge stays.", len(a.outer_offset.abs())));
        } else {
            out.notices.push("The outside boundary offset has no effect here: only an open boundary has an outer side.".to_owned());
        }
    }
    if a.corner == CornerStyle::Sharp && a.offset == 0.0 && a.outer_offset == 0.0 {
        out.notices.push("The corner style has no effect while the offset is 0.".to_owned());
    }
    if fam == Family::Face && !a.extra.is_empty() {
        out.notices.push(
            "Face clears the whole bounding band of its area: an include only widens that band, and an exclude the band crosses is refused - use a Pocket to follow a shaped area.".to_owned(),
        );
    }
    if state.op_ui.params.tilt().is_some() && !a.is_default() {
        out.notices.push(
            "On a 3+2 tilted plane every boundary, offset and pick is read in the tilted plane - the index's own XY.".to_owned(),
        );
    }
    out
}

/// The caption under the open boundary's entry: which side its role
/// machines, in the viewport's words, and how to flip it.
#[must_use]
pub fn side_caption(state: &State, role: Role) -> String {
    let ab = if role == Role::OpenB { "Side B" } else { "Side A" };
    let rule = if role == Role::OpenB {
        "the side holding less of the operation's own area"
    } else {
        "the side holding more of the operation's own area"
    };
    match open_side_word(state) {
        Some(w) => format!("{ab} machines the {w} side of the edge ({rule}) - the arrows point into it."),
        None => format!("{ab} machines {rule} - the arrows point into it."),
    }
}
