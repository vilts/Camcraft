//! M11.3 owed ("3D ladder / contour bore second dives"; RECOMMENDED "M11.3
//! owed: the centre-cutting law on every plunge" (3)): the user's rule is
//! one entry per level - "the first entry should be enough, and the linking
//! should go from that entry into the circles". The 3-D ladder's level
//! entry and the bore-side contour's helix each land ON the first pass or
//! link into it at depth: every level has exactly ONE descent into stock,
//! read off the quality report's descent counter against a block of stock
//! (a descent counts when it bites material), and verify is clean.
//!
//! Gate map (M11.3): covers camcraft_motion::machining3d (descent,
//! level_ctx, ladder), camcraft_motion::machining2d (contour_bore,
//! contour_step, descent_dive, lead_fit), camcraft_motion::quality.

use camcraft_motion::approach::{Approach, CENTER_CUT_REFUSAL, DEFAULT_PLUNGE_HEIGHT};
use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::links::{Link, Links};
use camcraft_motion::machining2d::{ContourParams, Lead, PocketStrategy, Side, contour};
use camcraft_motion::machining3d::{Pocket3DParams, pocket3d};
use camcraft_motion::quality::{QualitySpec, measure};
use camcraft_motion::stock_sim::{Backend, Stock};
use camcraft_motion::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).unwrap()
}

fn round(r: f64) -> Region {
    let pts = (0..256)
        .map(|k| {
            let t = std::f64::consts::TAU * f64::from(k) / 256.0;
            Point2::new(r * t.cos(), r * t.sin())
        })
        .collect();
    Region::new(vec![Loop::new(pts).unwrap()]).unwrap()
}

fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| Point3::new([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1]);
    let quad = |a: usize, b: usize, c: usize, d: usize| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
    let mut t = Vec::with_capacity(12);
    for q in [quad(4, 5, 7, 6), quad(0, 2, 3, 1), quad(0, 1, 5, 4), quad(2, 6, 7, 3), quad(1, 3, 7, 5), quad(2, 0, 4, 6)] {
        t.extend(q);
    }
    t
}

fn slab() -> Vec<Triangle> {
    box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, -10.0])
}

fn slab_with_boss() -> Vec<Triangle> {
    let mut f = slab();
    f.extend(box_facets([-5.0, -5.0, -10.0], [5.0, 5.0, 0.0]));
    f
}

fn tool(r: f64) -> ToolGeom {
    ToolGeom { radius: r, nose_radius: 0.0, flute_len: 30.0, shank_radius: r, shank_len: 40.0, holder_radius: 20.0, holder_len: 60.0 }
}

fn check(t: &Toolpath, region: &Region, model: Option<&[Triangle]>, mode: Mode, r: f64, clearance_z: f64) -> Vec<Warning> {
    let tool = tool(r);
    let cx = VerifyContext {
        region,
        stock: None,
        model,
        tool: &tool,
        cutter: None,
        mode,
        stock_top_z: 0.0,
        clearance_z,
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

/// The path's descents into stock on each level's ENTRY - the moves from
/// the previous level's last planar cut to this level's first - read by
/// the quality report's descent counter against a block of stock over
/// `region` (its top half a plunge height over the cut's top, so the
/// first level's descent onto Z 0 bites too), the whole path simulated
/// in order. The pass links inside a level (row restarts around a boss)
/// are the between-pass law's, counted apart.
fn descents_per_level(t: &Toolpath, region: &Region, d: f64) -> Vec<(f64, usize, usize)> {
    let (lo, hi) = region.bounds().expect("bounds");
    let top = DEFAULT_PLUNGE_HEIGHT * 0.5;
    let mut dex = Stock::block(Backend::Column, lo.x - d, lo.y - d, hi.x + d, hi.y + d, top, d / 40.0).expect("stock");
    let spec = QualitySpec::new(d);
    let planar = |m: &camcraft_motion::cl::Move| m.kind == MoveKind::Feed && (m.to.z - m.from.z).abs() <= 1e-9;
    // Each level's first and last planar cut.
    let mut cuts: Vec<(f64, usize, usize)> = Vec::new();
    for (i, m) in t.moves.iter().enumerate().filter(|(_, m)| planar(m)) {
        match cuts.last_mut() {
            Some(l) if l.0 == m.to.z => l.2 = i,
            _ => cuts.push((m.to.z, i, i)),
        }
    }
    let count = |dex: &mut Stock, a: usize, b: usize| {
        let piece = Toolpath { moves: t.moves[a..b].to_vec(), cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None };
        measure(&piece, dex, &spec).unwrap().descents
    };
    let mut out = Vec::new();
    let mut start = 0usize;
    for &(z, first, last) in &cuts {
        let entry = count(&mut dex, start, first + 1);
        let passes = count(&mut dex, first + 1, last + 1);
        out.push((z, entry, passes));
        start = last + 1;
    }
    out
}

fn p3(entry: Entry3D, strategy: PocketStrategy, links: Links) -> Pocket3DParams {
    Pocket3DParams {
        links,
        tool_diameter: 8.0,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        top_z: 0.0,
        bottom_z: -10.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy,
        retract_z: Some(3.0),
        entry,
        helix_radius: 2.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
    }
}

#[test]
fn the_3d_ladder_descends_into_each_level_once() {
    let footprint = Region::new(vec![rect(-20.0, -20.0, 20.0, 20.0)]).unwrap();
    let mut bad = Vec::new();
    for (fname, facets) in [("slab", slab()), ("boss", slab_with_boss())] {
        for strategy in [PocketStrategy::Zigzag, PocketStrategy::Waterline] {
            for entry in [Entry3D::Helix, Entry3D::Ramp, Entry3D::Arc, Entry3D::Plunge] {
                for (ln, links) in [
                    ("rough3d", Links::ROUGH3D),
                    ("retract", Links { pass: Link::Retract, level: Link::Retract, ..Links::ROUGH3D }),
                    ("keep down", Links { level: Link::KeepDown, ..Links::ROUGH3D }),
                ] {
                    let what = format!("{fname} {strategy:?} {entry:?} {ln}");
                    let t = pocket3d(&facets, &footprint, &p3(entry, strategy, links)).expect("3d");
                    let w = check(&t, &footprint, Some(&facets), Mode::Area, 4.0, 5.0);
                    let per = descents_per_level(&t, &footprint, 8.0);
                    println!("{what}: {per:?} warns {} notes {:?}", w.len(), t.notes);
                    if !w.is_empty() {
                        bad.push(format!("{what}: verify {w:?}"));
                    }
                    for (z, n, _) in &per {
                        if *n != 1 {
                            bad.push(format!("{what}: level {z}: {n} descents into stock on the entry ({:?})", t.notes));
                        }
                    }
                }
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

fn bore(entry: Entry3D, lead: Lead, links: Links) -> ContourParams {
    ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 25.0,
        retract_z: 15.0,
        top_z: 0.0,
        bottom_z: -6.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Inside,
        lead,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry,
        helix_radius: 1.5,
        blend_turns: false,
        links,
    }
}

#[test]
fn the_bore_contour_descends_into_each_level_once() {
    let hole = round(10.0);
    let mut bad = Vec::new();
    for (lname, lead) in [("none", Lead::None), ("arc", Lead::Arc { radius: 2.0 }), ("linear", Lead::Linear { length: 2.0 })] {
        for entry in [Entry3D::Helix, Entry3D::Plunge, Entry3D::Ramp, Entry3D::Arc] {
            for (ln, links) in [
                ("planar", Links::PLANAR),
                ("retract", Links { level: Link::Retract, ..Links::PLANAR }),
                ("helix transition", Links { transition: Some(Entry3D::Helix), helix_pitch: 1.0, ..Links::PLANAR }),
            ] {
                let what = format!("bore lead {lname} {entry:?} {ln}");
                let t = contour(&hole, &bore(entry, lead, links)).expect("bore contour");
                let w = check(&t, &hole, None, Mode::Area, 3.0, 25.0);
                let per = descents_per_level(&t, &hole, 6.0);
                println!("{what}: {per:?} warns {} notes {:?}", w.len(), t.notes);
                if !w.is_empty() {
                    bad.push(format!("{what}: verify {w:?}"));
                }
                let named = t.notes.iter().any(|n| n.contains("second dive"));
                for (z, n, _) in &per {
                    if *n != 1 {
                        bad.push(format!("{what}: level {z}: {n} descents into stock on the entry ({:?})", t.notes));
                    }
                }
                if named {
                    bad.push(format!("{what}: a second dive where the ring has room: {:?}", t.notes));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn the_linear_lead_bore_lands_its_one_descent_on_the_lead_start() {
    // #14: the linear lead's tangent start once stood 4.5 um outside the
    // eroded disc (inside the lead fit's slack), so no at-depth link to it
    // could be proved and a named second dive followed. The start is now
    // seated inside: no second dive, and a tool that cannot plunge never
    // plunges into stock either.
    let hole = round(10.0);
    let p = bore(Entry3D::Helix, Lead::Linear { length: 2.0 }, Links::PLANAR);
    let t = contour(&hole, &p).expect("bore contour");
    assert!(!t.notes.iter().any(|n| n.contains("second dive")), "{:?}", t.notes);
    let links = Links { approach: Approach { center_cutting: false, ..Links::PLANAR.approach }, ..Links::PLANAR };
    match contour(&hole, &bore(Entry3D::Helix, Lead::Linear { length: 2.0 }, links)) {
        Ok(t) => assert!(!t.moves.iter().any(|m| m.kind == MoveKind::Plunge && m.to.z < 0.0), "{:?}", t.notes),
        Err(e) => assert!(format!("{e:?}").contains(CENTER_CUT_REFUSAL), "{e:?}"),
    }
}
