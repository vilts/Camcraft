//! The wall-side arc entry and the level-to-level transition of an
//! outside contour (M11.1 follow-up to a hand-test report): the user's
//! own part (`testdata/models/Milling-test.step`, a D-shaped plate with
//! a blind bore) contoured by its whole section with a tangential lead
//! and Entry = Arc, regenerated through the commands; and a single-ring
//! wall to pin the level-to-level link - retract and re-enter by
//! default, stay down and ramp when the user picks it (`links::Links`).
//! Every path must verify silent - against the part SOLID, which is how
//! the blind bore's ring below its floor stopped passing (the depth law).
//!
//! Gate map (M11.1): covers app::commands, motion::machining2d (contour
//! wall arc, seam leads, level link, depth ladder), app::operation,
//! sim::verify, cl::cl.

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::geometry::Point3;
use camcraft_app::machining2d::{contour, ContourParams, Lead, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::{Mode, ToolGeom, VerifyContext, verify};
use camcraft_app::world::new_world;

/// The reported op: Contour, EM-1000-4FL, clearance stock top +20.5,
/// retract stock top +13.5, top = model top, bottom = Z -7 (the report's
/// work offset; here the stock bottom, which resolves to the same Z),
/// stepdown 5, leave 0, blends on, tangential lead r5, Entry = Arc,
/// no tabs, feed 1200 / plunge 300, climb.
fn user_op() -> OpParams {
    OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 20.5),
        retract_z: Height::of(HeightReference::TopOfStock, 13.5),
        top_z: Height::of(HeightReference::TopOfModel, 0.0),
        bottom_z: Height::of(HeightReference::BottomOfStock, 0.0),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::Tangential { radius: 5.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Arc,
        helix_radius: 1.0,
        blend_turns: true,
        tilt: None,
        links: camcraft_app::links::Links::PLANAR,
    }
}

/// Import the part, a 70 x 70 block from Z -7 to the part's top (Z 20),
/// the tool, the op - and generate it, all through commands.
fn user_job() -> (Toolpath, usize) {
    user_job_with(user_op())
}

fn user_job_with(params: OpParams) -> (Toolpath, usize) {
    let world = new_world();
    let mut h = History::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    let imp = ImportModel::new(&path).expect("the user's STEP imports");
    let name = imp.name.clone();
    h.execute(&world, imp);
    let loc = Location { pos: Point3::new(0.0, 0.0, -7.0), ..Default::default() };
    let stock = SetStock::new(&world, StockShape::Box { dx: 70.0, dy: 70.0, dz: 27.0 }, loc);
    h.execute(&world, stock.expect("block stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-1000-4FL").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "EM-1000-4FL", tool).expect("tool"));
    let op = CreateOperation::new(&world, "C1", "EM-1000-4FL", SrcRef::Entity(name), params);
    h.execute(&world, op.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "C1").expect("generate"));
    let e = model::find_by_name(&world, "C1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    (tp.path.clone(), tp.warnings.len())
}

/// An arc entry's chord: a feed that descends while it moves in XY.
fn is_chord(m: &Move) -> bool {
    m.kind == MoveKind::Feed && m.to.z < m.from.z && (m.to.x - m.from.x).hypot(m.to.y - m.from.y) > 1e-9
}

/// The arc entries: (index of the first chord, index past the last).
fn fans(t: &Toolpath) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < t.moves.len() {
        if is_chord(&t.moves[i]) {
            let s = i;
            while i < t.moves.len() && is_chord(&t.moves[i]) {
                i += 1;
            }
            out.push((s, i));
        } else {
            i += 1;
        }
    }
    out
}

/// A readable token per move: kind and what it does to Z.
fn token(m: &Move) -> &'static str {
    let xy = (m.to.x - m.from.x).hypot(m.to.y - m.from.y) > 1e-9;
    match (m.kind, m.to.z.total_cmp(&m.from.z), xy) {
        (MoveKind::Rapid, std::cmp::Ordering::Greater, _) => "rapid-up",
        (MoveKind::Rapid, std::cmp::Ordering::Less, false) => "rapid-down",
        (MoveKind::Rapid, _, _) => "rapid-across",
        (MoveKind::Plunge, _, _) => "plunge",
        (MoveKind::Feed, std::cmp::Ordering::Less, true) => "arc-chord",
        (MoveKind::Feed, std::cmp::Ordering::Equal, _) => "feed",
        (MoveKind::Feed, _, _) => "feed-z",
    }
}

/// The moves that carry the tool from the last planar cut before `at`
/// to the first chord at `at`, as tokens.
fn approach(t: &Toolpath, at: usize) -> Vec<&'static str> {
    let mut s = at;
    while s > 0 && !(t.moves[s - 1].kind == MoveKind::Feed && t.moves[s - 1].from.z == t.moves[s - 1].to.z) {
        s -= 1;
    }
    t.moves[s..at].iter().map(token).collect()
}

fn unit(x: f64, y: f64) -> (f64, f64) {
    let n = x.hypot(y);
    (x / n, y / n)
}

/// XY travel direction of `m` at its start (a line's chord, an arc's
/// tangent) and at its end.
fn dir_at(m: &Move, start: bool) -> (f64, f64) {
    match m.arc {
        None => unit(m.to.x - m.from.x, m.to.y - m.from.y),
        Some(a) => {
            let p = if start { m.from } else { m.to };
            let (rx, ry) = (p.x - a.center.x, p.y - a.center.y);
            if a.ccw { unit(-ry, rx) } else { unit(ry, -rx) }
        }
    }
}

fn dot(a: (f64, f64), b: (f64, f64)) -> f64 {
    a.0 * b.0 + a.1 * b.1
}

#[test]
fn the_reported_outside_contour_enters_every_level_on_one_arc() {
    let (t, warnings) = user_job();
    assert!(t.is_chained() && t.all_finite());
    // The verifier speaks silent: no gouge, no crossing - the 35 the
    // report carried were 14 lead gouges in the bore ring and 21
    // lead-in/lead-out crossings at the two seams.
    assert_eq!(warnings, 0, "{:?}", t.notes);
    let summary = t.status_summary("C1", warnings, Default::default());
    assert!(summary.contains("7 level(s)"), "{summary}");
    let notes = t.notes.join(" | ");
    assert!(!notes.contains("needs bounded air"), "the arc is no longer refused on the wall: {notes}");
    // The seams: no lead-in and lead-out left crossing - the seam law
    // writes "kept as planned" only when it could not part a crossing
    // pair, so its absence is the no-crossing-left check. Before #689 the
    // rings' fitted walks ended on a chord whose tangent kinked at the
    // seam, and the lead-out was re-solved to close on the lead-in's
    // start; a closed ring now fits whole, so the tangents agree and the
    // leads part without the re-solve - either way none is kept crossed.
    assert!(!notes.contains("kept as planned"), "{notes}");
    // The bore ring of the section takes no arc beside its wall: held
    // below (`bore` is empty). On the coarse mesh before #689 it was
    // refused by name ("no arc fits beside the wall"); on the capped
    // mesh that refusal no longer arises, and the note was never the law.
    // M11.2 B: the outer ring's lead-in starts about 4.4 deg off the X
    // axis - an oblique vertical plane, which no G17/G18/G19 names - so
    // its entries stay certified chord fans, and the path says so once.
    let oblique = "entry: oblique vertical arc posted as chords - no standard plane";
    assert_eq!(t.notes.iter().filter(|n| *n == oblique).count(), 1, "{notes}");
    assert!(!t.moves.iter().any(|m| m.arc.is_some_and(|a| !a.plane.is_xy())), "no axis-plane entry here");

    let levels = [20.0, 15.0, 10.0, 5.0, 0.0, -5.0, -7.0];
    // The outer ring's entries stand beyond the part's R30 arc; the bore
    // ring's (inside R12.5) are counted apart.
    let outer = |m: &Move| m.to.x.hypot(m.to.y) > 30.0;
    let all = fans(&t);
    let f: Vec<_> = all.iter().copied().filter(|&(s, _)| outer(&t.moves[s])).collect();
    assert_eq!(f.len(), levels.len(), "one arc entry per level on the outer ring");
    // The bore ring is BLIND (floor Z 0): it is walked at 20..0 and
    // never below - the old ladder walked it at -5 and -7 too, an
    // annulus cut into the solid under the floor that the flat-region
    // verifier could not see. On the coarse mesh its five 5 mm drops
    // found no arc room in the bore's air and grazed by name; on the
    // capped mesh (#689), with the ring fitted whole, a proved arc fits
    // beside the bore wall. Either way an entry there lands on one of the
    // bore's own levels, never below its floor, and the verifier above
    // holds every one of them silent.
    let bore: Vec<_> = all.iter().filter(|&&(s, _)| !outer(&t.moves[s])).collect();
    // Five: one proved arc entry per bore level (20, 15, 10, 5, 0), each
    // a chord fan of an arc entry - no graze, no warning (the verifier
    // silence above covers them: none is a gouge or a crossing).
    assert_eq!(bore.len(), 5, "one arc entry per bore level: {bore:?}");
    for &&(_, e) in &bore {
        let z = t.moves[e - 1].to.z;
        assert!([20.0, 15.0, 10.0, 5.0, 0.0].contains(&z), "a bore entry lands on a bore level, got {z}");
    }
    let ring_levels = |keep: &dyn Fn(f64) -> bool| -> Vec<f64> {
        let mut v: Vec<f64> = t
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z && keep(m.to.x.hypot(m.to.y)))
            .map(|m| m.to.z)
            .collect();
        v.dedup();
        v
    };
    assert_eq!(ring_levels(&|d| d < 13.0), [20.0, 15.0, 10.0, 5.0, 0.0], "the bore ring ends at its floor");
    assert_eq!(ring_levels(&|d| d > 30.0), levels, "the outer wall goes to the bottom");
    assert!(
        notes.contains("ends at its floor Z 0.000") && notes.contains("levels below skipped (-5.000, -7.000)"),
        "{notes}"
    );
    for (k, &(s, e)) in f.iter().enumerate() {
        let last = &t.moves[e - 1];
        assert_eq!(last.to.z, levels[k], "entry {k} lands on its level");
        assert!(e - s >= 4, "entry {k} is a chord fan, not one diagonal: {} chords", e - s);
        // The fan is tangent to the lead-in at the bottom: its last
        // chord heads the way the lead-in arc starts, and nearly level.
        let lead_in = &t.moves[e];
        assert!(lead_in.arc.is_some() && lead_in.to.z == levels[k], "the lead-in follows the arc");
        assert!(dot(dir_at(last, false), dir_at(lead_in, true)) > 0.999, "entry {k} meets the lead-in tangent");
        let drop = last.from.z - last.to.z;
        assert!(drop < 0.1, "entry {k}'s last chord is near level: {drop}");
        // The tangential lead lands on the wall tangent to the walk.
        let walk = &t.moves[e + 1];
        assert!(dot(dir_at(lead_in, false), dir_at(walk, true)) > 0.999, "lead-in {k} joins the wall tangent");
        // The transition reads as the standard sequence: first entry
        // from clearance; the second level drops onto the level above
        // (the cut top); deeper levels feed the rest at plunge feed.
        let seq = approach(&t, s);
        let want: &[&str] = match k {
            0 => &["rapid-across", "rapid-down"],
            1 => &["rapid-up", "rapid-across", "rapid-down"],
            _ => &["rapid-up", "rapid-across", "rapid-down", "plunge"],
        };
        assert_eq!(seq, want, "entry {k}'s approach");
    }
    // Plunges left: the outer ring's deeper approaches to the arc top
    // (5, every level past the second), the bore ring's the same way
    // for its arc entries, and one graze per bore level that took no
    // arc (all five grazed on the coarse mesh before #689).
    let plunges = t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count();
    // 5 outer + 3 bore (its levels past the second) + 0 grazes.
    assert_eq!(plunges, 8);
}

fn disc() -> Region {
    let pts = (0..128)
        .map(|k| {
            let a = std::f64::consts::TAU * f64::from(k) / 128.0;
            Point2::new(30.0 * a.cos(), 30.0 * a.sin())
        })
        .collect();
    Region::new(vec![Loop::new(pts).expect("ring")]).expect("disc")
}

fn wall_params(entry: Entry3D, tabs: u32) -> ContourParams {
    ContourParams {
        tool_diameter: 10.0,
        stock_to_leave: 0.0,
        clearance_z: 40.0,
        retract_z: 30.0,
        top_z: 20.0,
        bottom_z: 5.0,
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Outside,
        lead: Lead::Tangential { radius: 5.0 },
        tolerance: 0.01,
        tab_count: tabs,
        // Wider than the two lead arcs' reach over a tab (2 x 5): the
        // tab-gap lead crossing is a separate, pre-existing law.
        tab_width: 12.0,
        tab_height: 2.0,
        entry,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    }
}

fn verify_wall(t: &Toolpath, region: &Region) -> Vec<camcraft_app::verify::Warning> {
    let tool = ToolGeom {
        radius: 5.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: 5.0,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    };
    let cx = VerifyContext {
        region,
        stock: None,
        model: None,
        tool: &tool,
        cutter: None,
        mode: Mode::Boundary,
        stock_top_z: 20.0,
        clearance_z: 40.0,
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

/// Level transitions in path order: for each change of planar-feed
/// level, (level above, level below, the moves strictly between the
/// last planar feed above and the first planar feed below).
fn transitions(t: &Toolpath) -> Vec<(f64, f64, Vec<Move>)> {
    let planar = |m: &Move| m.kind == MoveKind::Feed && m.from.z == m.to.z;
    let mut out = Vec::new();
    let mut last: Option<usize> = None;
    for (i, m) in t.moves.iter().enumerate() {
        if !planar(m) {
            continue;
        }
        if let Some(j) = last
            && t.moves[j].to.z != m.to.z
        {
            out.push((t.moves[j].to.z, m.to.z, t.moves[j + 1..i].to_vec()));
        }
        last = Some(i);
    }
    out
}

#[test]
fn a_single_wall_ring_retracts_and_reenters_by_default() {
    // The default link is the documented one: lead out, lift to the
    // retract plane, move over, enter the next level by the Entry (here
    // the wall arc) - nothing stays down unless the user asks.
    let r = disc();
    let t = contour(&r, &wall_params(Entry3D::Arc, 0)).expect("contour");
    assert_eq!(verify_wall(&t, &r), Vec::new(), "{:?}", t.notes);
    let f = fans(&t);
    assert_eq!(f.len(), 4, "levels 20, 15, 10, 5 each enter on one arc");
    assert_eq!(approach(&t, f[0].0), ["rapid-across", "rapid-down"]);
    let tr = transitions(&t);
    assert_eq!(tr.len(), 3);
    for (above, below, between) in &tr {
        let top = between.iter().map(|m| m.to.z).fold(f64::NEG_INFINITY, f64::max);
        assert!(top >= 30.0, "{above} -> {below}: lifts to the retract plane, got {top}");
    }
    // Every fan stands off the wall: never nearer the axis than the ring.
    for &(s, e) in &f {
        for m in &t.moves[s..e] {
            assert!(m.to.x.hypot(m.to.y) >= 35.0 - 0.01, "fan point inside the offset: {m:?}");
        }
    }
}

#[test]
fn a_single_wall_ring_stays_down_and_ramps_when_asked() {
    use camcraft_app::links::{Link, Links};
    let r = disc();
    for angle in [3.0_f64, 1.0] {
        let p = ContourParams { links: Links { level: Link::KeepDown, transition: Some(Entry3D::Ramp), ramp_angle: angle, ..Links::PLANAR }, ..wall_params(Entry3D::Arc, 0) };
        let t = contour(&r, &p).expect("contour");
        assert_eq!(verify_wall(&t, &r), Vec::new(), "{:?}", t.notes);
        // One entry from clearance; after it the tool never plunges,
        // never rapids, and never rises above the level it just cut.
        // (`fans` also counts the ramps - descending feeds that move in
        // XY - so the arc entries are the descents that leave the ring.)
        let off_ring = fans(&t)
            .into_iter()
            .filter(|&(s, e)| t.moves[s..e].iter().any(|m| (m.to.x.hypot(m.to.y) - 35.0).abs() > 0.05))
            .count();
        assert_eq!(off_ring, 1, "only the first level enters on the arc");
        let tr = transitions(&t);
        assert_eq!(tr.len(), 3);
        let tan = angle.to_radians().tan();
        for (above, below, between) in &tr {
            assert!(!between.is_empty(), "the ramp is between the levels");
            for m in between {
                assert_eq!(m.kind, MoveKind::Feed, "{above} -> {below}: stays at feed: {m:?}");
                assert!(m.from.z <= *above + 1e-9 && m.to.z <= *above + 1e-9, "never above the level cut: {m:?}");
                assert!(m.to.z >= *below - 1e-9, "never below the next level: {m:?}");
                let run = (m.to.x - m.from.x).hypot(m.to.y - m.from.y);
                let drop = m.from.z - m.to.z;
                assert!(drop <= run * tan + 1e-9, "{above} -> {below}: descends at <= {angle} deg: {m:?}");
                // On the ring itself: the center never leaves the path.
                assert!((m.to.x.hypot(m.to.y) - 35.0).abs() < 0.05, "the ramp rides the ring: {m:?}");
            }
            assert_eq!(between.last().map(|m| m.to.z), Some(*below), "the ramp lands on the level");
        }
        assert!(!t.moves.iter().any(|m| m.kind == MoveKind::Plunge), "{:?}", t.notes);
    }
}

#[test]
fn the_stay_down_ramp_falls_back_to_retract_by_name() {
    use camcraft_app::links::{Link, Links};
    let r = disc();
    // Tabbed bottom level: the ramp would ride through a standing tab.
    let p = ContourParams { links: Links { level: Link::KeepDown, transition: Some(Entry3D::Ramp), ..Links::PLANAR }, ..wall_params(Entry3D::Arc, 3) };
    let t = contour(&r, &p).expect("contour");
    assert_eq!(verify_wall(&t, &r), Vec::new(), "{:?}", t.notes);
    assert!(
        t.notes.iter().any(|n| n.contains("no stay-down ramp onto Z 5.000") && n.contains("standing tab")),
        "{:?}",
        t.notes
    );
    // An angle the ring cannot hold: 0.01 deg over 5 mm wants ~130 laps.
    let p = ContourParams { links: Links { level: Link::KeepDown, transition: Some(Entry3D::Ramp), ramp_angle: 0.01, ..Links::PLANAR }, ..wall_params(Entry3D::Arc, 0) };
    let t = contour(&r, &p).expect("contour");
    assert_eq!(verify_wall(&t, &r), Vec::new(), "{:?}", t.notes);
    assert!(t.notes.iter().any(|n| n.contains("too short for a 0.0 deg ramp")), "{:?}", t.notes);
    let tr = transitions(&t);
    assert_eq!(tr.len(), 3);
    for (above, below, between) in &tr {
        let top = between.iter().map(|m| m.to.z).fold(f64::NEG_INFINITY, f64::max);
        assert!(top >= 30.0, "{above} -> {below}: the fallback retracts, got {top}");
    }
    // A nonsense angle is refused, never guessed.
    let p = ContourParams { links: Links { level: Link::KeepDown, transition: Some(Entry3D::Ramp), ramp_angle: 90.0, ..Links::PLANAR }, ..wall_params(Entry3D::Arc, 0) };
    assert!(contour(&r, &p).is_err());
}

#[test]
fn the_wall_arc_stands_down_by_name_on_tabbed_levels() {
    let r = disc();
    let t = contour(&r, &wall_params(Entry3D::Arc, 3)).expect("contour");
    assert_eq!(verify_wall(&t, &r), Vec::new(), "{:?}", t.notes);
    assert!(t.notes.iter().any(|n| n.contains("the arc stands down on tabbed levels")), "{:?}", t.notes);
    // Untabbed levels (20, 15, 10) still arc; the tabbed bottom grazes.
    assert_eq!(fans(&t).len(), 3);
}

#[test]
fn plunge_entry_on_the_wall_is_unchanged_by_the_arc_law() {
    // Opt-in: the same ring under Plunge walks the same planar cut, and
    // nothing about it descends on a fan.
    let r = disc();
    let plunge = contour(&r, &wall_params(Entry3D::Plunge, 0)).expect("contour");
    assert!(fans(&plunge).is_empty());
    let planar = |t: &Toolpath| -> Vec<_> {
        t.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z && m.arc.is_some()).map(|m| (m.from, m.to)).collect()
    };
    let arc = contour(&r, &wall_params(Entry3D::Arc, 0)).expect("contour");
    assert_eq!(planar(&plunge), planar(&arc), "the walked rings and leads are the same");
}

#[test]
fn the_users_part_links_levels_as_the_user_picks() {
    use camcraft_app::links::{Link, Links};
    let outer = |m: &Move| m.to.x.hypot(m.to.y) > 30.0;
    // Default: every level change of either ring lifts to the retract
    // plane (stock top + 13.5 = Z 33.5) before the next entry.
    let (t, w) = user_job();
    assert_eq!(w, 0, "{:?}", t.notes);
    for (above, below, between) in transitions(&t) {
        let top = between.iter().map(|m| m.to.z).fold(f64::NEG_INFINITY, f64::max);
        assert!(top >= 33.5, "{above} -> {below}: retracts, got {top}");
    }
    // Stay down and ramp: each ring all the way down, ramping along
    // itself at <= 3 deg; only the ring-to-ring move retracts.
    let mut op = user_op();
    if let OpParams::Contour { links, .. } = &mut op {
        *links = Links { level: Link::KeepDown, transition: Some(Entry3D::Ramp), ramp_angle: 3.0, ..Links::PLANAR };
    }
    let (t, w) = user_job_with(op);
    assert_eq!(w, 0, "{:?}", t.notes);
    let tr = transitions(&t);
    let levels: Vec<f64> = std::iter::once(tr[0].0).chain(tr.iter().map(|x| x.1)).collect();
    assert_eq!(
        levels,
        [20.0, 15.0, 10.0, 5.0, 0.0, -5.0, -7.0, 20.0, 15.0, 10.0, 5.0, 0.0],
        "ring by ring: the outer wall to the bottom, then the bore to its floor"
    );
    let tan = 3.0_f64.to_radians().tan();
    for (k, (above, below, between)) in tr.iter().enumerate() {
        if k == 6 {
            // Outer ring's bottom -> the bore ring's top: a new ring.
            let top = between.iter().map(|m| m.to.z).fold(f64::NEG_INFINITY, f64::max);
            assert!(top >= 33.5, "ring to ring retracts, got {top}");
            continue;
        }
        for m in between {
            assert_eq!(m.kind, MoveKind::Feed, "{above} -> {below}: {m:?}");
            assert!(m.to.z <= *above + 1e-9 && m.to.z >= *below - 1e-9, "{above} -> {below}: {m:?}");
            let run = (m.to.x - m.from.x).hypot(m.to.y - m.from.y);
            assert!(m.from.z - m.to.z <= run * tan + 1e-9, "{above} -> {below}: steeper than 3 deg: {m:?}");
            assert_eq!(outer(m), k < 6, "the ramp rides its own ring: {m:?}");
        }
    }
}
