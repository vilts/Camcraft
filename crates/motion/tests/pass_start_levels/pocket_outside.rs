//! #3820, #3822: a 2D Pocket with Outside containment is an area round a part
//! (`around_part`), so its rings link by the ring law of #3747 too. The
//! pins: on both ring strategies of the pocket, no overcut, no crossing
//! feeds, each lift named, and no lift round a part that splits no ring.
//! A failed ring link lifts to the plane of the user's pass link.

use super::ring_links::{assert_no_crossing, band, disc_part, dog_bone, lifts, named_lifts, side};
use super::{box_facets, mask, params, Z};
use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geo2d::Region;
use camcraft_motion::links::Link;
use camcraft_motion::machining2d::{
    CornerRelief, PassStart, PocketParams, PocketStrategy, StartSide, pocket_avoid_started_on, pocket_started_on,
};
use camcraft_motion::tools::ToolGeom;
use camcraft_motion::verify::{AVOID_VERIFY_TOL, AvoidItem, AvoidSpec, Cutter, avoid_hits};

/// The pocket's ring strategies, with the pocket's default links.
const STRATEGIES: [PocketStrategy; 2] = [PocketStrategy::Waterline, PocketStrategy::OffsetRings];

fn pocket(strategy: PocketStrategy) -> PocketParams {
    let mut p = params();
    p.stepover = 2.0;
    p.strategy = strategy;
    p
}

fn plan(part: &Region, p: &PocketParams, start: PassStart) -> Toolpath {
    pocket_started_on(&band(part), p, &[], &CornerRelief::default(), &side(start)).expect("plan")
}

/// #3820 / #3822 pin: round a disc (the part splits no ring) the Outside-contained
/// pocket keeps each level on the floor on Inside and Outside, on both
/// ring strategies: no lift, no re-entry, no overcut, no crossing feed.
/// Links between pass rings stay in the cut plane as feed moves.
#[test]
fn outside_pocket_rings_round_a_disc_do_not_lift() {
    for strategy in STRATEGIES {
        for start in [PassStart::Inside, PassStart::Outside] {
            let path = plan(&disc_part(), &pocket(strategy), start);
            let name = format!("pocket disc {strategy:?} {start:?}");
            let (up, back) = lifts(&path);
            eprintln!("{name}: {up} lift(s), {back} re-entry(ies), notes {:?}", path.notes);
            assert_eq!(named_lifts(&path), up, "{name}: a lift with no named reason: {:?}", path.notes);
            assert_eq!((up, back), (0, 0), "{name}: the rings lift: {:?}", path.notes);
            let floor_feeds = path
                .moves
                .iter()
                .filter(|m| {
                    m.kind == MoveKind::Feed
                        && (m.from.z - Z).abs() < 1e-9
                        && (m.to.z - Z).abs() < 1e-9
                })
                .count();
            assert!(floor_feeds > 0, "{name}: expected planar feed moves on the floor");
            mask("pocket_outside_disc", &band(&disc_part()), &path).assert_no_overcut();
            assert_no_crossing(&name, &path);
        }
    }
}

/// #3822 pin: on the dog bone of #3744, the Outside-contained pocket cuts
/// nothing outside its band, crosses no feed, and names each lift.
#[test]
fn outside_pocket_dog_bone_names_each_lift() {
    let part = dog_bone();
    for strategy in STRATEGIES {
        for start in [PassStart::Inside, PassStart::Outside] {
            let path = plan(&part, &pocket(strategy), start);
            let name = format!("pocket dog bone {strategy:?} {start:?}");
            let (up, back) = lifts(&path);
            eprintln!("{name}: {up} lift(s), {back} re-entry(ies), notes {:?}", path.notes);
            assert_eq!(named_lifts(&path), up, "{name}: a lift with no named reason: {:?}", path.notes);
            mask("pocket_outside_dog_bone", &band(&part), &path).assert_no_overcut();
            assert_no_crossing(&name, &path);
        }
    }
}

/// #3853: the same plan with `around_part` off, which never links by the
/// ring law; both are computed on this machine, so the comparison is exact
/// on every platform (a recorded path hash is not: the float results of a
/// long plan differ in the last bits across targets).
fn without_ring_law(region: &Region, p: &PocketParams, side: &StartSide) -> Toolpath {
    let off = StartSide { around_part: false, ..side.clone() };
    pocket_started_on(region, p, &[], &CornerRelief::default(), &off).expect("plan")
}

/// The two paths post the same moves (kind and both ends) and notes.
fn assert_same_moves(name: &str, got: &Toolpath, want: &Toolpath) {
    assert_eq!(got.moves.len(), want.moves.len(), "{name}: the move count changed");
    let first = got.moves.iter().zip(&want.moves).position(|(a, b)| a.kind != b.kind || a.from != b.from || a.to != b.to);
    assert!(first.is_none(), "{name}: move {first:?} differs: {:?} vs {:?}", first.map(|i| &got.moves[i].to), first.map(|i| &want.moves[i].to));
    assert_eq!(got.notes, want.notes, "{name}: the notes changed");
}

/// #3822 pin: Zigzag rows are no ring set, so the ring law does not link
/// them. Round the dog bone, Inside and Outside post the same moves as the
/// plan that never links by the ring law (#3853: compared live).
#[test]
fn zigzag_rows_keep_their_links_round_a_part() {
    for start in [PassStart::Inside, PassStart::Outside] {
        let p = pocket(PocketStrategy::Zigzag);
        let path = plan(&dog_bone(), &p, start);
        let want = without_ring_law(&band(&dog_bone()), &p, &side(start));
        assert_same_moves(&format!("zigzag {start:?}"), &path, &want);
    }
}

/// #3822 pin: round a part the HSM plan cannot start on the start side (a
/// hole wall Inside, the air past the stock Outside), and it falls back to
/// Offset rings, by name. Those passes are rings, so they link by the ring
/// law: no overcut, no crossing feed, each lift named.
#[test]
fn adaptive_fallback_rings_link_by_the_ring_law() {
    let part = dog_bone();
    for start in [PassStart::Inside, PassStart::Outside] {
        let path = plan(&part, &pocket(PocketStrategy::Adaptive), start);
        let name = format!("adaptive fallback {start:?}");
        assert!(
            path.notes.iter().any(|n| n.contains("Offset rings cut this pocket")),
            "{name}: the fallback is named: {:?}",
            path.notes
        );
        let (up, back) = lifts(&path);
        eprintln!("{name}: {up} lift(s), {back} re-entry(ies), notes {:?}", path.notes);
        assert_eq!(named_lifts(&path), up, "{name}: a lift with no named reason: {:?}", path.notes);
        mask("pocket_adaptive_fallback", &band(&part), &path).assert_no_overcut();
        assert_no_crossing(&name, &path);
    }
}

/// The square fixture of `pocket_hsm_start_tests` (#3355): a sketch 96 mm
/// wide less a 30 x 30 part, a 70 x 70 stock, a D10 tool, one level.
fn hsm_square() -> PocketParams {
    use camcraft_motion::entry::Entry3D;
    PocketParams {
        blend_radius: None,
        tool_diameter: 10.0,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: 40.0,
        retract_z: None,
        top_z: 10.0,
        bottom_z: 5.0,
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::Adaptive,
        entry: Entry3D::Helix,
        helix_radius: 2.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links: camcraft_motion::links::Links::PLANAR,
    }
}

/// #3822 pin (review of #3837): round a part with Outside and a stock, the
/// Adaptive spiral runs its own air plan (no fallback to rings). It is no
/// ring set, so the ring law does not link it: it posts the moves of the
/// plan that never links by the ring law (#3853: compared live).
#[test]
fn adaptive_spiral_round_a_part_outside_equals_main_before_3818() {
    use camcraft_motion::geo2d::Point2;
    let sq = |s: f64| vec![Point2::new(-s, -s), Point2::new(s, -s), Point2::new(s, s), Point2::new(-s, s)];
    let band = super::region(vec![sq(48.0), sq(15.0)]);
    let stock = super::region(vec![sq(35.0)]);
    let side = StartSide { start: PassStart::Outside, stock: Some(stock), around_part: true };
    let path = pocket_started_on(&band, &hsm_square(), &[], &CornerRelief::default(), &side).expect("plan");
    assert!(
        !path.notes.iter().any(|n| n.contains("Offset rings cut this pocket")),
        "the spiral falls back to rings: {:?}",
        path.notes
    );
    let want = without_ring_law(&band, &hsm_square(), &side);
    eprintln!("adaptive square Outside: {} move(s)", path.moves.len());
    assert_same_moves("adaptive square Outside", &path, &want);
}

/// The Z of each lift off the floor at `Z` (a cut on the floor still to
/// come).
fn lift_planes(path: &Toolpath) -> Vec<f64> {
    let on = |z: f64| (z - Z).abs() < 1e-9;
    let floor: Vec<usize> = (0..path.moves.len())
        .filter(|&i| on(path.moves[i].from.z) && on(path.moves[i].to.z))
        .collect();
    let (Some(&first), Some(&last)) = (floor.first(), floor.last()) else {
        return Vec::new();
    };
    (first..last)
        .map(|i| &path.moves[i])
        .filter(|m| m.to.z > Z + 1e-6 && m.from.z <= Z + 1e-6)
        .map(|m| m.to.z)
        .collect()
}

/// #3822 pin: a clamp across the in-plane link makes a ring link fail. The
/// lift goes to the clearance height when the user chose Clearance for the
/// pass link, and to the retract plane on Retract and Keep down.
#[test]
fn failed_ring_link_lifts_to_the_users_pass_plane() {
    let part = disc_part();
    let spec = AvoidSpec {
        items: vec![AvoidItem {
            label: "clamp".into(),
            tris: box_facets([-2.0, 20.0, -10.0], [2.0, 30.0, 5.0]),
            edges: vec![],
        }],
        cutter: Cutter::flat(&ToolGeom {
            radius: super::R,
            nose_radius: 0.0,
            flute_len: 10.0,
            shank_radius: super::R,
            shank_len: 20.0,
            holder_radius: 12.0,
            holder_len: 20.0,
        }),
        clearance: 0.5,
    };
    for strategy in STRATEGIES {
        for pass in [Link::Clearance, Link::Retract, Link::KeepDown] {
            let mut p = pocket(strategy);
            p.links.pass = pass;
            let retract = p.retract_z.expect("a retract plane");
            let path = pocket_avoid_started_on(&band(&part), &p, &spec, &CornerRelief::default(), &side(PassStart::Outside))
                .expect("plan");
            let name = format!("{strategy:?} {pass:?}");
            let hits = avoid_hits(&path, &spec, AVOID_VERIFY_TOL).expect("verify");
            assert!(hits.is_empty(), "{name}: the clamp is touched: {hits:?}");
            let planes = lift_planes(&path);
            eprintln!("{name}: lifts to {planes:?}, notes {:?}", path.notes);
            assert!(!planes.is_empty(), "{name}: the clamp splits the rings, so a link lifts");
            assert_eq!(named_lifts(&path), planes.len(), "{name}: a lift with no named reason: {:?}", path.notes);
            let want = if pass == Link::Clearance { p.clearance_z } else { retract };
            assert!(
                planes.iter().all(|&z| (z - want).abs() < 1e-9),
                "{name}: a failed link lifts to {planes:?}, not to {want}"
            );
            let said = if pass == Link::Clearance { "lifted to the clearance height" } else { "lifted to the retract plane" };
            assert!(
                path.notes.iter().filter(|n| n.starts_with("link: ")).all(|n| n.contains(said)),
                "{name}: a note names another plane than {said:?}: {:?}",
                path.notes
            );
        }
    }
}
