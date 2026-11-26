//! #2301: a bore-side contour (`Side::Inside`) cuts only its ring, never
//! the core inside it. A substituted axial dive on a later level that
//! lands off the cut path (along the first pass, a wall margin inward)
//! has its footprint over the uncut core, which stands at the top of the
//! material - not at the plane above. The rapid down to that dive must
//! stop the plunge height over the material's top there, never inside
//! the core (verify's RapidInStock).
//!
//! Gate map (#2301): covers camcraft_motion::machining2d (contour_bore,
//! descent_dive), camcraft_motion::verify.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::{Link, Links};
use camcraft_motion::machining2d::{ContourParams, Lead, Side, contour};
use camcraft_motion::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

fn round(r: f64) -> Region {
    let pts = (0..256)
        .map(|k| {
            let t = std::f64::consts::TAU * f64::from(k) / 256.0;
            Point2::new(r * t.cos(), r * t.sin())
        })
        .collect();
    Region::new(vec![Loop::new(pts).unwrap()]).unwrap()
}

fn bore(entry: Entry3D, stepdown: f64, links: Links) -> ContourParams {
    ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 25.0,
        retract_z: 15.0,
        top_z: 0.0,
        bottom_z: -6.0,
        stepdown,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Inside,
        lead: Lead::Linear { length: 2.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry,
        helix_radius: 1.5,
        blend_turns: false, blend_radius: None,
        links,
        picks: Default::default(),
    }
}

fn check(t: &Toolpath, region: &Region, stock_top_z: f64) -> Vec<Warning> {
    let tool = ToolGeom { radius: 3.0, nose_radius: 0.0, flute_len: 30.0, shank_radius: 3.0, shank_len: 40.0, holder_radius: 20.0, holder_len: 60.0 };
    let cx = VerifyContext {
        region,
        stock: None,
        model: None,
        tool: &tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z,
        clearance_z: 25.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(t, &cx)
}

/// The issue's case: carried stock 5 mm over Top Z (#1559), a linear
/// lead and a Ramp entry whose chord is blocked on the later levels, for
/// the planar and the retract level link.
#[test]
fn the_bore_dive_under_carried_stock_starts_over_the_core() {
    const CARRIED: f64 = 5.0;
    let hole = round(10.0);
    let mut bad = Vec::new();
    for (ln, links) in [("planar", Links::PLANAR), ("retract", Links { level: Link::Retract, ..Links::PLANAR })] {
        let mut links = links;
        links.approach.material_top = Some(CARRIED);
        links.approach.stock_top = Some(CARRIED);
        let t = contour(&hole, &bore(Entry3D::Ramp, 2.0, links)).expect("bore contour");
        assert!(t.notes.iter().any(|n| n.contains("axial dive")), "{ln}: the case takes no substituted dive: {:?}", t.notes);
        let w = check(&t, &hole, CARRIED);
        if !w.is_empty() {
            bad.push(format!("{ln}: verify {w:?} notes {:?}", t.notes));
        }
        let first = t.moves.iter().find(|m| m.kind == MoveKind::Feed && (m.to.z - m.from.z).abs() <= 1e-9).map(|m| m.to.z);
        if first != Some(0.0) {
            bad.push(format!("{ln}: first level {first:?}, not Top Z 0"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// The other half of the law: once the second level's plunge opened the
/// column at the dive point, the later levels' dives start the plunge
/// height over the plane above again - no feed through the air the first
/// dive left. The plunges start at Z 7 (carried top 5 + plunge height 2),
/// then Z 0 and Z -2.
#[test]
fn the_later_bore_dives_start_over_the_plane_above() {
    let hole = round(10.0);
    let mut links = Links::PLANAR;
    links.approach.material_top = Some(5.0);
    links.approach.stock_top = Some(5.0);
    let h = links.approach.plunge_height;
    let t = contour(&hole, &bore(Entry3D::Ramp, 2.0, links)).expect("bore contour");
    let starts: Vec<f64> = t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).map(|m| m.from.z).collect();
    assert_eq!(starts, vec![5.0 + h, -2.0 + h, -4.0 + h], "plunge starts; notes {:?}", t.notes);
}
