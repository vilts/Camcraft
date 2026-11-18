//! Rest Adaptive's guards, one at a time (issue 2031: "remove each
//! stock, depth, and entry guard separately; each mutation must fail
//! its pinning test"), and the pieces the plan stands on: the
//! conservative raster, the wall vertices, the phase radii.

use crate::cl::{MoveKind, Toolpath};
use crate::entry::Entry3D;
use crate::geo2d::{Loop, Point2, Region};
use crate::machining2d::pocket::{PocketParams, PocketStrategy};

use super::super::raster::Raster;
use super::canon::Canon;
use super::entry::Entries;
use super::stock::Guards;
use super::{RestStock, StockFloor, guarded};

const D: f64 = 4.0;

fn poly(pts: &[(f64, f64)]) -> Region {
    let Ok(l) = Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()) else { panic!("fixture loop") };
    let Ok(r) = Region::new(vec![l]) else { panic!("fixture region") };
    r
}

fn disc(c: Point2, r: f64) -> Region {
    let pts: Vec<(f64, f64)> = (0..96)
        .map(|k| {
            let a = f64::from(k) * std::f64::consts::TAU / 96.0;
            (c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect();
    poly(&pts)
}

/// A 40 x 30 pocket, cut from Z-2 to Z-4 with a 4 mm tool.
fn pocket() -> (Region, PocketParams) {
    let p = PocketParams {
        blend_radius: None,
        links: crate::links::Links::PLANAR,
        tool_diameter: D,
        stepover: 1.5,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: Some(1.0),
        top_z: -2.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 400.0,
        plunge_feed: 80.0,
        climb: true,
        strategy: PocketStrategy::Adaptive,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: crate::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    };
    (poly(&[(0.0, 0.0), (40.0, 0.0), (40.0, 30.0), (0.0, 30.0)]), p)
}

/// A pillar the earlier operations left standing in cleared floor,
/// proved down to `z`.
fn pillar(z: f64) -> Region {
    let _ = z;
    disc(Point2::new(20.0, 15.0), 3.0)
}

fn stock(z: f64, uncertain: Region) -> RestStock {
    let Ok(s) = RestStock::from_verified(vec![StockFloor { z, remaining: pillar(z), uncertain }]) else {
        panic!("fixture stock")
    };
    s
}

fn plunges(path: &Toolpath) -> Vec<(Point2, f64)> {
    path.moves.iter().filter(|m| m.kind == MoveKind::Plunge).map(|m| (Point2::new(m.to.x, m.to.y), m.to.z)).collect()
}

/// Does the cutter standing at `p` touch `material`?
fn touches(material: &Region, p: Point2) -> bool {
    material.offset(D * 0.5).is_ok_and(|grown| grown.contains(p))
}

fn plan(stock: &RestStock, guards: Guards) -> Toolpath {
    let (region, p) = pocket();
    match guarded(&region, &p, stock, guards) {
        Ok(path) => path,
        Err(e) => panic!("the fixture plans: {e}"),
    }
}

/// The entry pin: no plunge's footprint touches the stock's material.
#[test]
fn without_the_entry_guard_a_plunge_lands_on_material() {
    let s = stock(-4.0, Region::empty());
    let pin = |path: &Toolpath| plunges(path).iter().all(|&(p, _)| !touches(&pillar(-4.0), p));
    let guarded_path = plan(&s, Guards::ALL);
    assert!(!plunges(&guarded_path).is_empty());
    assert!(pin(&guarded_path), "a guarded plunge touches the pillar");
    let mutant = plan(&s, Guards { entry: false, ..Guards::ALL });
    assert!(!pin(&mutant), "the entry guard is not what keeps the plunges off the material");
}

/// The depth pin: no plunge goes below the deepest proved floor.
#[test]
fn without_the_depth_guard_a_plunge_goes_below_the_proved_floor() {
    let s = stock(-2.0, Region::empty());
    let (region, p) = pocket();
    match guarded(&region, &p, &s, Guards::ALL) {
        Err(e) => {
            let msg = e.to_string();
            assert!(msg.contains("not proved at Z-4.000") && msg.contains("proved down to Z-2.000"), "{msg}");
        }
        Ok(_) => panic!("a cut below the proved floor was planned"),
    }
    let pin = |path: &Toolpath| plunges(path).iter().all(|&(_, z)| z >= s.deepest() - 1e-9);
    let mutant = plan(&s, Guards { depth: false, ..Guards::ALL });
    assert!(!pin(&mutant), "the depth guard is not what keeps the plunges above the proved floor");
}

/// The stock pin: no plunge's footprint touches uncertain stock.
#[test]
fn without_the_stock_guard_a_plunge_lands_in_uncertain_stock() {
    // Where the plan plunges when nothing is uncertain ...
    let plain = plan(&stock(-4.0, Region::empty()), Guards::ALL);
    let Some(&(at, _)) = plunges(&plain).first() else { panic!("the fixture plunges") };
    // ... is where the proof could not decide.
    let doubt = disc(at, 0.8);
    let s = stock(-4.0, doubt.clone());
    let pin = |path: &Toolpath| plunges(path).iter().all(|&(p, _)| !touches(&doubt, p));
    let (region, p) = pocket();
    // Guarded: planned around it, or refused by name - never entered.
    if let Ok(path) = guarded(&region, &p, &s, Guards::ALL) {
        assert!(pin(&path), "a guarded plunge touches uncertain stock");
        // The uncertain stock is cut like material: a feed passes over it.
        let over = path.moves.iter().any(|m| m.kind == MoveKind::Feed && touches(&doubt, Point2::new(m.to.x, m.to.y)));
        assert!(over, "uncertain stock was not cut");
    }
    let mutant = plan(&s, Guards { stock: false, ..Guards::ALL });
    assert!(!pin(&mutant), "the stock guard is not what keeps the plunges out of uncertain stock");
}

/// A sliver thinner than a cell passes between cell centers: sampled
/// plainly it vanishes, grown by the conservative growth every cell it
/// touches stands.
#[test]
fn the_conservative_raster_stands_every_cell_a_sliver_touches() {
    let domain = poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
    let Ok(grid) = Raster::from_region(&domain, 0.5, 1.0) else { panic!("fixture grid") };
    // 0.05 wide, between the cell centers at y = 4.75 and y = 5.25.
    let sliver = poly(&[(1.0, 5.0), (9.0, 5.0), (9.0, 5.05), (1.0, 5.05)]);
    let mut plain = grid.clone();
    plain.keep(&sliver);
    assert_eq!(plain.standing(), 0, "the fixture's sliver should miss every cell center");
    let mut large = grid.clone();
    let Ok(grown) = sliver.offset(grid.conservative_growth()) else { panic!("fixture offset") };
    large.keep(&grown);
    for k in 0..=80 {
        let p = Point2::new(1.0 + 0.1 * f64::from(k), 5.025);
        assert!(large.mat(p), "the cell at ({}, {}) lost the sliver", p.x, p.y);
    }
    // A footprint is proved clear only with a cell of slack round it
    // (`entry` asks a quarter of its margin, at least a cell): a disc
    // that fits between cell centers proves nothing.
    let (on, off) = (Point2::new(5.0, 5.0), Point2::new(5.0, 8.0));
    assert!(!large.air(on, on, 0.2 + grid.h), "a footprint on the sliver read as clear");
    assert!(large.air(off, off, 0.2 + grid.h));
}

/// A room loop that follows two walls and cuts the corner between them
/// comes back on the walls' own vertices; the points where it leaves
/// and meets the walls (off any wall vertex) are dropped.
#[test]
fn a_wall_stretch_is_walked_on_the_walls_own_vertices() {
    let center = poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
    let canon = Canon::new(&center, 1.0);
    // Along the bottom wall to x = 6.3, across to (10, 3.7), up the right wall.
    let room = [(0.0, 0.0), (6.3, 0.0), (10.0, 3.7), (10.0, 10.0), (0.0, 10.0)].map(|(x, y)| Point2::new(x, y));
    let ring = canon.snap(&room);
    assert!(ring.iter().all(|v| v.at.is_some_and(|at| canon.point(at) == v.p)), "a point is off the walls' vertices");
    let has = |x: f64, y: f64| ring.iter().any(|v| v.p == Point2::new(x, y));
    assert!(has(6.0, 0.0) && has(10.0, 4.0), "the wall vertices next to the cut corner are missing");
    assert!(!has(6.3, 0.0) && !has(10.0, 3.7) && !has(8.0, 0.0) && !has(10.0, 0.0) && !has(10.0, 2.0));
    assert!(canon.wall_chord(Point2::new(5.0, 0.0), Point2::new(6.0, 0.0)));
    assert!(!canon.wall_chord(Point2::new(6.0, 0.0), Point2::new(10.0, 4.0)));
}

/// The phase radii fall from the widest by at most 15% and at most a
/// step and a half, and end sharp.
#[test]
fn the_phase_radii_fall_gently_and_end_sharp() {
    let (r, w) = (2.0, 0.586);
    let radii = super::peel::radii(r, w, 18.0);
    assert_eq!(radii.first().copied(), Some(18.0));
    assert_eq!(radii.last().copied(), Some(0.0));
    let open = &radii[..radii.len() - 1];
    for pair in open.windows(2) {
        let fall = pair[0] - pair[1];
        assert!(fall > 0.0 && fall <= 0.15 * pair[0] + 1e-12 && fall <= 1.5 * w + 1e-12, "{pair:?}");
    }
    assert!(open.iter().all(|&rho| rho > r / 8.0));
    assert_eq!(super::peel::radii(r, w, 0.1), vec![0.0], "a center area too small to open is cut sharp");
}

/// The raster half of the entry proof stands on its own (issue 2113):
/// a raster holding material the polygon bound does not (a sliver the
/// polygon offset lost, say) refuses a plunge whose footprint reaches it,
/// though the polygon half alone passes it. Goes red when the raster
/// half is dropped from `Entries::proved`, or its footprint is 0.
#[test]
fn the_raster_half_of_the_entry_proof_refuses_on_its_own() {
    let (region, _) = pocket();
    let r = D * 0.5;
    let Ok(center) = region.offset(-r) else { panic!("fixture center") };
    let Ok(reach) = center.offset(r) else { panic!("fixture reach") };
    let Ok(mut ras) = Raster::from_region(&reach, D / 120.0, r) else { panic!("fixture grid") };
    let grow = ras.conservative_growth();
    // The raster's pillar: 1 mm round (20, 15). The polygon bound: none.
    let Ok(pillar) = disc(Point2::new(20.0, 15.0), 1.0).offset(grow) else { panic!("fixture pillar") };
    ras.keep(&pillar);
    let margin = super::peel::ENTRY_MARGIN.max(4.0 * ras.h);
    let Ok(entries) = Entries::new(&center, &Region::empty(), ras, (r, grow, margin, r / 8.0), true) else {
        panic!("fixture entries")
    };
    // The cutter's rim at 1.8 mm from the pillar's edge would stand
    // 0.2 mm inside it: refused. (Off any cell center, so a footprint of
    // 0 holds no cell.)
    let over = Point2::new(22.8 + 1e-3, 15.0 + 1e-3);
    assert!(!entries.proved(over), "a plunge onto the raster's material was proved clear");
    // Clear of it by more than the margin: proved.
    let clear = Point2::new(23.6, 15.0);
    assert!(entries.proved(clear), "a plunge clear of the raster's material was refused");
    assert!(entries.any());
}

/// The travel budget (issue 2109) refuses by name past each limit: the
/// feed out of cut, then the retract hops beyond one per rest area.
#[test]
fn the_travel_budget_refuses_by_name_past_each_limit() {
    use super::budget::{REST_NONCUT_PER_CUT, Travel, check, hop_budget};
    let within = Travel { cut: 100.0, air: REST_NONCUT_PER_CUT * 100.0, hops: hop_budget(4) };
    assert!(check(&within, 4, -2.0).is_ok());
    let refused = |t: Travel| match check(&t, 4, -2.0) {
        Err(e) => e.to_string(),
        Ok(()) => panic!("{t:?} passed the budget"),
    };
    let msg = refused(Travel { air: within.air + 1.0, ..within });
    assert!(msg.contains("rest Adaptive at Z-2.000 would feed 301 mm out of cut for 100 mm in cut (over 3.0x)"), "{msg}");
    let msg = refused(Travel { hops: hop_budget(4) + 1, ..within });
    assert!(msg.contains("would reach 12 passes by a retract (over 11") && msg.contains("4 rest areas"), "{msg}");
    assert_eq!(hop_budget(0), hop_budget(1));
}

/// The first rest level pin (#2004, the floor review of PR 2013): the
/// rest stock stands over the rest Top Z, so the plane at Top Z (Z-2)
/// is a cut level. With no material top stated (`Links::PLANAR`) the
/// rest ladder keeps it; a plain `cut_levels` drops it and leaves the
/// band Z-2..Z-4 uncut without a word.
#[test]
fn the_first_rest_level_at_top_z_is_cut_without_a_stated_material_top() {
    let (region, p) = pocket();
    assert!(p.links.approach.stock_top.is_none(), "the fixture states no material top");
    let s = stock(-4.0, Region::empty());
    let feeds_at = |path: &Toolpath, z: f64| path.moves.iter().filter(|m| m.kind == MoveKind::Feed && (m.to.z - z).abs() < 1e-9).count();
    let path = plan(&s, Guards::ALL);
    assert!(feeds_at(&path, -2.0) > 0, "the first rest level (Z-2) has no cut");
    assert!(feeds_at(&path, -4.0) > 0, "the last rest level (Z-4) has no cut");
    // A stated material top over Top Z keeps the same ladder.
    let mut over = p;
    over.links.approach.stock_top = Some(0.0);
    let Ok(path) = guarded(&region, &over, &s, Guards::ALL) else { panic!("the fixture plans with a material top") };
    assert!(feeds_at(&path, -2.0) > 0, "a material top over Top Z drops the first rest level");
}
