//! M2.1a: the mesh-driven 3-D pocket. The level law (the model is the
//! FINAL part; each level cuts the footprint minus the columns the solid
//! still fills at its plane), the entry ladder's legality fallbacks, and
//! the generated paths' silence under the verify laws that read the same
//! islands. M2.1b adds the rest pass over the dexel sim: it closes the
//! staircase, plunges (never ramps) into rest material, and reports the
//! verdict without inventing a single gouge.
//! Provenance: our own design, specced from published CAM
//! behavior, per the Clean-Room Protocol.
//!
//! Gate map (M4.4): covers camcraft_motion::cl, camcraft_motion::geo2d, camcraft_motion::geometry, camcraft_motion::machining2d, camcraft_motion::machining3d, camcraft_motion::stock_sim, camcraft_motion::verify.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::{GenError, PocketStrategy};
use camcraft_motion::machining3d::{
    Entry3D, FFO_FLOOR, Pocket3DParams, engagement_step, material_above, optimize_feed, pocket3d,
    pocket3d_prev, pocket3d_with_rest, pocket3d_with_rest_prev,
};
use camcraft_motion::stock_sim::{Columns, Dexel};
use camcraft_motion::verify::{Mode, ToolGeom, VerifyContext, verify};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .unwrap()])
    .unwrap()
}

/// A solid axis-aligned box as a facet soup - six quads, two triangles
/// each, outward CCW. Watertight by construction; the slice law needs
/// only the closed shell.
fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| {
        p(
            [min[0], max[0]][i & 1],
            [min[1], max[1]][(i >> 1) & 1],
            [min[2], max[2]][(i >> 2) & 1],
        )
    };
    let quad = |a: usize, b: usize, c: usize, d: usize| -> [Triangle; 2] {
        [
            Triangle::new(v(a), v(b), v(c)),
            Triangle::new(v(a), v(c), v(d)),
        ]
    };
    let mut t = Vec::with_capacity(12);
    t.extend(quad(4, 5, 7, 6)); // +z top
    t.extend(quad(0, 2, 3, 1)); // -z bottom
    t.extend(quad(0, 1, 5, 4)); // -y
    t.extend(quad(2, 6, 7, 3)); // +y
    t.extend(quad(1, 3, 7, 5)); // +x
    t.extend(quad(2, 0, 4, 6)); // -x
    t
}

/// The workpiece slab: material from z = -20 up to the finished floor
/// at -10 - the pocket's job is everything of it above bottom_z.
fn slab() -> Vec<Triangle> {
    box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, -10.0])
}

/// The slab with a 10x10 boss at the center standing to z = 0: its
/// columns must never be cut below their own surface.
fn slab_with_boss() -> Vec<Triangle> {
    let mut f = slab();
    f.extend(box_facets([-5.0, -5.0, -10.0], [5.0, 5.0, 0.0]));
    f
}

fn params(entry: Entry3D, helix_radius: f64) -> Pocket3DParams {
    Pocket3DParams {
        links: camcraft_motion::links::Links::ROUGH3D,
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
        strategy: PocketStrategy::Zigzag,
        retract_z: None,
        entry,
        helix_radius,
        // The law's default; reads only under Adaptive, which the
        // helpers here pick explicitly when they need it.
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
    }
}

fn tool() -> ToolGeom {
    ToolGeom {
        radius: 4.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: 9.0,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    }
}

/// The 40x40 silhouette the slab projects to - what the operation layer
/// hands the generator (region_for's Pocket3D arm), asked of the law
/// itself here so the fixtures stay honest about it.
fn silhouette(facets: &[Triangle]) -> Region {
    camcraft_motion::geometry::slice::project(facets, 64).unwrap()
}

fn verify_against(facets: &[Triangle], region: &Region, path: &Toolpath) -> Vec<camcraft_motion::verify::Warning> {
    let tool = tool();
    let cx = VerifyContext {
        region,
        stock: None,
        model: Some(facets),
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

#[test]
fn flat_floor_comes_out_exact_and_silent() {
    // The ladder's last level snaps to bottom_z, so a flat-topped slab
    // gets a flat floor - and no law has a word against any of it.
    let f = slab();
    let region = silhouette(&f);
    let t = pocket3d(&f, &region, &params(Entry3D::Plunge, 1.0)).unwrap();
    let feed_zs: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed || m.kind == MoveKind::Plunge)
        .flat_map(|m| [m.from.z, m.to.z])
        .collect();
    assert!(
        feed_zs.iter().any(|&z| z == -10.0),
        "the floor level is cut: {feed_zs:?}"
    );
    assert!(
        t.moves.iter().all(|m| m.to.z >= -10.0),
        "nothing dips below the finished floor"
    );
    assert_eq!(verify_against(&f, &region, &t), Vec::new());
}

#[test]
fn a_raised_boss_is_never_cut_below_its_own_surface() {
    // The boss columns fill every plane below z = 0, so the level law
    // excludes them at every level beneath; the path rings around them.
    let f = slab_with_boss();
    let region = silhouette(&f);
    let t = pocket3d(&f, &region, &params(Entry3D::Plunge, 1.0)).unwrap();
    for m in &t.moves {
        for q in [&m.from, &m.to] {
            if q.z < 0.0 {
                assert!(
                    !(q.x.abs() < 5.0 && q.y.abs() < 5.0),
                    "move {:?} at z {} enters the boss column",
                    m.kind,
                    q.z
                );
            }
        }
    }
    assert_eq!(verify_against(&f, &region, &t), Vec::new());
}

#[test]
fn a_surface_between_levels_leaves_a_staircase_not_a_lie() {
    // The part tops out at z = -7.5, between the -5.0 and -10.0 levels:
    // the ladder floors those columns at -5.0 - at most one stepdown of
    // rest material, exactly what M2.1's simulation is for.
    let f = box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, -7.5]);
    // Pocket to -10 around it: deeper than the part anywhere = nothing
    // below -7.5 to cut, so the -10 level's target must come out empty.
    let region = rect(-15.0, -15.0, 15.0, 15.0);
    let t = pocket3d(
        &f,
        &region,
        &Pocket3DParams {
            bottom_z: -10.0,
            stepdown: 5.0,
            ..params(Entry3D::Plunge, 1.0)
        },
    )
    .unwrap();
    let deepest = t.moves.iter().map(|m| m.to.z.min(m.from.z)).fold(f64::INFINITY, f64::min);
    assert_eq!(deepest, -5.0, "one stepdown above the surface - never below it");
    assert_eq!(verify_against(&f, &region, &t), Vec::new());
}

#[test]
fn waterline_closes_its_rings() {
    // Ring walks are closed tours: every ring repeats its start point
    // in the pass data, and every join chord pairs one ring's leaving
    // with the next ring's entering. So per cutting level the feed XY
    // multiset balances at EVERY point except the level's two honest
    // open ends - the plunge enters the first ring's start without a
    // feed, and the level hands off (up to the next level, or to the
    // retract) without a feed leaving the last ring's start. An open
    // zigzag breaks even this weaker law: each of its N rows dangles a
    // departure and an arrival of its own.
    let f = slab();
    let region = silhouette(&f);
    let t = pocket3d(
        &f,
        &region,
        &Pocket3DParams {
            strategy: PocketStrategy::Waterline,
            ..params(Entry3D::Plunge, 1.0)
        },
    )
    .unwrap();
    let mut per_level: std::collections::BTreeMap<[u64; 2], std::collections::BTreeMap<[u64; 2], i32>> =
        Default::default();
    let mut cuts = 0;
    for m in t.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z) {
        if m.from.x == m.to.x && m.from.y == m.to.y {
            continue; // vertical at the plane: XY-neutral
        }
        cuts += 1;
        let k = [m.from.z.to_bits(), 0];
        let d = per_level.entry(k).or_default();
        *d.entry([m.from.x.to_bits(), m.from.y.to_bits()]).or_default() += 1;
        *d.entry([m.to.x.to_bits(), m.to.y.to_bits()]).or_default() -= 1;
    }
    assert!(cuts > 0, "the tour actually walks");
    // One tour per cutting plane: the tour closes at every XY but the
    // plunge's landing point (+1 leaving) and the level's last cut
    // point (-1). No ring start may dangle more than that.
    for (z, d) in &per_level {
        let out = d.values().filter(|&&v| v > 0).count();
        let into = d.values().filter(|&&v| v < 0).count();
        assert!(
            d.values().all(|&v| v == 1 || v == -1 || v == 0) && out == 1 && into == 1,
            "level z={}: a ring tour must close at every XY but its two open ends; deltas {d:?}",
            f64::from_bits(z[0]),
        );
    }
    assert_eq!(verify_against(&f, &region, &t), Vec::new());
}

#[test]
fn zigzag_walks_feed_lines() {
    // The same slab under Zigzag: feed moves with real XY travel, and
    // silence under the same laws - the strategies differ in shape,
    // never in what is allowed.
    let f = slab();
    let region = silhouette(&f);
    let t = pocket3d(&f, &region, &params(Entry3D::Plunge, 1.0)).unwrap();
    assert!(
        t.moves.iter().any(|m| m.kind == MoveKind::Feed
            && (m.from.x != m.to.x || m.from.y != m.to.y)),
        "a straight pass exists"
    );
    assert_eq!(verify_against(&f, &region, &t), Vec::new());
}

#[test]
fn a_helix_that_fits_descends_in_arcs() {
    // Open floor at -5.0 with a 60x60 footprint: the swept disc of a
    // 1 mm helix about a tool standing mid-floor clears everything, so
    // the entry is earned - Z-descending arc moves, silent below.
    let f = slab();
    let region = rect(-30.0, -30.0, 30.0, 30.0);
    let t = pocket3d(&f, &region, &params(Entry3D::Helix, 1.0)).unwrap();
    assert!(
        t.moves.iter().any(|m| m.arc.is_some() && m.to.z < -5.0),
        "the helix posts arcs that sink"
    );
    assert_eq!(verify_against(&f, &region, &t), Vec::new());
}

#[test]
fn the_entry_family_is_obeyed_at_the_first_level_3d() {
    // The ladder's side of M8.1's level-0 law (the probe reopened the
    // stage on exactly this): what the user STATED for entry is read
    // at the first level, not only at crossings. The helix spirals
    // into the first plane - no straight plunge meets it; the Plunge
    // default, the shape every plain pocket posts, dives in the
    // floor's room, never on `passes[0]`'s corner.
    let f = slab();
    let region = rect(-30.0, -30.0, 30.0, 30.0);
    let t = pocket3d(&f, &region, &params(Entry3D::Helix, 1.0)).unwrap();
    let reach = t
        .moves
        .iter()
        .find(|m| (m.to.z - 0.0).abs() < 1e-9 && m.kind != MoveKind::Rapid)
        .expect("the first plane is entered by something");
    assert!(reach.arc.is_some(), "entered by the spiral, not a wall dive: {reach:?}");
    assert!(t.notes.is_empty(), "room exists; nothing to substitute: {:?}", t.notes);
    assert_eq!(verify_against(&f, &region, &t), Vec::new());
    let t2 = pocket3d(&f, &region, &params(Entry3D::Plunge, 1.0)).unwrap();
    let dive = t2
        .moves
        .iter()
        .find(|m| m.kind == MoveKind::Plunge)
        .expect("a dive at the first level");
    let cen = region.offset(-4.0).expect("the skim plane's center");
    assert!(
        cen.contains(Point2::new(dive.to.x, dive.to.y)),
        "the level 0 dive is interior, got {:?}",
        dive.to
    );
    assert!(t2.notes.is_empty(), "the axial arm found its room: {:?}", t2.notes);
}

#[test]
fn a_helix_that_does_not_fits_falls_back_to_a_plunge() {
    // The boss slab at its own silhouette: every row's span is the 7 mm
    // band between wall-erosion and boss-erosion, narrower than the
    // widened disc (r + rh) of a 4 mm helix - except at the band's
    // corners (see below). Eroding the ring by that
    // much leaves an inverted annulus - the hole has swallowed the
    // outer loop - and M8.1's containment guard refuses its phantom
    // triangles, so the level lands the honest substitution: an axial
    // dive on the floor's real room, SAID on the notes. A slower dive,
    // never a gouge, never a silence. (Before the guard this posted a
    // helix whose dive point sat in the boss - the phantom made the
    // landing legal only in the tessellation's eye.)
    let f = slab_with_boss();
    let region = silhouette(&f);
    let t = pocket3d(&f, &region, &params(Entry3D::Helix, 4.0)).unwrap();
    // M11.3: the exact distance law (`entry::disc_clears`) finds the room
    // the triangulation's centroids missed - the band's corners, where
    // the boss's rounded offset leaves 4.5 mm - and the helix lands there
    // instead of substituting. Whatever lands, it is never a phantom:
    // every orbit's center stands a helix radius clear of both walls.
    for m in t.moves.iter().filter(|m| m.arc.is_some() && m.from.z != m.to.z) {
        let c = m.arc.map(|a| a.center).expect("arc");
        let to_wall = 16.0 - c.x.abs().max(c.y.abs());
        let to_boss = ((c.x.abs() - 5.0).max(0.0)).hypot((c.y.abs() - 5.0).max(0.0)) - 4.0;
        assert!(to_wall >= 4.0 && to_boss >= 4.0, "an orbit about {c:?} has no room");
    }
    let orbits = t.moves.iter().any(|m| m.arc.is_some() && m.from.z != m.to.z);
    assert!(orbits || t.notes.iter().any(|n| n.contains("widened disc")), "{:?}", t.notes);
    assert_eq!(verify_against(&f, &region, &t), Vec::new());
}

#[test]
fn a_ramp_over_open_floor_feeds_down_in_one_diagonal() {
    let f = slab();
    let region = rect(-30.0, -30.0, 30.0, 30.0);
    let t = pocket3d(&f, &region, &params(Entry3D::Ramp, 1.0)).unwrap();
    // M8.1 reads the family at level 0 too: home stands over open
    // floor, so even the skim's chord is proven and the FIRST plane
    // is entered on the slope.
    let open = t
        .moves
        .iter()
        .find(|m| (m.to.z - 0.0).abs() < 1e-9 && m.kind != MoveKind::Rapid)
        .expect("the first plane is entered by something");
    assert!(
        open.kind == MoveKind::Feed && open.from.z != open.to.z,
        "level 0 rides the ramp, not a wall dive: {open:?}"
    );
    assert!(
        t.moves.iter().any(|m| {
            m.kind == MoveKind::Feed
                && (m.from.x != m.to.x || m.from.y != m.to.y)
                && m.from.z != m.to.z
                && m.from.z > -10.0
                && m.to.z == -10.0
        }),
        "the -10.0 level is entered on a ramp"
    );
    assert_eq!(verify_against(&f, &region, &t), Vec::new());
}

#[test]
fn degenerate_input_refuses_cleanly() {
    let f = slab();
    let region = rect(-15.0, -15.0, 15.0, 15.0);
    // Nothing to section.
    assert!(matches!(
        pocket3d(&[], &region, &params(Entry3D::Plunge, 1.0)),
        Err(GenError::Empty(_))
    ));
    // An empty footprint is not a pocket.
    assert!(matches!(
        pocket3d(&f, &Region::empty(), &params(Entry3D::Plunge, 1.0)),
        Err(GenError::Empty(_))
    ));
    // Non-finite numbers die at validation, not in the slice.
    assert!(matches!(
        pocket3d(
            &f,
            &region,
            &Pocket3DParams {
                feed: f64::NAN,
                ..params(Entry3D::Plunge, 1.0)
            }
        ),
        Err(GenError::Invalid(_))
    ));
    // A stepdown of zero would hang the ladder.
    assert!(matches!(
        pocket3d(
            &f,
            &region,
            &Pocket3DParams {
                stepdown: 0.0,
                ..params(Entry3D::Plunge, 1.0)
            }
        ),
        Err(GenError::Invalid(_))
    ));
    // A helix wider than the tool leaves its own swept disc short of
    // overlap - unvalidated, that is a gouge shape.
    assert!(matches!(
        pocket3d(&f, &region, &params(Entry3D::Helix, 4.5)),
        Err(GenError::Invalid(_))
    ));
    // A block exactly filling the footprint: the top level's ε-probe
    // stands above the block top, so that face is cut tangentially -
    // one real path, not a refusal; the levels under it are all island
    // and get skipped.
    let block = box_facets([-5.0, -5.0, -10.0], [5.0, 5.0, 0.0]);
    assert!(
        pocket3d(
            &block,
            &rect(-5.0, -5.0, 5.0, 5.0),
            &Pocket3DParams {
                top_z: 0.0,
                ..params(Entry3D::Plunge, 1.0)
            }
        )
        .is_ok()
    );
    let filled = box_facets([-20.0, -20.0, -10.0], [20.0, 20.0, 2.0]);
    assert!(matches!(
        pocket3d(&filled, &region, &params(Entry3D::Plunge, 1.0)),
        Err(GenError::Empty(msg)) if msg.contains("fills")
    ));
}

#[test]
fn the_rest_pass_closes_the_staircase_it_inherited() {
    // The between-levels slab: the plain ladder floors those columns one
    // stepdown above the part surface and says so honestly; with the
    // rest pass, half the step closes the stair. The report is the
    // witness: rest gone, nothing gouged, and the coarse ladder still
    // leads the path move for move (the rest is an appendix, never a
    // rewrite - the same runs, then the closing levels).
    let f = box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, -7.5]);
    let region = rect(-15.0, -15.0, 15.0, 15.0);
    let pp = Pocket3DParams {
        bottom_z: -10.0,
        stepdown: 5.0,
        ..params(Entry3D::Ramp, 1.0)
    };
    let coarse = pocket3d(&f, &region, &pp).unwrap();
    let deepest_coarse = coarse
        .moves
        .iter()
        .map(|m| m.to.z.min(m.from.z))
        .fold(f64::INFINITY, f64::min);
    assert_eq!(deepest_coarse, -5.0);

    let (t, rep) = pocket3d_with_rest(&f, &region, &pp).unwrap();
    assert_eq!(
        &t.moves[..coarse.moves.len()],
        &coarse.moves[..],
        "the coarse ladder is the path's prefix, untouched"
    );
    assert!(t.moves.len() > coarse.moves.len(), "rest moves were added");
    let deepest = t
        .moves
        .iter()
        .map(|m| m.to.z.min(m.from.z))
        .fold(f64::INFINITY, f64::min);
    assert_eq!(deepest, -7.5, "the closing level lands exactly on the surface");
    // The one place a disc cannot go is the pocket's own square
    // corners: no center both covers a corner column and stays inside
    // the eroded footprint, so four columns stand at full stock, and
    // the report says so - the fillet's job in a later strategy,
    // never a number to hide. Everything the ladder can reach, it
    // closed.
    assert_eq!(rep.rest_cells, 4, "the four pocket corners, named");
    assert!((rep.max_rest_height - 7.5).abs() < 1e-6, "standing at full stock");
    assert!((rep.rest_volume - 67.5).abs() < 0.1, "{rep:?}");
    assert_eq!(rep.gouge_cells, 0, "and the closing cut never bit below");
    // 2.5 mm closed over all but four corner columns of the 30x30
    // footprint, stock counted from the cut top (the gross law).
    assert!(
        (rep.removed_volume - 6682.5).abs() < 1.0,
        "{rep:?}"
    );
    assert_eq!(verify_against(&f, &region, &t), Vec::new());
    // The rest level enters by plunge, never a ramp: deeper columns are
    // superset-clear (the nesting law), so a straight dive is legal and
    // a diagonal through standing stair material would be a lie.
    assert!(
        t.moves.iter().any(|m| m.kind == MoveKind::Plunge && m.to.z == -7.5),
        "the closing level plunges"
    );
}

#[test]
fn a_path_that_needs_nothing_adds_nothing() {
    // Two honest verdicts from the boss slab. The floor and the boss
    // faces land exactly on the part, and that emptiness is success -
    // the path stays byte-identical to the plain ladder. (Had the
    // ladder refused emptiness as an error, a fully-cut pocket would
    // fail to post.) What does remain is corner wedges - where two
    // pocket walls meet, and where two boss walls do (a disc there
    // would have to bite the boss to reach the column): the half-step
    // ladder finds them, plans nothing legal, and REPORTS them - rest
    // the sim refuses to hide, for the point-plunge and fillet work
    // of the later rest strategies.
    let f = slab_with_boss();
    let region = silhouette(&f);
    let pp = params(Entry3D::Plunge, 1.0);
    let coarse = pocket3d(&f, &region, &pp).unwrap();
    let (t, rep) = pocket3d_with_rest(&f, &region, &pp).unwrap();
    assert_eq!(t.moves, coarse.moves, "no legal rest pass, no rest moves");
    assert!(
        rep.rest_cells > 0 && rep.rest_cells < 32,
        "corner slivers only, and named: {:?}",
        rep.rest_cells
    );
    // Under 5% of the removed volume, and the residue is not noise:
    // it is the four square corners of the pocket and the four of the
    // boss - columns no legal disc center reaches (427.5 of 14872.5
    // mm3 on this fixture). The honest floor of a line-pass round
    // tool; fillet strategies are what close it, in a later milestone.
    assert!(
        rep.rest_volume < 0.05 * rep.removed_volume,
        "corner slivers of the whole: {:?} vs {:?}",
        rep.rest_volume,
        rep.removed_volume
    );
    assert_eq!(rep.gouge_cells, 0, "and it reached nothing it should not have");
    // The flat slab to its own floor: no islands, and the only rest
    // left is the corner wedges - columns farther than the disc's
    // radius from every legal center. Their exact count is grid
    // phase: the bbox starts the lattice at -20, so columns sit
    // 0.75 mm nearer the low walls than the high ones and the wedge
    // is 3 cells wide at (+, +), 2 at the mixed corners, 1 at (-, -).
    // Eight, standing 10 mm over the part, for 180 mm3 the sim names
    // instead of hiding - a fillet or point pass closes them later.
    let s = slab();
    let sr = silhouette(&s);
    let (_, rep2) = pocket3d_with_rest(&s, &sr, &pp).unwrap();
    assert_eq!(rep2.rest_cells, 8, "the corner wedges, per the grid phase");
    assert!((rep2.rest_volume - 180.0).abs() < 1e-6, "{rep2:?}");
    assert_eq!(rep2.gouge_cells, 0);
    // The grid's own area, cell-quantized by construction (the
    // measure law): 729 in-footprint columns x 2.25 mm2 x 10 mm,
    // minus the eight standing ones.
    assert!((rep2.removed_volume - 16222.5).abs() < 0.1, "{rep2:?}");
}

#[test]
fn the_rest_ladder_refuses_what_the_plain_one_refuses() {
    // Same guards, same vocabulary - the rest pass is a consumer of
    // pocket3d's contract, never a second door around it.
    let f = slab();
    let region = rect(-15.0, -15.0, 15.0, 15.0);
    assert!(matches!(
        pocket3d_with_rest(&[], &region, &params(Entry3D::Plunge, 1.0)),
        Err(GenError::Empty(_))
    ));
    assert!(matches!(
        pocket3d_with_rest(&f, &Region::empty(), &params(Entry3D::Plunge, 1.0)),
        Err(GenError::Empty(_))
    ));
    assert!(matches!(
        pocket3d_with_rest(
            &f,
            &region,
            &Pocket3DParams {
                feed: f64::NAN,
                ..params(Entry3D::Plunge, 1.0)
            }
        ),
        Err(GenError::Invalid(_))
    ));
}

#[test]
fn material_above_holds_its_monotonic_law() {
    // Deeper planes fill with more columns: the boss slab's islands grow
    // from nothing above 0, to the boss below it, to everything below
    // the slab top. (The inclusion is the level law's backbone.)
    let f = slab_with_boss();
    let at = |z: f64| material_above(&f, z).unwrap();
    let shallow = at(1.0);
    let mid = at(-5.0);
    let deep = at(-15.0);
    assert!(shallow.is_empty(), "nothing stands above the whole model");
    // Inclusion, probed where the two regions differ: every mid-column
    // is a deep column, and the boss is the witness that mid is not yet
    // everything.
    for q in [Point2::new(0.0, 0.0), Point2::new(15.0, 0.0)] {
        assert!(
            !mid.contains(q) || deep.contains(q),
            "{q:?}: an island above -5.0 stands above -15.0 too"
        );
    }
    assert!(mid.contains(Point2::new(0.0, 0.0)), "the boss fills -5.0");
    assert!(!mid.contains(Point2::new(15.0, 0.0)), "the slab top is at -10");
    assert!(deep.contains(Point2::new(15.0, 0.0)), "and below it all is solid");
}


// ---------------------------------------------------------------------------
// M2.1c: constant-engagement rings. The strategy v1 IS the waterline tiling
// at the sagitta-law step - these tests pin the law, the plumbing that makes
// the equality true, and the validation that guards it.
// ---------------------------------------------------------------------------

#[test]
fn the_engagement_law_is_the_sagitta() {
    // a = r(1 - cos theta): the ring step that keeps the wall bite inside
    // the angle. 60 degrees is exactly half a radius; 90 degrees exactly a
    // radius - the waterline reach bound, the law's closed end. The default
    // 45 degrees steps at 14.6% of the diameter, the published HEM band.
    let r = 4.0;
    assert!((engagement_step(r, 60.0) - 0.5 * r).abs() < 1e-12);
    assert!((engagement_step(r, 90.0) - r).abs() < 1e-12);
    let s45 = engagement_step(r, 45.0);
    assert!(
        (s45 / (2.0 * r) - 0.1464).abs() < 1e-3,
        "45 degrees must step at ~14.6% of D, got {s45}"
    );
    // Monotone in the angle, with the two known limits.
    assert!(engagement_step(r, 10.0) < s45 && s45 < engagement_step(r, 80.0));
    assert!(engagement_step(r, 0.0).abs() < 1e-12, "no angle bites, no step");
}

#[test]
fn adaptive_is_the_waterline_tiling_at_the_law_step() {
    // The equality is tiling_step made visible: one source for the ring
    // spacing, read by the pass planner (and, for the sim, by the cell
    // law). Any drift between the strategies' plumbing fails here.
    // M11.4: the rings are `OffsetRings` (Adaptive is the HSM plan per
    // level, `pocket3d_hsm`).
    let f = slab();
    let region = silhouette(&f);
    let step = engagement_step(4.0, 45.0);
    let adaptive = pocket3d(
        &f,
        &region,
        &Pocket3DParams {
            strategy: PocketStrategy::OffsetRings,
            ..params(Entry3D::Plunge, 1.0)
        },
    )
    .unwrap();
    let waterline_at_step = pocket3d(
        &f,
        &region,
        &Pocket3DParams {
            strategy: PocketStrategy::Waterline,
            stepover: step,
            ..params(Entry3D::Plunge, 1.0)
        },
    )
    .unwrap();
    assert_eq!(adaptive.moves, waterline_at_step.moves, "one law, one geometry");
    assert_eq!(verify_against(&f, &region, &adaptive), Vec::new());
    // And against the untouched params, the engagement step (1.17 mm)
    // tiles finer than the 3 mm stepover: strictly more cutting passes.
    let waterline_at_stepover = pocket3d(
        &f,
        &region,
        &Pocket3DParams {
            strategy: PocketStrategy::Waterline,
            ..params(Entry3D::Plunge, 1.0)
        },
    )
    .unwrap();
    assert!(
        adaptive.moves.len() > waterline_at_stepover.moves.len(),
        "the law step must tile finer than the stepover it replaces"
    );
}

#[test]
fn adaptive_refuses_what_the_law_refuses() {
    let f = slab();
    let region = silhouette(&f);
    let at = |strategy: PocketStrategy, angle: f64, stepover: f64| {
        pocket3d(
            &f,
            &region,
            &Pocket3DParams {
                strategy,
                stepover,
                optimal_load: camcraft_motion::load::load_of_legacy_degrees(angle),
                ..params(Entry3D::Plunge, 1.0)
            },
        )
    };
    // (0, 90] only: 0 rings forever, and past a quarter-turn the sagitta
    // claims bite the tool cannot take (90 degrees is the reach bound).
    for bad in [0.0, -45.0, 90.5] {
        let r = at(PocketStrategy::Adaptive, bad, 3.0);
        assert!(
            matches!(r, Err(GenError::Invalid(_))),
            "{bad} degrees must refuse under Adaptive, got {r:?}"
        );
    }
    assert!(at(PocketStrategy::OffsetRings, 90.0, 3.0).is_ok(), "90 is the closed end");
    assert!(at(PocketStrategy::OffsetRings, 45.0, 3.0).is_ok());
    assert!(matches!(at(PocketStrategy::OffsetRings, 0.0, 3.0), Err(GenError::Invalid(_))), "the rings read the door too");
    // Elsewhere the angle is inert: 0 degrees under Waterline changes
    // nothing, because nothing reads it.
    assert!(at(PocketStrategy::Waterline, 0.0, 3.0).is_ok());
    // Finiteness is strategy-blind, as it has always been.
    assert!(at(PocketStrategy::Zigzag, f64::NAN, 3.0).is_err());
    assert!(at(PocketStrategy::Adaptive, f64::NAN, 3.0).is_err());
    // The stepover gate is strategy-blind too: under Adaptive the number
    // never spaces a ring (tiling_step answers for it), but a non-positive
    // or over-wide stepover is still a nonsense parameter, refused by the
    // same gate every other pocket meets. The v1 stance - keep the gate
    // shared, document the inertness, do not fork legality per strategy.
    assert!(at(PocketStrategy::Adaptive, 45.0, 0.0).is_err());
    assert!(at(PocketStrategy::Adaptive, 45.0, 9.0).is_err(), "over the D");
}

#[test]
fn adaptive_rings_circle_the_boss() {
    // The boss law is strategy-independent: the ring family is carved
    // from the same standing-material centers, so the computed islands
    // exclude the boss columns exactly as under Waterline (the rings;
    // the HSM plan's island levels are `pocket3d_hsm`'s).
    let f = slab_with_boss();
    let region = silhouette(&f);
    let t = pocket3d(
        &f,
        &region,
        &Pocket3DParams {
            strategy: PocketStrategy::OffsetRings,
            ..params(Entry3D::Plunge, 1.0)
        },
    )
    .unwrap();
    for m in &t.moves {
        for q in [&m.from, &m.to] {
            if q.z < 0.0 {
                assert!(
                    !(q.x.abs() < 5.0 && q.y.abs() < 5.0),
                    "move {:?} at z {} enters the boss column",
                    m.kind,
                    q.z
                );
            }
        }
    }
    assert_eq!(verify_against(&f, &region, &t), Vec::new());
}

// ---------------------------------------------------------------- M2.1d

#[test]
fn ffo_moves_only_feed_words_and_only_down() {
    // The modulation law in one pass: same kinds, same points, same
    // order; every planar feed lands inside [floor, programmed], and
    // nothing else - rapids, plunges, level hops - changes at all.
    let f = slab();
    let region = silhouette(&f);
    let p = params(Entry3D::Plunge, 1.0);
    let t = pocket3d(&f, &region, &p).unwrap();
    let o = optimize_feed(&t, &f, &region, &p).unwrap();
    assert_eq!(o.moves.len(), t.moves.len());
    let mut slowed = 0;
    let mut kept = 0;
    for (a, b) in t.moves.iter().zip(&o.moves) {
        assert_eq!(a.kind, b.kind);
        assert_eq!(a.from, b.from);
        assert_eq!(a.to, b.to);
        let planar_cut = b.kind == MoveKind::Feed && (b.to.z - b.from.z).abs() <= 1e-6;
        if !planar_cut {
            // Vertical feeds keep the plunge word (axial engagement is
            // a different curve); rapids and links keep theirs entirely.
            assert_eq!(a.feed, b.feed, "non-planar moves are untouched: {a:?}");
        }
        if planar_cut {
            if let Some(y) = b.feed {
                assert!(y <= p.feed + 1e-9, "never past the programmed feed: {y}");
                assert!(
                    y >= p.feed * FFO_FLOOR - 1e-9,
                    "never below the floor: {y} of {}",
                    p.feed * FFO_FLOOR
                );
            }
            match (a.feed, b.feed) {
                (Some(x), Some(y)) if y + 1e-9 < x => slowed += 1,
                (Some(x), Some(y)) if (y - x).abs() <= 1e-9 => kept += 1,
                _ => {}
            }
        }
    }
    assert!(slowed > 0, "full bites must be read as heavier than the median");
    assert!(
        kept > 0,
        "the median load - the steady cut - keeps the programmed feed"
    );
    assert_eq!(verify_against(&f, &region, &o), Vec::new());
}

#[test]
fn ffo_the_first_cut_of_a_level_is_its_slowest_kind() {
    // The first row of a level runs a disc buried at full width in the
    // untouched shelf - engagement 1.0, the heaviest a planar move
    // gets - so it takes the level's slowest feed word. Later rows and
    // links are lighter and run at or above it. A level that engages
    // nothing anywhere (the top plane here is a zero-depth skin of the
    // stock face) keeps the programmed feed throughout: no load, no
    // slowdown, and "first == min of the level" still holds.
    let f = slab();
    let region = silhouette(&f);
    let p = params(Entry3D::Plunge, 1.0);
    let t = pocket3d(&f, &region, &p).unwrap();
    let o = optimize_feed(&t, &f, &region, &p).unwrap();
    let planar: Vec<(f64, f64)> = o
        .moves
        .iter()
        // M11.3: the short at-depth link from the entry dive onto the
        // row start runs inside the dive's own hole - not a cut.
        .filter(|m| m.kind == MoveKind::Feed && (m.to.z - m.from.z).abs() <= 1e-6)
        .filter(|m| (m.to.x - m.from.x).hypot(m.to.y - m.from.y) > 1.0)
        .map(|m| (m.to.z, m.feed.unwrap_or(0.0)))
        .collect();
    assert!(!planar.is_empty(), "the ladder has planar feeds");
    let mut slowed_levels = 0;
    let mut seen: Vec<f64> = Vec::new();
    for (z, _) in &planar {
        if seen.iter().any(|s| (s - z).abs() <= 1e-9) {
            continue;
        }
        seen.push(*z);
        let level: Vec<f64> = planar.iter().filter(|(lz, _)| (lz - z).abs() <= 1e-9).map(|(_, y)| *y).collect();
        let first = level[0];
        let min = level.iter().copied().fold(f64::INFINITY, f64::min);
        assert!(
            (first - min).abs() <= 1e-9,
            "level z={z}: the first cut must be the slowest kind (first {first}, min {min})"
        );
        if first < p.feed - 1e-9 {
            slowed_levels += 1;
        }
    }
    assert!(
        slowed_levels > 0,
        "at least one level actually cuts, and its full bite is slowed"
    );
    assert_eq!(verify_against(&f, &region, &o), Vec::new());
}

#[test]
fn ffo_is_deterministic_and_hands_a_rapid_only_path_back_untouched() {
    let f = slab();
    let region = silhouette(&f);
    let prm = params(Entry3D::Plunge, 1.0);
    let t = pocket3d(&f, &region, &prm).unwrap();
    let o1 = optimize_feed(&t, &f, &region, &prm).unwrap();
    let o2 = optimize_feed(&t, &f, &region, &prm).unwrap();
    assert_eq!(o1, o2, "two runs are the same answer");

    let mut b = camcraft_motion::cl::PathBuilder::new(p(0.0, 0.0, 5.0));
    b.push(MoveKind::Rapid, p(10.0, 10.0, 5.0), None);
    let rapids = b.finish();
    let out = optimize_feed(&rapids, &f, &region, &prm).unwrap();
    assert_eq!(out.moves, rapids.moves, "a path that cuts nothing modulates nothing");
}

#[test]
fn a_prev_mask_only_ever_removes_cuts() {
    // M3.1 D's mask law at the generator: the left half of the slab
    // stands machined to its floor (-10, what an operation ahead in
    // the program left), the right half stands full. The masked
    // ladder must cut the right half and never the left - fewer
    // moves, and no cut move enters the machined side. A `None`
    // mask reproduces `pocket3d` move for move: the first operation
    // of a program is exactly the old behavior.
    let f = slab();
    let region = silhouette(&f);
    let pp = params(Entry3D::Plunge, 0.0);
    let full = pocket3d(&f, &region, &pp).expect("the slab cuts");
    let none = pocket3d_prev(&f, &region, &pp, None).expect("None is the plain run");
    assert_eq!(none.moves, full.moves, "no mask, no change");
    assert!(
        full.moves
            .iter()
            .any(|m| { m.kind != MoveKind::Rapid && m.to.x < -6.0 && m.to.z < 0.0 }),
        "the unmasked run tiles the left half"
    );
    let mut mask = Dexel::block(-20.0, -20.0, 20.0, 20.0, 0.0, 1.0).unwrap();
    for iy in 0..mask.ny {
        for ix in 0..mask.nx {
            let x = mask.x0 + (ix as f64 + 0.5) * mask.cell;
            if x < 0.0 {
                mask.set(iy * mask.nx + ix, -10.0);
            }
        }
    }
    let masked = pocket3d_prev(&f, &region, &pp, Some(&Columns::of_grid(&mask))).expect("the right half cuts");
    assert!(
        masked.moves.len() < full.moves.len(),
        "masked {} vs full {}",
        masked.moves.len(),
        full.moves.len()
    );
    for m in &masked.moves {
        // Every non-rapid move stands on the untouched side; rapids
        // fly over whatever they like, above the standing material.
        if m.kind != MoveKind::Rapid {
            assert!(
                m.to.x >= -6.0,
                "cut move at x {} crossed the mask",
                m.to.x
            );
        }
    }
}

#[test]
fn a_fully_machined_mask_refuses_rather_than_cutting_air() {
    // The redundant-operation law: where the program ahead left the
    // whole footprint at or below every plane, every masked target
    // is empty and the ladder says so as `Empty` - the refusal that
    // keeps an air-cutting operation out of a posted program.
    let f = slab();
    let region = silhouette(&f);
    let pp = params(Entry3D::Plunge, 0.0);
    let mask = Dexel::block(-20.0, -20.0, 20.0, 20.0, -10.0, 1.0).unwrap();
    assert!(matches!(
        pocket3d_prev(&f, &region, &pp, Some(&Columns::of_grid(&mask))),
        Err(GenError::Empty(_))
    ));
    // The rest-rough variant refuses from its coarse leg too, before
    // spending a single rest-ladder step.
    assert!(matches!(
        pocket3d_with_rest_prev(&f, &region, &pp, Some(&Columns::of_grid(&mask))),
        Err(GenError::Empty(_))
    ));
}
