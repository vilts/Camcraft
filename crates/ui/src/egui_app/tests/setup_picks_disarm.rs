//! #664's frame tests: a Setups page pick (an axis by Edge, an axis by
//! Two points, the lay-on-a-plane face) armed on the setup page stops
//! collecting the moment its page is left. A real viewport click on the
//! operation page then fills the open operation's geometry, never the
//! hidden setup draft; back on the setup page the pick shows disarmed,
//! its picked item kept. #667 adds the same law for a Properties pane
//! closed, or hidden behind another tab: the shell's per-frame sweep.
//! Provenance: our own page law test over our own shell; no vendor
//! material consulted.

use super::*;

/// A pixel 4 px inside the most camera-facing side face of the seed's
/// block, beside its top edge (within the edge-snap radius).
fn beside_an_edge(sh: &ShellUi) -> egui::Pos2 {
    let ([x0, y0], [x1, y1]) = sh.state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    let mvp = sh.state.cam.mvp(f64::from(w / hgt));
    let eye = sh.state.cam.eye_f32();
    let sides = [
        ([60.0, 30.0, 15.0], [1.0, 0.0, 0.0]),
        ([0.0, 30.0, 15.0], [-1.0, 0.0, 0.0]),
        ([30.0, 60.0, 15.0], [0.0, 1.0, 0.0]),
        ([30.0, 0.0, 15.0], [0.0, -1.0, 0.0]),
    ];
    let px = |p: [f64; 3]| interact::project_uv(&mvp, p).map(|uv| egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt));
    let mut best: Option<(f32, egui::Pos2)> = None;
    for (c, n) in sides {
        let facing = ((f64::from(eye[0]) - c[0]) * n[0]
            + (f64::from(eye[1]) - c[1]) * n[1]
            + (f64::from(eye[2]) - c[2]) * n[2]) as f32;
        let (Some(pc), Some(pm)) = (px(c), px([c[0], c[1], 30.0])) else { continue };
        let d = pc - pm;
        if facing <= 0.0 || d.length() < 20.0 {
            continue;
        }
        let p = pm + d.normalized() * 4.0;
        if best.is_none_or(|(bf, _)| facing > bf) {
            best = Some((facing, p));
        }
    }
    best.expect("the block shows a side face to the default camera").1
}

/// The block's corner nearest the eye, inside the pane.
fn a_corner(sh: &ShellUi) -> egui::Pos2 {
    let ([x0, y0], [x1, y1]) = sh.state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    let mvp = sh.state.cam.mvp(f64::from(w / hgt));
    let eye = sh.state.cam.eye();
    let mut best: Option<(f64, egui::Pos2)> = None;
    for x in [0.0, 60.0] {
        for y in [0.0, 60.0] {
            for z in [0.0, 30.0] {
                let c = [x, y, z];
                let Some(uv) = interact::project_uv(&mvp, c) else { continue };
                if !(uv[0] > 0.05 && uv[0] < 0.95 && uv[1] > 0.05 && uv[1] < 0.95) {
                    continue;
                }
                let d = (c[0] - eye[0]).powi(2) + (c[1] - eye[1]).powi(2) + (c[2] - eye[2]).powi(2);
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt)));
                }
            }
        }
    }
    best.expect("the block shows a corner to the default camera").1
}

/// The seeded shell with Setup1 made and its setup page shown.
fn on_setup_page() -> Harness<'static, ShellUi> {
    let mut harness = shell_harness();
    harness.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    harness.run();
    show_setup_page(&mut harness);
    harness
}

fn show_setup_page(harness: &mut Harness<'static, ShellUi>) {
    crate::actions::target(
        &mut harness.state_mut().state,
        UiTarget::SetupNode("Setup1".into(), crate::state::SetupPage::Settings),
    );
    harness.run();
    assert!(harness.state().state.axis_rows_shown, "the setup page draws the pick rows");
}

/// #771: the plane row and the points row live behind the orientation
/// dropdown's entries - switch the plane method before clicking their
/// buttons. `method` is the entry's label ("Face", "Points", ...).
fn switch_plane_method(harness: &mut Harness<'static, ShellUi>, method: &str) {
    harness
        .get_by(|n| n.role() == egui::accesskit::Role::ComboBox && n.value().as_deref() == Some("Axes"))
        .click();
    harness.run();
    harness.get_by_label(method).click();
    harness.run();
}

/// Leave for Contour1's page, click `p` there: the operation's geometry
/// takes the pick. Returns the operation's picks.
fn click_on_operation_page(harness: &mut Harness<'static, ShellUi>, p: egui::Pos2) -> Vec<camcraft_app::operation::GeoSel> {
    crate::actions::target(&mut harness.state_mut().state, UiTarget::Operation("Contour1".into()));
    harness.run();
    assert!(!crate::axis_pick::collecting(&harness.state().state), "leaving the page disarms the axis pick");
    assert!(!crate::plane_pick::collecting(&harness.state().state), "leaving the page disarms the plane pick");
    click_viewport(harness, p);
    let st = &harness.state().state;
    assert_eq!(st.status, "geometry selection updated for Contour1", "the open operation takes the click");
    let op = model::find_by_name(&st.world, "Contour1").expect("seed has Contour1");
    match model::operation_of(&st.world, &op).map(|o| o.src) {
        Some(SrcRef::Selection(items)) => items,
        other => panic!("Contour1's source became {other:?}"),
    }
}

fn axis_y_picks(sh: &ShellUi) -> Vec<camcraft_app::operation::GeoSel> {
    sh.state.axis_pick.as_ref().and_then(|d| d.axes.y.clone()).map(|a| a.picks).unwrap_or_default()
}

#[test]
fn an_axis_edge_pick_disarms_when_its_page_is_left() {
    let mut harness = on_setup_page();
    harness.get_by_label("+Y by Edge").click();
    harness.run();
    assert!(crate::axis_pick::collecting(&harness.state().state), "choosing the mode arms it");
    let p = beside_an_edge(harness.state());
    click_viewport(&mut harness, p);
    let drafted = axis_y_picks(harness.state());
    assert_eq!(drafted.len(), 1, "the setup draft takes the edge: {}", harness.state().state.status);

    let items = click_on_operation_page(&mut harness, p);
    assert_eq!(items.len(), 1);
    assert!(matches!(items[0].feature, FeatureSel::Chain(_)), "an edge joins Contour1: {items:?}");
    assert_eq!(axis_y_picks(harness.state()), drafted, "the hidden draft is untouched");

    show_setup_page(&mut harness);
    assert!(!crate::axis_pick::collecting(&harness.state().state), "back on the page, disarmed");
    assert!(harness.query_by_label("Pick +Y edge").is_some(), "the row offers to pick again");
    assert!(harness.query_by_label("Stop picking +Y").is_none());
    assert_eq!(axis_y_picks(harness.state()), drafted, "the picked edge is kept");
}

#[test]
fn an_axis_two_points_pick_disarms_when_its_page_is_left() {
    let mut harness = on_setup_page();
    harness.get_by_label("+Y by Two points").click();
    harness.run();
    assert_eq!(harness.state().state.geo_pick, GeoPick::Vertex);
    let p = a_corner(harness.state());
    click_viewport(&mut harness, p);
    let drafted = axis_y_picks(harness.state());
    assert_eq!(drafted.len(), 1, "the setup draft takes the point: {}", harness.state().state.status);

    let items = click_on_operation_page(&mut harness, p);
    assert_eq!(items.len(), 1);
    assert!(matches!(items[0].feature, FeatureSel::Vertex(_)), "the corner joins Contour1: {items:?}");
    assert_eq!(axis_y_picks(harness.state()), drafted, "the hidden draft keeps one point, not two");

    show_setup_page(&mut harness);
    assert!(!crate::axis_pick::collecting(&harness.state().state), "back on the page, disarmed");
    assert!(harness.query_by_label("Pick +Y points").is_some(), "the row offers to pick again");
    assert_eq!(axis_y_picks(harness.state()), drafted, "the picked point is kept");
}

#[test]
fn a_plane_face_pick_disarms_when_its_page_is_left() {
    let mut harness = on_setup_page();
    switch_plane_method(&mut harness, "Face");
    harness.get_by_label("Pick a face").click();
    harness.run();
    assert!(crate::plane_pick::collecting(&harness.state().state), "the plane row arms");
    let p = beside_an_edge(harness.state());
    click_viewport(&mut harness, p);
    let drafted = harness.state().state.plane_pick.as_ref().map(|d| d.picks.clone()).unwrap_or_default();
    assert_eq!(drafted.len(), 1, "the setup draft takes the face: {}", harness.state().state.status);
    let face = drafted[0].clone();

    // The same pixel on the operation page: the face goes to Contour1.
    let items = click_on_operation_page(&mut harness, p);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0], face, "the face joins Contour1, not the hidden plane draft");
    let kept = harness.state().state.plane_pick.clone().expect("the paused draft is kept");
    assert!(!kept.picking);
    assert_eq!(kept.picks, drafted, "the hidden draft is untouched");

    show_setup_page(&mut harness);
    assert!(!crate::plane_pick::collecting(&harness.state().state), "back on the page, disarmed");
    assert!(harness.query_by_label("Stop picking").is_none(), "the row is not collecting");
    assert!(harness.query_by_label("Pick a face").is_some(), "the row offers to pick again");
    let kept = harness.state().state.plane_pick.as_ref().map(|d| d.picks.clone()).unwrap_or_default();
    assert_eq!(kept, drafted, "the picked face is kept");

    // Re-arming resumes the same draft with its face.
    harness.get_by_label("Pick a face").click();
    harness.run();
    assert!(crate::plane_pick::collecting(&harness.state().state));
    let resumed = harness.state().state.plane_pick.as_ref().map(|d| d.picks.clone()).unwrap_or_default();
    assert_eq!(resumed, drafted, "re-arming keeps the pick");
}

// ------------------------------------------------------------- #667
// The sweep runs every frame from the shell, whatever the dock shows:
// a Properties pane closed, or behind another tab in its leaf, disarms
// its picks before the viewport can route a click into them.

/// Press the ribbon's View door, then its `panel` row - the user's way
/// to close or reopen a pane.
fn view_door_toggle(harness: &mut Harness<'static, ShellUi>, panel: crate::state::Panel) {
    let strip = egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, super::super::shell::RIBBON_H));
    let (cells, _) = mill_plan_at(1280.0);
    let button = cells.iter().find(|c| c.cmd == Cmd::View).expect("the View door stands on the bar").rect;
    let rows = panes::view_rows_of(button, strip, &theme::current());
    let at = rows
        .iter()
        .find(|(r, _)| *r == super::super::layout::ViewRow::Panel(panel))
        .expect("the View door lists the panel")
        .1
        .center();
    super::machines_pane::click(harness, button.center());
    super::machines_pane::click(harness, at);
}

fn properties_shown(harness: &Harness<'static, ShellUi>) -> bool {
    super::super::layout::is_shown(&harness.state().dock, &Tab::Properties)
}

#[test]
fn an_axis_pick_disarms_when_the_properties_tab_is_closed() {
    let mut harness = on_setup_page();
    harness.get_by_label("+Y by Edge").click();
    harness.run();
    let p = beside_an_edge(harness.state());
    click_viewport(&mut harness, p);
    let drafted = axis_y_picks(harness.state());
    assert_eq!(drafted.len(), 1, "the setup draft takes the edge: {}", harness.state().state.status);
    assert!(crate::axis_pick::collecting(&harness.state().state), "still armed for another pick");

    view_door_toggle(&mut harness, crate::state::Panel::Properties);
    assert!(!super::super::layout::is_open(&harness.state().dock, &Tab::Properties), "the pane closed");
    assert!(!crate::axis_pick::collecting(&harness.state().state), "a closed pane disarms its pick");
    assert!(!harness.state().state.axis_rows_shown, "the rows are off screen");
    click_viewport(&mut harness, p);
    assert_eq!(axis_y_picks(harness.state()), drafted, "the click never reaches the hidden draft");
    assert!(!harness.state().state.status.contains("+Y"), "{}", harness.state().state.status);

    view_door_toggle(&mut harness, crate::state::Panel::Properties);
    assert!(properties_shown(&harness), "the pane reopened");
    assert!(!crate::axis_pick::collecting(&harness.state().state), "reopened, disarmed");
    assert!(harness.query_by_label("Pick +Y edge").is_some(), "the row offers to pick again");
    assert!(harness.query_by_label("Stop picking +Y").is_none());
    assert_eq!(axis_y_picks(harness.state()), drafted, "the picked edge is kept");
}

#[test]
fn a_plane_pick_disarms_when_another_tab_hides_the_properties_pane() {
    let mut harness = on_setup_page();
    // The Machines pane lists no user folder - never the tester's own.
    harness.state_mut().state.machines_pane.user_dir = Some(None);
    switch_plane_method(&mut harness, "Face");
    harness.get_by_label("Pick a face").click();
    harness.run();
    let p = beside_an_edge(harness.state());
    click_viewport(&mut harness, p);
    let drafted = harness.state().state.plane_pick.as_ref().map(|d| d.picks.clone()).unwrap_or_default();
    assert_eq!(drafted.len(), 1, "the setup draft takes the face: {}", harness.state().state.status);
    assert!(crate::plane_pick::collecting(&harness.state().state));

    // The Machines pane opens as the front tab of the properties leaf.
    view_door_toggle(&mut harness, crate::state::Panel::Machines);
    assert!(super::super::layout::is_open(&harness.state().dock, &Tab::Properties), "still in the dock");
    assert!(!properties_shown(&harness), "behind the Machines tab");
    assert!(!crate::plane_pick::collecting(&harness.state().state), "a hidden pane disarms its pick");
    click_viewport(&mut harness, p);
    let now = harness.state().state.plane_pick.clone().expect("the paused draft is kept");
    assert!(!now.picking);
    assert_eq!(now.picks, drafted, "the click never reaches the hidden draft");

    // Closing the Machines tab brings the properties pane to the front.
    view_door_toggle(&mut harness, crate::state::Panel::Machines);
    assert!(properties_shown(&harness), "the pane is in front again");
    assert!(!crate::plane_pick::collecting(&harness.state().state), "in front again, disarmed");
    assert!(harness.query_by_label("Stop picking").is_none(), "the row is not collecting");
    assert!(harness.query_by_label("Pick a face").is_some(), "the row offers to pick again");
    let kept = harness.state().state.plane_pick.as_ref().map(|d| d.picks.clone()).unwrap_or_default();
    assert_eq!(kept, drafted, "the picked face is kept");
}

/// The pre-dock half of the sweep: the pane is hidden in the SAME frame
/// a click reaches the viewport. The View door's ask settles before the
/// dock draws, and the viewport routes its click inside the dock, before
/// the after-dock sweep runs - so only the before-dock disarm keeps the
/// click off the hidden draft. The press lands a frame earlier (the pick
/// fires on release), and the Machines tab hides the pane without moving
/// the viewport, so the pixel still lands on the same corner. Two points
/// mode, so a leaked click would show: a second point joins the draft.
#[test]
fn a_click_in_the_frame_the_pane_hides_misses_the_hidden_draft() {
    let mut harness = on_setup_page();
    harness.state_mut().state.machines_pane.user_dir = Some(None);
    harness.get_by_label("+Y by Two points").click();
    harness.run();
    let p = a_corner(harness.state());
    click_viewport(&mut harness, p);
    let drafted = axis_y_picks(harness.state());
    assert_eq!(drafted.len(), 1, "the setup draft takes the point: {}", harness.state().state.status);
    assert!(crate::axis_pick::collecting(&harness.state().state), "still armed for another pick");

    set_pos(&mut harness, p);
    button_at(&mut harness, egui::PointerButton::Primary, p, true);
    assert!(properties_shown(&harness), "the pane still shows at the press");
    assert!(crate::axis_pick::collecting(&harness.state().state), "armed at the press");

    // One frame: the View door's ask (the Machines tab to the front of
    // the properties leaf) and the release that completes the click.
    harness.state_mut().state.layout_request =
        Some(crate::state::LayoutRequest::Toggle(crate::state::Panel::Machines));
    harness.state_mut().state.status.clear();
    harness.input_mut().events.push(Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    harness.step();
    assert!(!properties_shown(&harness), "the pane hid this frame");
    assert!(!crate::axis_pick::collecting(&harness.state().state), "hidden, disarmed");
    assert_eq!(axis_y_picks(harness.state()), drafted, "the click never reaches the hidden draft: one point, not two");
    assert!(!harness.state().state.status.contains("+Y"), "{}", harness.state().state.status);
    harness.run();
    assert_eq!(axis_y_picks(harness.state()), drafted, "nor in the frames after");
}

/// #673: the same pre-dock law for the locate draft (#566). The draft is
/// armed on Setup1's Fixtures page and has taken the block's corner as
/// its part point; the pane hides in the SAME frame a release reaches
/// the viewport. Without the pre-dock drop the release lands in the
/// still-live draft (its status asks for the vise again) before the
/// after-dock sweep runs.
#[test]
fn a_click_in_the_frame_the_pane_hides_misses_the_hidden_locate_draft() {
    let mut harness = on_setup_page();
    {
        let st = &mut harness.state_mut().state;
        let vise = camcraft_app::geometry::cube(
            camcraft_app::geometry::Point3::new(200.0, 200.0, -10.0),
            camcraft_app::geometry::Point3::new(260.0, 240.0, 0.0),
        );
        st.history.execute(&st.world, camcraft_app::commands::ImportModel::from_mesh("vise", vise));
        let tag = camcraft_app::commands::TagFixture::new(&st.world, "vise", Some("Setup1")).expect("the vise is a fixture");
        st.history.execute(&st.world, tag);
        st.machines_pane.user_dir = Some(None);
    }
    crate::actions::target(
        &mut harness.state_mut().state,
        UiTarget::SetupNode("Setup1".into(), crate::state::SetupPage::Fixtures),
    );
    harness.run();
    harness.get_by_label("Locate part…").click();
    harness.run();
    assert!(harness.state().state.locate_pick.is_some(), "the locate draft is armed");
    let p = a_corner(harness.state());
    click_viewport(&mut harness, p);
    assert!(
        harness.state().state.status.contains("vise"),
        "the draft takes the block's corner and asks for the vise: {}",
        harness.state().state.status
    );

    set_pos(&mut harness, p);
    button_at(&mut harness, egui::PointerButton::Primary, p, true);
    assert!(properties_shown(&harness), "the pane still shows at the press");
    assert!(harness.state().state.locate_pick.is_some(), "armed at the press");

    harness.state_mut().state.layout_request =
        Some(crate::state::LayoutRequest::Toggle(crate::state::Panel::Machines));
    harness.state_mut().state.status.clear();
    harness.input_mut().events.push(Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    harness.step();
    assert!(!properties_shown(&harness), "the pane hid this frame");
    assert!(harness.state().state.locate_pick.is_none(), "hidden, dropped");
    assert!(
        !harness.state().state.status.contains("vise"),
        "the click never reaches the hidden locate draft: {}",
        harness.state().state.status
    );
}

/// #555 meets #664: the three-points draft is dropped once its Setups
/// page is left, so a later viewport click never feeds it.
#[test]
fn a_three_points_pick_disarms_when_its_page_is_left() {
    let mut harness = on_setup_page();
    switch_plane_method(&mut harness, "Points");
    harness.get_by_label("Pick three points").click();
    harness.run();
    assert!(harness.state().state.points_pick.is_some(), "the points row arms");
    crate::actions::target(&mut harness.state_mut().state, UiTarget::Operation("Contour1".into()));
    harness.run();
    assert!(harness.state().state.points_pick.is_none(), "the page was left: the points draft is dropped");
    show_setup_page(&mut harness);
    assert!(harness.query_by_label("Pick three points").is_some(), "the row offers to pick again");
}
