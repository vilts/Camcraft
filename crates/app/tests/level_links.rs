//! Level transitions and links through the app (M11.2 C/D): the user's
//! own part (`testdata/models/Milling-test.step`, a D-shaped plate with a
//! blind bore) contoured and pocketed under every level link and
//! transition, generated through the commands and verified against the
//! part solid (the depth-aware law) - every path silent; the project
//! file writes the `links` key only when the choice differs from the
//! kind's earlier shape and round-trips it byte for byte; the stamp
//! tells the choices apart and leaves a default op's digest alone; and
//! the helix entry posts as continuous helical G2/G3 words.
//!
//! Gate map (M11.2): covers app::operation (params, stamp, sane, links),
//! app::project (links DTO, contour migration), app::commands, sim::verify,
//! app::posting.

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::Point3;
use camcraft_app::links::{Link, Links};
use camcraft_app::machining2d::{Lead, PocketStrategy, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{self, Height, HeightReference, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting, project};
use flecs_ecs::prelude::World;

const TOOL: &str = "EM-1000-4FL";

fn contour_op(links: Links) -> OpParams {
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
        links,
    }
}

/// The 3-D pocket ladder on the part: the footprint minus the part
/// standing above each level - the blind bore, cleared to its floor.
fn pocket_op(entry: Entry3D, links: Links) -> OpParams {
    OpParams::Pocket3D {
        links,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 20.5),
        retract_z: Some(Height::of(HeightReference::TopOfStock, 2.0)),
        top_z: Height::of(HeightReference::TopOfModel, 0.0),
        // The blind bore's floor (Z 0): the ladder clears the bore, not the
        // part's underside (see `probe_helix_gouge` for the stock-bottom case).
        bottom_z: Height::fixed(0.0),
        stepdown: 2.5,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::Waterline,
        entry,
        helix_radius: 2.0,
        rest_rough: false,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        feed_opt: false,
        shell: false,
    }
}

/// The user's part in a 70 x 70 block from Z -7 to its top (Z 20), the
/// 10 mm tool, one op - generated through the commands.
fn user_job(params: OpParams) -> (World, History, Toolpath, Vec<String>) {
    let world = new_world();
    let mut h = History::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    let imp = ImportModel::new(&path).expect("the user's STEP imports");
    let name = imp.name.clone();
    h.execute(&world, imp);
    let loc = Location { pos: Point3::new(0.0, 0.0, -7.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 70.0, dy: 70.0, dz: 27.0 }, loc).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "Op1", TOOL, SrcRef::Entity(name), params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Op1").expect("generate"));
    let e = model::find_by_name(&world, "Op1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    let warns = tp.warnings.iter().map(|w| format!("{w:?}")).collect();
    let path = tp.path.clone();
    (world, h, path, warns)
}

fn planar(m: &Move) -> bool {
    m.kind == MoveKind::Feed && m.from.z == m.to.z
}

/// `(above, below, moves between)` for every level change.
fn transitions(t: &Toolpath) -> Vec<(f64, f64, Vec<Move>)> {
    let mut out = Vec::new();
    let mut level: Option<f64> = None;
    let mut between = Vec::new();
    for m in &t.moves {
        if planar(m) {
            if let Some(z) = level
                && m.to.z < z - 1e-9
            {
                out.push((z, m.to.z, std::mem::take(&mut between)));
            }
            between.clear();
            level = Some(m.to.z);
        } else {
            between.push(*m);
        }
    }
    out
}

fn top(ms: &[Move]) -> f64 {
    ms.iter().map(|m| m.to.z.max(m.from.z)).fold(f64::NEG_INFINITY, f64::max)
}

// #1715: the three generating tests were each one loop over their
// cases (a full generate per case, 74 s for the binary in debug). Each
// case is its own test now, in a module with the old test's name, so
// libtest runs the cases in parallel. Each case builds its own world.

/// One contour case. Stock top 20: clearance 40.5, retract 33.5.
fn contours_clean(links: Links) {
    let (_w, _h, t, warns) = user_job(contour_op(links));
    assert!(warns.is_empty(), "{links:?}: {warns:?} / {:?}", t.notes);
    assert!(t.is_chained() && t.all_finite());
    let tr = transitions(&t);
    assert!(!tr.is_empty(), "{links:?}");
    for (above, below, ms) in &tr {
        if links.level == Link::Retract {
            assert!(top(ms) < 40.5, "{links:?}: {above} -> {below} stays under clearance");
        }
    }
    if links.level == Link::KeepDown {
        // Ring by ring: each ring's own ladder stays down (the only
        // rise is the move to the next ring's top, or a level the
        // notes name).
        let stayed = tr.iter().filter(|(above, _, ms)| top(ms) <= above + 1e-9).count();
        let rose = tr.len() - stayed;
        let named = t.notes.iter().filter(|n| n.starts_with("level link")).count();
        assert!(stayed >= 4, "{links:?}: the rings stay down: {stayed} of {}", tr.len());
        assert!(rose <= 1 + named, "{links:?}: {rose} rises, {named} named: {:?}", t.notes);
    }
}

mod the_users_part_contours_clean_under_every_level_link {
    use super::*;

    #[test]
    fn planar() {
        contours_clean(Links::PLANAR);
    }

    #[test]
    fn level_retract() {
        contours_clean(Links { level: Link::Retract, ..Links::PLANAR });
    }

    #[test]
    fn level_keep_down_ramp() {
        contours_clean(Links { level: Link::KeepDown, transition: Some(Entry3D::Ramp), ..Links::PLANAR });
    }

    #[test]
    fn level_keep_down_plunge() {
        contours_clean(Links { level: Link::KeepDown, transition: Some(Entry3D::Plunge), ..Links::PLANAR });
    }

    #[test]
    fn level_keep_down_helix() {
        contours_clean(Links { level: Link::KeepDown, transition: Some(Entry3D::Helix), helix_pitch: 2.5, ..Links::PLANAR });
    }

    #[test]
    fn level_keep_down_arc() {
        contours_clean(Links { level: Link::KeepDown, transition: Some(Entry3D::Arc), ..Links::PLANAR });
    }

    #[test]
    fn pass_keep_down() {
        contours_clean(Links { pass: Link::KeepDown, ..Links::PLANAR });
    }

    #[test]
    fn pass_clearance_level_retract() {
        contours_clean(Links { pass: Link::Clearance, level: Link::Retract, ..Links::PLANAR });
    }
}

/// One pocket case on the bore.
fn pockets_clean(entry: Entry3D, links: Links) {
    let (_w, _h, t, warns) = user_job(pocket_op(entry, links));
    assert!(warns.is_empty(), "{entry:?} {links:?}: {warns:?} / {:?}", t.notes);
    assert!(t.is_chained() && t.all_finite() && !t.moves.is_empty());
    if links.level == Link::Retract {
        for (above, below, ms) in transitions(&t) {
            assert!(top(&ms) <= 22.0 + 1e-9, "{above} -> {below}: retract plane 22, got {}", top(&ms));
        }
    }
    if links.level == Link::KeepDown {
        let said = t.notes.iter().any(|n| n.starts_with("link:"));
        for (above, below, ms) in transitions(&t) {
            assert!(top(&ms) <= above + 1e-9 || said, "{above} -> {below}: {:?}", t.notes);
        }
    }
}

mod the_users_part_pockets_its_bore_clean_under_every_choice {
    use super::*;

    #[test]
    fn plunge() {
        pockets_clean(Entry3D::Plunge, Links::ROUGH3D);
    }

    #[test]
    fn helix() {
        pockets_clean(Entry3D::Helix, Links::ROUGH3D);
    }

    #[test]
    fn plunge_helix_transition() {
        pockets_clean(Entry3D::Plunge, Links { transition: Some(Entry3D::Helix), helix_pitch: 1.0, ..Links::ROUGH3D });
    }

    #[test]
    fn plunge_ramp_transition() {
        pockets_clean(Entry3D::Plunge, Links { transition: Some(Entry3D::Ramp), ramp_angle: 5.0, ..Links::ROUGH3D });
    }

    #[test]
    fn helix_level_retract() {
        pockets_clean(Entry3D::Helix, Links { level: Link::Retract, ..Links::ROUGH3D });
    }

    #[test]
    fn helix_level_keep_down() {
        pockets_clean(Entry3D::Helix, Links { level: Link::KeepDown, ..Links::ROUGH3D });
    }

    #[test]
    fn plunge_level_keep_down_arc() {
        pockets_clean(Entry3D::Plunge, Links { level: Link::KeepDown, transition: Some(Entry3D::Arc), ..Links::ROUGH3D });
    }

    #[test]
    fn plunge_pass_retract() {
        pockets_clean(Entry3D::Plunge, Links { pass: Link::Retract, ..Links::ROUGH3D });
    }

    #[test]
    fn plunge_pass_keep_down_max_5() {
        pockets_clean(Entry3D::Plunge, Links { pass: Link::KeepDown, max_stay_down: 5.0, ..Links::ROUGH3D });
    }
}

#[test]
fn a_helix_entry_posts_continuous_helical_arcs() {
    let (world, _h, t, warns) = user_job(pocket_op(Entry3D::Helix, Links::ROUGH3D));
    assert!(warns.is_empty(), "{warns:?}");
    let helix: Vec<&Move> = t.moves.iter().filter(|m| m.arc.is_some() && m.to.z < m.from.z).collect();
    assert!(!helix.is_empty(), "{:?}", t.notes);
    let nc = std::env::temp_dir().join(format!("camcraft-level-links-helix-{}.nc", std::process::id()));
    posting::post_setup(&world, &nc).expect("posts");
    let text = std::fs::read_to_string(&nc).expect("written");
    let _ = std::fs::remove_file(&nc);
    // Every helical arc word carries its Z: G2/G3 with X, Y and Z.
    let helical = text
        .lines()
        .filter(|l| {
            let w: Vec<&str> = l.split_whitespace().collect();
            w.iter().any(|t| matches!(*t, "G2" | "G3" | "G02" | "G03"))
                && w.iter().any(|t| t.starts_with('Z'))
        })
        .count();
    assert!(helical >= helix.len(), "{helical} helical words for {} helix moves", helix.len());
}

/// XY distance from `q` to the triangle's projection (0 inside it).
fn to_tri(q: Point3, t: [Point3; 3]) -> f64 {
    let seg = |a: Point3, b: Point3| {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l2 = dx * dx + dy * dy;
        let s = if l2 > 0.0 { (((q.x - a.x) * dx + (q.y - a.y) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
        (a.x + s * dx - q.x).hypot(a.y + s * dy - q.y)
    };
    let side = |a: Point3, b: Point3| (b.x - a.x) * (q.y - a.y) - (b.y - a.y) * (q.x - a.x);
    let (s0, s1, s2) = (side(t[0], t[1]), side(t[1], t[2]), side(t[2], t[0]));
    if (s0 >= 0.0 && s1 >= 0.0 && s2 >= 0.0) || (s0 <= 0.0 && s1 <= 0.0 && s2 <= 0.0) {
        return 0.0;
    }
    seg(t[0], t[1]).min(seg(t[1], t[2])).min(seg(t[2], t[0]))
}

/// Found while building M11.2 (pre-existing, not a links defect): the
/// 3-D ladder run to the STOCK bottom on this part found a sliver level
/// at Z -7 under the part's underside (Z -5) - the plane sections no
/// part there, so the old islands law read it as open - and a helix
/// entry there orbited from Z 0 down through the bore's solid floor (the
/// depth-aware verifier convicted it, so it never posted silently). The
/// islands law now subtracts all the material standing above a level
/// (`machining3d::material_above`): under the closed floor no level
/// exists, so no move of any entry arm goes below the bore floor inside
/// the bore - nor anywhere under the part - and the path verifies clean.
fn never_under_the_bore_floor(entry: Entry3D) {
    let mut op = pocket_op(entry, Links::ROUGH3D);
    if let OpParams::Pocket3D { bottom_z, .. } = &mut op {
        *bottom_z = Height::of(HeightReference::BottomOfStock, 0.0);
    }
    let (w, _h, t, warns) = user_job(op);
    assert!(!t.moves.is_empty(), "{entry:?}: the bore is still cut");
    let in_bore = |p: &Point3| p.x.hypot(p.y) < 12.5;
    let under = t.moves.iter().find(|m| [m.from, m.to].iter().any(|q| in_bore(q) && q.z < -1e-6));
    assert!(under.is_none(), "{entry:?}: a move under the bore floor: {under:?}");
    // #1774: the stock AROUND the part is cleared down to the stock's
    // bottom (the 2D pocket's #12 reading); under the part's own
    // shadow nothing goes below Z 0 - every lower move keeps its
    // tool disc off the part's projected facets.
    let part = model::find_by_name(&w, "Milling-test").and_then(|e| model::world_facets(&w, &e)).expect("part facets");
    let low = t
        .moves
        .iter()
        .flat_map(|m| [m.from, m.to])
        .filter(|q| q.z < -1e-6)
        .map(|q| part.iter().map(|f| to_tri(q, [f.a, f.b, f.c])).fold(f64::INFINITY, f64::min))
        .fold(f64::INFINITY, f64::min);
    assert!(low >= 5.0 - 0.02, "{entry:?}: a move below Z 0 is {low} from the part's footprint");
    assert!(warns.is_empty(), "{entry:?}: {warns:?} / {:?}", t.notes);
}

mod a_helix_under_the_bore_floor_is_never_silent {
    use super::*;

    #[test]
    fn helix() {
        never_under_the_bore_floor(Entry3D::Helix);
    }

    #[test]
    fn plunge() {
        never_under_the_bore_floor(Entry3D::Plunge);
    }

    #[test]
    fn ramp() {
        never_under_the_bore_floor(Entry3D::Ramp);
    }

    #[test]
    fn arc() {
        never_under_the_bore_floor(Entry3D::Arc);
    }
}

#[path = "level_links/choices.rs"]
mod choices;
