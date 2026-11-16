//! Integration and unit tests for the 2D toolpath shape-mask verifier (#1768).
//! Gate map: covers camcraft_motion::machining2d (adaptive, pocket, pocket_hsm, slots, slot_ends, trochoid_slot).

mod common;

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use camcraft_motion::cl::{MoveKind, PathBuilder, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::Point3;
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{
    PocketParams, PocketStrategy, SlotParams, pocket, slot, slot_extended_ends,
};
use common::shape_mask::{CutDepth, MaskSpec, ShapeMask, Symmetry};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let lp = Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ]);
    let Ok(lp) = lp else { panic!("rect loop") };
    let Ok(reg) = Region::new(vec![lp]) else { panic!("rect region") };
    reg
}

fn scratch_dir(tag: &str) -> PathBuf {
    common::png::default_diff_dir().join(tag)
}

#[test]
fn shape_mask_scratch_dirs_preserve_other_runs() {
    let first = scratch_dir("independent_runs");
    fs::create_dir_all(&first).expect("create first run directory");
    let sentinel = first.join("first-run.txt");
    fs::write(&sentinel, b"keep first run").expect("write first run sentinel");
    let second = scratch_dir("independent_runs");
    assert!(sentinel.is_file(), "allocating a scratch directory deleted another run");
    assert_ne!(first, second, "scratch directories must be unique per allocation");
    fs::remove_dir_all(first).expect("remove first run directory");
    if second.exists() {
        fs::remove_dir_all(second).expect("remove second run directory");
    }
}

#[test]
fn shape_mask_default_diff_dirs_are_independent() {
    let first = common::png::default_diff_dir();
    let second = common::png::default_diff_dir();
    assert_ne!(first, second, "default diagnostic directories must not share output");
    fs::remove_dir_all(first).expect("remove first default directory");
    fs::remove_dir_all(second).expect("remove second default directory");
}

fn line_path(pts: &[(f64, f64, f64)]) -> Toolpath {
    let mut b = PathBuilder::approached(Point3::new(pts[0].0, pts[0].1, 5.0));
    b.push(MoveKind::Rapid, Point3::new(pts[0].0, pts[0].1, 5.0), None);
    b.push(MoveKind::Plunge, Point3::new(pts[0].0, pts[0].1, pts[0].2), Some(200.0));
    for &(x, y, z) in &pts[1..] {
        b.push(MoveKind::Feed, Point3::new(x, y, z), Some(600.0));
    }
    b.push(MoveKind::Rapid, Point3::new(pts[pts.len() - 1].0, pts[pts.len() - 1].1, 5.0), None);
    b.finish()
}

#[test]
fn shape_mask_ramp_only_clears_the_selected_depth_interval() {
    let Ok(expected) = Region::sweep(&[vec![Point2::new(-7.0, 0.0), Point2::new(7.0, 0.0)]], 3.0)
    else {
        panic!("stadium")
    };
    let path = line_path(&[(-7.0, 0.0, 0.0), (7.0, 0.0, -4.0)]);
    let dir = scratch_dir("ramp_depth");
    for depth in [
        CutDepth::AtOrBelow(-4.0),
        CutDepth::NearZ {
            z: -4.0,
            tol: 0.001,
        },
    ] {
        let spec = MaskSpec {
            name: "ramp_depth",
            expected: &expected,
            path: &path,
            tool_diameter: 6.0,
            cell: 0.25,
            depth,
            diff_dir: Some(&dir),
        };
        let Ok(mask) = ShapeMask::evaluate(&spec) else {
            panic!("evaluate")
        };
        // Only the final cutter disc reaches the floor, leaving most of the stadium uncut.
        assert!(mask.metrics().uncut_cells > mask.metrics().expected_cells / 2);
        assert!(
            catch_unwind(AssertUnwindSafe(|| mask.assert_bounded_leftover(0.0, None))).is_err()
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn shape_mask_ramp_does_not_preclear_a_floor_return() {
    let expected = rect(-10.0, -3.0, 10.0, 3.0);
    let path = line_path(&[(-7.0, 0.0, 0.0), (7.0, 0.0, -4.0), (-7.0, 0.0, -4.0)]);
    let dir = scratch_dir("ramp_floor_return");
    let spec = MaskSpec {
        name: "ramp_floor_return",
        expected: &expected,
        path: &path,
        tool_diameter: 6.0,
        cell: 0.25,
        depth: CutDepth::AtOrBelow(-4.0),
        diff_dir: Some(&dir),
    };
    let Ok(mask) = ShapeMask::evaluate(&spec) else {
        panic!("evaluate")
    };
    assert!(mask.metrics().max_cut_width > 3.0);
    assert!(catch_unwind(AssertUnwindSafe(|| mask.assert_max_engagement(0.25))).is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn shape_mask_descending_helix_does_not_preclear_its_floor() {
    let expected = rect(-4.0, -4.0, 4.0, 4.0);
    let mut b = PathBuilder::approached(Point3::new(3.0, 0.0, 0.0));
    let center = Point2::new(0.0, 0.0);
    for to in [
        Point3::new(-3.0, 0.0, -2.0),
        Point3::new(3.0, 0.0, -4.0),
        Point3::new(-3.0, 0.0, -4.0),
        Point3::new(3.0, 0.0, -4.0),
    ] {
        b.push_arc(MoveKind::Feed, to, Some(600.0), center, true);
    }
    let path = b.finish();
    let dir = scratch_dir("helix_floor");
    let spec = MaskSpec {
        name: "helix_floor",
        expected: &expected,
        path: &path,
        tool_diameter: 2.0,
        cell: 0.125,
        depth: CutDepth::AtOrBelow(-4.0),
        diff_dir: Some(&dir),
    };
    let Ok(mask) = ShapeMask::evaluate(&spec) else {
        panic!("evaluate")
    };
    assert!(mask.metrics().max_cut_width > 0.25 + 1.5 * spec.cell);
    assert!(catch_unwind(AssertUnwindSafe(|| mask.assert_max_engagement(0.25))).is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn shape_mask_precleared_bore_respects_its_radius_and_depth() {
    let circle = |radius| {
        let Ok(region) = Region::sweep(&[vec![Point2::new(0.0, 0.0)]], radius) else {
            panic!("circle")
        };
        region
    };
    let orbit = |radius| {
        let mut b = PathBuilder::approached(Point3::new(radius, 0.0, -2.0));
        for x in [-radius, radius] {
            b.push_arc(
                MoveKind::Feed,
                Point3::new(x, 0.0, -2.0),
                Some(600.0),
                Point2::new(0.0, 0.0),
                true,
            );
        }
        b.finish()
    };
    let expected = circle(8.0);
    let bore = circle(5.0);
    let path = orbit(4.2);
    let dir = scratch_dir("precleared_bore");
    let spec = MaskSpec {
        name: "precleared_bore",
        expected: &expected,
        path: &path,
        tool_diameter: 2.0,
        cell: 0.125,
        depth: CutDepth::AtOrBelow(-2.0),
        diff_dir: Some(&dir),
    };
    let Ok(mask) = ShapeMask::evaluate_with_precleared(&spec, &bore, -2.0) else {
        panic!("evaluate precleared")
    };
    // A 5 mm starter bore leaves a 0.2 mm radial cut on this 4.2 mm orbit.
    mask.assert_max_engagement(0.5);
    let Ok(solid) = ShapeMask::evaluate(&spec) else { panic!("evaluate solid") };
    assert!(catch_unwind(AssertUnwindSafe(|| solid.assert_max_engagement(0.5))).is_err());

    for (region, floor) in [(&circle(3.0), -2.0), (&bore, -1.0)] {
        let Ok(insufficient) = ShapeMask::evaluate_with_precleared(&spec, region, floor) else {
            panic!("evaluate insufficient bore")
        };
        assert!(
            catch_unwind(AssertUnwindSafe(|| insufficient.assert_max_engagement(0.5))).is_err()
        );
    }

    // Starting stock is independent of the generated path's shape coverage.
    // The unvisited bore centre must still be reported as unswept by this path.
    assert_eq!(mask.metrics().swept_cells, solid.metrics().swept_cells);
    assert_eq!(mask.metrics().uncut_cells, solid.metrics().uncut_cells);
    assert!(catch_unwind(AssertUnwindSafe(|| mask.assert_bounded_leftover(0.0, None))).is_err());

    // The same bore cannot conceal a full-width orbit outside its radius.
    let outside = orbit(6.0);
    let outside_spec = MaskSpec { path: &outside, ..spec };
    let Ok(outside_mask) = ShapeMask::evaluate_with_precleared(&outside_spec, &bore, -2.0) else {
        panic!("evaluate outside bore")
    };
    assert!(catch_unwind(AssertUnwindSafe(|| outside_mask.assert_max_engagement(0.5))).is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn shape_mask_catches_overcut_and_writes_diff_png() {
    let expected = rect(-10.0, -5.0, 10.0, 5.0);
    // D=6 (r=3): y=0 keeps edges inside [-3, 3], y=4 extends edge to y=7 (2 mm outside [-5, 5]).
    let path = line_path(&[(-7.0, 0.0, -2.0), (7.0, 0.0, -2.0), (7.0, 4.0, -2.0)]);
    let dir = scratch_dir("overcut");
    let spec = MaskSpec {
        name: "overcut_unit",
        expected: &expected,
        path: &path,
        tool_diameter: 6.0,
        cell: 0.25,
        depth: CutDepth::AllCut,
        diff_dir: Some(&dir),
    };
    let Ok(mask) = ShapeMask::evaluate(&spec) else { panic!("evaluate") };
    assert!(mask.metrics().overcut_cells > 0);
    let res = catch_unwind(AssertUnwindSafe(|| mask.assert_no_overcut()));
    assert!(res.is_err(), "expected overcut assertion to fail");
    let png_path = dir.join("overcut_unit.png");
    let Ok(bytes) = fs::read(&png_path) else { panic!("expected diff png at {png_path:?}") };
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "invalid PNG signature");
    assert!(bytes.windows(4).any(|w| w == b"IHDR"));
    assert!(bytes.windows(4).any(|w| w == b"IDAT"));
    assert!(bytes.windows(4).any(|w| w == b"IEND"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn shape_mask_catches_out_of_grid_escape_without_oom() {
    let expected = rect(-10.0, -10.0, 10.0, 10.0);
    let path = line_path(&[(0.0, 0.0, -1.0), (50_000.0, 0.0, -1.0)]);
    let dir = scratch_dir("oob");
    let spec = MaskSpec {
        name: "escape_unit",
        expected: &expected,
        path: &path,
        tool_diameter: 6.0,
        cell: 0.25,
        depth: CutDepth::AllCut,
        diff_dir: Some(&dir),
    };
    let Ok(mask) = ShapeMask::evaluate(&spec) else { panic!("evaluate should handle escape without OOM") };
    assert!(mask.metrics().out_of_grid_moves > 0);
    assert!(catch_unwind(AssertUnwindSafe(|| mask.assert_no_overcut())).is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn shape_mask_catches_disallowed_interior_leftover_and_allows_stated_corners() {
    // 20x20 square pocket, cutter diameter 6 (r=3): pass covers top/bottom bands leaving center strip uncut,
    // plus small sharp-corner cusps.
    let expected = rect(-10.0, -10.0, 10.0, 10.0);
    let mut b = PathBuilder::approached(Point3::new(-7.0, -7.0, 5.0));
    b.push(MoveKind::Rapid, Point3::new(-7.0, -7.0, 5.0), None);
    b.push(MoveKind::Plunge, Point3::new(-7.0, -7.0, -1.0), Some(200.0));
    b.push(MoveKind::Feed, Point3::new(7.0, -7.0, -1.0), Some(600.0));
    b.push(MoveKind::Feed, Point3::new(7.0, 7.0, -1.0), Some(600.0));
    b.push(MoveKind::Feed, Point3::new(-7.0, 7.0, -1.0), Some(600.0));
    b.push(MoveKind::Feed, Point3::new(-7.0, -7.0, -1.0), Some(600.0));
    b.push(MoveKind::Rapid, Point3::new(-7.0, -7.0, 5.0), None);
    let path = b.finish();
    let dir = scratch_dir("leftover");
    let spec = MaskSpec {
        name: "leftover_unit",
        expected: &expected,
        path: &path,
        tool_diameter: 6.0,
        cell: 0.25,
        depth: CutDepth::AllCut,
        diff_dir: Some(&dir),
    };
    let Ok(mask) = ShapeMask::evaluate(&spec) else { panic!("evaluate") };
    // Interior core [-4, 4] x [-4, 4] stays untouched -> must fail when no leftover allowed.
    assert!(mask.metrics().interior_uncut_cells > 0);
    assert!(catch_unwind(AssertUnwindSafe(|| mask.assert_bounded_leftover(25.0, None))).is_err());

    // When the unreached interior island + 4 corner cusps are explicitly allowed, and total area cap permits it, passes.
    let corners = expected.difference(&rect(-7.0, -7.0, 7.0, 7.0));
    let Ok(corners) = corners else { panic!("corners") };
    let Ok(allowed) = rect(-4.5, -4.5, 4.5, 4.5).union(&corners) else { panic!("allowed") };
    mask.assert_bounded_leftover(100.0, Some(&allowed));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn shape_mask_catches_over_stepover_engagement_on_planar_cut() {
    let expected = rect(-20.0, -12.0, 20.0, 12.0);
    // D=10 (r=5). First pass sweeps y=-7 (covers y in [-12, -2]).
    // Second pass at y=0 steps 7 mm into stock (> 3 mm stepover cap), while first move is a plunge.
    let mut b = PathBuilder::approached(Point3::new(-15.0, -7.0, 5.0));
    b.push(MoveKind::Rapid, Point3::new(-15.0, -7.0, 5.0), None);
    b.push(MoveKind::Plunge, Point3::new(-15.0, -7.0, -2.0), Some(200.0));
    b.push(MoveKind::Feed, Point3::new(15.0, -7.0, -2.0), Some(600.0));
    b.push(MoveKind::Rapid, Point3::new(15.0, -7.0, 5.0), None);
    b.push(MoveKind::Rapid, Point3::new(15.0, 0.0, 5.0), None);
    b.push(MoveKind::Plunge, Point3::new(15.0, 0.0, -2.0), Some(200.0));
    b.push(MoveKind::Feed, Point3::new(-15.0, 0.0, -2.0), Some(600.0));
    b.push(MoveKind::Rapid, Point3::new(-15.0, 0.0, 5.0), None);
    let overloaded = b.finish();
    let dir = scratch_dir("engage");
    let spec = MaskSpec {
        name: "engage_overloaded",
        expected: &expected,
        path: &overloaded,
        tool_diameter: 10.0,
        cell: 0.25,
        depth: CutDepth::AllCut,
        diff_dir: Some(&dir),
    };
    let Ok(mask) = ShapeMask::evaluate(&spec) else { panic!("evaluate") };
    // Full slot at y=-7 measures ~10 mm (> 3 mm), failing engagement cap.
    assert!(catch_unwind(AssertUnwindSafe(|| mask.assert_max_engagement(3.0))).is_err());

    // Now simulate valid light side-milling: open the whole slot at or below Z=-2 via a slight ramp, then plunge/open end at
    // (15, -5) and side-mill back to (-15, -5) at 2 mm radial stepover.
    let mut ok = PathBuilder::approached(Point3::new(-15.0, -7.0, 5.0));
    ok.push(MoveKind::Rapid, Point3::new(-15.0, -7.0, 5.0), None);
    ok.push(MoveKind::Plunge, Point3::new(-15.0, -7.0, -2.0), Some(200.0));
    ok.push(MoveKind::Feed, Point3::new(15.0, -7.0, -2.01), Some(400.0)); // the entire ramp reaches the measured floor
    ok.push(MoveKind::Rapid, Point3::new(15.0, -7.0, 5.0), None);
    ok.push(MoveKind::Rapid, Point3::new(15.0, -5.0, 5.0), None);
    ok.push(MoveKind::Plunge, Point3::new(15.0, -5.0, -2.0), Some(200.0)); // non-planar bore at end
    ok.push(MoveKind::Feed, Point3::new(-12.0, -5.0, -2.0), Some(600.0)); // 2 mm planar side-mill stepover
    ok.push(MoveKind::Rapid, Point3::new(-12.0, -5.0, 5.0), None);
    let light_path = ok.finish();
    let spec_ok = MaskSpec {
        name: "engage_ok",
        expected: &expected,
        path: &light_path,
        tool_diameter: 10.0,
        cell: 0.25,
        depth: CutDepth::NearZ { z: -2.0, tol: 1e-3 },
        diff_dir: Some(&dir),
    };
    let Ok(mask_ok) = ShapeMask::evaluate(&spec_ok) else { panic!("evaluate ok") };
    mask_ok.assert_max_engagement(2.5);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn shape_mask_checks_symmetry_and_stock_extension() {
    let expected = rect(-10.0, -6.0, 10.0, 6.0);
    let stock = rect(-8.0, -6.0, 8.0, 6.0);
    // Symmetric rectangle cut with D=6 covering [-10,10] x [-6,6].
    let mut b = PathBuilder::approached(Point3::new(-7.0, -3.0, 5.0));
    b.push(MoveKind::Rapid, Point3::new(-7.0, -3.0, 5.0), None);
    b.push(MoveKind::Plunge, Point3::new(-7.0, -3.0, -2.0), Some(200.0));
    b.push(MoveKind::Feed, Point3::new(7.0, -3.0, -2.0), Some(600.0));
    b.push(MoveKind::Feed, Point3::new(7.0, 3.0, -2.0), Some(600.0));
    b.push(MoveKind::Feed, Point3::new(-7.0, 3.0, -2.0), Some(600.0));
    b.push(MoveKind::Feed, Point3::new(-7.0, -3.0, -2.0), Some(600.0));
    b.push(MoveKind::Rapid, Point3::new(-7.0, -3.0, 5.0), None);
    let sym_path = b.finish();
    let dir = scratch_dir("sym");
    let spec = MaskSpec {
        name: "sym_ok",
        expected: &expected,
        path: &sym_path,
        tool_diameter: 6.0,
        cell: 0.25,
        depth: CutDepth::AllCut,
        diff_dir: Some(&dir),
    };
    let Ok(mask) = ShapeMask::evaluate(&spec) else { panic!("evaluate") };
    mask.assert_symmetric(Symmetry::MirrorX(0.0));
    mask.assert_symmetric(Symmetry::MirrorY(0.0));
    mask.assert_symmetric(Symmetry::Rotate180(0.0, 0.0));
    mask.assert_within_stock_extension(&stock, 2.0);
    // Tighter extension (1.0 < 2.0 actual overhang) must fail.
    assert!(catch_unwind(AssertUnwindSafe(|| mask.assert_within_stock_extension(&stock, 1.0))).is_err());

    // Asymmetric cut inside symmetric region fails MirrorX.
    let asym = line_path(&[(-7.0, 0.0, -2.0), (2.0, 0.0, -2.0)]);
    let spec_asym = MaskSpec {
        name: "sym_fail",
        expected: &expected,
        path: &asym,
        tool_diameter: 6.0,
        cell: 0.25,
        depth: CutDepth::AllCut,
        diff_dir: Some(&dir),
    };
    let Ok(mask_asym) = ShapeMask::evaluate(&spec_asym) else { panic!("evaluate asym") };
    assert!(catch_unwind(AssertUnwindSafe(|| mask_asym.assert_symmetric(Symmetry::MirrorX(0.0)))).is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn slot_extension_mask_matches_extended_stadium_and_stock_bounds() {
    // #1731 shape: slot (-15, 0) -> (15, 0), width 8, tool diameter 4, extend_start=6, extend_end=4.
    let a = Point2::new(-15.0, 0.0);
    let b = Point2::new(15.0, 0.0);
    let width = 8.0;
    let ext_start = 6.0;
    let ext_end = 4.0;
    let Ok((ea, eb)) = slot_extended_ends(a, b, ext_start, ext_end) else { panic!("slot_extended_ends") };
    let Ok(stock) = Region::sweep(&[vec![a, b]], 0.5 * width) else { panic!("stock") };
    let Ok(expected) = Region::sweep(&[vec![ea, eb]], 0.5 * width) else { panic!("expected") };

    let p = SlotParams {
        tool_diameter: 4.0,
        width,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        retract_z: Some(2.0),
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 4.0,
        stepover: 0.5,
        feed: 600.0,
        plunge_feed: 200.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        links: Links::PLANAR,
    };
    let Ok(path) = slot(ea, eb, &p) else { panic!("slot path") };
    let spec = MaskSpec {
        name: "slot_extension_1731",
        expected: &expected,
        path: &path,
        tool_diameter: p.tool_diameter,
        cell: 0.25,
        depth: CutDepth::AtOrBelow(-4.0),
        diff_dir: None,
    };
    let Ok(mask) = ShapeMask::evaluate(&spec) else { panic!("evaluate") };
    mask.assert_no_overcut();
    mask.assert_bounded_leftover(10.0, None);
    mask.assert_symmetric(Symmetry::MirrorY(0.0));
    mask.assert_within_stock_extension(&stock, ext_start.max(ext_end));
}

#[test]
fn open_band_mask_clears_band_within_open_end_extension() {
    // #1735 shape: stock [-8, 8] x [-35, 35] with open +/-Y ends extended by r + 1.0 = 6.0.
    let tool_d = 10.0;
    let ext = 0.5 * tool_d + 1.0;
    let stock = rect(-8.0, -35.0, 8.0, 35.0);
    let expected = rect(-8.0, -35.0 - ext, 8.0, 35.0 + ext);
    let p = PocketParams {
        tool_diameter: tool_d,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        top_z: 0.0,
        bottom_z: -5.0,
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 250.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        retract_z: Some(2.0),
        entry: Entry3D::Plunge,
        helix_radius: 2.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        blend_radius: None,
        links: Links::PLANAR,
    };
    let Ok(path) = pocket(&expected, &p) else { panic!("open band pocket") };
    // Corner cusps at the outer extended ends [-8,8] x [-(35+ext), (35+ext)] sit outside the stock strip.
    let Ok(allowed_corners) = expected.difference(&stock) else { panic!("allowed corners") };
    let spec = MaskSpec {
        name: "open_band_1735",
        expected: &expected,
        path: &path,
        tool_diameter: tool_d,
        cell: 0.25,
        depth: CutDepth::AtOrBelow(-5.0),
        diff_dir: None,
    };
    let Ok(mask) = ShapeMask::evaluate(&spec) else { panic!("evaluate") };
    mask.assert_no_overcut();
    mask.assert_bounded_leftover(35.0, Some(&allowed_corners));
    mask.assert_symmetric(Symmetry::MirrorX(0.0));
    mask.assert_symmetric(Symmetry::MirrorY(0.0));
    mask.assert_within_stock_extension(&stock, ext);
}
