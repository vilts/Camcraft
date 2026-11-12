//! HSM adaptive clearing, measured (M11.4 C): the new planar Adaptive
//! against the Offset-rings baseline on five pockets - the user's R12.5
//! bore, a rectangle, an L, a pocket with an island and a narrow slot -
//! read by `camcraft_motion::quality` on the dexel stock (never by the
//! planner's own nominal step).
//!
//! Gate map (M4.4): covers camcraft_motion::machining2d (adaptive,
//! pocket_hsm, pocket), camcraft_motion::quality. The full-resolution
//! report is the heavy lane (`--ignored`, release).

use camcraft_motion::cl::Toolpath;
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_motion::machining2d::{PocketParams, PocketStrategy, pocket};
use camcraft_motion::quality::{PathQuality, QualitySpec, measure, rest_area, stock_for};
use camcraft_motion::verify::{Mode, ToolGeom, VerifyContext, Warning, crossed_pairs, crossed_pairs_all, verify};

const D: f64 = 10.0;
const BOTTOM: f64 = -5.0;

fn poly(pts: &[(f64, f64)]) -> Loop {
    let Ok(l) = Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()) else {
        panic!("fixture loop")
    };
    l
}

fn region(loops: Vec<Loop>) -> Region {
    let Ok(r) = Region::new(loops) else { panic!("fixture region") };
    r
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    poly(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
}

fn disc(cx: f64, cy: f64, r: f64) -> Loop {
    let n = 256;
    let pts: Vec<(f64, f64)> = (0..n)
        .map(|k| {
            let a = k as f64 * std::f64::consts::TAU / n as f64;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect();
    poly(&pts)
}

/// The five pockets of the M11.4 C report.
fn fixtures() -> Vec<(&'static str, Region)> {
    vec![
        ("bore R12.5 (user)", region(vec![disc(0.0, 0.0, 12.5)])),
        ("rectangle 60x40", region(vec![rect(0.0, 0.0, 60.0, 40.0)])),
        (
            "L-pocket 60x60-30x30",
            region(vec![poly(&[(0.0, 0.0), (60.0, 0.0), (60.0, 30.0), (30.0, 30.0), (30.0, 60.0), (0.0, 60.0)])]),
        ),
        ("island 60x40, 12x12", region(vec![rect(0.0, 0.0, 60.0, 40.0), rect(24.0, 14.0, 36.0, 26.0)])),
        ("narrow slot 14x60", region(vec![rect(0.0, 0.0, 60.0, 14.0)])),
        // Round 4: a 2.2 D channel, round enough for the old star ratio
        // (it overloaded there at 17.7%) but under the 1.4 D width law.
        ("channel 22x60", region(vec![rect(0.0, 0.0, 60.0, 22.0)])),
    ]
}

fn params(strategy: PocketStrategy) -> PocketParams {
    PocketParams {
        tool_diameter: D,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: Some(1.0),
        top_z: 0.0,
        bottom_z: BOTTOM,
        stepdown: 5.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        strategy,
        entry: Entry3D::Helix,
        helix_radius: 2.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        // M11.3: one entry per level, 2 mm plunge height, centre cutting.
        links: camcraft_motion::links::Links::PLANAR,
    }
}

/// The part the pocket is cut in, as the solid the verifier's solid law
/// (b2) sweeps the tool stack against: a block 10 mm past the region's
/// bounds from `BOTTOM - 5` to the top (Z 0), the region hollowed out of
/// it down to the floor at `BOTTOM` (islands standing). One closed shell,
/// outward-facing: block walls and bottom, the top cap (block less the
/// pocket), the cavity's walls on every region loop (facing into the
/// cavity - the loop's left) and its floor.
fn part(reg: &Region) -> Vec<Triangle> {
    let Some((lo, hi)) = reg.bounds() else { panic!("fixture bounds") };
    let (z0, z1) = (BOTTOM - 5.0, 0.0);
    let block = region(vec![rect(lo.x - 10.0, lo.y - 10.0, hi.x + 10.0, hi.y + 10.0)]);
    let Ok(top) = block.difference(reg) else { panic!("top cap") };
    let p3 = |q: Point2, z: f64| Point3::new(q.x, q.y, z);
    let mut f = Vec::new();
    // A cap at `z`, every triangle wound to face up (or down).
    let cap = |r: &Region, z: f64, up: bool, f: &mut Vec<Triangle>| {
        let Ok(tris) = r.triangles() else { panic!("cap triangles") };
        for [a, b, c] in tris {
            let ccw = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x) > 0.0;
            let (b, c) = if ccw == up { (b, c) } else { (c, b) };
            f.push(Triangle::new(p3(a, z), p3(b, z), p3(c, z)));
        }
    };
    cap(&block, z0, false, &mut f);
    cap(&top, z1, true, &mut f);
    cap(reg, BOTTOM, true, &mut f);
    // Walls: `(a0, b0, b1)` faces the right of travel a -> b.
    let wall = |l: &Loop, za: f64, zb: f64, right: bool, f: &mut Vec<Triangle>| {
        let pts = l.points();
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            let (a0, b0, b1, a1) = (p3(a, za), p3(b, za), p3(b, zb), p3(a, zb));
            if right {
                f.push(Triangle::new(a0, b0, b1));
                f.push(Triangle::new(a0, b1, a1));
            } else {
                f.push(Triangle::new(a0, b1, b0));
                f.push(Triangle::new(a0, a1, b1));
            }
        }
    };
    for l in block.loops() {
        wall(l, z0, z1, l.is_ccw(), &mut f);
    }
    for l in reg.loops() {
        wall(l, BOTTOM, z1, false, &mut f);
    }
    f
}

/// The depth-aware verifier on a planar pocket path (the area law the
/// operation layer runs), with the solid law (b2) judging the tool
/// stack against the part's solid ([`part`]): every warning it raises.
fn verified(path: &Toolpath, reg: &Region, tool_d: f64) -> Vec<Warning> {
    let solid = part(reg);
    let tool = ToolGeom {
        radius: tool_d * 0.5,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: tool_d * 0.5,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    };
    let cx = VerifyContext {
        region: reg,
        stock: None,
        model: Some(&solid),
        tool: &tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 0.0,
        clearance_z: 5.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(path, &cx)
}

/// Warnings by kind, for the report line.
fn warn_kinds(w: &[Warning]) -> String {
    let mut kinds: Vec<(String, usize)> = Vec::new();
    for x in w {
        let k = format!("{x:?}").split([' ', '{']).next().unwrap_or("?").to_owned();
        match kinds.iter_mut().find(|e| e.0 == k) {
            Some(e) => e.1 += 1,
            None => kinds.push((k, 1)),
        }
    }
    format!("{kinds:?}")
}

fn run(reg: &Region, strategy: PocketStrategy, cell: f64) -> (Toolpath, PathQuality, f64) {
    let t0 = std::time::Instant::now();
    let path = match pocket(reg, &params(strategy)) {
        Ok(p) => p,
        Err(e) => panic!("{strategy:?}: {e}"),
    };
    if std::env::var("HSM_DUMP").is_ok() {
        eprintln!("{strategy:?} generated in {:.2} s", t0.elapsed().as_secs_f64());
    }
    let mut spec = QualitySpec::new(D);
    spec.cell = cell;
    // The acceptance law reads breaks past 5 degrees in cut.
    spec.break_deg = 5.0;
    // The stock stands one stepdown above Top: the ladder's first level
    // (at Top) is a real cut, not the zero-depth skin.
    let Ok(mut dex) = stock_for(reg, 5.0, BOTTOM, 1.0, &spec) else { panic!("stock") };
    let q = measure(&path, &mut dex, &spec).unwrap();
    if let Ok(dir) = std::env::var("HSM_DUMP") {
        dump(&dir, reg, &path, &q, strategy);
    }
    // Rest inside what a D10 can reach, a grid cell and a half in.
    let reach = reg.offset(-(D * 0.5)).and_then(|c| c.offset(D * 0.5 - 2.0 * cell));
    let Ok(reach) = reach else { panic!("reach") };
    (path, q, rest_area(&dex, &reach, BOTTOM).unwrap())
}

/// Debug aid (opt-in by `HSM_DUMP=<dir>`): the region's loops and every
/// move's XY chords with its peak contact angle, one text file per run.
fn dump(dir: &str, reg: &Region, path: &Toolpath, q: &PathQuality, strategy: PocketStrategy) {
    use std::fmt::Write as _;
    let mut s = String::new();
    for l in reg.loops() {
        let pts = l.points();
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            let _ = writeln!(s, "W {} {} {} {}", a.x, a.y, b.x, b.y);
        }
    }
    for (i, m) in path.moves.iter().enumerate() {
        let k = match m.kind {
            camcraft_motion::cl::MoveKind::Rapid => 'R',
            camcraft_motion::cl::MoveKind::Feed => 'F',
            camcraft_motion::cl::MoveKind::Plunge => 'P',
        };
        let deg = q.move_max.get(i).copied().unwrap_or(0.0);
        let air = q.move_air.get(i).copied().unwrap_or(0.0);
        let len: f64 = m.chords(0.01).iter().map(|(a, b)| (b.x - a.x).hypot(b.y - a.y)).sum();
        // A move mostly in air is drawn as air.
        let deg = if air > 0.5 * len { 0.0 } else { deg };
        for (a, b) in m.chords(0.01) {
            let _ = writeln!(s, "{k} {} {} {} {} {} {deg}", a.x, a.y, b.x, b.y, m.to.z);
        }
    }
    for a in &q.air_at {
        let _ = writeln!(s, "A {} {} {}", a.x, a.y, a.z);
    }
    let n = reg.area().round() as i64;
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(format!("{dir}/{n}_{strategy:?}.txt"), s);
}

/// The fixtures the plan holds every law on: the round star cells
/// (round 2) and, since round 3, the 1.4 D slot too - grown as one ring
/// family (`adaptive::grow`), never the Offset rings' fallback. The
/// island and the channel are owed (round 5): their lifted plan runs
/// past the hop or rest budget and falls back to the Offset rings BY
/// NAME, which the report prints as a GAP.
const HELD: [&str; 4] = ["bore", "rectangle", "L-pocket", "narrow slot"];

/// Round 4: the fixtures cut with microlift returns, and the practicality
/// law they are held to - out-of-cut travel (floor air plus lifted
/// returns, `quality`'s air and lift) at most this share of the cut, and
/// at most [`MOVES_MAX`] moves for the whole program.
const LIFTED: [&str; 4] = ["L-pocket", "island", "narrow slot", "channel"];
const NONCUT_PER_CUT: f64 = 1.5;
const MOVES_MAX: usize = 120_000;

/// The M11.4 C acceptance law, per fixture, measured: (1) the load
/// within 10% of the optimal load, (2) no fallback, (3) no direction
/// change past 5 degrees in cut, (4) air against twice the rings'
/// (reported), (5) the verifier silent and the rest only the named
/// corner scallop; and one descent into stock per level, never a slot.
/// Returns the broken laws by name; prints the report when asked.
fn judge(name: &str, reg: &Region, (cell, tol): (f64, f64), print: bool) -> Vec<String> {
    let (base_path, base, base_rest) = run(reg, PocketStrategy::OffsetRings, cell);
    let (path, new, new_rest) = run(reg, PocketStrategy::Adaptive, cell);
    let warns = verified(&path, reg, D);
    let fell = path.notes.iter().any(|n| n.contains("Offset rings cut this pocket instead"));
    let air_ok = new.air_len <= 3.0 * base.air_len;
    let noncut = new.air_len + new.lift_len;
    if print {
        println!("{}", base.report_line(&format!("{name} / rings")));
        println!("    rest {base_rest:.2} mm2, verify {}, moves {}", warn_kinds(&verified(&base_path, reg, D)), base_path.moves.len());
        println!("{}", new.report_line(&format!("{name} / HSM")));
        println!("    rest {new_rest:.2} mm2, verify {} {}, moves {}", warns.len(), warn_kinds(&warns), path.moves.len());
        println!(
            "    ROW {name}: air {:.0} lift {:.0} out-of-cut {:.0} cut {:.0} mm (x{:.2}) | rings air {:.0} (air x{:.1} of rings) | moves {} | breaks>5 {} | load {:.1}%",
            new.air_len,
            new.lift_len,
            noncut,
            new.cut_len,
            noncut / new.cut_len.max(1e-9),
            base.air_len,
            new.air_len / base.air_len.max(1e-9),
            path.moves.len(),
            new.breaks,
            new.max_load() * 100.0
        );
        println!("    notes: {:?}", path.notes);
        if !air_ok {
            println!("    GAP {name}: floor air {:.0} mm past 3x the rings' {:.0} mm", new.air_len, base.air_len);
        }
    }
    let mut bad = Vec::new();
    if !warns.is_empty() {
        bad.push(format!("{name}: verifier {}", warn_kinds(&warns)));
    }
    // Round 4: the bucketed crossing law reports the all-pairs law's
    // move pairs exactly, on both plans.
    for (who, p) in [("rings", &base_path), ("HSM", &path)] {
        if crossed_pairs(p, 0.01) != crossed_pairs_all(p, 0.01) {
            bad.push(format!("{name} / {who}: the bucketed crossing law differs from all pairs"));
        }
    }
    // Round 4's practicality law on the lifted plans: out of cut (floor
    // air and lifted returns) at most the budget's share of the cut, and
    // a bounded program.
    if LIFTED.iter().any(|h| name.starts_with(h)) && !fell {
        if noncut > NONCUT_PER_CUT * new.cut_len {
            bad.push(format!("{name}: {noncut:.0} mm out of cut past {NONCUT_PER_CUT}x the {:.0} mm cut", new.cut_len));
        }
        if path.moves.len() > MOVES_MAX {
            bad.push(format!("{name}: {} moves past {MOVES_MAX}", path.moves.len()));
        }
    }
    if fell {
        if print {
            println!("    GAP {name}: HSM fell back to Offset rings");
        }
        if HELD.iter().any(|h| name.starts_with(h)) {
            bad.push(format!("{name}: fell back to Offset rings"));
        }
        return bad;
    }
    if new.descents != 2 {
        bad.push(format!("{name}: {} descents into stock for 2 levels", new.descents));
    }
    if new.slot_len > 0.0 {
        bad.push(format!("{name}: {:.1} mm of slotting", new.slot_len));
    }
    if new.max_load() > DEFAULT_OPTIMAL_LOAD * (1.0 + tol) {
        bad.push(format!("{name}: load {:.1}% D past {:.1}% + {:.0}%", new.max_load() * 100.0, DEFAULT_OPTIMAL_LOAD * 100.0, tol * 100.0));
    }
    // The 5 deg law is measured honestly since round 4 (main's report had
    // read 10 deg under a "5 deg" label). The slot's two seam kinks (the
    // concentric revolutions handing over to the band's, 5.0 deg) rode a
    // RATCHET (RECOMMENDED "M11.4 C round 5" (3)) until the growth
    // revolutions landed on their rings tangentially (`rings::LAND`):
    // the ratchet is spent, every fixture is held at zero. (The table
    // stays for the next owed count; it may only tighten.)
    const KINK_RATCHET: &[(&str, usize, f64)] = &[];
    let (cap, worst_cap) = KINK_RATCHET
        .iter()
        .find(|(p, _, _)| name.starts_with(p))
        .map_or((0, 0.0), |&(_, c, w)| (c, w));
    if new.breaks > cap || (new.breaks > 0 && new.worst_break_deg > worst_cap) {
        bad.push(format!(
            "{name}: {} in-cut breaks past 5 deg, worst {:.1} (ratchet: <= {cap}, worst <= {worst_cap:.2})",
            new.breaks, new.worst_break_deg
        ));
    }
    // Rest: grid crumbs, or the corner scallop the plan named.
    let scallop = path.notes.iter().any(|n| n.contains("corner scallop"));
    if new_rest > 0.001 * reg.area() && !(scallop && new_rest <= 0.01 * reg.area()) {
        bad.push(format!("{name}: {new_rest:.1} mm2 rest (scallop named: {scallop})"));
    }
    if !(path.is_chained() && path.all_finite()) {
        bad.push(format!("{name}: path not chained / finite"));
    }
    bad
}

#[test]
#[ignore = "heavy lane: the M11.4 C metric report on five fixtures at full resolution (release)"]
fn hsm_report_full_resolution() {
    let mut bad = Vec::new();
    for (name, reg) in fixtures() {
        bad.extend(judge(name, &reg, (D / 120.0, 0.10), true));
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn hsm_on_the_user_bore_enters_once_and_holds_every_law() {
    let (name, reg) = fixtures().swap_remove(0);
    // The debug lane measures on a grid twice as coarse as the plan's:
    // its rim reads a cell and a half further out, the load law is the
    // heavy lane's (10%), this one only catches a regression.
    let bad = judge(name, &reg, (D / 60.0, 0.35), false);
    assert!(bad.is_empty(), "{bad:#?}");
}

/// Issue #850: the island's growth plan never finishes; it ran about a
/// minute (release) before the length budget refused it. The geometry
/// budget refuses it about halfway into its first margin attempt, by
/// name, and the Offset rings cut it, named - verifier silent. The
/// count in the note is deterministic: past the 600 k budget by at most
/// one revolution (~6 k on the island). The wall clock is a loose
/// backstop only, so a loaded machine cannot fail it. Its fast twin in
/// the default suite (the same refusal at lowered limits) is
/// `adaptive::budget::fixtures::the_island_trips_lowered_geometry_limits_by_name`.
#[test]
#[ignore = "heavy lane: release --ignored (the island grown to the 600 k geometry budget, 46-59 s debug)"]
fn the_island_is_refused_by_the_geometry_budget_in_seconds() {
    let (_, reg) = fixtures().swap_remove(3);
    let t0 = std::time::Instant::now();
    let Ok(t) = pocket(&reg, &params(PocketStrategy::Adaptive)) else { panic!("pocket") };
    let secs = t0.elapsed().as_secs_f64();
    eprintln!("island: {secs:.1} s, {:?}", t.notes);
    let Some(n) = t.notes.iter().find(|n| n.contains("too detailed") && n.contains("Offset rings cut this pocket instead")) else {
        panic!("refused by the geometry budget, by name: {:?}", t.notes)
    };
    let k = n.split("both limits passed, ~").nth(1).and_then(|s| s.split(" k").next()).and_then(|s| s.parse::<f64>().ok());
    assert!(k.is_some_and(|k| (600.0..=620.0).contains(&k)), "the count past the budget by at most a revolution: {n}");
    assert!(!t.moves.is_empty() && t.is_chained() && t.all_finite(), "the Offset rings cut it");
    let w = verified(&t, &reg, D);
    assert!(w.is_empty(), "{} {:?}", warn_kinds(&w), t.notes);
    let limit = if cfg!(debug_assertions) { 600.0 } else { 60.0 };
    assert!(secs < limit, "{secs:.1} s");
}

/// Issue #5 round 2: the channel the spiral refuses is tried as a
/// trochoid next (spiral -> trochoid -> rings), and what keeps the
/// trochoid off - its lifted returns land on loop starts already in cut,
/// one descent into stock each loop - is named before the Offset rings
/// cut it; the program verifies clean.
#[test]
#[ignore = "heavy lane: the channel spiral, trochoid and rings planned in turn (80 s debug; release)"]
fn the_channel_names_the_trochoid_fallback_before_the_rings() {
    let (_, reg) = fixtures().swap_remove(5);
    let Ok(t) = pocket(&reg, &params(PocketStrategy::Adaptive)) else { panic!("pocket") };
    assert!(
        t.notes.iter().any(|n| n.contains("a trochoid") && n.contains("lands on a loop start") && n.contains("Offset rings cut this pocket instead")),
        "{:?}",
        t.notes
    );
    let w = verified(&t, &reg, D);
    assert!(w.is_empty(), "{} {:?}", warn_kinds(&w), t.notes);
}

/// The merged-batch gate's case (`level_links`, M11.2): a 60 x 40
/// Adaptive pocket, Helix entry at a 1 mm radius, three levels - the
/// verifier silent (the HSM path never crosses itself) and exactly one
/// helix per level, at the asked radius. The verifier carries the solid
/// law (b2) against the part ([`part`]).
#[test]
#[ignore = "heavy lane: HSM adaptive generation"]
fn a_small_helix_rectangle_never_crosses_itself() {
    let reg = region(vec![rect(0.0, 0.0, 60.0, 40.0)]);
    let mut p = params(PocketStrategy::Adaptive);
    p.helix_radius = 1.0;
    p.bottom_z = -4.0;
    p.stepdown = 2.0;
    p.retract_z = Some(2.0);
    let Ok(t) = pocket(&reg, &p) else { panic!("pocket") };
    assert!(!t.notes.iter().any(|n| n.contains("Offset rings")), "{:?}", t.notes);
    let w = verified(&t, &reg, D);
    assert!(w.is_empty(), "{} {:?}", warn_kinds(&w), t.notes);
    let mut helices = 0;
    let mut inside = false;
    for m in &t.moves {
        let down_arc = m.arc.is_some() && m.to.z < m.from.z;
        if down_arc {
            let c = m.arc.map(|a| a.center).unwrap_or_default();
            assert!(((m.from.x - c.x).hypot(m.from.y - c.y) - 1.0).abs() < 1e-3, "the asked radius");
        }
        helices += usize::from(down_arc && !inside);
        inside = down_arc;
    }
    assert_eq!(helices, 3, "one helix per level: {:?}", t.notes);
}

/// Debug aid: one fixture (`HSM_ONE=<name prefix>`), HSM only.
#[test]
#[ignore = "debug aid: one fixture by HSM_ONE, release"]
fn hsm_one() {
    let want = std::env::var("HSM_ONE").unwrap_or_default();
    for (name, reg) in fixtures().into_iter().filter(|(n, _)| n.starts_with(want.as_str())) {
        let (path, q, rest) = run(&reg, PocketStrategy::Adaptive, D / 120.0);
        let w = verified(&path, &reg, D);
        println!("{}", q.report_line(name));
        let arcs = path.moves.iter().filter(|m| m.arc.is_some()).count();
        println!("    rest {rest:.2} mm2, moves {} (arcs {arcs}), verify {}, notes: {:?}", path.moves.len(), warn_kinds(&w), path.notes);
        let mut br: Vec<String> = q.break_list.iter().take(40).map(|(p, t)| format!("({:.2},{:.2},{:.1}) {t:.1}", p.x, p.y, p.z)).collect();
        br.truncate(40);
        println!("    breaks: {br:?}");
    }
}
