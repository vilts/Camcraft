//! The 4-axis wire cut's laws (#537): a square-bottom / round-top punch
//! pairs and holds its kerf out of the ruled surface at every height;
//! arc-length and sync-point pairing differ as stated; a constant taper
//! through the sync-point path is #536's tapered cut; crossed, folded,
//! unpaired and off-profile pairings, profiles of different kinds, a
//! ruling past the maximum taper and a twist past the tolerance are
//! refused by name; degenerate input errors, never panics.
//!
//! The wire, gap and machine below are SYNTHETIC (#290 D4, D10).

use super::ring::pt_ring;
use super::*;

const K: f64 = 0.165;
const H: f64 = 12.0;

fn params() -> WireParams {
    WireParams {
        wire_diameter: 0.25,
        gap: 0.04,
        condition: "C110".into(),
        side: WireSide::Punch,
        start: Some(Point2::new(-30.0, 0.0)),
        plane_z: 0.0,
        tabs: None,
        stock: None,
        clearance: 1.0,
    }
}

fn ruled(pairing: WirePairing) -> WireRuled {
    WireRuled { top_z: H, pairing, tolerance: 0.05, max_deg: Some(30.0), machine: "M".into() }
}

fn rect(hx: f64, hy: f64) -> Loop {
    Loop::new(vec![Point2::new(-hx, -hy), Point2::new(hx, -hy), Point2::new(hx, hy), Point2::new(-hx, hy)]).unwrap()
}

fn round(r: f64, n: u32) -> Loop {
    Loop::new((0..n).map(|i| (f64::from(i) * std::f64::consts::TAU / f64::from(n)).sin_cos()).map(|(s, c)| Point2::new(r * c, r * s)).collect())
        .unwrap()
}

fn cut(upper: &Loop, lower: &Loop, r: &WireRuled) -> Result<Toolpath, GenError> {
    wire_contour_ruled(
        WireProfile { loops: std::slice::from_ref(upper), chains: &[] },
        WireProfile { loops: std::slice::from_ref(lower), chains: &[] },
        &params(),
        r,
    )
}

fn refusal(upper: &Loop, lower: &Loop, r: &WireRuled) -> String {
    cut(upper, lower, r).expect_err("refused").to_string()
}

/// Each cutting move's ruling: its lower point and the upper point its
/// pose reaches on the top plane (the leads left out).
fn rulings(path: &Toolpath) -> Vec<(Point2, Point2)> {
    path.moves[..path.moves.len() - 1]
        .iter()
        .map(|m| {
            let a = m.axis.expect("every 4-axis move states its pose");
            (Point2::new(m.to.x, m.to.y), Point2::new(m.to.x + a[0] / a[2] * H, m.to.y + a[1] / a[2] * H))
        })
        .collect()
}

type P2 = (f64, f64);

fn sync(pairs: &[(P2, P2)]) -> WirePairing {
    WirePairing::SyncPoints(
        pairs.iter().map(|(u, l)| WireSync { upper: Some(Point2::new(u.0, u.1)), lower: Some(Point2::new(l.0, l.1)) }).collect(),
    )
}

/// Corner to corner, upper `hu` to lower `hl` half-sizes.
fn corners(hu: (f64, f64), hl: (f64, f64)) -> WirePairing {
    sync(&[(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(x, y)| ((x * hu.0, y * hu.1), (x * hl.0, y * hl.1))))
}

/// Done means: a square-bottom / round-top punch pairs by arc length;
/// every move states its pose, the lower ends trace the square's
/// parallel, the upper ends the round's, and the wire stands off both
/// by the kerf offset plus what the twist needs - never less.
#[test]
fn a_square_to_round_punch_pairs_and_keeps_the_kerf_out() {
    let (sq, rd) = (rect(10.0, 10.0), round(8.0, 64));
    let path = cut(&rd, &sq, &ruled(WirePairing::ArcLength)).expect("a square-to-round punch generates");
    let r = rulings(&path);
    assert!(r.len() >= 64, "a ruling at every vertex of either profile: {}", r.len());
    let (near_l, near_u) = r.iter().fold((f64::INFINITY, f64::INFINITY), |(a, b), (l, u)| {
        (a.min(pt_ring(*l, sq.points())), b.min(pt_ring(*u, rd.points())))
    });
    assert!(near_l >= K && near_u >= K - 1e-9, "the wire never stands nearer than the kerf offset: {near_l} {near_u}");
    assert!(near_l - K <= 0.05 + 1e-9 && near_l - K > 1e-4, "the twist's widening is within the tolerance and not nothing: {}", near_l - K);
    // Upright at the start hole, both ends.
    assert_eq!(path.moves.last().and_then(|m| m.axis), Some([0.0, 0.0, 1.0]));
    let said = path.notes.join("\n");
    assert!(said.contains("4-axis") && said.contains("by normalised arc length") && said.contains("stands on the profile planes"), "{said}");
    // A tolerance of nothing refuses the same cut by name.
    let why = refusal(&rd, &sq, &WireRuled { tolerance: 0.0, ..ruled(WirePairing::ArcLength) });
    assert!(why.contains("twists the surface too far for a straight wire") && why.contains("tolerance is 0.0000 mm"), "{why}");
}

/// Done means: arc-length and sync-point pairing differ as stated - by
/// sync pairs at the corners every corner's ruling runs corner to
/// corner; by arc length from the start hole it does not.
#[test]
fn arc_length_and_sync_points_pair_differently_as_stated() {
    let (up, lo) = (rect(20.0, 10.0), rect(22.0, 11.0));
    let by_sync = rulings(&cut(&up, &lo, &ruled(corners((20.0, 10.0), (22.0, 11.0)))).expect("sync"));
    let by_arc = rulings(&cut(&up, &lo, &ruled(WirePairing::ArcLength)).expect("arc"));
    // How far each lower corner's ruling ends from the upper corner
    // (the corners are the rings' extreme points).
    let miss = |r: &[(Point2, Point2)]| {
        let far = |f: fn(&(Point2, Point2)) -> f64| r.iter().map(f).fold(0.0, f64::max);
        let (lx, ly) = (far(|q| q.0.x.abs()), far(|q| q.0.y.abs()));
        let (ux, uy) = (far(|q| q.1.x.abs()), far(|q| q.1.y.abs()));
        let at: Vec<f64> = r
            .iter()
            .filter(|(l, _)| (l.x.abs() - lx).abs() < 1e-9 && (l.y.abs() - ly).abs() < 1e-9)
            .map(|(_, u)| (u.x.abs() - ux).hypot(u.y.abs() - uy))
            .collect();
        assert!(at.len() >= 4, "four lower corners: {at:?}");
        at
    };
    assert!(miss(&by_sync).iter().all(|d| *d < 1e-9), "sync: corner to corner: {:?}", miss(&by_sync));
    assert!(miss(&by_arc).iter().all(|d| *d > 1e-3), "arc length: the corners do not meet: {:?}", miss(&by_arc));
    // Both start from the start hole's nearest point, upper over lower.
    assert!((by_arc[0].0.y).abs() < 1e-9 && (by_arc[0].1.y).abs() < 1e-9, "{:?}", by_arc[0]);
}

/// Done means: a constant taper stated as two profiles with sync pairs
/// at the corners is #536's tapered cut, move for move.
#[test]
fn a_constant_taper_through_the_ruled_path_is_the_tapered_cut() {
    let d = H * 3.0f64.to_radians().tan();
    let (up, lo) = (rect(20.0, 10.0), rect(20.0 + d, 10.0 + d));
    let four = cut(&up, &lo, &ruled(corners((20.0, 10.0), (20.0 + d, 10.0 + d)))).expect("ruled");
    let t = WireTaper { angle_deg: 3.0, top_z: H, max_deg: Some(30.0), machine: "M".into(), corner: WireCorner::Mitre };
    let taper = wire_contour_tapered(std::slice::from_ref(&up), &[], &params(), &t).expect("tapered");
    assert_eq!(four.moves.len(), taper.moves.len());
    for (a, b) in four.moves.iter().zip(&taper.moves) {
        assert!((a.to.x - b.to.x).abs() < 1e-6 && (a.to.y - b.to.y).abs() < 1e-6, "{:?} vs {:?}", a.to, b.to);
        let (p, q) = (a.axis.expect("posed"), b.axis.expect("posed"));
        assert!(p.iter().zip(&q).all(|(x, y)| (x - y).abs() < 1e-7), "{p:?} vs {q:?}");
    }
}

/// Done means: a deliberately crossed sync point is refused by name.
#[test]
fn a_crossed_sync_point_is_refused_by_name() {
    let (up, lo) = (rect(20.0, 10.0), rect(22.0, 14.0));
    // Pairs 2 and 3 swap their upper corners.
    let crossed = sync(&[
        ((-20.0, -10.0), (-22.0, -14.0)),
        ((20.0, 10.0), (22.0, -14.0)),
        ((20.0, -10.0), (22.0, 14.0)),
        ((-20.0, 10.0), (-22.0, 14.0)),
    ]);
    let why = refusal(&up, &lo, &ruled(crossed));
    assert!(why.contains("sync pair #3 crosses sync pair #2") && why.contains("rulings between them would cross"), "{why}");
}

/// Done means: a folding pairing is refused by name - by arc length (a
/// notched profile over a plain one: walking into the notch the upper
/// end runs backward) and by a sync pair set a half turn off.
#[test]
fn a_folding_pairing_is_refused_by_name() {
    let notched = Loop::new(
        [(-20.0, -10.0), (20.0, -10.0), (20.0, 10.0), (5.0, 10.0), (5.0, -5.0), (-5.0, -5.0), (-5.0, 10.0), (-20.0, 10.0)]
            .map(|(x, y)| Point2::new(x, y))
            .to_vec(),
    )
    .unwrap();
    let why = refusal(&notched, &rect(20.0, 10.0), &WireRuled { max_deg: Some(80.0), ..ruled(WirePairing::ArcLength) });
    assert!(why.contains("folds near") && why.contains("consecutive rulings cross within the part height"), "{why}");
    assert!(why.contains("by normalised arc length") && why.contains("state sync points"), "{why}");
    let (up, lo) = (rect(20.0, 10.0), rect(22.0, 14.0));
    let why = refusal(&up, &lo, &WireRuled { max_deg: Some(80.0), ..ruled(sync(&[((20.0, 10.0), (-22.0, -14.0))])) });
    assert!(why.contains("folds near") && why.contains("by 1 sync pair(s)") && why.contains("restate the sync points"), "{why}");
}

/// Unpaired, off-profile, doubled and missing sync points - by name.
#[test]
fn bad_sync_points_are_refused_by_name() {
    let (up, lo) = (rect(20.0, 10.0), rect(22.0, 14.0));
    let one = |upper: Option<(f64, f64)>, lower: Option<(f64, f64)>| {
        let pt = |q: Option<(f64, f64)>| q.map(|(x, y)| Point2::new(x, y));
        ruled(WirePairing::SyncPoints(vec![WireSync { upper: pt(upper), lower: pt(lower) }]))
    };
    let why = refusal(&up, &lo, &one(Some((20.0, 10.0)), None));
    assert!(why.contains("sync pair #1 is unpaired") && why.contains("no lower point"), "{why}");
    let why = refusal(&up, &lo, &one(Some((20.0, 10.0)), Some((0.0, 0.0))));
    assert!(why.contains("sync pair #1's lower point X0.000 Y0.000 is not on the lower profile"), "{why}");
    let why = refusal(&up, &lo, &one(Some((f64::NAN, 10.0)), Some((22.0, 14.0))));
    assert!(why.contains("not a finite point"), "{why}");
    let why = refusal(&up, &lo, &ruled(WirePairing::SyncPoints(Vec::new())));
    assert!(why.contains("by sync points and none are stated"), "{why}");
    let twice = sync(&[((20.0, 10.0), (22.0, 14.0)), ((-20.0, 10.0), (22.0, 14.0))]);
    let why = refusal(&up, &lo, &ruled(twice));
    assert!(why.contains("sync pairs #1 and #2 state the same point of the lower profile"), "{why}");
}

/// Profiles of different kinds, a ruling past the maximum taper, a
/// machine stating no maximum, and planes the wrong way up - by name.
#[test]
fn mismatched_profiles_and_steep_rulings_are_refused_by_name() {
    let (up, lo) = (rect(20.0, 10.0), rect(22.0, 14.0));
    let run = vec![vec![Point2::new(-20.0, 0.0), Point2::new(20.0, 0.0)]];
    let open = WireProfile { loops: &[], chains: &run };
    let closed = WireProfile { loops: std::slice::from_ref(&lo), chains: &[] };
    let r = ruled(WirePairing::ArcLength);
    let why = wire_contour_ruled(open, closed, &params(), &r).expect_err("mismatch").to_string();
    assert!(why.contains("the upper profile is open and the lower profile is closed"), "{why}");
    let why = wire_contour_ruled(open, open, &params(), &r).expect_err("open").to_string();
    assert!(why.contains("both profiles are open"), "{why}");
    let why = refusal(&up, &lo, &WireRuled { max_deg: Some(5.0), ..r.clone() });
    assert!(why.contains("leans the wire") && why.contains("at most 5.000 degrees (edm.max_taper_deg)"), "{why}");
    let why = refusal(&up, &lo, &WireRuled { max_deg: None, ..r.clone() });
    assert!(why.contains("states no edm.max_taper_deg"), "{why}");
    let why = refusal(&up, &lo, &WireRuled { top_z: -1.0, ..r.clone() });
    assert!(why.contains("does not stand above the programmed plane"), "{why}");
    let why = refusal(&up, &lo, &WireRuled { tolerance: f64::NAN, ..r });
    assert!(why.contains("tolerance"), "{why}");
}

/// The die side: the wire runs inside both profiles of a conical
/// opening, every ruling leaning by the cone's angle, and the widening
/// is a constant taper's `1 / cos(angle)` - nothing more.
#[test]
fn a_conical_die_runs_inside_both_profiles() {
    let (up, lo) = (round(8.0, 64), round(10.0, 64));
    let p = WireParams { side: WireSide::Die, start: Some(Point2::new(-3.0, 0.0)), ..params() };
    let path = wire_contour_ruled(
        WireProfile { loops: std::slice::from_ref(&up), chains: &[] },
        WireProfile { loops: std::slice::from_ref(&lo), chains: &[] },
        &p,
        &ruled(WirePairing::ArcLength),
    )
    .expect("a conical die generates");
    let angle = (2.0f64 / H).atan();
    let facet = (std::f64::consts::PI / 64.0).cos();
    let wide = K / angle.cos() + 0.02;
    for (l, u) in &rulings(&path) {
        // Inside both profiles, by the kerf offset widened for the lean.
        let (rl, ru) = (l.x.hypot(l.y), u.x.hypot(u.y));
        assert!(rl < 10.0 - K && ru < 8.0 - K, "{rl} {ru}");
        assert!(rl > 10.0 * facet - wide && ru > 8.0 * facet - wide, "{rl} {ru}");
    }
    let cut = &path.moves[..path.moves.len() - 1];
    let lean = |a: [f64; 3]| a[0].hypot(a[1]).atan2(a[2]);
    assert!(cut.iter().all(|m| m.axis.is_some_and(|a| (lean(a) - angle).abs() < 2e-3)), "every ruling leans the cone's angle");
}

/// Done means (M3 review of PR #1181, item 6): a pairing whose stated
/// surface does not fold, but whose wire - paired over the kerf-offset
/// rings - does, is refused by the fold law on the offset wire, by name.
/// One sync pair takes the lower profile's left middle to a point near
/// the smaller upper profile's lower left corner: the stated surface
/// turns no ruling back, the wire's rounded offset corners do.
#[test]
fn a_pairing_whose_offset_wire_folds_is_refused_by_name() {
    let (up, lo) = (rect(14.0, 7.0), rect(20.0, 10.0));
    let pairing = sync(&[((-7.0, -7.0), (-20.0, 0.0))]);
    let why = refusal(&up, &lo, &ruled(pairing));
    assert!(why.contains("folds near") && why.contains("consecutive rulings cross within the part height"), "{why}");
    assert!(why.contains("by 1 sync pair(s)") && why.contains("restate the sync points"), "{why}");
}

/// Done means (M3 review of PR #1181, item 7): a start hole whose lead
/// brings the leaning wire's kerf into the paired surface is refused by
/// name. The prism's one sync pair takes the lower left middle to a
/// point on the upper's bottom edge, so the ruling the lead runs to
/// leans round the corner, and at the top plane the lead from the
/// upright start hole cuts across that corner.
#[test]
fn a_lead_whose_leaning_kerf_enters_the_paired_surface_is_refused_by_name() {
    let r = rect(20.0, 10.0);
    let pairing = sync(&[((-15.0, -10.0), (-20.0, 0.0))]);
    let why = refusal(&r, &r, &WireRuled { tolerance: 1.0, max_deg: Some(80.0), ..ruled(pairing) });
    assert!(why.contains("the lead from the start hole would bring the leaning wire's kerf"), "{why}");
    assert!(why.contains("into the paired surface") && why.contains("move the start hole"), "{why}");
}

/// The crossing law: rulings whose mid-height section is a bow tie
/// (corners 2 and 3 swapped) are refused by name, a plain one passes.
#[test]
fn a_section_that_crosses_itself_is_refused_by_name() {
    let ring = |c: [(f64, f64); 4]| -> Vec<(Point2, Point2)> {
        c.iter().map(|&(x, y)| (Point2::new(x - 1.0, y - 1.0), Point2::new(x + 1.0, y + 1.0))).collect()
    };
    let bow = ring([(0.0, 0.0), (10.0, 10.0), (10.0, 0.0), (0.0, 10.0)]);
    let why = ruled_laws::crossing(&bow, 0.0, 12.0).expect_err("a bow tie").to_string();
    assert!(why.contains("the paired surface crosses itself at Z"), "{why}");
    let square = ring([(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
    ruled_laws::crossing(&square, 0.0, 12.0).expect("a simple section passes");
}
