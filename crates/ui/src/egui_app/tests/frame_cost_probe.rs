//! #2238: the frame-cost probe. Each named piece of an unchanged frame
//! (phases `commands`, `scene`, `ui`, the names `crate::frame_cost` spans
//! carry) is measured on fig8.step in five states: nothing selected, an
//! operation with its top edge chains, the 2D Adaptive area preview
//! (#2246), a generated toolpath, and the simulation transport. Debug
//! asserts counts, never times; the times print (`--nocapture`, release
//! for real numbers).
//! Provenance: our own frame-cost law over our own shell; no vendor
//! material consulted.

use std::time::{Duration, Instant};

use camcraft_app::geometry::feature;
use camcraft_app::operation::{self, AreaControls, Containment, GeoSel, OpType};

use super::*;
use crate::frame_cost::{self, Cost};

/// Still frames measured per state, after the warm-up frames.
const N: u32 = 6;
pub(super) const CONTOUR: &str = "Probe contour";
pub(super) const ADAPTIVE: &str = "Probe adaptive";
const FACE: &str = "Probe face";

fn fig8_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/fig8.step")
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Counts {
    regions: u64,
    walks: u64,
    digests: u64,
    slices: u64,
    patches: u64,
    selections: u64,
    pick_lines: u64,
}

fn counts() -> Counts {
    Counts {
        regions: crate::frame_memo::region_builds(),
        walks: model::chain_walks(),
        digests: model::feature_digests(),
        slices: operation::pocket_slices(),
        patches: model::patch_builds(),
        selections: operation::selection_walks(),
        pick_lines: crate::pick_line_memo::pick_line_builds(),
    }
}

/// What N still frames of one state cost.
struct Probe {
    costs: Vec<Cost>,
    /// `commands`, `scene`, `ui` wall time over the N frames.
    phases: [Duration; 3],
    /// Counter growth over the N frames.
    delta: Counts,
    /// The redraw delay the window would take after the last frame:
    /// `None` is "sleep until input".
    redraw: Option<Duration>,
}

pub(super) fn run(h: &mut Harness<'static, ShellUi>, st: impl FnOnce(&mut State)) {
    st(&mut h.state_mut().state);
    h.state_mut().state.settle();
    h.run_steps(2);
}

/// fig8.step beside the seed document, in a wide window.
pub(super) fn fig8() -> Harness<'static, ShellUi> {
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 1000.0)).build_ui_state(frame, ShellUi::seeded());
    h.run_steps(2);
    run(&mut h, |st| st.pending.push_back(CommandKind::ImportFile(fig8_path())));
    assert!(model::find_by_name(&h.state().state.world, "fig8").is_some(), "fig8 lands: {}", h.state().state.status);
    h
}

fn features(st: &State) -> std::sync::Arc<feature::Features> {
    let e = model::find_by_name(&st.world, "fig8").expect("fig8");
    model::features_of(&st.world, &e).expect("features")
}

/// Up to ten edge chains that run along the part's top.
pub(super) fn top_chains(st: &State) -> Vec<GeoSel> {
    let f = features(st);
    let runs = feature::edge_runs(&f);
    let top = runs
        .iter()
        .flat_map(|r| feature::chain_points(&f, r))
        .map(|q| q[2])
        .fold(f64::NEG_INFINITY, f64::max);
    runs.into_iter()
        .filter(|r| feature::chain_points(&f, r).iter().all(|q| (q[2] - top).abs() < 1e-6))
        .take(10)
        .map(|r| GeoSel { entity: "fig8".into(), feature: FeatureSel::Chain(r.id) })
        .collect()
}

/// A convex welded vertical wall with two horizontal rims of the same XY bounds.
/// Strip counts change when STEP refinement changes. A wall's projection
/// gives this fixture a closed area without the folds of a rounded fillet.
fn biggest_patch(st: &State) -> GeoSel {
    let f = features(st);
    let rim_bounds = |ring: &feature::PatchRing| {
        if !ring.closed || ring.verts.len() < 3 { return None; }
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for &v in &ring.verts {
            let p = f.verts.get(v as usize)?;
            for axis in 0..3 {
                lo[axis] = lo[axis].min(p[axis]);
                hi[axis] = hi[axis].max(p[axis]);
            }
        }
        (hi[2] - lo[2] < 1e-6 && hi[0] > lo[0] && hi[1] > lo[1]).then_some((lo, hi))
    };
    let p = feature::patches(&f).into_iter().filter_map(|p| {
        let rings = feature::patch_rings(&f, p.id)?;
        if rings.len() != 2 { return None; }
        let (lo0, hi0) = rim_bounds(&rings[0])?;
        let (lo1, hi1) = rim_bounds(&rings[1])?;
        if (lo0[2] - lo1[2]).abs() < 1e-6
            || (0..2).any(|axis| (lo0[axis] - lo1[axis]).abs() > 1e-6 || (hi0[axis] - hi1[axis]).abs() > 1e-6)
        { return None; }
        // A bore rim is convex. Keep the fill-cache fixture independent
        // of STEP refinement on the concave outer fig8 wall.
        let points: Vec<_> = rings[0].verts.iter().map(|&i| f.verts[i as usize]).collect();
        let mut turn = 0.0_f64;
        for i in 0..points.len() {
            let (a, b, c) = (points[i], points[(i + 1) % points.len()], points[(i + 2) % points.len()]);
            let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
            if cross.abs() > 1e-8 {
                if turn != 0.0 && cross.signum() != turn { return None; }
                turn = cross.signum();
            }
        }
        if turn == 0.0 { return None; }
        Some((p, (hi0[0] - lo0[0]) * (hi0[1] - lo0[1])))
    }).max_by(|a, b| a.1.total_cmp(&b.1)).map(|(p, _)| p)
        .expect("fig8 has a convex welded vertical wall with matching horizontal rims");
    GeoSel { entity: "fig8".into(), feature: FeatureSel::Patch(p.id) }
}

pub(super) fn create(h: &mut Harness<'static, ShellUi>, name: &str, kind: OpType, items: Vec<GeoSel>) {
    run(h, |st| {
        let seed = model::find_by_name(&st.world, "Contour1").and_then(|e| model::operation_of(&st.world, &e)).expect("seed op");
        st.pending.push_back(CommandKind::CreateOperation {
            name: name.to_owned(),
            tool: seed.tool.clone(),
            src: SrcRef::Stock,
            params: operation::retype_params(&seed.params, kind.ordinal()),
        });
        st.pending.push_back(CommandKind::SetOperationGeometry { name: name.to_owned(), items });
    });
}

fn open(h: &mut Harness<'static, ShellUi>, t: UiTarget) {
    run(h, |st| crate::actions::target(st, t));
}

/// Warm up, then measure N frames where nothing changes.
fn still(h: &mut Harness<'static, ShellUi>) -> Probe {
    h.run_steps(3);
    // The shell collects the scene on each frame; the harness does not.
    // One collect here is the first frame of the new input, so the window
    // measures still frames only.
    std::hint::black_box(world::collect(&h.state().state));
    let before = counts();
    let mut phases = [Duration::ZERO; 3];
    let ((), costs) = frame_cost::record(|| {
        for _ in 0..N {
            // Shell::draw's order: the queue, the scene and its digest,
            // then the UI pass (the harness frame drains once more).
            let t0 = Instant::now();
            h.state_mut().state.drain_actions();
            let t1 = Instant::now();
            let scene = frame_cost::time("scene/collect", || world::collect(&h.state().state));
            frame_cost::time("scene/digest", || std::hint::black_box(scene.digest()));
            let t2 = Instant::now();
            h.step();
            let t3 = Instant::now();
            for (k, d) in [t1 - t0, t2 - t1, t3 - t2].into_iter().enumerate() {
                phases[k] += d;
            }
        }
    });
    let after = counts();
    let delta = Counts {
        regions: after.regions - before.regions,
        walks: after.walks - before.walks,
        digests: after.digests - before.digests,
        slices: after.slices - before.slices,
        patches: after.patches - before.patches,
        selections: after.selections - before.selections,
        pick_lines: after.pick_lines - before.pick_lines,
    };
    let egui_delay =
        h.output().viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);
    let redraw = crate::egui_app::view_pass::state_redraw_delay(&h.state().state, egui_delay, false);
    Probe { costs, phases, delta, redraw }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0 / f64::from(N)
}

fn report(state: &str, p: &Probe) {
    println!(
        "== {state}: commands {:.2} ms, scene {:.2} ms, ui {:.2} ms per frame; {:?} per {N} frames; redraw {:?}",
        ms(p.phases[0]),
        ms(p.phases[1]),
        ms(p.phases[2]),
        p.delta,
        p.redraw
    );
    let mut costs = p.costs.clone();
    costs.sort_by_key(|c| std::cmp::Reverse(c.total));
    for c in costs {
        println!("   {:<40} {:>9.3} ms  {:>5.1}x", c.name, ms(c.total), f64::from(c.count) / f64::from(N));
    }
}

fn calls(p: &Probe, name: &str) -> u32 {
    p.costs.iter().find(|c| c.name == name).map_or(0, |c| c.count)
}

#[test]
fn still_frames_on_fig8_name_their_costs() {
    let mut h = fig8();

    // (a) Nothing selected.
    run(&mut h, |st| st.pending.push_back(CommandKind::Select(None)));
    open(&mut h, UiTarget::None);
    assert_eq!(h.state().state.pane_target(), UiTarget::None, "nothing is open");
    let a = still(&mut h);
    report("a nothing selected", &a);

    // (b) An operation open with its top edge chains.
    let chains = top_chains(&h.state().state);
    assert!(!chains.is_empty(), "fig8 has edge chains along its top");
    create(&mut h, CONTOUR, OpType::Contour, chains);
    open(&mut h, UiTarget::Operation(CONTOUR.into()));
    let b = still(&mut h);
    report("b chains selected", &b);

    // (c) 2D Adaptive, Containment Outside, one welded face: the
    // Boundary group open and its filled area preview shown (#2246).
    adaptive_preview(&mut h);
    let c = still(&mut h);
    report("c adaptive outside preview", &c);

    // (d) A generated toolpath on screen: a facing pass over the stock.
    create(&mut h, FACE, OpType::Face, Vec::new());
    run(&mut h, |st| st.pending.push_back(CommandKind::GenerateOperation(FACE.into())));
    open(&mut h, UiTarget::Operation(FACE.into()));
    let has_path = model::find_by_name(&h.state().state.world, FACE)
        .and_then(|e| model::toolpath_of(&h.state().state.world, &e))
        .is_some();
    assert!(has_path, "the facing pass generates: {}", h.state().state.status);
    let d = still(&mut h);
    report(&format!("d toolpath shown (generated: {has_path})"), &d);

    // (e) The simulation transport up, paused.
    h.state_mut().state.sim = Some(crate::state::Sim { t: 1.0, playing: false, speed: 1.0 });
    let e = still(&mut h);
    report("e simulation paused", &e);

    // Counts, not times: a still frame walks no chain, digests no facet
    // and slices no part in any state (#2235's law, now over fig8).
    for (name, p) in [("a", &a), ("b", &b), ("c", &c), ("d", &d), ("e", &e)] {
        assert_eq!((p.delta.walks, p.delta.digests), (0, 0), "{name}: no chain walk, no digest on a still frame");
        assert_eq!(p.delta.patches, 0, "{name}: no patch grouping on a still frame (#2247)");
        assert_eq!(p.delta.selections, 0, "{name}: no selection walk for the page on a still frame (#2248, #2249)");
        assert_eq!(p.delta.pick_lines, 0, "{name}: no pick line build on a still frame (#2250)");
        // Each piece of the scene is collected once per scene.
        assert_eq!(calls(p, "scene/collect"), N, "{name}");
        assert_eq!(calls(p, "scene/parts.models"), N, "{name}");
    }
    // The Boundary group draws only in (c): its preview refresh runs in
    // the UI pass, its fill in the scene.
    assert_eq!(calls(&c, "ui/properties.boundary.preview"), N);
    assert_eq!(calls(&a, "ui/properties.boundary.preview"), 0);
    // #2246: the fill draws every frame, but its triangles were made with
    // the preview - a still frame triangulates nothing.
    assert_eq!(calls(&c, "scene/parts.boundary_preview"), N);
    assert_eq!(calls(&c, "ui/properties.boundary.preview.triangulate"), 0, "c: no preview triangulation");
}

/// The 2D Adaptive operation of state (c), open with its Boundary group
/// and its filled area preview.
pub(super) fn adaptive_preview(h: &mut Harness<'static, ShellUi>) {
    let patch = biggest_patch(&h.state().state);
    adaptive_preview_with(h, patch, Some(Containment::Outside));
}

/// [`adaptive_preview`] on `patch` under the given Containment (`None`:
/// the default).
fn adaptive_preview_with(h: &mut Harness<'static, ShellUi>, patch: GeoSel, containment: Option<Containment>) {
    create(h, ADAPTIVE, OpType::Adaptive2D, vec![patch]);
    run(h, |st| {
        let op = model::find_by_name(&st.world, ADAPTIVE).and_then(|e| model::operation_of(&st.world, &e)).expect("adaptive");
        let area = AreaControls { containment, ..AreaControls::default() };
        st.pending.push_back(CommandKind::EditOperationBoundary { name: ADAPTIVE.into(), params: op.params, area });
    });
    open(h, UiTarget::Operation(ADAPTIVE.into()));
    if !h.state().state.boundary_view.shown {
        // The group remembers that (b) left it closed: open it as a user does.
        // The header's arrow sits left of its title.
        let r = h.get_by_label("Boundary").rect();
        let p = egui::pos2(r.left() - 10.0, r.center().y);
        h.input_mut().events.push(egui::Event::PointerMoved(p));
        h.run_steps(1);
        for pressed in [true, false] {
            let m = egui::Modifiers::NONE;
            h.input_mut().events.push(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: m });
            h.run_steps(1);
        }
        h.run_steps(2);
    }
    assert!(h.state().state.boundary_view.shown, "the Boundary group is open: {}", h.state().state.status);
    assert!(crate::boundary_viz::preview_fill(&h.state().state).is_some(), "the area preview is filled");
}

fn fill(h: &Harness<'static, ShellUi>) -> crate::boundary_viz::Fill {
    let p = h.state().state.boundary_view.preview.as_ref().expect("a preview");
    p.fill.clone().expect("a filled preview")
}

/// #2766: the default Containment fills too. The region of the patch is
/// the enclosed area minus the part that stands in it; that difference
/// left loops of about 1e-8 mm2, and the triangulator refused them. The
/// patch is the welded face with the most strips (the concave outer wall).
#[test]
fn area_preview_of_the_default_containment_is_filled() {
    let mut h = fig8();
    let f = features(&h.state().state);
    let wall = feature::patches(&f).into_iter().max_by_key(|p| p.faces.len()).expect("fig8 has a welded face");
    adaptive_preview_with(&mut h, GeoSel { entity: "fig8".into(), feature: FeatureSel::Patch(wall.id) }, None);
    let st = &h.state().state;
    let op = model::find_by_name(&st.world, ADAPTIVE).and_then(|e| model::operation_of(&st.world, &e)).expect("adaptive");
    assert_eq!(op.area.containment, None, "the default Containment");
    let region = operation::region_of(&st.world, &op).expect("a region");
    let slivers: Vec<f64> = region.loops().iter().map(|l| l.area()).filter(|a| a.abs() < 1e-4).collect();
    assert!(slivers.is_empty(), "no loop of near-zero area: {slivers:?}");
    let tri = |t: &[camcraft_app::geo2d::Point2; 3]| 0.5 * ((t[1].x - t[0].x) * (t[2].y - t[0].y) - (t[2].x - t[0].x) * (t[1].y - t[0].y)).abs();
    let filled: f64 = fill(&h).iter().map(tri).sum();
    assert!((filled - region.area()).abs() < 1e-6 * region.area(), "the fill covers the region: {filled} of {}", region.area());
}

/// #2246: the preview's triangles are made when the draft or the document
/// moves, and only then - the key is the draft plus the document version.
#[test]
fn area_preview_fill_is_made_once_per_draft() {
    let mut h = fig8();
    adaptive_preview(&mut h);
    let outside = fill(&h);
    h.run_steps(3);
    assert!(std::sync::Arc::ptr_eq(&outside, &fill(&h)), "still frames keep the same triangles");

    // A draft edit (not yet applied) makes new triangles.
    let set = |h: &mut Harness<'static, ShellUi>, c: Containment| {
        h.state_mut().state.op_ui.area.containment = Some(c);
        h.run_steps(2);
    };
    set(&mut h, Containment::Inside);
    let inside = fill(&h);
    assert_ne!(&*inside, &*outside, "Inside fills another area than Outside");
    set(&mut h, Containment::Outside);
    let back = fill(&h);
    assert!(!std::sync::Arc::ptr_eq(&back, &outside), "the draft moved, so the fill was made again");
    assert_eq!(&*back, &*outside, "the same draft gives the same triangles");

    // A committed change and its undo move the document: the fill follows.
    run(&mut h, |st| {
        let op = model::find_by_name(&st.world, ADAPTIVE).and_then(|e| model::operation_of(&st.world, &e)).expect("adaptive");
        let area = AreaControls { containment: Some(Containment::Inside), ..AreaControls::default() };
        st.pending.push_back(CommandKind::EditOperationBoundary { name: ADAPTIVE.into(), params: op.params, area });
    });
    set(&mut h, Containment::Inside);
    assert_eq!(&*fill(&h), &*inside, "the applied Inside fills the Inside area");
    run(&mut h, |st| st.pending.push_back(CommandKind::Undo));
    set(&mut h, Containment::Outside);
    let undone = fill(&h);
    assert!(!std::sync::Arc::ptr_eq(&undone, &back), "the undo moved the document, so the fill was made again");
    assert_eq!(&*undone, &*outside, "after the undo the Outside area fills again");
}
