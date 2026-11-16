//! Adaptive on the rotary developed wrap (issue #9), specced from the
//! wrap/development literature in `multiaxis::wrap_exit` and the HSM
//! planner's own load and kink laws; no vendor material consulted.
//!
//! Issue #9: Adaptive clearing on the rotary developed wrap. Each
//! developed level (s the arc length at the wrap's radius, v the
//! station, height the radius) goes to the HSM planner exactly as a
//! planar 3-D level does; the planned path is wrapped back by the wrap
//! exit's own chord law (no developed arc stays an arc on the
//! cylinder). Read here: the load each planned level holds and its
//! in-cut breaks past 5 degrees, measured on the DEVELOPED path from
//! the clamped bar's skin; the planner's own overload notes; the same kink law
//! on the POSTED world program pulled back into the developed frame
//! (its chorded arcs); every world move posed radial; each level rings
//! cut (read from the program's shape, not its notes) named by radius; OffsetRings move for move what it always was (digest
//! read without the patch). The seam law itself (`hsm::Seam`) is pinned
//! by its unit test in `machining3d::hsm`.
//!
//! Gate map (M4.4): covers camcraft_motion::multiaxis (rotary_offset,
//! wrap_exit), camcraft_motion::machining3d (hsm, entry), and
//! camcraft_motion::quality, on this suite's own revolve fixtures.
//! Lane shape (M4.4): the rings digest and the narrow window's refusal
//! (issue #55, rings only) run in seconds in debug; the Adaptive plans
//! (window ~3 min, r6.2 bar ~3 min, whole bar ~10 min debug) are the
//! heavy lane's (release --ignored, ~1 min). The fixtures and the level
//! reader live in `rotary_adaptive_fixture/`.

mod rotary_adaptive_fixture;

use camcraft_motion::cl::{PathBuilder, Toolpath};
use camcraft_motion::geometry::Point3;
use camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_motion::machining2d::PocketStrategy;
use camcraft_motion::machining3d::{NARROW_HELIX, Pocket3DParams};
use camcraft_motion::multiaxis::{rotary_offset_developed, rotary_offset_in};
use rotary_adaptive_fixture::{
    assert_refusals_named, bar, control, params, planner_overloads, read_levels, ring_tag, stock, LevelRead, R, TAU,
};

/// The whole turned bar in an r7 bar stock: four developed levels of
/// real material (radius 6.5, 6.0, 5.5, 5.0) are HSM-planned. Level 5.0
/// was refused past the planning budget until issue #1051: its first
/// margin attempt runs out of margin and the second was refused for what
/// the first had spent (`adaptive::budget`, now held per attempt; its
/// sub-micron twins are pinned in `adaptive::budget::twins`). About ten
/// minutes in debug, a minute in release: the heavy lane's.
#[test]
#[ignore = "heavy lane: release --ignored (three HSM plans over the developed bar, ~10 min debug)"]
fn adaptive_plans_the_developed_levels_under_the_load_and_kink_laws() {
    let (f, st) = (bar(), stock(7.0, -16.0, 16.0));
    let p = params(PocketStrategy::Adaptive);
    let dev = match rotary_offset_developed(&f, Some(&st), &p) {
        Ok(t) => t,
        Err(e) => panic!("the developed adaptive ladder: {e}"),
    };
    assert!(dev.is_chained() && dev.all_finite());
    // The measuring stock is the clamped bar itself, developed: its top
    // the r7 skin. The ladder's first level stands ON that skin (zero
    // depth); the levels below it cut real stock, each read from the
    // skin above it - round 1's r6.2 stock left only the skin level to
    // the planner and its 0% reading proved nothing.
    let reads = read_levels(&dev, 7.0, 0.0, p.stepdown);
    for r in &reads {
        println!(
            "radius {:.3}: {} planar moves ({} arcs), load {:.1}% D, breaks>5 {}, rings {}",
            r.rho, r.moves, r.arcs, r.load * 100.0, r.breaks, r.ringed(7.0)
        );
    }
    println!("notes: {:?}", dev.notes);
    // The planner's own peak load reading: it names every overload it
    // could not avoid (a spiral step out of the entry bore, the plan's
    // worst past its tolerance); a planned developed level names none.
    let over = planner_overloads(&dev.notes);
    assert!(over.is_empty(), "the planner's own load reading broke the law: {over:?}");
    let planned: Vec<&LevelRead> = reads.iter().filter(|r| !r.ringed(7.0) && r.moves > 0).collect();
    assert!(!planned.is_empty(), "the planner plans developed levels: {:?}", dev.notes);
    assert!(
        planned.iter().any(|r| r.rho < 7.0 - 1e-6 && r.arcs > 0),
        "a developed level under the skin is HSM-planned, its proved arcs posted: {:?}",
        dev.notes
    );
    assert!(!dev.notes.iter().any(|n| n.contains("planning budget")), "a level is refused past the budget: {:?}", dev.notes);
    for rho in [6.5, 6.0, 5.5, 5.0] {
        assert!(planned.iter().any(|r| (r.rho - rho).abs() < 1e-6 && r.arcs > 0), "radius {rho} is HSM-planned: {:?}", dev.notes);
    }
    for r in &planned {
        assert!(
            r.load <= DEFAULT_OPTIMAL_LOAD * 1.35,
            "radius {}: load {:.1}% D past {:.1}% + 35%",
            r.rho,
            r.load * 100.0,
            DEFAULT_OPTIMAL_LOAD * 100.0
        );
        assert_eq!(r.breaks, 0, "radius {}: in-cut breaks past 5 deg", r.rho);
    }
    // Every ring-cut level is said by name with its radius.
    assert_refusals_named(&reads, 7.0);
    // The world program: every move posed, every pose the radial of its
    // ball center (the axis stands on world Z here).
    let world = match rotary_offset_in(&f, Some(&st), &p, &control()) {
        Ok(t) => t,
        Err(e) => panic!("the wrapped adaptive rough: {e}"),
    };
    assert!(world.all_finite());
    assert!(world.moves.iter().all(|m| m.arc.is_none()), "no developed arc survives the wrap as an arc");
    let mut posed = 0usize;
    for m in &world.moves {
        let Some(a) = m.axis else { panic!("an unposed move at {:?}", m.to) };
        let c = Point3::new(m.to.x + R * a[0], m.to.y + R * a[1], m.to.z + R * a[2]);
        let l = c.x.hypot(c.y);
        if l < 1e-6 {
            continue;
        }
        let dot = (a[0] * c.x + a[1] * c.y) / l;
        assert!(a[2].abs() < 1e-9 && dot > 1.0 - 1e-9, "a pose off the radial at {:?}", m.to);
        posed += 1;
    }
    assert!(posed > 50, "a whole bar of radial poses: {posed}");
    // The kink law on the POSTED program: the world moves pulled back
    // into the developed frame (each developed arc now its chords), read
    // by the same reader; every planned level holds it there too.
    let pulled = pull_back(&world, 7.0);
    let wreads = read_levels(&pulled, 7.0, 0.0, p.stepdown);
    let wbreaks: usize = wreads.iter().filter(|w| planned.iter().any(|r| (r.rho - w.rho).abs() < 1e-5)).map(|w| w.breaks).sum();
    println!("world: {} moves (developed {}), in-cut breaks>5 on the planned levels {wbreaks}", world.moves.len(), dev.moves.len());
    for w in &wreads {
        // (No load read here: the chorded helix is no arc, so the bore
        // law cannot tell the entry's cleanup from the clearing.)
        println!("world radius {:.3}: {} planar moves, breaks>5 {}", w.rho, w.moves, w.breaks);
    }
    assert_eq!(wbreaks, 0, "the posted chords break the 5 deg in-cut law");
}

/// The world program (axis on world Z, through the origin) pulled back
/// into the developed frame: s the unwrapped azimuth at `rho_ref`, v the
/// station, height the tip's radius (to the micron, so a level's moves
/// share it). A radial pose's tip stands on the developed level itself.
fn pull_back(world: &Toolpath, rho_ref: f64) -> Toolpath {
    let micron = |v: f64| (v * 1e6).round() / 1e6;
    let mut th_prev: Option<f64> = None;
    let mut dev = |p: Point3| {
        let raw = p.y.atan2(p.x);
        let th = match th_prev {
            Some(t) => t + (raw - t + std::f64::consts::PI).rem_euclid(TAU) - std::f64::consts::PI,
            None => raw,
        };
        th_prev = Some(th);
        Point3::new(th * rho_ref, p.z, micron(p.x.hypot(p.y)))
    };
    let Some(first) = world.moves.first() else { panic!("an empty world program") };
    let mut b = PathBuilder::new(dev(first.from));
    for m in &world.moves {
        b.push(m.kind, dev(m.to), m.feed);
    }
    b.finish()
}

/// A 12 long window of the neck in the r7 bar, a 1 mm stepdown. The pin
/// is lifted - every developed level under the skin reaches the HSM
/// planner, which plans it or refuses it naming its radius, Offset rings
/// cutting the refused ones - and nothing it plans names an overload.
/// Since issue #1051 (the planning budget held per margin attempt) both
/// levels under the skin, 6.0 and 5.0, are HSM-planned; before, they
/// were refused past the budget, or not, on nanometres.
/// Still ~3 min in debug (the skin level's two-lap plan): heavy lane.
#[test]
#[ignore = "heavy lane: release --ignored (the developed window's HSM plan, ~3 min debug)"]
fn the_developed_window_reaches_the_planner_and_names_each_refusal() {
    let (f, st) = (bar(), stock(7.0, -16.0, 16.0));
    let p = Pocket3DParams { top_z: 16.0, bottom_z: 4.0, stepdown: 1.0, ..params(PocketStrategy::Adaptive) };
    let dev = match rotary_offset_developed(&f, Some(&st), &p) {
        Ok(t) => t,
        Err(e) => panic!("the developed window: {e}"),
    };
    assert!(dev.is_chained() && dev.all_finite());
    println!("window notes: {:?}", dev.notes);
    let reads = read_levels(&dev, 7.0, 0.0, p.stepdown);
    for r in &reads {
        println!(
            "radius {:.3}: {} planar moves ({} arcs), load {:.1}% D, breaks>5 {}, rings {}",
            r.rho, r.moves, r.arcs, r.load * 100.0, r.breaks, r.ringed(7.0)
        );
    }
    let asked = |r: &LevelRead| r.arcs > 0 || r.ringed(7.0);
    assert!(
        reads.iter().any(|r| r.rho < 7.0 - 1e-6 && asked(r)),
        "a developed level reached the HSM planner (the old pin rang every one unnamed): {:?}",
        dev.notes
    );
    assert_refusals_named(&reads, 7.0);
    assert!(!dev.notes.iter().any(|n| n.contains("planning budget")), "a level is refused past the budget: {:?}", dev.notes);
    for rho in [6.0, 5.0] {
        assert!(reads.iter().any(|r| (r.rho - rho).abs() < 1e-6 && r.arcs > 0 && !r.ringed(7.0)), "radius {rho} is HSM-planned: {:?}", dev.notes);
    }
    let over = planner_overloads(&dev.notes);
    assert!(over.is_empty(), "the planner's own load reading broke the law: {over:?}");
    for r in reads.iter().filter(|r| r.arcs > 0) {
        assert!(r.load <= DEFAULT_OPTIMAL_LOAD * 1.35, "radius {}: load {:.1}% D", r.rho, r.load * 100.0);
        assert_eq!(r.breaks, 0, "radius {}: in-cut breaks past 5 deg", r.rho);
    }
}

/// Issue #55: a 10 long window of the neck in the r7 bar at a 2 mm
/// stepdown. Its centre area is a 3 mm band, where the plan narrowed
/// the r1 helix to 0.900 and then named its own overload (15.7 % D
/// against the 14.6 % law + 5 %). Now the wrap asks the entry law
/// first: the band is refused before planning, named by radius with
/// both helix radii, and Offset rings cut it - the planner never
/// narrows, and nothing names an overload. (The 5.0 level, the same
/// band, is refused the same way - round 2 ground 197 revolutions into
/// its planning budget first.) Rings only: under a second in debug.
#[test]
fn a_window_too_narrow_for_the_helix_is_refused_by_name_before_planning() {
    let (f, st) = (bar(), stock(7.0, -16.0, 16.0));
    let p = Pocket3DParams { top_z: 16.0, bottom_z: 6.0, stepdown: 2.0, ..params(PocketStrategy::Adaptive) };
    let dev = match rotary_offset_developed(&f, Some(&st), &p) {
        Ok(t) => t,
        Err(e) => panic!("the narrow window: {e}"),
    };
    assert!(dev.is_chained() && dev.all_finite());
    println!("narrow window notes: {:?}", dev.notes);
    assert!(!dev.notes.iter().any(|n| n.contains("does not fit the area")), "the planner narrowed a helix: {:?}", dev.notes);
    let tag = ring_tag(7.0);
    assert!(
        dev.notes.iter().any(|n| n.contains(NARROW_HELIX) && n.contains("radius 1.000 asked, 0.900 fits") && n.ends_with(&tag)),
        "the skin band is refused for the narrowed helix, by name: {:?}",
        dev.notes
    );
    let over = planner_overloads(&dev.notes);
    assert!(over.is_empty(), "the planner's own load reading broke the law: {over:?}");
    let reads = read_levels(&dev, 7.0, 0.0, p.stepdown);
    for r in &reads {
        println!("radius {:.3}: {} planar moves ({} arcs), rings {}", r.rho, r.moves, r.arcs, r.ringed(7.0));
    }
    assert!(reads.iter().any(|r| (r.rho - 7.0).abs() < 1e-6 && r.moves > 0 && r.arcs == 0), "rings cut the band");
    assert_refusals_named(&reads, 7.0);
}

/// Issues #55 and #982: on the app suites' r6.2 bar the neck's material
/// levels (5.7, 5.2) are bands ~78 x 7.8 (5.2: ~78 x 5.8) round the
/// developed neck. The spiral re-walks the whole cleared ring every
/// revolution, so only its microlift returns keep it in cut (unlifted it
/// rides ~8200 mm out of cut for ~2500 in cut, past the 1.5x budget,
/// and Offset rings cut the band at a full-width load). #55 pinned the
/// levels refused for travel: the lifted plans read 48.0 deg against the
/// 47.5 bound. #982 found that reading was a sliver the pieces' longer
/// chords left for the next piece - and the refusal a coin: with every
/// ring's opening 16, 62, 250 or 510 nm deeper one level or the other
/// was HSM-planned anyway. The pieces now take longer chords only where
/// those leave no sliver, so both levels read the spiral's own 47.5 and
/// are HSM-planned on microlift returns, with or without the nanometres:
/// nothing refused for travel, no planner overload named. Issue #1050:
/// both are held to the whole bar's laws (load within optimal + 35 %,
/// no in-cut break past 5 degrees) - the bore exit's conforming contact
/// is held on a coarse rim too and the closing laps' fillet kinks are
/// spread (`adaptive::conform`); they read 20.3 % D and 3 breaks
/// before. ~3 min debug: heavy lane.
#[test]
#[ignore = "heavy lane: release --ignored (three HSM plans over the r6.2 bar, ~3 min debug)"]
fn the_r62_neck_levels_are_hsm_planned_on_microlift_returns() {
    let (f, st) = (bar(), stock(6.2, -16.0, 16.0));
    let p = params(PocketStrategy::Adaptive);
    let dev = match rotary_offset_developed(&f, Some(&st), &p) {
        Ok(t) => t,
        Err(e) => panic!("the r6.2 developed ladder: {e}"),
    };
    assert!(dev.is_chained() && dev.all_finite());
    println!("r6.2 notes: {:?}", dev.notes);
    let reads = read_levels(&dev, 6.2, 0.0, p.stepdown);
    for r in &reads {
        println!(
            "radius {:.3}: {} planar moves ({} arcs), load {:.1}% D, breaks>5 {}, rings {}",
            r.rho, r.moves, r.arcs, r.load * 100.0, r.breaks, r.ringed(6.2)
        );
    }
    assert!(!dev.notes.iter().any(|n| n.contains("out of cut")), "a level is refused for travel: {:?}", dev.notes);
    for rho in [5.7, 5.2] {
        let Some(r) = reads.iter().find(|r| (r.rho - rho).abs() < 1e-6) else { panic!("radius {rho} is not cut: {:?}", dev.notes) };
        assert!(r.arcs > 0 && !r.ringed(6.2) && !r.named, "radius {rho} is HSM-planned, its proved arcs posted: {:?}", dev.notes);
        assert!(r.load <= DEFAULT_OPTIMAL_LOAD * 1.35, "radius {rho}: load {:.1}% D past the optimal + 35 %", r.load * 100.0);
        assert_eq!(r.breaks, 0, "radius {rho}: in-cut breaks past 5 deg");
    }
    // Each neck level says how it is kept in cut.
    let lifted = dev.notes.iter().filter(|n| n.contains("microlift returns") && n.contains("replace")).count();
    assert!(lifted >= 2, "both neck levels are cut on microlift returns: {:?}", dev.notes);
    assert!(reads.iter().any(|r| (r.rho - 6.2).abs() < 1e-6 && r.arcs > 0), "the skin level stays HSM-planned");
    assert_refusals_named(&reads, 6.2);
    let over = planner_overloads(&dev.notes);
    assert!(over.is_empty(), "the planner's own load reading broke the law: {over:?}");
}

/// FNV-1a over the moves at a 1e-6 lattice: the program, portable (the
/// trig under the wrap differs in its last bits across platforms' libm).
fn digest(t: &Toolpath) -> (usize, u64) {
    let q = |v: f64| (v * 1e6).round() as i64;
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for m in &t.moves {
        let arc = m.arc.map(|a| (q(a.center.x), q(a.center.y), a.ccw, format!("{:?}", a.plane)));
        let s = format!(
            "{:?}{:?}{:?}{:?}{:?}{:?}",
            m.kind,
            [q(m.from.x), q(m.from.y), q(m.from.z)],
            [q(m.to.x), q(m.to.y), q(m.to.z)],
            m.feed.map(q),
            arc,
            m.axis.map(|a| a.map(q))
        );
        for b in s.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    (t.moves.len(), h)
}

#[test]
fn offset_rings_on_the_wrap_are_the_program_they_always_were() {
    let (f, st) = (bar(), stock(6.2, -16.0, 16.0));
    let t = match rotary_offset_in(&f, Some(&st), &params(PocketStrategy::OffsetRings), &control()) {
        Ok(t) => t,
        Err(e) => panic!("rings: {e}"),
    };
    let got = digest(&t);
    println!("rings digest {got:?}");
    assert_eq!(got, GOLDEN_RINGS, "the OffsetRings wrap program moved");
}

/// Read on a build of db95014 WITHOUT the issue #9 patch (this
/// fixture, Helix entry): the rings path is untouched, byte for byte.
/// (Its Plunge twin read (5194, 6097177489831100083) both there and on
/// round 1's patched build.) Re-pinned on the 1e-6 lattice in the review
/// round: the bitwise digest (5263, 2919766036457321567) still matched
/// on the same build, so the lattice is the only change.
const GOLDEN_RINGS: (usize, u64) = (5263, 11_592_310_142_122_113_043);

/// Issue #983: the level reader reads the ladder's own planes only. A
/// microlift on the 5.0 level lifts to 5.6 and its down ramp passes a
/// vertex on 5.6 (5.7 -> 5.6 -> 5.0): read as a level, those lifted
/// runs looked like a ring-cut level no note named (radius 5.6 in #882's
/// variants). Now only 5.0 (named) is read, and the law holds.
#[test]
fn a_microlift_plane_is_not_read_as_an_unnamed_ring_level() {
    use camcraft_motion::cl::MoveKind;
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, 8.0));
    b.push(MoveKind::Plunge, Point3::new(0.0, 0.0, 5.0), Some(100.0));
    for (x, z) in [(10.0, 5.0), (12.0, 5.7), (14.0, 5.6), (20.0, 5.6), (22.0, 5.0), (30.0, 5.0)] {
        b.push(MoveKind::Feed, Point3::new(x, 0.0, z), Some(500.0));
    }
    // The 5.0 level itself posts lines: say it rings-cut, by name.
    b.note(format!("adaptive: the fixture - {}", ring_tag(5.0)));
    let path = b.finish();
    let reads = read_levels(&path, 7.0, 0.0, 0.5);
    let zs: Vec<f64> = reads.iter().map(|r| r.rho).collect();
    assert!(zs.iter().all(|z| (z - 5.0).abs() < 1e-9) && !zs.is_empty(), "only the ladder's 5.0 is read: {zs:?}");
    assert_refusals_named(&reads, 7.0);
}
