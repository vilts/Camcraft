//! #2235: a chain selection on an open pocket costs its stitch, its chain
//! walks and its part slices once - at the change - and never again on
//! a frame where nothing changed. A selection change or a model landing
//! is a new answer, equal to the fresh compute.
//! Provenance: our own frame-cost law over our own shell; no vendor
//! material consulted.

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Mesh, Point3, Triangle, feature};
use camcraft_app::operation::{self, GeoSel, OpType};

use super::*;

const TOP: f64 = 10.0;

/// Two triangles over the quad `a b c d` (counter-clockwise seen from
/// the outward side).
fn quad(t: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3, d: Point3) {
    t.push(Triangle::new(a, b, c));
    t.push(Triangle::new(a, c, d));
}

/// A square frame `o` wide and 10 high with a 20x20 hole through it
/// (a pocket the part never fills), beside the seed.
fn tray(o: f64) -> Vec<Triangle> {
    let p = |x: f64, y: f64, z: f64| Point3::new(200.0 + x, y, z);
    let mut t = Vec::new();
    let (i0, i1) = (20.0, 40.0);
    // Outer walls.
    quad(&mut t, p(0.0, 0.0, 0.0), p(o, 0.0, 0.0), p(o, 0.0, TOP), p(0.0, 0.0, TOP));
    quad(&mut t, p(o, 0.0, 0.0), p(o, o, 0.0), p(o, o, TOP), p(o, 0.0, TOP));
    quad(&mut t, p(o, o, 0.0), p(0.0, o, 0.0), p(0.0, o, TOP), p(o, o, TOP));
    quad(&mut t, p(0.0, o, 0.0), p(0.0, 0.0, 0.0), p(0.0, 0.0, TOP), p(0.0, o, TOP));
    // The top (faces up) and bottom (faces down) annuli, four quads each.
    for (z, up) in [(TOP, true), (0.0, false)] {
        let ring = [(0.0, 0.0, i0, i0), (o, 0.0, i1, i0), (o, o, i1, i1), (0.0, o, i0, i1)];
        for k in 0..4 {
            let (ax, ay, ix, iy) = ring[k];
            let (bx, by, jx, jy) = ring[(k + 1) % 4];
            let (a, b, c, d) = (p(ax, ay, z), p(bx, by, z), p(jx, jy, z), p(ix, iy, z));
            if up { quad(&mut t, a, b, c, d) } else { quad(&mut t, d, c, b, a) }
        }
    }
    // The hole's walls (face inward).
    quad(&mut t, p(i0, i0, 0.0), p(i0, i0, TOP), p(i1, i0, TOP), p(i1, i0, 0.0));
    quad(&mut t, p(i1, i0, 0.0), p(i1, i0, TOP), p(i1, i1, TOP), p(i1, i1, 0.0));
    quad(&mut t, p(i1, i1, 0.0), p(i1, i1, TOP), p(i0, i1, TOP), p(i0, i1, 0.0));
    quad(&mut t, p(i0, i1, 0.0), p(i0, i1, TOP), p(i0, i0, TOP), p(i0, i0, 0.0));
    t
}

/// The tray's top-rim chains: the cavity's (`inner`) or the outline's.
fn rim(st: &State, inner: bool) -> Vec<GeoSel> {
    let e = model::find_by_name(&st.world, "tray").expect("tray");
    let f = model::features_of(&st.world, &e).expect("features");
    feature::edge_runs(&f)
        .into_iter()
        .filter(|r| {
            let pts = feature::chain_points(&f, r);
            pts.iter().all(|q| (q[2] - TOP).abs() < 1e-9)
                && pts.iter().all(|q| (q[0] > 215.0 && q[0] < 245.0) == inner)
        })
        .map(|r| GeoSel { entity: "tray".into(), feature: FeatureSel::Chain(r.id) })
        .collect()
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Counts {
    regions: u64,
    walks: u64,
    digests: u64,
    slices: u64,
}

fn counts() -> Counts {
    Counts {
        regions: crate::frame_memo::region_builds(),
        walks: model::chain_walks(),
        digests: model::feature_digests(),
        slices: operation::pocket_slices(),
    }
}

/// The region the overlay draws for `name`, against a fresh compute.
fn drawn_is_fresh(st: &State, name: &str) -> Option<camcraft_app::geo2d::Region> {
    let op = model::find_by_name(&st.world, name).and_then(|e| model::operation_of(&st.world, &e)).expect("op");
    let drawn = crate::frame_memo::scene_scope(|| crate::frame_memo::region_of(&st.world, name, &op));
    assert_eq!(drawn, operation::region_of(&st.world, &op), "the drawn region is the fresh one");
    drawn
}

fn select(h: &mut Harness<'static, ShellUi>, name: &str, items: Vec<GeoSel>) {
    let st = &mut h.state_mut().state;
    st.pending.push_back(CommandKind::SetOperationGeometry { name: name.to_owned(), items });
    st.drain_actions();
    h.run_steps(2);
}

/// The tray beside the seed, a 2D Pocket open on its cavity rim.
fn pocket_on_tray() -> (Harness<'static, ShellUi>, String) {
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 1000.0)).build_ui_state(frame, ShellUi::seeded());
    h.run_steps(2);
    let name = "Pocket2".to_owned();
    {
        let st = &mut h.state_mut().state;
        st.history.execute(&st.world, ImportModel::from_mesh("tray", Mesh::from_facets(tray(60.0))));
        let seed = model::find_by_name(&st.world, "Contour1").and_then(|e| model::operation_of(&st.world, &e)).expect("seed op");
        st.pending.push_back(CommandKind::CreateOperation {
            name: name.clone(),
            tool: seed.tool.clone(),
            src: SrcRef::Stock,
            params: operation::retype_params(&seed.params, OpType::Pocket.ordinal()),
        });
        st.drain_actions();
        crate::actions::target(st, UiTarget::Operation(name.clone()));
    }
    h.run_steps(2);
    let items = rim(&h.state().state, true);
    assert_eq!(items.len(), 4, "the cavity rim is four runs");
    select(&mut h, &name, items);
    (h, name)
}

#[test]
fn frames_without_a_change_walk_digest_and_slice_nothing() {
    let (mut h, name) = pocket_on_tray();
    let region = drawn_is_fresh(&h.state().state, &name);
    assert!(region.is_some(), "the cavity rim is a pocket region: {}", h.state().state.status);
    h.run_steps(2);
    let before = counts();
    const N: u64 = 10;
    for _ in 0..N {
        h.step();
    }
    let after = counts();
    assert_eq!(after.walks, before.walks, "no chain walk on a still frame");
    assert_eq!(after.digests, before.digests, "no feature digest on a still frame");
    assert_eq!(after.slices, before.slices, "no part slice on a still frame");
    // #2201: the shown region and the Pocket page's note keep their region
    // across frames under its input: a still frame builds none.
    assert_eq!(after.regions, before.regions, "no region build on a still frame");
}

#[test]
fn a_selection_change_or_a_landing_is_a_new_answer() {
    let (mut h, name) = pocket_on_tray();
    let cavity = drawn_is_fresh(&h.state().state, &name);
    h.run_steps(2);

    // Another selection: the outline rim. The answer is computed again.
    let before = counts();
    let outline = rim(&h.state().state, false);
    assert_eq!(outline.len(), 4, "the outline rim is four runs");
    select(&mut h, &name, outline);
    let after = counts();
    assert!(after.walks > before.walks, "a new selection walks its chains");
    assert!(after.slices > before.slices, "a new selection slices the part");
    assert_ne!(drawn_is_fresh(&h.state().state, &name), cavity, "the region follows the selection");

    // Back to the cavity, then a landing that widens the frame.
    let items = rim(&h.state().state, true);
    select(&mut h, &name, items);
    let before = counts();
    {
        let st = &h.state().state;
        let e = model::find_by_name(&st.world, "tray").expect("tray");
        model::set_facets(&st.world, &e, tray(70.0).into());
    }
    h.run_steps(2);
    let after = counts();
    assert!(after.digests > before.digests, "new facets take a new digest");
    assert!(after.walks > before.walks, "new features walk the chains again");
    drawn_is_fresh(&h.state().state, &name);
    let before = counts();
    h.run_steps(3);
    let after = counts();
    assert_eq!(after, before, "and the frames after it are still");
}
