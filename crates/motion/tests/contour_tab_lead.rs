//! An outside 2D Contour with the default linear lead on a plate with
//! round corners (#3019): the ring's seam falls on a fitted corner arc,
//! whose end tangent is a hair off the ring's first chord. The lead-in
//! and the lead-out must leave the seam on the air side of the ring, so
//! no lead crosses a feed move of its own level, with tabs or without.
//!
//! Gate map (M4.4): covers camcraft_motion::machining2d (contour,
//! lead_fit), camcraft_motion::verify.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{contour, ContourParams, Lead, Side};
use camcraft_motion::verify::{crossed_pairs, verify, Mode, ToolGeom, VerifyContext, Warning};

/// The 120 x 80 plate of the issue, with four outside R6 corners, each
/// corner a 16-chord polyline.
fn plate() -> Region {
    let (w, h, r) = (120.0, 80.0, 6.0);
    let mut pts = Vec::new();
    let corners = [(w - r, r, -90.0), (w - r, h - r, 0.0), (r, h - r, 90.0), (r, r, 180.0)];
    for (cx, cy, a0) in corners {
        for k in 0..=16 {
            let a = f64::to_radians(a0 + 90.0 * f64::from(k) / 16.0);
            pts.push(Point2::new(cx + r * a.cos(), cy + r * a.sin()));
        }
    }
    Region::new(vec![Loop::new(pts).unwrap()]).unwrap()
}

/// The issue's operation: a 10 mm flat end mill, 3 mm stepdown to 12 mm
/// deep, 4 tabs 3 wide and 1 high, the default 8 mm linear lead.
fn params(tab_count: u32, climb: bool) -> ContourParams {
    ContourParams {
        tool_diameter: 10.0,
        stock_to_leave: 0.0,
        clearance_z: 15.0,
        retract_z: 5.0,
        top_z: 0.0,
        bottom_z: -12.0,
        stepdown: 3.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb,
        side: Side::Outside,
        lead: Lead::Linear { length: 8.0 },
        tolerance: 0.01,
        tab_count,
        tab_width: 3.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        blend_radius: None,
        links: Links::PLANAR,
        picks: Default::default(),
    }
}

/// The moves of each crossing pair, for the failure message.
fn dump(t: &Toolpath, pairs: &[[usize; 2]]) -> String {
    let mut s = String::new();
    for i in pairs.iter().flatten() {
        let m = &t.moves[*i];
        s += &format!(
            "\n#{i} {:?} ({:.3}, {:.3}, {:.3}) -> ({:.3}, {:.3}, {:.3})",
            m.kind, m.from.x, m.from.y, m.from.z, m.to.x, m.to.y, m.to.z
        );
    }
    s
}

/// The path of `p` on the plate has no XY feed crossing, and the verifier
/// finds no crossing and no gouge.
fn assert_clean(p: &ContourParams, what: &str) -> Toolpath {
    let region = plate();
    let t = contour(&region, p).unwrap();
    let pairs = crossed_pairs(&t, 0.01);
    assert!(pairs.is_empty(), "{what}: feed crossings {pairs:?}:{}\nnotes {:?}", dump(&t, &pairs), t.notes);
    let tool = ToolGeom { radius: 5.0, nose_radius: 0.0, flute_len: 30.0, shank_radius: 5.0, shank_len: 10.0, holder_radius: 20.0, holder_len: 30.0 };
    let cx = VerifyContext { region: &region, tool: &tool, mode: Mode::Boundary, stock_top_z: p.top_z, clearance_z: p.clearance_z, gouge_tol: 0.01, z_tol: 0.05, ..VerifyContext::default() };
    let bad: Vec<Warning> = verify(&t, &cx).into_iter().filter(|w| matches!(w, Warning::Crossing { .. } | Warning::Gouge { .. })).collect();
    assert!(bad.is_empty(), "{what}: expected no Crossing/Gouge warnings, got {bad:?}");
    t
}

/// The linear lead stays a lead: one 8 mm straight feed onto the ring and
/// one off it on the first level.
fn assert_full_leads(t: &Toolpath, what: &str) {
    let z = t.moves.iter().find(|m| m.kind == MoveKind::Feed).map(|m| m.to.z).unwrap();
    let long: Vec<_> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.arc.is_none() && m.from.z == z && m.to.z == z)
        .map(|m| (m.to.x - m.from.x).hypot(m.to.y - m.from.y))
        .filter(|l| (l - 8.0).abs() < 1e-6)
        .collect();
    assert_eq!(long.len(), 2, "{what}: an 8 mm lead-in and lead-out on the first level: {:?}", t.notes);
}

#[test]
fn an_outside_tabbed_contour_with_a_linear_lead_has_no_crossing() {
    let t = assert_clean(&params(4, true), "4 tabs, climb");
    assert_full_leads(&t, "4 tabs, climb");
}

/// Climb pins the lead-out (it crossed the ring's first move); the
/// conventional cut pins the lead-in (it crossed the ring's last move).
#[test]
fn an_outside_contour_with_a_linear_lead_has_no_crossing_with_or_without_tabs() {
    for (tabs, climb) in [(0, true), (0, false), (4, false)] {
        let what = format!("{tabs} tabs, climb {climb}");
        let t = assert_clean(&params(tabs, climb), &what);
        assert_full_leads(&t, &what);
    }
}
