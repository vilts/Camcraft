//! #2898: a Circular with several radial passes (rings) linked each
//! ring's floor to the next ring's start with one diagonal rapid - up
//! from the floor and inward at once - through the core the next ring
//! has still to cut. The verifier convicted it and held the post. These
//! pin the link between rings: every lateral rapid rides at or above the
//! stock top, the verifier finds nothing, and a spliced diagonal link of
//! the old shape is still convicted.
//!
//! Provenance: our own fixture (the issue's hole-work numbers: R10,
//! Ø6 cutter, 2 mm steps, Z20 to Z0 in a 90 x 90 x 20 block), specced
//! from the Prime Directive and the shop rule "never traverse through
//! uncut stock"; no vendor material.
//!
//! Gate map: covers camcraft_motion::machining2d (circular),
//! camcraft_sim::verify (rapids).

use std::f64::consts::PI;

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{CircularParams, circular};
use camcraft_motion::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

const C: Point2 = Point2 { x: 30.0, y: 30.0 };
const TOP: f64 = 20.0;
const CLEAR: f64 = 30.0;
const R_TOOL: f64 = 3.0;

fn params(entry: Entry3D, links: Links, vault: bool) -> CircularParams {
    CircularParams {
        pass_start: Default::default(),
        links,
        center: C,
        radius: 10.0,
        vault,
        bottom_z: 0.0,
        top_z: TOP,
        tool_diameter: 2.0 * R_TOOL,
        stock_to_leave: 0.0,
        stepover: 2.0,
        stepdown: 2.0,
        clearance_z: CLEAR,
        retract_z: None,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry,
        helix_radius: 1.0,
    }
}

fn disk(r: f64) -> Region {
    let pts: Vec<Point2> = (0..256)
        .map(|k| {
            let t = 2.0 * PI * f64::from(k) / 256.0;
            Point2::new(C.x + r * t.cos(), C.y + r * t.sin())
        })
        .collect();
    Region::new(vec![Loop::new(pts).unwrap()]).unwrap()
}

/// The 90 x 90 block about the hole.
fn block() -> Region {
    let sq = |x: f64, y: f64| Point2::new(C.x + x, C.y + y);
    Region::new(vec![Loop::new(vec![sq(-45.0, -45.0), sq(45.0, -45.0), sq(45.0, 45.0), sq(-45.0, 45.0)]).unwrap()]).unwrap()
}

fn lateral(m: &Move) -> bool {
    (m.to.x - m.from.x).hypot(m.to.y - m.from.y) > 1e-6
}

/// The rapids that move in XY under the stock top.
fn low_traverses(t: &Toolpath) -> Vec<usize> {
    t.moves
        .iter()
        .enumerate()
        .filter(|(_, m)| m.kind == MoveKind::Rapid && lateral(m) && m.from.z.min(m.to.z) < TOP - 1e-9)
        .map(|(i, _)| i)
        .collect()
}

fn judge(t: &Toolpath) -> Vec<Warning> {
    let region = disk(10.0);
    let stock = block();
    let tool = ToolGeom {
        radius: R_TOOL,
        nose_radius: 0.0,
        flute_len: 25.0,
        shank_radius: R_TOOL,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    };
    verify(
        t,
        &VerifyContext {
            region: &region,
            stock: Some(&stock),
            model: None,
            tool: &tool,
            cutter: None,
            mode: Mode::Area,
            stock_top_z: TOP,
            clearance_z: CLEAR,
            stock_to_leave: 0.0,
            gouge_tol: 0.01,
            z_tol: 1e-6,
            finish_tol: None,
            fixtures: &[],
            spindle: None,
            ..Default::default()
        },
    )
}

fn rapids_in_stock(w: &[Warning]) -> Vec<usize> {
    w.iter()
        .filter_map(|w| match *w {
            Warning::RapidInStock { at } => Some(at),
            _ => None,
        })
        .collect()
}

/// The issue's case and its neighbours: every entry, the planar and the
/// default links, flat and vault. Each ring after the first is reached
/// over the stock, and the verifier convicts no rapid.
#[test]
fn the_link_between_circular_rings_clears_the_stock() {
    for entry in [Entry3D::Plunge, Entry3D::Helix, Entry3D::Ramp] {
        for (tag, links) in [("planar", Links::PLANAR), ("default", Links::default())] {
            for vault in [false, true] {
                let t = circular(&disk(10.0), &params(entry, links, vault)).expect("a legal hole");
                let low = low_traverses(&t);
                assert!(low.is_empty(), "{entry:?}/{tag}/vault {vault}: rapids under the stock top at {low:?}: {:?}", low.first().map(|&i| &t.moves[i]));
                let convicted = rapids_in_stock(&judge(&t));
                assert!(convicted.is_empty(), "{entry:?}/{tag}/vault {vault}: verify convicts rapids {convicted:?}");
            }
        }
    }
}

/// The issue's plunge case cuts four rings (R 7, 5, 3, 1), so it has
/// three links between rings (onto R 5, 3 and 1; ring 0 enters at its
/// own dive point), each a climb to clearance before the traverse - not
/// a single ring that hides the law.
#[test]
fn the_issue_case_climbs_to_clearance_between_its_rings() {
    let t = circular(&disk(10.0), &params(Entry3D::Plunge, Links::PLANAR, false)).expect("a legal hole");
    let traverses: Vec<&Move> = t.moves.iter().filter(|m| m.kind == MoveKind::Rapid && lateral(m)).collect();
    let radii: Vec<f64> = traverses.iter().map(|m| ((m.to.x - C.x).hypot(m.to.y - C.y) * 1e6).round() / 1e6).collect();
    for rho in [5.0, 3.0, 1.0] {
        assert!(radii.contains(&rho), "a traverse onto ring {rho}: {radii:?}");
    }
    assert!(traverses.iter().all(|m| m.from.z >= CLEAR - 1e-9 && m.to.z >= CLEAR - 1e-9), "{traverses:?}");
}

/// The verifier still holds a link of the old shape: one diagonal rapid
/// from a ring's floor up to clearance over the next ring's start.
#[test]
fn a_spliced_diagonal_link_between_rings_is_still_convicted() {
    let t = circular(&disk(10.0), &params(Entry3D::Plunge, Links::PLANAR, false)).expect("a legal hole");
    // The first climb off the floor with a traverse after it (ring 0 to
    // ring 1): the two become the one diagonal rapid the old code posted.
    let lift = (0..t.moves.len().saturating_sub(1))
        .find(|&i| {
            let (m, n) = (&t.moves[i], &t.moves[i + 1]);
            m.kind == MoveKind::Rapid && !lateral(m) && m.from.z < 1e-9 && m.to.z >= CLEAR - 1e-9 && n.kind == MoveKind::Rapid && lateral(n)
        })
        .expect("a climb from the first ring's floor to the next ring");
    let (m, n) = (&t.moves[lift], &t.moves[lift + 1]);
    let diag = Move { from: m.from, to: n.to, ..*m };
    let mut bad = t.clone();
    bad.moves.splice(lift..lift + 2, [diag]);
    assert_eq!(rapids_in_stock(&judge(&t)), Vec::<usize>::new(), "the real path is clean");
    let convicted = rapids_in_stock(&judge(&bad));
    assert!(convicted.contains(&lift), "the diagonal link at move {lift} must be convicted: {convicted:?}");
}

/// #2898 + #3109: after merging helix descent tracking with the ring lift,
/// a flat multi-ring helix climbs to clearance with zero warnings (neither
/// `RapidInStock` nor `Crossing`), and an un-staggerable helix is still
/// refused by name.
#[test]
fn a_multi_ring_helix_keeps_both_the_clearance_lift_and_descent_tracking() {
    let t = circular(&disk(10.0), &params(Entry3D::Helix, Links::PLANAR, false)).expect("a legal helix hole");
    assert!(low_traverses(&t).is_empty(), "every inter-ring traverse must ride at clearance");
    let warns = judge(&t);
    assert!(warns.is_empty(), "multi-ring helix must verify clean (no RapidInStock or Crossing): {warns:?}");

    let mut tight = params(Entry3D::Helix, Links::PLANAR, false);
    tight.stepover = 3.0;
    tight.helix_radius = 3.0;
    let err = circular(&disk(10.0), &tight).expect_err("crossing helix orbits must still be refused");
    assert!(format!("{err:?}").contains("helix entry: the descent"), "unexpected refusal: {err:?}");
}

#[test]
fn a_forced_vault_helix_stays_inside_its_band_after_lattice_rounding() {
    use camcraft_motion::machining2d::PassStart;

    let mut p = params(Entry3D::Helix, Links::PLANAR, true);
    p.pass_start = PassStart::Inside;
    p.center.x += 0.49e-6;
    p.helix_radius = 0.7;
    let path = circular(&disk(10.0), &p).expect("the tangent helix fits the inner ring");
    let mut unequal = false;
    for m in path.moves.iter().take_while(|m| (m.from.z - m.to.z).abs() > 1e-9 || m.arc.is_none()) {
        let Some(arc) = m.arc else { continue };
        let r = |q: camcraft_motion::geometry::Point3| (q.x - arc.center.x).hypot(q.y - arc.center.y);
        let (a, b) = (r(m.from), r(m.to));
        unequal |= (a - b).abs() > 1e-7;
        let center_offset = (arc.center.x - p.center.x).hypot(arc.center.y - p.center.y);
        // The CL sweep interpolates the radius between the two endpoint
        // radii. The full swept circle stays inside this bound even if the
        // maximum lies between the ends.
        assert!(center_offset + a.max(b) <= 1.0 + 1e-9, "the entire first helix stays in the R1 band: {m:?}");
    }
    assert!(unequal, "the first descent uses a helix with unequal rounded endpoint radii");
}

#[test]
fn a_forced_vault_never_emits_a_sub_resolution_helix_arc() {
    use camcraft_motion::machining2d::PassStart;
    use camcraft_motion::ring_zig::MIN_ARC_CHORD;

    let mut p = params(Entry3D::Helix, Links::PLANAR, true);
    p.pass_start = PassStart::Inside;
    p.radius = 10.0002;
    p.stepover = 1.75;
    p.top_z = 0.001;
    p.stepdown = 0.002;
    p.links.approach.plunge_height = 0.0;
    p.helix_radius = 0.00015;
    match circular(&disk(10.01), &p) {
        Ok(path) => {
            assert!(!path.moves.is_empty(), "the legal ring ladders still cut");
            for m in path.moves.iter().filter(|m| m.arc.is_some()) {
                let chord = (m.to.x - m.from.x).hypot(m.to.y - m.from.y);
                assert!(chord >= MIN_ARC_CHORD, "this arc can round to one posted XY point: {m:?}");
            }
        }
        Err(err) => {
            let text = format!("{err:?}");
            assert!(text.contains("helix") || text.contains("vault ring"), "the refusal names the geometry: {text}");
        }
    }
}
