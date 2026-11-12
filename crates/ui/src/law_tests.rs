//! The scene and interaction law tests, relocated at M1.4.2 M-g from the
//! retired imgui `app.rs` - bodies verbatim, only the scene-collector
//! helpers they call re-pointed to `egui_app::world` (the same law, where
//! it lives now). They sit at the crate root because the law they pin is
//! the shared one: `interact`'s overlay, pick and gate behaviour, which
//! no panel layer may fork.
//!
//! Provenance: our own tests, carried from the app's first gizmo steps.

use camcraft_app::geometry::{feature, transform_point, Point3};
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::schema::STOCK_NAME;
use camcraft_app::{model, operation};

use crate::egui_app::world;
use crate::interact::{
    CLEARANCE_RGB, CUT_RGB, FEED_RGB, MAX_OVERLAY_VERTICES, PLUNGE_RGB, RAPID_RGB,
    REGION_RGB, RETRACT_RGB, anchor_hit, anchor_points, anchor_reference, capped, chevrons,
    collect_overlay, ftoz, ground_grid_lines, height_plane_lines, height_planes,
    gizmo_segments, hover_features, is_axis_label, overlay_labels, plane_corner, plane_rect, project_uv, queue_feature_pick,
    region_lines, selection_feature_lines, shown_regions, toolpath_lines,
};
use crate::state::{CommandKind, HoverHit, State};
use crate::viewport::GpuPart;

/// The per-frame part collector the retired imgui shell kept private in
/// app.rs; `world::collect` is that same law where it lives now, so the
/// tests reach it under the name they have always used.
fn collect_parts(state: &State) -> Vec<GpuPart> {
    world::collect(state).parts
}

#[cfg(test)]
mod gizmo_tests {
    //! The overlay's gizmo rules (feedbacks 6 and 7) are pure world reads;
    //! these pin them down without a GPU.

    use super::*;
    use crate::state::CommandKind;

    /// Place the active setup's one WCS (#342).
    fn set_origin(state: &mut State, pos: [f64; 3], yaw_deg: f64) {
        state.pending.push_back(CommandKind::SetWcs {
            pos,
            roll_deg: 0.0,
            pitch_deg: 0.0,
            yaw_deg,
        });
        state.drain_actions();
    }

    /// The Z=0 ground grid: the floor the empty viewport was missing. It is
    /// world-space overlay geometry on the table plane, spaced in 1-2-5
    /// decades that follow the zoom, and it obeys the axes switch.
    #[test]
    fn the_ground_grid_is_a_round_spaced_floor() {
        let mut state = State::new();
        let grid = ground_grid_lines(&state);
        assert!(!grid.is_empty(), "a fresh document still has a floor");
        assert!(grid.iter().all(|v| v[2] == 0.0), "the grid lies on Z=0");
        // dist 80 -> step 10 mm: every line lands on a whole 10 mm.
        assert!(
            grid.iter().all(|v| (v[0] / 10.0).fract().abs() < 1e-3
                || (v[1] / 10.0).fract().abs() < 1e-3),
            "lines sit on round machine coordinates"
        );
        // Ten times the distance, ten times the step.
        state.cam.dist = 800.0;
        let coarse = ground_grid_lines(&state);
        assert!(
            (coarse[0][1] / 100.0).fract().abs() < 1e-3,
            "the step follows the zoom: {:?}",
            coarse[0]
        );
        state.show_world_axes = false;
        assert!(ground_grid_lines(&state).is_empty(), "hiding the axes hides the floor");
    }

    /// #948: a non-finite or absurd camera must not stretch the grid's
    /// line-index range (dist 1e308 made `reach` infinite and the loop
    /// walk the whole i64 range): the floor stays out of that frame.
    #[test]
    fn the_ground_grid_survives_a_nan_or_huge_camera() {
        for (dist, target) in [
            (f64::NAN, [0.0, 0.0, 0.0]),
            (f64::INFINITY, [0.0, 0.0, 0.0]),
            (1.0e308, [0.0, 0.0, 0.0]),
            (80.0, [f64::NAN, 0.0, 0.0]),
            (80.0, [0.0, f64::INFINITY, 0.0]),
            (80.0, [f64::NEG_INFINITY, 1.0e300, 0.0]),
        ] {
            let mut state = State::new();
            state.cam.dist = dist;
            state.cam.target = target;
            let grid = ground_grid_lines(&state);
            assert!(grid.len() < 1000, "dist {dist} target {target:?}: {} lines", grid.len());
        }
    }

    #[test]
    fn a_fresh_world_shows_only_the_world_axes() {
        let state = State::new();
        let lines = collect_overlay(&state);
        assert_eq!(lines.len(), 6, "three dimmed axes");
        // #195: dimmed less than before (0.8 of the triad's 0.9 red), but dimmed.
        assert!(lines[0][3] < 0.8, "world axes are dimmed");
    }

    /// #195: the origin triads are stroked again in screen space (a wgpu
    /// line is 1 px): one segment per axis that the overlay draws, none
    /// when the axes are hidden, and the axis letters are told apart from
    /// the other overlay labels so they can be drawn larger.
    #[test]
    fn the_origin_triad_is_also_stroked_in_screen_space() {
        let mut state = State::new();
        let segs = gizmo_segments(&state, 1.0);
        assert_eq!(segs.len(), 3, "the world axes, one stroke each: {segs:?}");
        assert!(segs.iter().all(|(a, b, _)| a != b), "each stroke has length on screen");
        let blue = segs[2].2;
        assert!(blue[2] >= 0.75, "Z is a readable blue on the dark ground: {blue:?}");
        state.show_world_axes = false;
        assert!(gizmo_segments(&state, 1.0).is_empty(), "hidden axes stroke nothing");
        assert!(is_axis_label("Z") && !is_axis_label("Operation1 · Top 30.00"));
    }

    #[test]
    fn an_origin_placed_at_machine_zero_does_not_duplicate_the_gizmo() {
        let mut state = State::new();
        set_origin(&mut state, [0.0, 0.0, 0.0], 0.0);
        let lines = collect_overlay(&state);
        assert_eq!(lines.len(), 6, "the triad replaces the world axes");
        assert!(lines[0][3] > 0.5, "the single gizmo is the bright triad");
        let labels = overlay_labels(&state, 1.0);
        // #727: the letters, and the triad's own name in place of the
        // suppressed world set.
        assert_eq!(labels.len(), 4, "one letter per triad axis and its name, no ghost set");
        assert!(
            labels.iter().all(|(_, rgb, _)| rgb[0].max(rgb[1]).max(rgb[2]) > 0.5),
            "all labels are bright"
        );
    }

    #[test]
    fn a_moved_origin_shows_axes_and_triad_together() {
        let mut state = State::new();
        set_origin(&mut state, [50.0, 0.0, 0.0], 0.0);
        assert_eq!(collect_overlay(&state).len(), 12, "both gizmos");
        let labels = overlay_labels(&state, 1.0);
        assert_eq!(labels.len(), 8, "three tips and a name per gizmo, both gizmos (#727)");
        let xs: Vec<f32> = labels
            .iter()
            .filter(|(_, _, text)| *text == "X")
            .map(|(_, rgb, _)| rgb[0])
            .collect();
        assert!(
            xs.iter().any(|r| *r > 0.85) && xs.iter().any(|r| *r < 0.8), // #195: dimmed to 0.8
            "one bright and one dimmed X: {xs:?}"
        );
    }

    #[test]
    fn moved_work_offset_letters_follow_their_axis_tips() {
        let mut state = State::new();
        set_origin(&mut state, [50.0, 0.0, 0.0], 0.0);
        let lines = collect_overlay(&state);
        let labels = overlay_labels(&state, 1.0);
        for (axis, name) in ["X", "Y", "Z"].into_iter().enumerate() {
            let tip = lines[axis * 2 + 1];
            let expected = project_uv(
                &state.cam.mvp(1.0),
                [
                    50.0 + f64::from(tip[0] - 50.0) * 1.08,
                    f64::from(tip[1]) * 1.08,
                    f64::from(tip[2]) * 1.08,
                ],
            )
            .expect("work offset axis tip projects");
            let actual = labels.iter()
                .find(|(_, rgb, label)| label.as_str() == name && rgb.iter().copied().fold(0.0f32, f32::max) > 0.7)
                .map(|(uv, _, _)| *uv)
                .expect("bright work offset axis label");
            assert!((actual[0] - expected[0]).abs() < 1e-3, "{name} horizontal");
            assert!((actual[1] - expected[1]).abs() < 1e-3, "{name} vertical");
        }
    }

    #[test]
    fn hidden_work_offset_removes_only_its_gizmo() {
        let mut state = State::new();
        // #342: one WCS per setup - two setups, two gizmos.
        state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        state.drain_actions();
        set_origin(&mut state, [0.0, 0.0, 0.0], 0.0);
        state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        state.drain_actions();
        set_origin(&mut state, [50.0, 0.0, 0.0], 0.0);
        assert_eq!(collect_overlay(&state).len(), 12);
        State::toggle_visibility(&mut state.hidden_offsets, "G54");
        assert_eq!(collect_overlay(&state).len(), 12, "Setup2's WCS and world axes remain");
        assert!(model::find_by_name(&state.world, "G54").is_some());
        State::toggle_visibility(&mut state.hidden_offsets, "Setup2/WCS");
        assert_eq!(collect_overlay(&state).len(), 6, "only world axes remain");
    }

    /// #727: every gizmo wears its name - the offset triads take the
    /// entity name the Setup pane shows them under, the dimmed world axes
    /// take "World". Always visible: telling whose origin is whose must
    /// not need a hover or a click.
    #[test]
    fn the_dimmed_world_axes_are_labelled_world() {
        let state = State::new();
        let labels = overlay_labels(&state, 1.0);
        let world = labels
            .iter()
            .find(|(_, _, text)| text == "World")
            .unwrap_or_else(|| panic!("the world axes have a name: {labels:?}"));
        assert!(
            world.1.iter().all(|c| *c < 0.7),
            "dimmed like the axes it stands for: {:?}",
            world.1
        );
        assert!(
            (world.1[0] - world.1[1]).abs() < 1e-6 && (world.1[1] - world.1[2]).abs() < 1e-6,
            "neutral gray, no axis claims the world: {:?}",
            world.1
        );
    }

    #[test]
    fn each_triad_is_labelled_with_its_own_offset_name() {
        let mut state = State::new();
        // #342's two-setup shape, as in the hidden-offset test: G54 at
        // world zero (its triad replaces the world axes), Setup2's WCS off it.
        state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        state.drain_actions();
        set_origin(&mut state, [0.0, 0.0, 0.0], 0.0);
        state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        state.drain_actions();
        set_origin(&mut state, [50.0, 0.0, 0.0], 0.0);
        let labels = overlay_labels(&state, 1.0);
        for want in ["G54", "Setup2/WCS"] {
            let label = labels
                .iter()
                .find(|(_, _, text)| text == want)
                .unwrap_or_else(|| panic!("{want} labels its own origin: {labels:?}"));
            assert!(
                label.1.iter().copied().fold(0.0f32, f32::max) > 0.85,
                "offset names read bright against the letters: {want} {:?}",
                label.1
            );
        }
    }

    #[test]
    fn an_offset_name_stands_at_its_origin_not_its_axis_tips() {
        let mut state = State::new();
        set_origin(&mut state, [50.0, 0.0, 0.0], 0.0);
        let expected = project_uv(&state.cam.mvp(1.0), [50.0, 0.0, 0.0]).expect("origin projects");
        let label = overlay_labels(&state, 1.0)
            .into_iter()
            .find(|(_, _, text)| text == "G54")
            .expect("G54 labels its origin");
        assert!((label.0[0] - expected[0]).abs() < 1e-3, "the name rides its origin horizontally");
        assert!(
            label.0[1] > expected[1] && label.0[1] - expected[1] < 0.15,
            "just below the origin, not crowded onto the axes: {:?}",
            label.0
        );
    }

    #[test]
    fn a_hidden_work_offset_drops_its_name_with_its_gizmo() {
        let mut state = State::new();
        state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        state.drain_actions();
        set_origin(&mut state, [0.0, 0.0, 0.0], 0.0);
        state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        state.drain_actions();
        set_origin(&mut state, [50.0, 0.0, 0.0], 0.0);
        assert!(overlay_labels(&state, 1.0).iter().any(|(_, _, text)| text == "G54"));
        State::toggle_visibility(&mut state.hidden_offsets, "G54");
        let texts: Vec<String> =
            overlay_labels(&state, 1.0).into_iter().map(|(_, _, text)| text).collect();
        assert!(!texts.iter().any(|t| t == "G54"), "the pane's eye and the label agree: {texts:?}");
        assert!(texts.iter().any(|t| t == "Setup2/WCS"), "the other offset keeps its name: {texts:?}");
    }

    #[test]
    fn gizmo_names_are_not_the_big_axis_letters() {
        // Names label; only X/Y/Z draw at the letter's size.
        assert!(!is_axis_label("World") && !is_axis_label("G54") && !is_axis_label("Setup2/WCS"));
    }

    #[test]
    fn a_rotated_offset_at_the_origin_still_shows_the_world_axes() {
        // Yaw 90 moves the triad axes off X/Y/Z - the coincidence was about
        // lines, not just the origin point.
        let mut state = State::new();
        set_origin(&mut state, [0.0, 0.0, 0.0], 90.0);
        assert_eq!(collect_overlay(&state).len(), 12, "the triad no longer claims the axes");
    }

    #[test]
    fn hiding_the_world_axes_leaves_the_offset_triads_alone() {
        let mut state = State::new();
        state.show_world_axes = false;
        assert!(collect_overlay(&state).is_empty(), "nothing but hidden furniture");
        assert!(overlay_labels(&state, 1.0).is_empty(), "and no labels for it");
        set_origin(&mut state, [50.0, 0.0, 0.0], 0.0);
        assert_eq!(collect_overlay(&state).len(), 6, "the work offset still shows");
        assert_eq!(overlay_labels(&state, 1.0).len(), 4, "letters plus the offset's name (#727)");
        assert!(
            overlay_labels(&state, 1.0)
                .iter()
                .all(|(_, rgb, _)| rgb[0].max(rgb[1]).max(rgb[2]) > 0.5),
            "only the bright triad labels remain, no dimmed world letters"
        );
    }

    /// #645: the "Show origin names" switch asks for over #727. Its law:
    /// it quiets the name rows and nothing else - the axis lines, the
    /// triad strokes and the big `X`/`Y`/`Z` letters all ignore it.
    #[test]
    fn the_name_switch_quiets_names_not_letters_or_lines() {
        let mut state = State::new();
        assert_eq!(overlay_labels(&state, 1.0).len(), 4, "default on: the world set plus its name");
        state.show_gizmo_names = false;
        let labels = overlay_labels(&state, 1.0);
        assert_eq!(labels.len(), 3, "the three letters survive");
        assert!(labels.iter().all(|(_, _, text)| is_axis_label(text)), "no name anywhere: {labels:?}");
        assert_eq!(collect_overlay(&state).len(), 6, "the axis lines are untouched");
        assert_eq!(gizmo_segments(&state, 1.0).len(), 3, "and so are the triad strokes");
        // With an offset out at a G5x, both gizmos shed names together.
        set_origin(&mut state, [50.0, 0.0, 0.0], 0.0);
        assert_eq!(overlay_labels(&state, 1.0).len(), 6, "one letter set per gizmo, no names");
        state.show_gizmo_names = true;
        assert_eq!(overlay_labels(&state, 1.0).len(), 8, "back on: both names return (#727)");
    }
}

#[cfg(test)]
mod anchor_tests {
    //! The pane buttons, the viewport markers and the click hit-test all
    //! read the same anchor helpers; these pin that shared math down.

    use super::*;
    use crate::state::{CommandKind, quat_yaw_deg};
    use camcraft_app::stock::StockShape;

    fn stock_box(state: &mut State, dx: f64, dy: f64, dz: f64, pos: [f64; 3]) {
        state.pending.push_back(CommandKind::SetStock {
            shape: StockShape::Box { dx, dy, dz },
            pos,
            yaw_deg: 0.0,
        });
        state.drain_actions();
    }

    /// Box 60x40x20, XY-centered at (10, -5), bottom at z=0:
    /// corners span (-20,-25,0)..(40,15,20).
    fn state_with_stock() -> State {
        let mut state = State::new();
        stock_box(&mut state, 60.0, 40.0, 20.0, [10.0, -5.0, 0.0]);
        state
    }

    #[test]
    fn anchors_need_a_reference_and_prefer_the_chosen_source() {
        let mut state = State::new();
        assert!(anchor_reference(&state).is_none(), "empty world anchors nothing");
        stock_box(&mut state, 60.0, 40.0, 20.0, [0.0, 0.0, 0.0]);
        state.wcs_ui.anchor_source = 0;
        assert_eq!(anchor_reference(&state).map(|(_, w)| w), Some("stock box"));
        state.wcs_ui.anchor_source = 1;
        assert_eq!(
            anchor_reference(&state).map(|(_, w)| w),
            Some("stock box"),
            "no model bounds yet - fall back to stock, and the pane says so"
        );
    }

    #[test]
    fn the_nine_markers_are_the_box_points_of_the_chosen_level() {
        let mut state = state_with_stock();
        state.wcs_ui.anchor_z = 2;
        let pts = anchor_points(&state);
        assert_eq!(pts.len(), 9);
        assert_eq!(pts[0], ([-20.0, 15.0, 20.0], "NW"));
        assert_eq!(pts[2], ([40.0, 15.0, 20.0], "NE"));
        assert_eq!(pts[4], ([10.0, -5.0, 20.0], "C"));
        assert_eq!(pts[8], ([40.0, -25.0, 20.0], "SE"));
        state.wcs_ui.anchor_z = 0;
        assert_eq!(anchor_points(&state)[6], ([-20.0, -25.0, 0.0], "SW"));
        state.wcs_ui.anchor_z = 1;
        assert_eq!(anchor_points(&state)[1], ([10.0, 15.0, 10.0], "N"));
    }

    #[test]
    fn markers_appear_in_the_overlay_only_in_pick_mode() {
        let mut state = state_with_stock();
        assert_eq!(collect_overlay(&state).len(), 6, "world axes only");
        state.wcs_ui.pick_anchor = true;
        let lines = collect_overlay(&state);
        assert_eq!(lines.len(), 6 + 9 * 6, "nine crosses plus the axes");
        assert!(
            lines[6..].iter().any(|v| v[3] > 0.9 && v[4] > 0.9),
            "the pane-highlighted anchor (center, draft default) is brightest"
        );
        assert!(
            lines[6..].iter().any(|v| (v[3] - 0.82).abs() < 0.01),
            "the others are yellow"
        );
    }

    #[test]
    fn clicking_a_marker_selects_exactly_the_anchor_it_draws() {
        let mut state = state_with_stock();
        state.wcs_ui.pick_anchor = true;
        state.wcs_ui.anchor_z = 2;
        state.viewport_rect = ([0.0, 0.0], [800.0, 600.0]);
        let mvp = state.cam.mvp(800.0 / 600.0);
        let pts = anchor_points(&state);
        for (k, (pt, _)) in pts.iter().enumerate() {
            let Some(uv) = project_uv(&mvp, *pt) else {
                panic!("marker {k} ({}) off-screen from the default camera", pts[k].1);
            };
            let (hit_k, hit_pos) =
                anchor_hit(&state, [uv[0] * 800.0, uv[1] * 600.0], 8.0).expect("own position hits self");
            assert_eq!((hit_k, hit_pos), (k, *pt), "nearest marker to a click is the one drawn there");
        }
        // A degenerate rect (pre-layout) can never hit, so no phantom origin.
        state.viewport_rect = ([0.0, 0.0], [0.0, 0.0]);
        assert!(anchor_hit(&state, [4.0, 4.0], 12.0).is_none());
    }

    #[test]
    fn a_marker_click_queues_the_same_placement_as_the_grid() {
        // The end of the viewport path: a picked anchor point plus the pane
        // drafts lands exactly like the button grid's command would.
        let mut state = state_with_stock();
        state.wcs_ui.anchor_z = 2;
        state.viewport_rect = ([0.0, 0.0], [800.0, 600.0]);
        let mvp = state.cam.mvp(800.0 / 600.0);
        let pts = anchor_points(&state);
        let uv = project_uv(&mvp, pts[8].0).expect("SE visible");
        let (_, pos) = anchor_hit(&state, [uv[0] * 800.0, uv[1] * 600.0], 8.0).expect("hit SE");
        assert_eq!(pos, [40.0, -25.0, 20.0]);
        state.pending.push_back(CommandKind::SetWcs {
            pos,
            roll_deg: 0.0,
            pitch_deg: 0.0,
            yaw_deg: 30.0,
        });
        state.drain_actions();
        let entity = model::find_by_name(&state.world, "G54").expect("origin entity");
        let loc = model::transform_of(&state.world, &entity).expect("location");
        assert_eq!([loc.pos.x, loc.pos.y, loc.pos.z], [40.0, -25.0, 20.0]);
        assert!(
            (quat_yaw_deg(loc.rot) - 30.0).abs() < 1e-9,
            "the draft orientation rides along with the clicked position"
        );
    }
}

#[cfg(test)]
mod path_tests {
    //! The display half of M1.3: a generated path shows as colored lines
    //! with direction chevrons, and the toggle says how much of that is on
    //! screen. The tool/operation commands run straight off the history -
    //! their ribbon buttons (M1.4) are elsewhere; the producer feeding
    //! the overlay buffer is what these pin down.

    use super::*;
    use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, ImportModel};
    use camcraft_app::geometry::cube;
    use camcraft_app::operation::{Height, OpParams, SrcRef};
    use camcraft_app::stock::StockShape;
    use camcraft_app::tools::catalog;

    fn state_with_generated_path() -> State {
        let mut state = State::new();
        state.pending.push_back(CommandKind::SetStock {
            shape: StockShape::Box {
                dx: 60.0,
                dy: 60.0,
                dz: 20.0,
            },
            pos: [0.0, 0.0, 0.0],
            yaw_deg: 0.0,
        });
        state.drain_actions();
        let em = catalog()
            .into_iter()
            .find(|(n, _)| *n == "EM-0400-2FL")
            .map(|(_, t)| t)
            .expect("catalog entry");
        state
            .history
            .execute(&state.world, AddTool::new(&state.world, "EM-0400-2FL", em).expect("tool"));
        state.history.execute(
            &state.world,
            CreateOperation::new(
                &state.world,
                "Face1",
                "EM-0400-2FL",
                SrcRef::Stock,
                OpParams::Face { holes: Default::default(), tilt: None, blend_turns: false,
                    stepover: 3.0,
                    clearance_z: Height::fixed(25.0),
                    cut_z: Height::fixed(19.5),
                    top_z: None,
                    stepdown: None,
                    feed: 1200.0,
                    plunge_feed: 300.0,
                    climb: true,
                },
            )
            .expect("operation"),
        );
        state.history.execute(
            &state.world,
            GenerateOperation::new(&state.world, "Face1").expect("generate"),
        );
        state
    }

    fn select(state: &mut State, name: Option<&str>) {
        state.pending.push_back(CommandKind::Select(
            name.map(|n| n.to_string()),
        ));
        state.drain_actions();
    }

    #[test]
    fn nothing_shows_until_the_op_is_selected() {
        let state = state_with_generated_path();
        assert!(toolpath_lines(&state).is_empty(), "an unselected path stays hidden by default");
    }

    #[test]
    fn selecting_the_operation_brings_its_path_up() {
        let mut state = state_with_generated_path();
        select(&mut state, Some("Face1"));
        let lines = toolpath_lines(&state);
        assert!(!lines.is_empty());
        let cache = model::find_by_name(&state.world, "Face1")
            .and_then(|e| model::toolpath_of(&state.world, &e))
            .expect("cache");
        assert!(
            lines.len() > cache.path.moves.len() * 2,
            "chevrons ride on top of the move lines"
        );
        // One vertex pair per move minimum: every move's own segment made it.
        assert!(lines.iter().all(|v| v[3] <= 1.0 && v[4] <= 1.0 && v[5] <= 1.0));
        let green = lines.iter().any(|v| v[3..6] == FEED_RGB);
        let blue = lines.iter().any(|v| v[3..6] == RAPID_RGB);
        let red = lines.iter().any(|v| v[3..6] == PLUNGE_RGB);
        assert!(green && blue && red, "feed, rapid and plunge each show: {green} {blue} {red}");
    }

    /// M13.4: the move kinds wear the colour-vision palette, switched
    /// through the pick and back to Off by another pick; Off is the
    /// three house consts exactly.
    #[test]
    fn the_path_wears_the_colour_vision_palette_and_off_restores_it() {
        use crate::egui_app::colorvision::Semantic;
        use camcraft_app::schema::ColorVision;
        let mut state = state_with_generated_path();
        select(&mut state, Some("Face1"));
        let has = |state: &State, c: [f32; 3]| toolpath_lines(state).iter().any(|v| v[3..6] == c);
        assert!(has(&state, FEED_RGB) && has(&state, PLUNGE_RGB), "Off: the house colours");
        state.pending.push_back(CommandKind::SelectColorVision(ColorVision::Deutan));
        state.drain_actions();
        let rg = Semantic::RED_GREEN;
        assert!(has(&state, rg.feed) && has(&state, rg.plunge) && has(&state, rg.rapid), "deutan palette");
        assert!(!has(&state, FEED_RGB), "the green feed is gone under deutan");
        state.pending.push_back(CommandKind::SelectColorVision(ColorVision::Off));
        state.drain_actions();
        assert!(has(&state, FEED_RGB) && has(&state, PLUNGE_RGB) && has(&state, RAPID_RGB), "Off again");
    }

    #[test]
    fn deselecting_hides_the_path_again() {
        let mut state = state_with_generated_path();
        select(&mut state, Some("Face1"));
        assert!(!toolpath_lines(&state).is_empty());
        select(&mut state, None);
        assert!(toolpath_lines(&state).is_empty(), "no lingering path after deselect");
    }

    #[test]
    fn show_all_dims_instead_of_hiding() {
        let mut state = state_with_generated_path();
        state.show_all_paths = true;
        let lines = toolpath_lines(&state);
        assert!(!lines.is_empty(), "the toggle draws unselected paths");
        assert!(
            lines.iter().all(|v| v[3] < 0.5 && v[4] < 0.5 && v[5] < 0.5),
            "everything behind the selection is dimmed"
        );
        select(&mut state, Some("Face1"));
        let bright = toolpath_lines(&state)
            .iter()
            .any(|v| v[3..6] == FEED_RGB || v[3..6] == RAPID_RGB || v[3..6] == PLUNGE_RGB);
        assert!(bright, "the selected one draws at full strength");
    }

    #[test]
    fn show_and_hide_all_toolpaths_update_every_operation_eye() {
        let mut state = state_with_generated_path();
        select(&mut state, Some("Face1"));
        state.hide_all_toolpaths();
        assert!(state.hidden_paths.contains(&"Face1".to_owned()));
        assert!(!state.show_all_paths);
        assert!(toolpath_lines(&state).is_empty(), "hide all includes the selected path");

        state.show_all_toolpaths();
        assert!(state.hidden_paths.is_empty());
        assert!(state.show_all_paths);
        assert!(!toolpath_lines(&state).is_empty(), "show all restores the hidden path");

        state.toggle_all_toolpaths();
        assert!(!state.show_all_paths);
        assert!(state.hidden_paths.contains(&"Face1".to_owned()));
        assert!(toolpath_lines(&state).is_empty(), "an active path icon hides all paths");
        state.toggle_all_toolpaths();
        assert!(state.show_all_paths);
        assert!(state.hidden_paths.is_empty());
    }

    #[test]
    fn collect_overlay_adds_the_path_after_the_furniture() {
        let mut state = state_with_generated_path();
        select(&mut state, Some("Face1"));
        let all = collect_overlay(&state);
        let paths = toolpath_lines(&state);
        // Step 5's region preview joins the tail; the invariant (furniture,
        // then path) is asserted positionally, not by totals.
        assert_eq!(all.len(), 6 + paths.len() + region_lines(&state).len());
        assert_eq!(&all[6..6 + paths.len()], &paths[..], "axes first, then the path");
    }

    #[test]
    fn arc_moves_draw_curved() {
        // Cylinder stock, contour at a tolerance the tessellated wall
        // satisfies: the cache holds arc moves, and the overlay must
        // draw each as a run of chords, not one straight chord.
        let mut state = State::new();
        state.pending.push_back(CommandKind::SetStock {
            shape: StockShape::Cylinder {
                diameter: 30.0,
                height: 10.0,
            },
            pos: [0.0, 0.0, 0.0],
            yaw_deg: 0.0,
        });
        state.drain_actions();
        let em = catalog()
            .into_iter()
            .find(|(n, _)| *n == "EM-0400-2FL")
            .map(|(_, t)| t)
            .expect("catalog entry");
        state
            .history
            .execute(&state.world, AddTool::new(&state.world, "EM-0400-2FL", em).expect("tool"));
        state.history.execute(
            &state.world,
            CreateOperation::new(
                &state.world,
                "Skin1",
                "EM-0400-2FL",
                SrcRef::Stock,
                OpParams::Contour { tilt: None,
                    stock_to_leave: 0.0,
                    clearance_z: Height::fixed(20.0),
                    retract_z: Height::fixed(15.0),
                    top_z: Height::fixed(10.0),
                    bottom_z: Height::fixed(8.0),
                    stepdown: 2.0,
                    feed: 800.0,
                    plunge_feed: 200.0,
                    climb: true,
                    side: camcraft_app::machining2d::Side::Auto,
                    lead: camcraft_app::machining2d::Lead::None,
                    tolerance: 0.1,
                    tab_count: 0,
                    tab_width: 0.0,
                    tab_height: 0.0,
                    entry: camcraft_app::machining3d::Entry3D::Plunge,
                    helix_radius: 1.0,
                    blend_turns: false,
                    links: camcraft_app::links::Links::PLANAR,
                },
            )
            .expect("operation"),
        );
        state.history.execute(
            &state.world,
            GenerateOperation::new(&state.world, "Skin1").expect("generate"),
        );
        select(&mut state, Some("Skin1"));
        let cache = model::find_by_name(&state.world, "Skin1")
            .and_then(|e| model::toolpath_of(&state.world, &e))
            .expect("cache");
        assert!(cache.path.has_arcs(), "the tessellated wall fits to arcs at tol 0.1");

        let lines = toolpath_lines(&state);
        // At the display chord tolerance a half-circle arc fans into a
        // couple dozen chords; a straight-chord draw would emit exactly
        // one line (two vertices) per arc move, and arcs carry no
        // chevrons to pad the count with.
        let arcs = cache.path.arc_count().max(1);
        let feed = lines.iter().filter(|v| v[3..6] == FEED_RGB).count();
        assert!(
            feed > 8 * 2 * arcs,
            "{feed} feed vertices for {arcs} arc moves: arcs must draw as chord runs"
        );
    }

    #[test]
    fn the_cap_truncates_on_whole_lines() {
        let over = vec![[0.0f32; 6]; MAX_OVERLAY_VERTICES + 3];
        let out = capped(over);
        assert_eq!(out.len(), MAX_OVERLAY_VERTICES & !1);
        assert_eq!(out.len() % 2, 0, "never half a line");
    }

    #[test]
    fn degenerate_feeds_get_no_chevrons() {
        // A zero-length feed is the pathological cache entry; it must draw
        // its (zero-length) segment and nothing else - no NaN marker.
        let mut out = Vec::new();
        let p = Point3::new(1.0, 2.0, 3.0);
        chevrons(&mut out, p, p, FEED_RGB, 2.0);
        assert!(out.is_empty());
        let mut out = Vec::new();
        chevrons(&mut out, p, Point3::new(1.0, 2.0, 3.5), FEED_RGB, 2.0);
        assert!(out.is_empty(), "a feed shorter than two chevron arms stays plain");
    }

    #[test]
    fn the_eye_hides_a_path_without_touching_the_cache() {
        let mut state = state_with_generated_path();
        select(&mut state, Some("Face1"));
        assert!(!toolpath_lines(&state).is_empty());
        State::toggle_visibility(&mut state.hidden_paths, "Face1");
        assert!(toolpath_lines(&state).is_empty(), "hidden: nothing draws");
        let cache = model::find_by_name(&state.world, "Face1")
            .and_then(|e| model::toolpath_of(&state.world, &e))
            .expect("hiding is display: the path still exists");
        assert!(!cache.path.moves.is_empty());
        State::toggle_visibility(&mut state.hidden_paths, "Face1");
        assert!(!toolpath_lines(&state).is_empty(), "the second click shows it again");
    }

    #[test]
    fn hiding_a_model_undraws_only_its_triangles() {
        let mut state = state_with_generated_path();
        state.history.execute(
            &state.world,
            ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 10.0))),
        );
        let with = collect_parts(&state);
        State::toggle_visibility(&mut state.hidden_models, "part");
        let hidden = collect_parts(&state);
        assert_eq!(with.len(), hidden.len() + 1, "just the model leaves the buffer");
        assert_eq!(model::model_entities(&state.world).len(), 1, "the entity is not gone");
    }

    #[test]
    fn the_visibility_fold_follows_the_lists() {
        // The rebuild gate is the only consumer: a stale key would leave a
        // toggle visually dead for the session. The order-sensitive name
        // fold it hashed retired with the imgui shell (M-g); the gate IS
        // the scene digest now, so what the key promised reads on the
        // scene: every set difference moves it, and a same-set list in
        // the other order must NOT (the digest folds the scene, not the
        // bookkeeping - no churn for an alias).
        let mut state = state_with_generated_path();
        state.history.execute(
            &state.world,
            ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 10.0))),
        );
        state.history.execute(
            &state.world,
            ImportModel::from_mesh("qart", cube(Point3::ZERO, Point3::new(5.0, 5.0, 5.0))),
        );
        let empty = world::collect(&state).digest();
        State::toggle_visibility(&mut state.hidden_models, "part");
        let one = world::collect(&state).digest();
        State::toggle_visibility(&mut state.hidden_models, "qart");
        let both = world::collect(&state).digest();
        assert_ne!(empty, one, "hiding one model moves the gate");
        assert_ne!(one, both, "hiding the second moves it again");
        // Rebuild the same hidden set through the other insertion order.
        State::toggle_visibility(&mut state.hidden_models, "part");
        State::toggle_visibility(&mut state.hidden_models, "qart");
        State::toggle_visibility(&mut state.hidden_models, "qart");
        State::toggle_visibility(&mut state.hidden_models, "part");
        assert_eq!(
            world::collect(&state).digest(),
            both,
            "same set, other order - the gate must not churn (hidden: {:?})",
            state.hidden_models,
        );
    }
}

#[cfg(test)]
mod preview_tests {
    //! Step 5's "what gets cut": the selected operation's region loops in
    //! the 3D overlay, and the hovered facet's edges as screen-space
    //! segments. Both are pure world reads; pinned down headless.

    use super::*;
    use camcraft_app::commands::ImportModel;
    use camcraft_app::geometry::cube;
    use camcraft_app::operation::{Height, OpParams, SrcRef};
    use camcraft_app::stock::StockShape;
    use camcraft_app::tools::catalog;

    fn face_state() -> State {
        let mut state = State::new();
        let em = catalog()
            .into_iter()
            .find(|(n, _)| *n == "EM-0400-2FL")
            .map(|(_, t)| t)
            .expect("catalog entry");
        state.pending.extend([
            CommandKind::SetStock {
                shape: StockShape::Box {
                    dx: 60.0,
                    dy: 60.0,
                    dz: 20.0,
                },
                pos: [0.0, 0.0, 0.0],
                yaw_deg: 0.0,
            },
            CommandKind::AddTool {
                name: "EM-0400-2FL".into(),
                tool: em,
            },
            CommandKind::CreateOperation {
                name: "Face1".into(),
                tool: "EM-0400-2FL".into(),
                src: SrcRef::Stock,
                params: OpParams::Face { holes: Default::default(), tilt: None, blend_turns: false,
                    stepover: 3.0,
                    clearance_z: Height::fixed(25.0),
                    cut_z: Height::fixed(19.5),
                    top_z: None,
                    stepdown: None,
                    feed: 1200.0,
                    plunge_feed: 300.0,
                    climb: true,
                },
            },
        ]);
        state.drain_actions();
        state
    }

    #[test]
    fn selecting_an_operation_outlines_its_cut_region() {
        let mut state = face_state();
        // Created but never generated: the preview answers "what WOULD get
        // cut", which is the whole point of showing it before Generate.
        assert!(region_lines(&state).is_empty(), "nothing selected: nothing previewed");
        state.pending.push_back(CommandKind::Select(Some("Face1".into())));
        state.drain_actions();
        let lines = region_lines(&state);
        assert!(!lines.is_empty(), "the footprint of the 60 mm box shows");
        assert_eq!(lines.len() % 2, 0, "whole line segments only");
        for v in &lines {
            assert!(
                (v[2] - ftoz(19.5)).abs() <= f32::EPSILON,
                "a face region sits at cut_z, got {v:?}"
            );
            assert!(v[3] > 0.8 && v[4] > 0.4 && v[4] < 0.8 && v[5] < 0.3, "warning orange");
        }
        // A parameter edit moves the outline at once: it is a live world
        // read, never a leftover from the last generation.
        state.pending.push_back(CommandKind::EditOperationParams {
            name: "Face1".into(),
            params: OpParams::Face { holes: Default::default(), tilt: None, blend_turns: false,
                stepover: 3.0,
                clearance_z: Height::fixed(25.0),
                cut_z: Height::fixed(10.0),
                top_z: None,
                stepdown: None,
                feed: 1200.0,
                plunge_feed: 300.0,
                climb: true,
            },
        });
        state.drain_actions();
        assert!(region_lines(&state).iter().all(|v| (v[2] - 10.0).abs() < 1e-4));
    }

    #[test]
    fn picks_are_pinned_in_the_world_not_the_window() {
        // The overlay twin of hover: the open operation's resolved pick
        // list draws where the features ARE, in violet, so the picks
        // hold their place under orbiting (the screen-space highlight
        // cannot). Orange `region_lines` is the complement - "what the
        // path cuts, at the cut plane" - and the two coexist on purpose.
        let mut state = face_state();
        state.history.execute(
            &state.world,
            ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
        );
        state.op_ui.name = "Face1".into();
        assert!(
            selection_feature_lines(&state).is_empty(),
            "the source is still Stock: no picks, nothing pinned"
        );
        let (top, rim) = {
            let e = model::find_by_name(&state.world, "part").expect("entity");
            let f = model::features_of(&state.world, &e).expect("features");
            let face = f
                .faces
                .iter()
                .filter(|fa| {
                    matches!(fa.kind, feature::FaceKind::Planar) && fa.normal[2].abs() > 0.9
                })
                .max_by(|a, b| a.offset.total_cmp(&b.offset))
                .map(|fa| fa.id)
                .expect("the box top");
            let chain = feature::chains(&f)
                .into_iter()
                .find(|c| {
                    c.edges.len() == 1
                        && feature::chain_points(&f, c).iter().all(|p| (p[2] - 5.0).abs() < 1e-9)
                })
                .expect("one top-rim edge")
                .id;
            (FeatureSel::Face(face), FeatureSel::Chain(chain))
        };
        state.pending.push_back(CommandKind::SetOperationGeometry {
            name: "Face1".into(),
            items: vec![
                GeoSel { entity: "part".into(), feature: top },
                GeoSel { entity: "part".into(), feature: rim },
            ],
        });
        state.drain_actions();
        let lines = selection_feature_lines(&state);
        assert_eq!(lines.len(), 10, "four-sided top ring plus one rim edge");
        for v in &lines {
            assert!(
                (v[3] - 0.72).abs() < 0.01
                    && (v[4] - 0.44).abs() < 0.01
                    && (v[5] - 0.86).abs() < 0.01,
                "violet, never region orange: {v:?}"
            );
            assert!(
                (v[2] - ftoz(5.0)).abs() <= f32::EPSILON,
                "the picks draw at their own z, not the op's cut plane: {v:?}"
            );
        }
        assert!(
            collect_overlay(&state).iter().any(|v| (v[5] - 0.86).abs() < 0.01),
            "the picks reach the real overlay buffer, not just the helper"
        );

        state.hidden_models.push("part".into());
        assert!(
            selection_feature_lines(&state).is_empty(),
            "a hidden model's picks draw nothing - the hover rule"
        );
        state.hidden_models.clear();

        let name = state.op_ui.name.clone();
        state.op_ui.name.clear();
        assert!(
            selection_feature_lines(&state).is_empty(),
            "no operation open: the pick list shown is no one's"
        );
        state.op_ui.name = name;

        // Stale ids: only a file can queue them (the command validates),
        // so swap the mesh through the app's write chokepoint - the top
        // face moves to z 8 and every picked id goes stale.
        let part = model::find_by_name(&state.world, "part").expect("part");
        model::set_facets(
            &state.world,
            &part,
            cube(Point3::ZERO, Point3::new(10.0, 10.0, 8.0)).facets,
        );
        assert!(
            selection_feature_lines(&state).is_empty(),
            "a pick that outlived its geometry leaves no ghost on screen"
        );
    }

    #[test]
    fn hover_features_lights_the_face_boundary() {
        let mut state = State::new();
        state.history.execute(
            &state.world,
            ImportModel::from_mesh("c", cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0))),
        );
        // The triangle outline is gone with the feature modes: only a
        // resolved feature highlights anything.
        assert!(hover_features(&state, 4.0 / 3.0).is_empty(), "no hover: nothing draws");
        state.hover = Some(HoverHit { entity: "c".into(), facet: 0, feature: None });
        assert!(
            hover_features(&state, 4.0 / 3.0).is_empty(),
            "a bare facet hover (Model mode) draws no feature outline"
        );

        // Face hover: the planar face's boundary loop - the square its
        // corner edges weld into, not the two triangles that tessellate it.
        let top = {
            let e = model::find_by_name(&state.world, "c").expect("entity");
            let f = model::features_of(&state.world, &e).expect("features");
            f.faces
                .iter()
                .find(|fa| matches!(fa.kind, feature::FaceKind::Planar) && fa.normal[2] > 0.9)
                .map(|fa| fa.id)
                .expect("a planar top face")
        };
        state.hover =
            Some(HoverHit { entity: "c".into(), facet: 0, feature: Some(FeatureSel::Face(top)) });
        let segs = hover_features(&state, 4.0 / 3.0);
        assert_eq!(segs.len(), 4, "the square's four sides");
        for (a, b) in &segs {
            for uv in [a, b] {
                assert!(
                    uv.iter().all(|v| (0.0..=1.0).contains(v)),
                    "uv lands inside the image: {uv:?}"
                );
            }
        }

        // Chain hover: a box edge is its own open run - one segment.
        let chain = {
            let e = model::find_by_name(&state.world, "c").expect("entity");
            let f = model::features_of(&state.world, &e).expect("features");
            feature::chains(&f).into_iter().next().expect("a box edge")
        };
        state.hover = Some(HoverHit {
            entity: "c".into(),
            facet: usize::MAX,
            feature: Some(FeatureSel::Chain(chain.id)),
        });
        assert_eq!(hover_features(&state, 4.0 / 3.0).len(), 1, "one edge, one segment");

        // Gone or stale: never light up a stranger's feature.
        state.hover =
            Some(HoverHit { entity: "c".into(), facet: 0, feature: Some(FeatureSel::Face(7)) });
        assert!(
            hover_features(&state, 4.0 / 3.0).is_empty(),
            "a face id the re-mesh dropped draws nothing"
        );
        state.hover =
            Some(HoverHit { entity: "nope".into(), facet: 2, feature: Some(FeatureSel::Face(top)) });
        assert!(hover_features(&state, 4.0 / 3.0).is_empty());
        state.hover =
            Some(HoverHit { entity: "c".into(), facet: 2, feature: Some(FeatureSel::Face(top)) });
        state.hidden_models.push("c".into());
        assert!(
            hover_features(&state, 4.0 / 3.0).is_empty(),
            "a hidden model's feature draws nothing - it is display-off"
        );
    }

    #[test]
    fn row_hover_highlights_in_viewport() {
        // The Geometry list's row hover rides the same amber channel as
        // viewport hover: same segments, same nothing-for-stale rules, and
        // one feature pointed at twice draws once.
        let mut state = State::new();
        state.history.execute(
            &state.world,
            ImportModel::from_mesh("c", cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0))),
        );
        let (top, chain) = {
            let e = model::find_by_name(&state.world, "c").expect("entity");
            let f = model::features_of(&state.world, &e).expect("features");
            let top = f
                .faces
                .iter()
                .find(|fa| matches!(fa.kind, feature::FaceKind::Planar) && fa.normal[2] > 0.9)
                .map(|fa| fa.id)
                .expect("a planar top face");
            let chain = feature::chains(&f).into_iter().next().expect("a box edge").id;
            (top, chain)
        };
        let face = GeoSel { entity: "c".into(), feature: FeatureSel::Face(top) };
        let run = GeoSel { entity: "c".into(), feature: FeatureSel::Chain(chain) };

        state.geo_hover = Some(face.clone());
        let segs = hover_features(&state, 4.0 / 3.0);
        assert_eq!(segs.len(), 4, "the row's face lights its four sides");
        for (a, b) in &segs {
            for uv in [a, b] {
                assert!(
                    uv.iter().all(|v| (0.0..=1.0).contains(v)),
                    "uv lands inside the image: {uv:?}"
                );
            }
        }

        // Both pointers on the same feature: once, not twice.
        state.hover =
            Some(HoverHit { entity: "c".into(), facet: 0, feature: Some(face.feature) });
        assert_eq!(hover_features(&state, 4.0 / 3.0).len(), 4, "no double-draw");

        // Two different features: the viewport keeps its, the row adds its.
        state.hover = Some(HoverHit { entity: "c".into(), facet: 0, feature: Some(run.feature) });
        assert_eq!(hover_features(&state, 4.0 / 3.0).len(), 5, "one edge plus the square");

        // Stale ids and hidden models draw nothing through the row either -
        // the highlight never promises a pick the click cannot make.
        state.hover = None;
        state.geo_hover = Some(GeoSel { entity: "c".into(), feature: FeatureSel::Face(7) });
        assert!(hover_features(&state, 4.0 / 3.0).is_empty(), "a stale face id draws no ghost");
        state.geo_hover = Some(GeoSel { entity: "nope".into(), feature: face.feature });
        assert!(hover_features(&state, 4.0 / 3.0).is_empty(), "a gone entity draws nothing");
        state.geo_hover = Some(face);
        state.hidden_models.push("c".into());
        assert!(hover_features(&state, 4.0 / 3.0).is_empty(), "display-off stays off");
    }
}

#[cfg(test)]
mod feature_pick_tests {
    //! The Face/Edge click path as it lands on State: a plain click
    //! replaces the open operation's geometry list, Ctrl/Shift adds to it,
    //! a repeat is refused as a no-op, and with no operation open nothing
    //! queues. The edge pick - the one pick rule (#692) as the viewport
    //! asks it, `feature_click::feature_at` - is pinned here too, at the
    //! same projection the gizmo and anchor tests use.

    use super::*;
    use camcraft_app::commands::{AddTool, CreateOperation, ImportModel};
    use camcraft_app::geometry::{cube, cylinder};
    use camcraft_app::operation::{GeoSel, Height, OpParams, SrcRef};
    use camcraft_app::stock::StockShape;
    use camcraft_app::tools::catalog;

    /// The welded face a facet belongs to, by the one pick rule's graph
    /// (what a Face click on that facet stores).
    fn feature_of_facet(state: &State, entity: &str, facet: usize) -> Option<FeatureSel> {
        camcraft_app::pick::graph_of(&state.world, entity)?.face_of(facet)?.sel
    }

    /// The feature edge an Edge-only pick takes at `p`: (entity, edge).
    fn edge_nearest(state: &State, p: [f32; 2]) -> Option<(String, u32)> {
        let ask = crate::feature_click::Ask {
            kinds: camcraft_app::pick::PickKinds::EDGES,
            unit: camcraft_app::pick::EdgeUnit::Chain,
        };
        let (pick, _) = crate::feature_click::feature_at(state, p, ask)?;
        match pick.target {
            camcraft_app::pick::PickTarget::Edge { edge } => Some((pick.entity, edge)),
            _ => None,
        }
    }

    fn state_with_op() -> State {
        let mut state = State::new();
        state.pending.push_back(CommandKind::SetStock {
            shape: StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 },
            pos: [0.0, 0.0, 0.0],
            yaw_deg: 0.0,
        });
        state.drain_actions();
        let em = catalog()
            .into_iter()
            .find(|(n, _)| *n == "EM-0400-2FL")
            .map(|(_, t)| t)
            .expect("catalog entry");
        state
            .history
            .execute(&state.world, AddTool::new(&state.world, "EM-0400-2FL", em).expect("tool"));
        state.history.execute(
            &state.world,
            ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
        );
        state.history.execute(
            &state.world,
            CreateOperation::new(
                &state.world,
                "G1",
                "EM-0400-2FL",
                SrcRef::Stock,
                OpParams::Face { holes: Default::default(), tilt: None, blend_turns: false,
                    stepover: 3.0,
                    clearance_z: Height::fixed(25.0),
                    cut_z: Height::fixed(4.5),
                    top_z: None,
                    stepdown: None,
                    feed: 1200.0,
                    plunge_feed: 300.0,
                    climb: true,
                },
            )
            .expect("operation"),
        );
        state.op_ui.name = "G1".into();
        state
    }

    fn src(state: &State) -> SrcRef {
        let e = model::find_by_name(&state.world, "G1").expect("op");
        model::operation_of(&state.world, &e).expect("operation").src
    }

    /// A horizontal planar face of the named entity, the higher or lower
    /// one. Face normals are sign-canonical (winding-independent), so the
    /// two horizontal faces of a box share a normal and are told apart by
    /// plane offset - exactly what the generation-side tilt test does.
    fn face_sel(state: &State, name: &str, top: bool) -> FeatureSel {
        let e = model::find_by_name(&state.world, name).expect("entity");
        let f = model::features_of(&state.world, &e).expect("features");
        let mut hs: Vec<&_> = f
            .faces
            .iter()
            .filter(|fa| matches!(fa.kind, feature::FaceKind::Planar) && fa.normal[2].abs() > 0.9)
            .collect();
        hs.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        let fa = if top { hs.last() } else { hs.first() }.expect("a horizontal planar face");
        FeatureSel::Face(fa.id)
    }

    /// The screen midpoint of the first edge of `name` whose ends both sit
    /// above `zmin` in local space - where a rim edge actually draws.
    fn rim_mid_px(
        state: &State,
        mvp: &[[f32; 4]; 4],
        name: &str,
        zmin: f64,
        w: f32,
        h: f32,
    ) -> Option<(u32, [f32; 2])> {
        let e = model::find_by_name(&state.world, name)?;
        let f = model::features_of(&state.world, &e)?;
        let loc = model::transform_of(&state.world, &e).unwrap_or_default();
        let at = |vi: u32| {
            let v = *f.verts.get(vi as usize)?;
            let x = transform_point(Point3::new(v[0], v[1], v[2]), loc.pos, loc.rot);
            project_uv(mvp, [x.x, x.y, x.z]).map(|uv| [uv[0] * w, uv[1] * h])
        };
        f.edges.iter().enumerate().find_map(|(i, ed)| {
            let va = *f.verts.get(ed.a as usize)?;
            let vb = *f.verts.get(ed.b as usize)?;
            if va[2] < zmin || vb[2] < zmin {
                return None;
            }
            let (a, b) = (at(ed.a)?, at(ed.b)?);
            Some((i as u32, [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]))
        })
    }

    #[test]
    fn a_plain_click_replaces_and_ctrl_adds() {
        let mut state = state_with_op();
        assert_eq!(src(&state), SrcRef::Stock);
        let top = face_sel(&state, "part", true);
        let bottom = face_sel(&state, "part", false);
        let part = |feature| GeoSel { entity: "part".into(), feature };

        queue_feature_pick(&mut state, part(top), false);
        assert!(state.status.contains("added to G1"), "the click says what it did: {}", state.status);
        state.drain_actions();
        assert_eq!(src(&state), SrcRef::Selection(vec![part(top)]));
        assert!(
            state.status.contains("updated for G1"),
            "and the executed command says so too: {}",
            state.status
        );

        // Ctrl adds: the base is the applied list, so order is stable.
        queue_feature_pick(&mut state, part(bottom), true);
        state.drain_actions();
        assert_eq!(src(&state), SrcRef::Selection(vec![part(top), part(bottom)]));

        // A repeat is refused where it stands: no command, no history entry.
        let before = state.pending.len();
        queue_feature_pick(&mut state, part(top), true);
        assert_eq!(state.pending.len(), before, "the duplicate queues nothing");
        assert!(state.status.contains("already selected"), "{}", state.status);

        // A plain click replaces: the list is exactly what was clicked.
        queue_feature_pick(&mut state, part(bottom), false);
        state.drain_actions();
        assert_eq!(src(&state), SrcRef::Selection(vec![part(bottom)]));
    }

    #[test]
    fn a_chain_pick_rides_the_same_list() {
        // Edge mode folds a Chain id into exactly the same queue: the
        // geometry list is kind-blind and order-keeping.
        let mut state = state_with_op();
        let chain = {
            let e = model::find_by_name(&state.world, "part").expect("entity");
            let f = model::features_of(&state.world, &e).expect("features");
            feature::chains(&f).into_iter().next().expect("a box edge").id
        };
        let top = face_sel(&state, "part", true);
        let (face_pick, chain_pick) = (
            GeoSel { entity: "part".into(), feature: top },
            GeoSel { entity: "part".into(), feature: FeatureSel::Chain(chain) },
        );
        queue_feature_pick(&mut state, face_pick.clone(), false);
        state.drain_actions();
        queue_feature_pick(&mut state, chain_pick.clone(), true);
        state.drain_actions();
        assert_eq!(src(&state), SrcRef::Selection(vec![face_pick, chain_pick]));
    }

    #[test]
    fn a_wall_face_pick_lands_on_the_patch() {
        // The hand-test complaint on the pick side: clicking a faceted
        // bore answered with the one tessellation strip under the
        // cursor. A strip of a welded wall now resolves to the whole
        // weld; flat faces answer with themselves, so an ordinary box
        // re-keys nothing.
        let mut state = state_with_op();
        state.history.execute(
            &state.world,
            ImportModel::from_mesh("boss", cylinder(20.0, 8.0, 16)),
        );
        // `cylinder`'s facet layout: per segment, two wall triangles,
        // then the top and bottom cap fans.
        let wall = feature_of_facet(&state, "boss", 0).expect("wall facet classified");
        let cap = feature_of_facet(&state, "boss", 2).expect("cap facet classified");
        assert!(
            matches!(wall, FeatureSel::Patch(_)),
            "a welded strip picks as the wall, not a strip: {wall}"
        );
        assert!(matches!(cap, FeatureSel::Face(_)), "a planar cap stays a face: {cap}");
        let pid = match wall {
            FeatureSel::Patch(id) => id,
            _ => panic!("wall pick is a patch"),
        };
        let e = model::find_by_name(&state.world, "boss").expect("entity");
        let f = model::features_of(&state.world, &e).expect("features");
        let p = feature::patches(&f)
            .into_iter()
            .find(|x| x.id == pid)
            .expect("the weld the pick named");
        assert_eq!(p.faces.len(), 16, "all sixteen wall strips, no cap");
        // Same facet slot on the box: a single flat strip, no weld.
        let boxy = feature_of_facet(&state, "part", 4).expect("box facet classified");
        assert!(matches!(boxy, FeatureSel::Face(_)), "a flat wall picks as itself: {boxy}");
    }

    #[test]
    fn no_open_operation_queues_nothing() {
        let mut state = state_with_op();
        state.op_ui.name.clear();
        let top = face_sel(&state, "part", true);
        queue_feature_pick(&mut state, GeoSel { entity: "part".into(), feature: top }, false);
        assert!(state.pending.is_empty(), "a click with nothing open queues nothing");
        assert!(state.status.contains("open an operation"), "{}", state.status);
    }

    #[test]
    fn edge_picking_finds_the_drawn_edge_and_respects_the_eye() {
        let mut state = state_with_op();
        state.viewport_rect = ([0.0, 0.0], [800.0, 600.0]);
        let mvp = state.cam.mvp(800.0 / 600.0);
        let (edge, mid) = rim_mid_px(&state, &mvp, "part", 4.9, 800.0, 600.0)
            .expect("a part rim edge visible from the default camera");
        let (name, hit) =
            edge_nearest(&state, mid).expect("an edge under its own midpoint");
        assert_eq!(name, "part", "the part draws over the stock here");
        assert_eq!(hit, edge, "and it is the edge that drew there");

        // The browser eye closes the geometry off the screen too.
        state.hidden_models.push("part".into());
        assert!(
            edge_nearest(&state, mid).is_none_or(|(n, _)| n != "part"),
            "a hidden model's edges are not on screen, even nearby"
        );
    }

    #[test]
    fn stock_edges_are_pickable_and_an_unlaid_rect_finds_nothing() {
        let mut state = State::new();
        state.pending.push_back(CommandKind::SetStock {
            shape: StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 },
            pos: [0.0, 0.0, 0.0],
            yaw_deg: 0.0,
        });
        state.drain_actions();
        assert!(
            edge_nearest(&state, [10.0, 10.0]).is_none(),
            "pre-layout there is no image to project into - no phantom picks"
        );
        state.viewport_rect = ([0.0, 0.0], [800.0, 600.0]);
        let mvp = state.cam.mvp(800.0 / 600.0);
        let (_, mid) = rim_mid_px(&state, &mvp, STOCK_NAME, 19.9, 800.0, 600.0)
            .expect("a stock rim edge visible from the default camera");
        let (name, _) = edge_nearest(&state, mid).expect("the stock's own edge");
        assert_eq!(name, STOCK_NAME, "the stock's top is pickable geometry, no tree needed");
    }
}

#[cfg(test)]
mod height_plane_tests {
    //! M1.5.1 step 3: the viewport answers "where are the heights". The
    //! planes read the OPEN operation's DRAFT levels - the spinbox value,
    //! not the applied one - and the region plane defers to `region_lines`
    //! wherever the orange outline already stands at the same level, so a
    //! plane never draws twice and never silently vanishes. No op open, no
    //! planes; a source that bounds nothing still gets its levels marked.

    use super::*;
    use camcraft_app::commands::{AddTool, CreateOperation, ImportModel};
    use camcraft_app::geometry::cube;
    use camcraft_app::machining2d::{Lead, PocketStrategy, Side};
    use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
    use camcraft_app::stock::StockShape;
    use camcraft_app::tools::catalog;

    fn contour(clearance_z: Height) -> OpParams {
        OpParams::Contour { tilt: None,
            stock_to_leave: 0.0,
            clearance_z,
            retract_z: Height::fixed(15.0),
            top_z: Height::fixed(10.0),
            bottom_z: Height::fixed(2.0),
            stepdown: 2.0,
            feed: 800.0,
            plunge_feed: 200.0,
            climb: true,
            side: Side::Auto,
            lead: Lead::None,
            tolerance: 0.01,
            tab_count: 0,
            tab_width: 0.0,
            tab_height: 0.0,
            entry: camcraft_app::machining3d::Entry3D::Plunge,
            helix_radius: 1.0,
            blend_turns: false,
            links: camcraft_app::links::Links::PLANAR,
        }
    }

    fn pocket() -> OpParams {
        OpParams::Pocket { tilt: None,
            links: camcraft_app::links::Links::PLANAR,
            stepover: 3.0,
            stock_to_leave: 0.0,
            clearance_z: Height::fixed(25.0),
            retract_z: None,
            top_z: Height::fixed(10.0),
            bottom_z: Height::fixed(2.0),
            stepdown: 2.0,
            feed: 800.0,
            plunge_feed: 200.0,
            climb: true,
            strategy: PocketStrategy::Zigzag,
            entry: camcraft_app::machining3d::Entry3D::Plunge,
            helix_radius: 1.0,
            optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
            blend_turns: false,
        }
    }

    /// Stock, a tool, a part (so a fallback extent exists even stockless)
    /// and one operation - with the pane draft mirroring it exactly, the
    /// way sync_ops_ui leaves it when the op is opened.
    fn state_with(name: &str, params: OpParams) -> State {
        let mut state = State::new();
        state.pending.push_back(CommandKind::SetStock {
            shape: StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 },
            pos: [0.0, 0.0, 0.0],
            yaw_deg: 0.0,
        });
        state.drain_actions();
        let em = catalog()
            .into_iter()
            .find(|(n, _)| *n == "EM-0400-2FL")
            .map(|(_, t)| t)
            .expect("catalog entry");
        state
            .history
            .execute(&state.world, AddTool::new(&state.world, "EM-0400-2FL", em).expect("tool"));
        state.history.execute(
            &state.world,
            ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
        );
        state.history.execute(
            &state.world,
            CreateOperation::new(&state.world, name, "EM-0400-2FL", SrcRef::Stock, params.clone())
                .expect("operation"),
        );
        state.op_ui.name = name.into();
        state.op_ui.tool = "EM-0400-2FL".into();
        state.op_ui.params = params;
        // Open means selected in the pane, and the plane gates read the
        // selection, not the pane: mirror what a click on the tree does.
        state.pending.push_back(CommandKind::Select(Some(name.into())));
        state.drain_actions();
        state
    }

    /// Vertices at world height `z` painted exactly `rgb` - rectangle
    /// corners are the only geometry that carries plane colors.
    fn count_at(lines: &[[f32; 6]], z: f64, rgb: [f32; 3]) -> usize {
        lines
            .iter()
            .filter(|v| {
                (f64::from(v[2]) - z).abs() < 1e-3
                    && (0..3).all(|k| (v[3 + k] - rgb[k]).abs() < 1e-3)
            })
            .count()
    }

    #[test]
    fn height_planes_draw_for_open_contour_only_where_defined() {
        let state = state_with("C1", contour(Height::fixed(25.0)));
        let lines = height_plane_lines(&state);
        assert_eq!(count_at(&lines, 25.0, CLEARANCE_RGB), 8, "clearance rectangle");
        assert_eq!(count_at(&lines, 15.0, RETRACT_RGB), 8, "retract is a Contour level");
        assert_eq!(count_at(&lines, 2.0, CUT_RGB), 8, "bottom depth");
        assert_eq!(
            count_at(&lines, 10.0, REGION_RGB),
            0,
            "the region outline already stands at top_z - never a coincident double"
        );

        // Pocket stores no retract: nothing honest to draw at 15.
        let pocket = state_with("P1", pocket());
        let plines = height_plane_lines(&pocket);
        assert_eq!(count_at(&plines, 25.0, CLEARANCE_RGB), 8);
        assert_eq!(count_at(&plines, 15.0, RETRACT_RGB), 0, "no retract level exists");
        assert_eq!(count_at(&plines, 2.0, CUT_RGB), 8);

        // Every drawn plane carries its corner label - the outline-borne
        // Top included, which is exactly the one with no rectangle.
        let texts: Vec<String> =
            overlay_labels(&state, 800.0 / 600.0).into_iter().map(|(_, _, t)| t).collect();
        for want in ["C1 · Clearance 25.00", "C1 · Retract 15.00", "C1 · Top 10.00", "C1 · Bottom 2.00"] {
            assert!(texts.iter().any(|t| t == want), "{want} among {texts:?}");
        }
    }

    /// The Face that stages: outline at its Top, the finished Cut below.
    /// The old blanket gate skipped the Cut rectangle off a single
    /// "region shown" bool - invisible level, no outline standing for it.
    /// The level-by-level gate leaves the Cut rectangle exactly when no
    /// outline paints its level, and drops it exactly when one does.
    #[test]
    fn height_planes_stand_in_only_when_the_level_is_undrawn() {
        let staged = state_with(
            "F1",
            OpParams::Face { holes: Default::default(), tilt: None, blend_turns: false,
                stepover: 3.0,
                clearance_z: Height::fixed(25.0),
                cut_z: Height::fixed(18.0),
                top_z: Some(Height::fixed(20.0)),
                stepdown: Some(1.0),
                feed: 1200.0,
                plunge_feed: 400.0,
                climb: false,
            },
        );
        let lines = height_plane_lines(&staged);
        assert_eq!(
            count_at(&lines, 20.0, REGION_RGB),
            0,
            "the outline already stands at the staged Top"
        );
        assert_eq!(
            count_at(&lines, 18.0, CUT_RGB),
            8,
            "the finished Cut level carries no outline - the rectangle marks it"
        );
        // Unstaged: region_z IS the cut, one outline at one level.
        let flat = state_with(
            "F2",
            OpParams::Face { holes: Default::default(), tilt: None, blend_turns: false,
                stepover: 3.0,
                clearance_z: Height::fixed(25.0),
                cut_z: Height::fixed(18.0),
                top_z: None,
                stepdown: None,
                feed: 1200.0,
                plunge_feed: 400.0,
                climb: false,
            },
        );
        let lines = height_plane_lines(&flat);
        assert_eq!(
            count_at(&lines, 18.0, CUT_RGB),
            0,
            "the outline stands at the cut - no coincident double"
        );
    }

    #[test]
    fn height_planes_stand_in_when_the_source_bounds_nothing() {
        let mut state = state_with("C1", contour(Height::fixed(25.0)));
        // The hand-test C shape from the other side: an operation whose
        // source vanished shows no region outline, so the Top rectangle
        // appears in its orange to mark the level all the same.
        state.pending.push_back(CommandKind::DeleteByName(STOCK_NAME.into()));
        state.drain_actions();
        let lines = height_plane_lines(&state);
        assert_eq!(count_at(&lines, 10.0, REGION_RGB), 8, "the standing-in rectangle");
        assert_eq!(count_at(&lines, 25.0, CLEARANCE_RGB), 8, "the rest is unchanged");
    }

    #[test]
    fn height_planes_track_draft_edits() {
        let mut state = state_with("C1", contour(Height::fixed(25.0)));
        // The gate is the scene digest since M-g retired the imgui
        // doc-version + draft-key pair: a draft-only spin edit moves the
        // planes the overlay draws, so it moves the digest without any
        // document change - exactly what re-gates the rebuild now.
        let before = world::collect(&state).digest();
        if let OpParams::Contour { clearance_z, .. } = &mut state.op_ui.params {
            *clearance_z = Height::fixed(30.0);
        }
        assert_ne!(
            world::collect(&state).digest(),
            before,
            "a spin edit must re-gate the overlay rebuild",
        );
        let lines = height_plane_lines(&state);
        assert_eq!(count_at(&lines, 30.0, CLEARANCE_RGB), 8, "the plane moved with the draft");
        assert_eq!(count_at(&lines, 25.0, CLEARANCE_RGB), 0, "and not half-old");
    }

    #[test]
    fn plane_labels_stay_inside_the_viewport() {
        let mut state = state_with("C1", contour(Height::fixed(25.0)));
        // Zoom so tight the padded rectangle's corners project far off
        // frame: their labels must ride the margin, not vanish with the
        // corner. (project_uv itself is untouched - picking needs true
        // positions; only the label placement clamps.)
        state.cam.dist = 6.0;
        let labels = overlay_labels(&state, 800.0 / 600.0);
        let planes: Vec<&([f32; 2], [f32; 3], String)> =
            labels.iter().filter(|(_, _, t)| t.contains(" · ")).collect();
        assert!(!planes.is_empty(), "the open operation's planes still label");
        for (uv, _, t) in &planes {
            assert!(
                (0.0..=1.0).contains(&uv[0]) && (0.0..=1.0).contains(&uv[1]),
                "label {t} at {uv:?} left the viewport"
            );
        }
        // And the clamp earned its keep: raw, some corner did project out.
        let rect = plane_rect(&state).expect("stock gives an extent");
        let mvp = state.cam.mvp(800.0 / 600.0);
        let pushed_out = height_planes(&state).iter().any(|p| {
            let (cx, cy) = plane_corner(rect, p.corner);
            project_uv(&mvp, [cx, cy, p.z])
                .is_some_and(|uv| !(0.0..=1.0).contains(&uv[0]) || !(0.0..=1.0).contains(&uv[1]))
        });
        assert!(pushed_out, "a camera that fits everything proves nothing");
    }

    #[test]
    fn plane_labels_name_their_operation() {
        let state = state_with("Operation1", contour(Height::fixed(25.0)));
        let texts: Vec<String> = overlay_labels(&state, 800.0 / 600.0)
            .into_iter()
            .map(|(_, _, t)| t)
            .collect();
        assert!(
            texts.iter().any(|t| t.starts_with("Operation1 · Clearance")),
            "the label says whose plane it is: {texts:?}"
        );
        assert!(
            texts
                .iter()
                .all(|t| !t.contains(" · ") || t.starts_with("Operation1 ·")),
            "no plane labels anonymously: {texts:?}"
        );
    }

    #[test]
    fn height_planes_gone_when_no_op_open() {
        let state = State::new();
        assert!(height_planes(&state).is_empty() && height_plane_lines(&state).is_empty());
        // A name typed but not an operation: no planes, no phantom labels.
        let mut typed = state_with("C1", contour(Height::fixed(25.0)));
        typed.op_ui.name = "Ghost".into();
        assert!(height_planes(&typed).is_empty());
        assert!(height_plane_lines(&typed).is_empty());
        let texts: Vec<String> =
            overlay_labels(&typed, 800.0 / 600.0).into_iter().map(|(_, _, t)| t).collect();
        assert!(
            !texts.iter().any(|t| t.contains("Clearance")),
            "nothing open labels nothing: {texts:?}"
        );
    }

    #[test]
    fn hidden_operation_has_no_height_planes_or_cut_region_overlay() {
        let mut state = state_with("C1", contour(Height::fixed(25.0)));
        assert!(height_planes(&state).iter().any(|plane| plane.name == "Clearance"));
        assert!(height_planes(&state).iter().any(|plane| plane.name == "Bottom"));
        State::toggle_visibility(&mut state.hidden_paths, "C1");

        assert!(height_planes(&state).is_empty());
        assert!(height_plane_lines(&state).is_empty());
        assert!(region_lines(&state).is_empty());
        assert!(shown_regions(&state).is_empty());
        let labels = overlay_labels(&state, 800.0 / 600.0);
        assert!(!labels.iter().any(|(_, _, text)| text.starts_with("C1 · ")));
        assert!(model::find_by_name(&state.world, "C1").is_some());

        State::toggle_visibility(&mut state.hidden_paths, "C1");
        assert!(height_planes(&state).iter().any(|plane| plane.name == "Clearance"));
    }

    #[test]
    fn face_top_z_walks_its_plane() {
        // An unstaged face draws only its Cut plane - no phantom Top for a
        // level it will never cut. Staged, Top appears at the first level,
        // and editing the draft walks it live, the same honesty rule the
        // depth kinds follow.
        let face_at = |top_z: Option<Height>| OpParams::Face { holes: Default::default(), tilt: None, blend_turns: false,
            stepover: 8.0,
            clearance_z: Height::fixed(15.0),
            cut_z: Height::fixed(2.0),
            top_z,
            stepdown: if top_z.is_some() { Some(2.0) } else { None },
            feed: 800.0,
            plunge_feed: 200.0,
            climb: true,
        };
        let plain = state_with("F1", face_at(None));
        let names: Vec<&str> = height_planes(&plain).iter().map(|p| p.name).collect();
        assert_eq!(names, ["Clearance", "Cut"], "unstaged: no Top plane: {names:?}");

        let mut staged = state_with("F2", face_at(Some(Height::fixed(6.0))));
        let at = |s: &State| -> Vec<(&'static str, f64)> {
            height_planes(s).iter().map(|p| (p.name, p.z)).collect()
        };
        assert_eq!(
            at(&staged),
            [("Clearance", 15.0), ("Top", 6.0), ("Cut", 2.0)],
            "staged facing draws its first level above the finish"
        );
        // Walk the draft's Top Z: the plane follows before Apply, the
        // committed entity does not move at all.
        if let OpParams::Face { top_z, .. } = &mut staged.op_ui.params {
            *top_z = Some(Height::fixed(9.0));
        }
        assert_eq!(
            at(&staged),
            [("Clearance", 15.0), ("Top", 9.0), ("Cut", 2.0)],
            "the draft owns the plane"
        );
        let zs: Vec<f32> = height_plane_lines(&staged).iter().map(|l| l[2]).collect();
        assert!(zs.contains(&9.0), "the ring geometry walks too: {zs:?}");
        let back = state_with("F2", face_at(Some(Height::fixed(6.0))));
        assert_eq!(
            operation::region_z(&back.world, None, None, &back.op_ui.params),
            Some(6.0),
            "region_z stages with top_z - the draft's 9.0 never reached the document"
        );
    }

    #[test]
    fn a_referenced_plane_says_so_in_its_chip() {
        // The parenthetical is the From + Offset that produced the number:
        // what the corner label rides on, not an accident of the last
        // resolve. Fixed planes stay bare - their number is the whole
        // story - and moving the source walks the chip with it.
        let params = OpParams::Contour { tilt: None,
            stock_to_leave: 0.0,
            clearance_z: Height::of(HeightReference::TopOfStock, 5.0),
            retract_z: Height::fixed(15.0),
            top_z: Height::fixed(10.0),
            bottom_z: Height::of(HeightReference::WorkOffset, -1.0),
            stepdown: 2.0,
            feed: 800.0,
            plunge_feed: 200.0,
            climb: true,
            side: Side::Auto,
            lead: Lead::None,
            tolerance: 0.01,
            tab_count: 0,
            tab_width: 0.0,
            tab_height: 0.0,
            entry: camcraft_app::machining3d::Entry3D::Plunge,
            helix_radius: 1.0,
            blend_turns: false,
            links: camcraft_app::links::Links::PLANAR,
        };
        let mut state = state_with("C1", params);
        // G54 at z 3.0: stock top (20) + 5 = 25, G54 - 1 = 2.
        state.pending.push_back(CommandKind::SetWcs {
            pos: [0.0, 0.0, 3.0],
            roll_deg: 0.0,
            pitch_deg: 0.0,
            yaw_deg: 0.0,
        });
        state.drain_actions();
        let texts: Vec<String> = overlay_labels(&state, 800.0 / 600.0)
            .into_iter()
            .map(|(_, _, t)| t)
            .collect();
        for want in [
            "C1 · Clearance 25.00 (stock top +5.00)",
            "C1 · Bottom 2.00 (WCS G54 -1.00)",
            "C1 · Top 10.00",
        ] {
            assert!(texts.iter().any(|t| t == want), "{want} among {texts:?}");
        }
        // Slide G54 up: the plane, and the number on it, track the source
        // with no edit to the operation.
        state.pending.push_back(CommandKind::SetWcs {
            pos: [0.0, 0.0, 6.0],
            roll_deg: 0.0,
            pitch_deg: 0.0,
            yaw_deg: 0.0,
        });
        state.drain_actions();
        let texts: Vec<String> = overlay_labels(&state, 800.0 / 600.0)
            .into_iter()
            .map(|(_, _, t)| t)
            .collect();
        assert!(
            texts.iter().any(|t| t == "C1 · Bottom 5.00 (WCS G54 -1.00)"),
            "the chip tracks its source: {texts:?}"
        );
    }

    #[test]
    fn an_unresolved_source_draws_no_plane_and_labels_nothing() {
        // The same reference with G54 never placed: no rectangle at a
        // guessed z, no chip claiming a number. The planes that DO
        // resolve keep theirs - an honest mixed state, drawn as one.
        let params = OpParams::Contour { tilt: None,
            stock_to_leave: 0.0,
            clearance_z: Height::fixed(25.0),
            retract_z: Height::fixed(15.0),
            top_z: Height::fixed(10.0),
            bottom_z: Height::of(HeightReference::WorkOffset, -1.0),
            stepdown: 2.0,
            feed: 800.0,
            plunge_feed: 200.0,
            climb: true,
            side: Side::Auto,
            lead: Lead::None,
            tolerance: 0.01,
            tab_count: 0,
            tab_width: 0.0,
            tab_height: 0.0,
            entry: camcraft_app::machining3d::Entry3D::Plunge,
            helix_radius: 1.0,
            blend_turns: false,
            links: camcraft_app::links::Links::PLANAR,
        };
        let state = state_with("C1", params);
        assert!(
            height_planes(&state).iter().all(|p| p.name != "Bottom"),
            "an unplaced G54 draws no bottom plane",
        );
        let texts: Vec<String> = overlay_labels(&state, 800.0 / 600.0)
            .into_iter()
            .map(|(_, _, t)| t)
            .collect();
        assert!(texts.iter().all(|t| !t.contains(" · Bottom")), "{texts:?}");
        assert!(
            texts.iter().any(|t| t.starts_with("C1 · Clearance")),
            "the resolved planes still label: {texts:?}"
        );
        let cut_zs: Vec<f32> = height_plane_lines(&state)
            .iter()
            .filter(|l| {
                (0..3).all(|k| (l[3 + k] - CUT_RGB[k]).abs() < 1e-3)
            })
            .map(|l| l[2])
            .collect();
        assert!(cut_zs.is_empty(), "no cut geometry without a bottom: {cut_zs:?}");
    }
}

#[cfg(test)]
mod view_cube_tests {
    //! The M1.5 laws: the standard-orientation rule `Camera::toward`
    //! bakes, the hud's layout off the pane rect alone, and the cube's
    //! pick classification - every feature the painter draws facing the
    //! eye must classify back as exactly what it is.

    use crate::egui_app::viewcube::{self, Hud};
    use crate::viewport::Camera;

    /// A pane the size the 1280-wide dock leaves the viewport.
    fn hud() -> Hud {
        Hud::new(egui::Rect::from_min_max(
            egui::pos2(0.0, 0.0),
            egui::pos2(900.0, 640.0),
        ))
        .expect("a 900 x 640 pane has room for the furniture")
    }

    fn near(a: [f64; 3], b: [f64; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    #[test]
    fn toward_turns_the_eye_without_moving_it() {
        let cam = Camera {
            target: [2.0, -3.0, 4.0],
            yaw: 0.7,
            pitch: 0.2,
            dist: 55.0,
            ..Camera::default()
        };
        let top = cam.toward([0.0, 0.0, 1.0]);
        assert_eq!((top.target, top.dist), (cam.target, cam.dist));
        assert!((top.pitch - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        // At a pole the vanishing atan2 takes the roll from the pre-click
        // yaw snapped to the nearest quarter turn (#622), never NaN. This
        // start yaw (0.7) is nearer 0 than pi/2, so it still reads 0.
        assert!(top.yaw.abs() < 1e-12);
        let top = cam.oriented(2.0, 0.2).toward([0.0, 0.0, 1.0]);
        assert!(
            (top.yaw - std::f64::consts::FRAC_PI_2).abs() < 1e-12,
            "a pole click keeps the pre-click yaw's quarter turn: {}",
            top.yaw
        );
        let bottom = cam.oriented(-2.3, 0.2).toward([0.0, 0.0, -1.0]);
        assert!(
            (bottom.yaw + std::f64::consts::FRAC_PI_2).abs() < 1e-12,
            "the south pole snaps the same way: {}",
            bottom.yaw
        );
        let front = cam.toward([0.0, -1.0, 0.0]);
        assert!((front.yaw + std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!(front.pitch.abs() < 1e-12);
        let right = cam.toward([1.0, 0.0, 0.0]);
        assert!(right.yaw.abs() < 1e-12 && right.pitch.abs() < 1e-12);
        // Degenerate directions are no-ops, never NaN cameras (the
        // Prime Directive's belt on the cube's finite-by-construction
        // classifications).
        assert!(cam.toward([0.0, 0.0, 0.0]).yaw.is_finite());
        assert_eq!(cam.toward([0.0, 0.0, 0.0]).yaw, cam.yaw);
        assert_eq!(cam.toward([f64::NAN, 0.0, 0.0]).pitch, cam.pitch);
    }

    #[test]
    fn pole_views_hold_a_stable_roll() {
        // Straight up and straight down are the basis degeneracies; there
        // the quarter turn `toward` snapped the yaw to (#622) decides the
        // roll: screen-right rides the guard frame (-sin y, cos y, 0) that
        // is continuous with every near-pole view, and basis_up follows.
        // Views whose pre-click yaw shared a quarter turn roll alike.
        let snap = |y: f64| (y / std::f64::consts::FRAC_PI_2).round() * std::f64::consts::FRAC_PI_2;
        for d in [[0.0, 0.0, 1.0], [0.0, 0.0, -1.0]] {
            for yaw0 in [0.0, 0.9, -2.3] {
                let cam = Camera {
                    target: [0.0; 3],
                    yaw: yaw0,
                    pitch: 0.0,
                    dist: 80.0,
                    ..Camera::default()
                }
                .toward(d);
                let (r, u, _) = cam.view_basis();
                let (r, u): ([f64; 3], [f64; 3]) = (r.into(), u.into());
                let (rs, cs) = (snap(yaw0).sin(), snap(yaw0).cos());
                let want_r = [-rs, cs, 0.0];
                let want_u = if d[2] > 0.0 { [-cs, -rs, 0.0] } else { [cs, rs, 0.0] };
                assert!(near(r, want_r), "screen right at the {d:?} pole: {r:?}");
                assert!(near(u, want_u), "screen up at the {d:?} pole: {u:?}");
            }
        }
    }

    #[test]
    fn a_pole_click_keeps_the_roll_the_view_had() {
        // The #622 complaint: a TOP click from a near-top view swung the
        // screen frame up to half a turn (the old +X pin ignored the yaw
        // the orbit had put there). The snap law bounds the swing to a
        // half-quarter - 45 degrees.
        let cam = Camera {
            yaw: 1.0,
            pitch: 1.5,
            dist: 80.0,
            ..Camera::default()
        };
        let (_, u0, _) = cam.view_basis();
        let u0: [f64; 3] = u0.into();
        let top = cam.toward([0.0, 0.0, 1.0]);
        let (_, u1, _) = top.view_basis();
        let u1: [f64; 3] = u1.into();
        let dot: f64 = (0..3).map(|i| u0[i] * u1[i]).sum();
        assert!(
            dot > std::f64::consts::FRAC_1_SQRT_2,
            "a pole click may not turn the screen up more than 45 degrees: {u0:?} -> {u1:?}"
        );
    }

    #[test]
    fn a_second_pole_click_lands_the_same_view() {
        // The reported repro was clicking TOP twice and watching the model
        // twitch: under the snap law the second click's yaw is already a
        // quarter turn, so it changes nothing.
        let cam = Camera {
            yaw: 1.0,
            pitch: 1.5,
            dist: 80.0,
            ..Camera::default()
        };
        let once = cam.toward([0.0, 0.0, 1.0]);
        let twice = once.toward([0.0, 0.0, 1.0]);
        assert_eq!(twice.target, once.target);
        assert_eq!(twice.dist, once.dist);
        assert!(
            (twice.yaw - once.yaw).abs() < 1e-12 && (twice.pitch - once.pitch).abs() < 1e-12,
            "the second click moved the camera: {:?} -> {:?}",
            (once.yaw, once.pitch),
            (twice.yaw, twice.pitch)
        );
    }

    #[test]
    fn every_feature_facing_the_eye_classifies_as_itself() {
        // The three feature families over the house iso, demanded of the
        // exact screen point the painter would put them at: face centers,
        // edges whose two faces both face the eye, the corner all three
        // face through. Hidden features must still answer with SOME
        // feature - a solid cube never picks through its own front.
        let h = hud();
        let cam = Camera::default();
        let (_, _, rf) = cam.view_basis();
        let eye = [-rf.0, -rf.1, -rf.2];
        let dot = |n: [f64; 3], v: [f64; 3]| n[0] * v[0] + n[1] * v[1] + n[2] * v[2];
        let unit = |v: [f64; 3]| {
            let l = dot(v, v).sqrt();
            [v[0] / l, v[1] / l, v[2] / l]
        };
        let axes: [[f64; 3]; 6] = [
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ];
        let is_feature = |d: [f64; 3]| {
            axes.iter().any(|a| near(d, *a))
                || d.iter().all(|v| {
                    v.abs() < 1e-9
                        || (v.abs() - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-6
                })
                || d.iter().all(|v| (v.abs() - 3.0f64.sqrt() / 3.0).abs() < 1e-6)
        };
        for &n in &axes {
            let p = viewcube::point_screen(&cam, &h, n);
            let got = viewcube::hit_dir(&cam, &h, p).expect("a feature projects onto a feature");
            assert!(is_feature(got), "{n:?} answered a non-feature {got:?}");
            if dot(n, eye) > 0.3 {
                assert!(near(got, n), "visible face {n:?} answered {got:?}");
            }
        }
        for &a in &axes {
            for &b in &axes {
                if dot(a, b).abs() > 1e-9 || dot(a, eye) <= 0.3 || dot(b, eye) <= 0.3 {
                    continue;
                }
                let m = unit([a[0] + b[0], a[1] + b[1], a[2] + b[2]]);
                let p = viewcube::point_screen(&cam, &h, [a[0] + b[0], a[1] + b[1], a[2] + b[2]]);
                let got = viewcube::hit_dir(&cam, &h, p).expect("an edge projects onto a feature");
                assert!(near(got, m), "edge {m:?} answered {got:?}");
            }
        }
        // The near corner: the three faces' shared sign vector, the cube
        // point standing toward the eye - unit(sign(eye)).
        let sgn = |v: f64| if v >= 0.0 { 1.0 } else { -1.0 };
        let c = unit([sgn(eye[0]), sgn(eye[1]), sgn(eye[2])]);
        let p = viewcube::point_screen(&cam, &h, [c[0] * 3.0f64.sqrt(), c[1] * 3.0f64.sqrt(), c[2] * 3.0f64.sqrt()]);
        let got = viewcube::hit_dir(&cam, &h, p).expect("a corner projects onto a feature");
        assert!(near(got, c), "corner {c:?} answered {got:?}");
    }

    #[test]
    fn a_click_past_the_silhouette_is_a_miss() {
        // The bounding box's corners stand outside the projected cube at
        // any sane angle - they must answer None, not a stray feature.
        let h = hud();
        for (yaw, pitch) in [(0.9, 0.5), (0.0, 0.3), (-2.0, 1.2)] {
            let cam = Camera {
                target: [0.0; 3],
                yaw,
                pitch,
                dist: 80.0,
                ..Camera::default()
            };
            let corner = egui::pos2(h.cube.min.x + 0.5, h.cube.min.y + 0.5);
            assert_eq!(
                viewcube::hit_dir(&cam, &h, corner),
                None,
                "yaw {yaw} pitch {pitch}"
            );
        }
    }

    #[test]
    fn the_hud_is_laid_out_from_the_pane_alone() {
        // Cube top-right (the hint owns bottom-right), five buttons
        // below it, right-aligned; and no room, no furniture.
        let pane = egui::Rect::from_min_max(egui::pos2(300.0, 100.0), egui::pos2(1200.0, 700.0));
        let h = Hud::new(pane).expect("900 x 600 has room");
        assert!((h.cube.max.x - (1200.0 - 10.0)).abs() < 1e-6);
        assert!((h.cube.min.y - 110.0).abs() < 1e-6, "the cube hugs the top");
        let bottom = h.nav_rect(viewcube::Nav::Axes);
        assert!(
            bottom.max.x <= h.cube.max.x + 1e-6 && bottom.min.x >= pane.min.x,
            "the nav column stays in the pane"
        );
        assert!(bottom.max.y < pane.max.y, "the column ends inside the pane");
        let top_button = h.nav_rect(viewcube::Nav::Frame);
        assert!(top_button.min.y > h.cube.max.y, "buttons start under the cube");
        assert!(Hud::new(egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(120.0, 800.0))).is_none());
        assert!(Hud::new(egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(800.0, 250.0))).is_none());
        assert!(Hud::new(egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(130.0, 260.0))).is_some());
    }
}

mod projection_tests {
    //! The #434 orthographic laws: an affine matrix (clip.w is 1, so
    //! every screen-space consumer that divides by w lands cleanly),
    //! parallel pick rays that close against the same matrix, sizes that
    //! ignore depth, a mode switch that never jumps the framing at the
    //! target plane, and a dolly that holds the point under the cursor.

    use crate::viewport::{Camera, ProjectionMode};

    const ASPECT: f64 = 16.0 / 9.0;

    fn ortho() -> Camera {
        Camera { projection: ProjectionMode::Orthographic, ..Camera::default() }
    }

    /// The column-major point (x, y, z, 1) through an mvp, returned as
    /// (ndc_x, ndc_y, clip_w) - the divide the rasterizer does.
    fn project(m: &[[f32; 4]; 4], p: [f64; 3]) -> (f32, f32, f32) {
        let v = [p[0] as f32, p[1] as f32, p[2] as f32, 1.0f32];
        let mut out = [0.0f32; 4];
        for c in 0..4 {
            for r in 0..4 {
                out[r] += v[c] * m[c][r];
            }
        }
        (out[0] / out[3], out[1] / out[3], out[3])
    }

    fn shifted(p: [f64; 3], d: (f64, f64, f64), k: f64) -> [f64; 3] {
        [p[0] + k * d.0, p[1] + k * d.1, p[2] + k * d.2]
    }

    /// clip z over clip w - the depth the rasterizer compares, which
    /// WebGPU requires inside [0, 1] alongside -1 <= x, y <= 1 (#600).
    fn z_ndc(m: &[[f32; 4]; 4], p: [f64; 3]) -> f32 {
        let v = [p[0] as f32, p[1] as f32, p[2] as f32, 1.0f32];
        let mut z = 0.0f32;
        let mut w = 0.0f32;
        for c in 0..4 {
            z += v[c] * m[c][2];
            w += v[c] * m[c][3];
        }
        z / w
    }

    #[test]
    fn the_ortho_depth_spans_zero_to_one_between_the_planes() {
        // The OpenGL ortho convention spans NDC z over [-1, +1], but
        // WebGPU clips on 0 <= z <= w, and ortho keeps clip.w at 1 -
        // so every point nearer than (near + far) / 2 was clipped away
        // and the viewport went blank (#600). The planes themselves
        // must land on 0 and 1, and depth must climb between them.
        let cam = ortho();
        let m = cam.mvp(ASPECT);
        let eye = cam.eye();
        let look = [
            cam.target[0] - eye[0],
            cam.target[1] - eye[1],
            cam.target[2] - eye[2],
        ];
        let len = (look[0] * look[0] + look[1] * look[1] + look[2] * look[2]).sqrt();
        let at = |d: f64| {
            z_ndc(
                &m,
                [
                    eye[0] + look[0] / len * d,
                    eye[1] + look[1] / len * d,
                    eye[2] + look[2] / len * d,
                ],
            )
        };
        let near = 0.1f64;
        let far = (cam.dist * 40.0).max(1000.0);
        assert!((at(near) - 0.0).abs() < 1e-3, "the near plane sits at {}", at(near));
        assert!((at(far) - 1.0).abs() < 1e-3, "the far plane sits at {}", at(far));
        let mut last = at(near);
        for d in [cam.dist, (near + far) / 2.0, far * 0.9] {
            let z = at(d);
            assert!(z > 0.0 && z < 1.0, "depth at {d} mm is {z}, outside the clip volume");
            assert!(z > last, "depth at {d} mm is {z}, not deeper than {last}");
            last = z;
        }
    }

    #[test]
    fn the_ortho_matrix_is_affine_and_perspective_is_not() {
        let o = ortho().mvp(ASPECT);
        for (c, col) in o.iter().take(3).enumerate() {
            assert_eq!(col[3], 0.0, "ortho mvp column {c} must not touch w");
        }
        assert_eq!(o[3][3], 1.0);
        // Perspective keeps its converging row: without it the house
        // default would have silently turned parallel.
        assert_ne!(Camera::default().mvp(ASPECT)[2][3], 0.0);
    }

    #[test]
    fn ortho_rays_are_parallel_and_the_cursor_closes_through_the_matrix() {
        let cam = ortho();
        let (o1, d1) = cam.ray_through([0.4, -0.3], ASPECT).expect("ortho ray");
        let (o2, d2) = cam.ray_through([-0.7, 0.55], ASPECT).expect("ortho ray");
        for k in 0..3 {
            assert!((d1[k] - d2[k]).abs() < 1e-9, "ortho rays must be parallel");
        }
        let off = [o2[0] - o1[0], o2[1] - o1[1], o2[2] - o1[2]];
        assert!(
            off[0].abs() + off[1].abs() + off[2].abs() > 1e-6,
            "parallel rays from different cursors must not be one and the same ray"
        );
        // The cursor ray's point on the target plane must project back
        // to the very NDC the ray was cast from: pick and draw agree.
        let hit = [
            o1[0] + d1[0] * cam.dist,
            o1[1] + d1[1] * cam.dist,
            o1[2] + d1[2] * cam.dist,
        ];
        let (x, y, w) = project(&cam.mvp(ASPECT), hit);
        assert!((x - 0.4).abs() < 1e-3 && (y + 0.3).abs() < 1e-3, "cursor closure: {x}, {y}");
        assert!((w - 1.0).abs() < 1e-3, "ortho clip.w is 1 everywhere");
    }

    #[test]
    fn ortho_sizes_ignore_depth_where_perspective_shrinks_them() {
        for cam in [ortho(), Camera::default()] {
            let m = cam.mvp(ASPECT);
            let (r, _, f) = cam.view_basis();
            // A unit segment along screen-right, at the target plane and
            // pushed 35% of the way toward the eye.
            let far_end = project(&m, shifted(cam.target, r, 0.5));
            let far_mid = project(&m, cam.target);
            let near_center = shifted(cam.target, f, -0.35 * cam.dist);
            let near_end = project(&m, shifted(near_center, r, 0.5));
            let near_mid = project(&m, near_center);
            let far_w = ((far_end.0 - far_mid.0).powi(2) + (far_end.1 - far_mid.1).powi(2)).sqrt();
            let near_w = ((near_end.0 - near_mid.0).powi(2) + (near_end.1 - near_mid.1).powi(2)).sqrt();
            if cam.projection == ProjectionMode::Orthographic {
                assert!(
                    (far_w - near_w).abs() < 1e-3 * far_w,
                    "ortho must not shrink with depth: {far_w} vs {near_w}"
                );
                // Same screen x,y at both depths (no lateral parallax).
                assert!((far_mid.0 - near_mid.0).abs() < 1e-3);
                assert!((far_mid.1 - near_mid.1).abs() < 1e-3);
            } else {
                assert!(near_w > 1.1 * far_w, "perspective must shrink: {far_w} vs {near_w}");
            }
        }
    }

    #[test]
    fn the_mode_switch_never_jumps_the_framing_at_the_target_plane() {
        let o = ortho();
        let p = Camera { projection: ProjectionMode::Perspective, ..o };
        let mo = o.mvp(ASPECT);
        let mp = p.mvp(ASPECT);
        for (nx, ny) in [(-0.9, -0.9), (-0.3, 0.2), (0.0, 0.0), (0.6, 0.85)] {
            let (origin, dir) = o.ray_through([nx, ny], ASPECT).expect("ortho ray");
            let hit = [
                origin[0] + dir[0] * o.dist,
                origin[1] + dir[1] * o.dist,
                origin[2] + dir[2] * o.dist,
            ];
            let a = project(&mo, hit);
            let b = project(&mp, hit);
            assert!(
                (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3,
                "switch moves the target plane: {a:?} vs {b:?}"
            );
        }
    }

    #[test]
    fn an_ortho_dolly_holds_the_point_under_the_cursor() {
        let ndc = [0.6f32, -0.25];
        let o = ortho();
        let (origin, dir) = o.ray_through(ndc, 1.5).expect("ortho ray");
        let point = [
            origin[0] + dir[0] * o.dist,
            origin[1] + dir[1] * o.dist,
            origin[2] + dir[2] * o.dist,
        ];
        let mut cam = o;
        cam.dolly_cursor(ndc, 1.5, 1.0).expect("ortho dolly");
        assert_ne!(cam.dist, o.dist, "the dolly must actually zoom");
        assert_ne!(cam.target, o.target, "and must actually move the target");
        let (x, y, _) = project(&cam.mvp(1.5), point);
        assert!(
            (x - ndc[0]).abs() < 1e-3 && (y - ndc[1]).abs() < 1e-3,
            "the point under the cursor slid to {x}, {y}"
        );
    }

    #[test]
    fn camera_transforms_carry_the_projection_mode() {
        assert_eq!(Camera::default().projection, ProjectionMode::Perspective);
        let o = ortho();
        assert_eq!(Camera::frame([1.0, 2.0, 3.0], 5.0).projection, ProjectionMode::Perspective);
        assert_eq!(o.toward([1.0, 0.0, 0.0]).projection, ProjectionMode::Orthographic);
        assert_eq!(o.oriented(0.3, 0.2).projection, ProjectionMode::Orthographic);
        let mut cam = o;
        cam.orbit(1.0, 1.0);
        cam.pan(1.0, 1.0);
        cam.zoom(1.0);
        assert_eq!(cam.projection, ProjectionMode::Orthographic, "nav keeps the mode");
    }
}
