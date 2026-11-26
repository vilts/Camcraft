//! #2004 review: where the stock stands over Top Z, the plane at Top Z
//! cuts stock, so Face and the vault rim ring keep it as their first
//! level - the first bite is at most one stepdown, the pre-#2004 ladder.
//! Without stock over Top Z (`stock_top` `None`) they keep the #2004 law
//! (no level at Top Z), pinned in `top_z_level.rs`.
//!
//! Provenance: our own fixtures; the depth-of-cut ladder is general
//! machining knowledge.
//!
//! Gate map: covers camcraft_motion::machining2d (face, circular, levels).

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{circular, face, CircularParams, FaceParams};

const TOP: f64 = 13.0;
const STOCK: f64 = 15.0;

fn square(x0: f64, y0: f64, s: f64) -> Region {
    let l = Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x0 + s, y0),
        Point2::new(x0 + s, y0 + s),
        Point2::new(x0, y0 + s),
    ]);
    Region::new(vec![l.expect("loop")]).expect("region")
}

/// The Z of every level feed move, highest first, without repeats.
fn cut_planes(t: &Toolpath) -> Vec<f64> {
    let mut zs: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid && (m.from.z - m.to.z).abs() < 1e-9)
        .map(|m| m.to.z)
        .collect();
    zs.sort_by(|a, b| b.total_cmp(a));
    zs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    zs
}

fn fp(stock_top: Option<f64>) -> FaceParams {
    FaceParams {
        holes: Default::default(),
        tool_diameter: 6.0,
        stepover: 4.0,
        clearance_z: 20.0,
        cut_z: 0.0,
        top_z: Some(TOP),
        stepdown: Some(5.0),
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false, blend_radius: None,
        stock_top,
    }
}

#[test]
fn a_staged_face_under_stock_over_top_z_keeps_the_level_at_top_z() {
    let over = face(&square(0.0, 0.0, 30.0), &fp(Some(STOCK))).expect("face");
    assert_eq!(cut_planes(&over), vec![13.0, 8.0, 3.0, 0.0], "first bite 2 mm, not 7 mm");
    // Stock at Top Z or none stated: the #2004 law.
    for st in [None, Some(TOP)] {
        let t = face(&square(0.0, 0.0, 30.0), &fp(st)).expect("face");
        assert_eq!(cut_planes(&t), vec![8.0, 3.0, 0.0], "stock top {st:?}");
    }
    // A stated stock top that is not finite is refused, never read as "none".
    assert!(face(&square(0.0, 0.0, 30.0), &fp(Some(f64::NAN))).is_err());
}

fn vault(stock_top: Option<f64>) -> CircularParams {
    let mut links = Links::PLANAR;
    links.approach.stock_top = stock_top;
    CircularParams {
        pass_start: Default::default(),
        links,
        center: Point2::new(0.0, 0.0),
        radius: 10.0,
        vault: true,
        bottom_z: 0.0,
        top_z: TOP,
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        stepover: 2.0,
        stepdown: 50.0,
        clearance_z: 20.0,
        retract_z: None,
        feed: 500.0,
        plunge_feed: 250.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
    }
}

#[test]
fn a_vault_under_stock_over_top_z_cuts_its_rim_ring_at_top_z() {
    let region = square(-12.0, -12.0, 24.0);
    let over = cut_planes(&circular(&region, &vault(Some(STOCK))).expect("vault"));
    assert_eq!(over.first().copied(), Some(TOP), "the rim ring cuts the stock over Top Z: {over:?}");
    // Each plane of the vault without stock over stays a plane here.
    let plain = cut_planes(&circular(&region, &vault(None)).expect("vault"));
    assert!(plain.iter().all(|&z| z < TOP - 1e-6), "no stock over: no level at Top Z: {plain:?}");
    for z in &plain {
        assert!(over.iter().any(|q| (q - z).abs() < 1e-9), "plane {z} kept under stock over: {over:?}");
    }
}

#[test]
fn forced_vault_rim_cuts_every_step_above_top_z() {
    use camcraft_motion::machining2d::PassStart;

    let region = square(-12.0, -12.0, 24.0);
    for start in [PassStart::Inside, PassStart::Outside] {
        for (raw, carried) in [(Some(18.0), None), (None, Some(18.0)), (Some(18.0), Some(16.0))] {
            let mut p = vault(raw);
            p.pass_start = start;
            p.stepdown = 2.0;
            p.links.approach.material_top = carried;
            let path = circular(&region, &p).expect("a legal vault under stock over Top Z");
            let cuts: Vec<_> = path.moves.iter().filter(|m| {
                m.kind == MoveKind::Feed && m.arc.is_some()
                    && (m.from.x.hypot(m.from.y) - 7.0).abs() < 1e-9
                    && (m.from.z - m.to.z).abs() < 1e-9
            }).map(|m| m.to.z).collect();
            assert_eq!(cuts, [16.0, 16.0, 14.0, 14.0, 13.0, 13.0], "{start:?}/{raw:?}/{carried:?}");
        }
    }
}

#[test]
fn a_forced_vault_clearance_link_ignores_an_unused_lower_retract() {
    use camcraft_motion::machining2d::PassStart;

    let region = square(-12.0, -12.0, 24.0);
    let mut p = vault(Some(18.0));
    p.pass_start = PassStart::Inside;
    p.stepdown = 2.0;
    let normal = circular(&region, &p).expect("all links use clearance");
    p.retract_z = Some(16.0);
    let with_retract = circular(&region, &p).expect("the lower retract is unused with clearance links");
    assert_eq!(with_retract.moves, normal.moves);
}
