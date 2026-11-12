//! #255: the structural twins of the three pixel snapshots. The PNG
//! goldens (`tests/snapshots/egui_app*.png`, `egui_tool_editor.png`) ride
//! the release lane now; what each one guarded is asserted here as text -
//! the AccessKit tree egui_kittest exposes (widgets by label, role,
//! enabled/disabled, window presence) plus the text the painted strips
//! lay out (the ribbon, the browser rows and the dock's tab titles are
//! painted, not widgets, so their galleys are the readable surface).
//! Text-only and coarse-geometry-only, so they hold on every OS and merge
//! like code.
//! Provenance: our own tests, specced from the egui_kittest/kittest and
//! AccessKit public APIs; no HSMWorks material involved.

use egui_kittest::kittest::NodeT;

use super::*;

/// Every painted text of the last frame with its top-left, in paint order.
pub(super) fn painted(h: &Harness<'static, ShellUi>) -> Vec<(String, egui::Pos2)> {
    fn walk(shape: &egui::Shape, out: &mut Vec<(String, egui::Pos2)>) {
        match shape {
            egui::Shape::Text(t) => out.push((t.galley.text().to_owned(), t.pos)),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for s in &h.output().shapes {
        walk(&s.shape, &mut out);
    }
    out
}

/// Where a painted text sits; fails with the whole list so a miss reads.
fn at(texts: &[(String, egui::Pos2)], want: &str) -> egui::Pos2 {
    at_where(texts, want, |_| true)
}

/// As `at`, for a text painted more than once (the ribbon's "Setups" cell
/// and the browser's "Setups" row): the first copy `inside` accepts.
fn at_where(
    texts: &[(String, egui::Pos2)],
    want: &str,
    inside: impl Fn(egui::Pos2) -> bool,
) -> egui::Pos2 {
    match texts.iter().find(|(t, p)| t == want && inside(*p)) {
        Some((_, p)) => *p,
        None => panic!(
            "no painted text {want:?}; painted: {:?}",
            texts.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>()
        ),
    }
}

fn disabled(h: &Harness<'static, ShellUi>, label: &str) -> bool {
    h.get_by_label(label).accesskit_node().is_disabled()
}

/// The seeded shell's skeleton - what `egui_app.png` pinned, theme-free:
/// ribbon on top with its groups and cells, the three dock leaves in
/// browser | viewport | properties order, the status strip at the bottom,
/// the seed's rows and page, and every floating window closed.
fn assert_shell_skeleton(h: &Harness<'static, ShellUi>) {
    let texts = painted(h);
    // Ribbon: the group headers and a cell per group, all above the dock.
    let groups: [&str; 0] = [];
    let cells = ["New", "Open", "Save", "Post", "Stock", "Origin", "Undo", "Redo", "Simulate"];
    let tab_y = at(&texts, "Browser").y;
    for t in groups.iter().chain(cells.iter()) {
        assert!(at(&texts, t).y < tab_y, "ribbon text {t:?} must sit above the dock tabs");
    }
    // The build line rides the ribbon's corner (#352's fixed test identity).
    // The build identity's version, from the same helper the toolbar paints
    // (#556): tests pin it (#449), so the real CARGO_PKG_VERSION never shows
    // here, and the ribbon paints the full "v… · test" or, when the ribbon is
    // crowded, the version alone - either form starts with the version.
    let ver = crate::build_info::toolbar_version();
    let build = texts
        .iter()
        .find(|(t, _)| t.starts_with(&ver))
        .map(|(_, p)| *p)
        .unwrap_or_else(|| panic!("no painted build identity starting {ver:?}"));
    assert!(build.y < tab_y, "the build identity sits on the ribbon");
    // Dock: three leaves, left to right, titles on one row.
    let (b, v, p) = (at(&texts, "Browser"), at(&texts, "Viewport"), at(&texts, "Properties"));
    assert!(b.x < v.x && v.x < p.x, "dock order must be Browser | Viewport | Properties");
    assert!((b.y - v.y).abs() < 1.0 && (v.y - p.y).abs() < 1.0, "the tab titles share a row");
    // Browser rows of the seed, under its tab.
    let in_browser = |q: egui::Pos2| q.y > b.y && q.x < v.x;
    // #343: each setup owns its WCS, Stock, Machine, Post, Fixtures and
    // Tool list rows; a row paints its name and value as one text.
    for row in ["Setups", "Setup", "Stock", "Fixtures", "Operations", "Models", "Block", "Tools"] {
        at_where(&texts, row, in_browser);
    }
    for (name, value) in [("WCS", "G54"), ("Machine", ""), ("Post", ""), ("Tool list", "")] {
        assert!(
            texts.iter().any(|(t, q)| t.starts_with(name) && t.contains(value) && in_browser(*q)),
            "the setup's {name} row ({value:?}) must show in the browser"
        );
    }
    assert!(
        texts.iter().any(|(t, q)| t.starts_with("Contour1") && t.contains("2D Contour") && q.x < v.x),
        "the seed's operation row must show in the browser"
    );
    // Viewport: its toolbar widgets and the view cube's faces.
    for t in ["TOP", "RIGHT", "BACK"] {
        let q = at(&texts, t);
        assert!(q.x > v.x && q.x < p.x, "view cube face {t:?} belongs to the viewport");
    }
    for label in ["Show world origin axes", "Show origin names", "Show all toolpaths", "Shaded", "Wireframe"] {
        assert!(!disabled(h, label), "viewport control {label:?} must be live");
    }
    // Properties: the seed's page, its sections, and the draft law's
    // disabled Apply (nothing staged on a fresh frame).
    assert!(h.get_by_label("Operation: Contour1").rect().min.x > v.x, "the page is the right leaf");
    for label in ["Type: 2D Contour", "Geometry", "Heights", "Passes", "Lead in / Lead out", "Feeds"] {
        assert!(h.query_by_label(label).is_some(), "property page must show {label:?}");
    }
    assert!(!disabled(h, "Generate"), "Generate is live on a valid op");
    assert!(disabled(h, "Apply"), "Apply waits for a staged edit");
    assert!(disabled(h, "Align WCS (3-2-1)"), "3-2-1 waits for three picks");
    // Status strip: the status line and the Debug info door at the bottom.
    let status = h.state().state.status.clone();
    assert!(status.starts_with("generated Contour1"), "seed status: {status}");
    let s = at(&texts, &status);
    assert!(s.y > p.y && s.y > 600.0, "the status line sits on the bottom strip");
    assert!(h.get_by_label("Debug info").rect().min.y > 600.0, "Debug info is on the status strip");
    // Every floating surface is closed by default.
    let titled: Vec<_> = h
        .query_all_by_role(egui::accesskit::Role::Window)
        .filter_map(|n| n.accesskit_node().label())
        .collect();
    assert!(titled.is_empty(), "no floating window may be open by default: {titled:?}");
    assert!(h.query_by_label("Shank diameter").is_none(), "the tool editor stays closed");
    // #561: no locate-in-fixture mate draft is armed by default.
    assert!(h.state().state.locate_pick.is_none(), "the locate picks start unarmed");
    assert!(h.query_by_label("Apply locate").is_none(), "the locate form is drawn only while armed");
    // #561 review: nor its turn row, its offset frame or its refusal line.
    for gone in ["Turn (deg)", "Offset (setup X / Y / Z)", "Not yet: "] {
        assert!(h.query_by_label_contains(gone).is_none(), "{gone} is drawn only while armed");
    }
}

/// `egui_app.png`'s intent in the dark house look.
#[test]
fn shell_structure_dark() {
    let _g = snapshot_guard();
    let mut harness = shell_harness();
    harness.run_steps(3);
    assert_eq!(theme::current(), theme::Tokens::DARK, "the house look is dark");
    let fill = harness.ctx.global_style().visuals.panel_fill;
    assert_eq!(fill, theme::Tokens::DARK.ink, "egui's visuals follow the dark table");
    assert_shell_skeleton(&harness);
}

/// `egui_app_light.png`'s intent: the light column lands through the
/// switcher's door (the world's `ThemeRef`, synced on the frame edge),
/// reads (contrast law), and moves no widget out of the skeleton.
#[test]
fn shell_structure_light() {
    let _g = snapshot_guard();
    let mut harness = shell_harness();
    theme_set(&harness.state_mut().state.world, &ThemeRef::Named("light".to_owned()));
    harness.run_steps(3);
    let light = theme::current();
    let fill = harness.ctx.global_style().visuals.panel_fill;
    // Restore before asserting, so a failure cannot leave the shared
    // table light for the next test that takes the lock.
    let skeleton = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_shell_skeleton(&harness);
    }));
    theme_set(&harness.state_mut().state.world, &ThemeRef::Dark);
    harness.run_steps(1);
    assert_eq!(light, theme::Tokens::LIGHT, "the frame-edge sync rode the world's reference");
    assert_eq!(contrast::readable(&light), None, "the light table reads");
    assert_eq!(fill, theme::Tokens::LIGHT.ink, "egui's visuals follow the light table");
    if let Err(e) = skeleton {
        std::panic::resume_unwind(e);
    }
}

/// `egui_tool_editor.png`'s intent: Edit opens one floating window over
/// the dock, titled by the tool, fields left of the to-scale profile,
/// every field on the world's numbers, Apply/Close at its foot.
#[test]
fn tool_editor_structure() {
    let _g = snapshot_guard();
    let mut harness = shell_harness();
    harness.state_mut().state.ui_target = UiTarget::Tool("EM-0600-4FL".to_owned());
    harness.run();
    harness.run();
    harness.get_by_label("Edit").click();
    harness.run_steps(3);
    let win = harness
        .query_all_by_role(egui::accesskit::Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some("Tool: EM-0600-4FL"))
        .expect("Edit must open the editor window titled by the tool");
    let wr = win.rect();
    assert!(
        wr.min.x > 0.0 && wr.max.x < 1280.0 && wr.min.y > 0.0 && wr.max.y <= 720.0,
        "the editor floats inside the shell: {wr:?}"
    );
    // Fields in their order, each on the seed tool's number.
    let fields = [
        ("Diameter", "6.0 mm"),
        ("Corner radius", "0.0 mm"),
        ("Flute length", "15.0 mm"),
        ("Shank diameter", "8.0 mm"),
        ("Shank length", "40.0 mm"),
        ("Holder diameter", "44.0 mm"),
        ("Holder length", "35.0 mm"),
        ("Spindle rpm", "9500.0"),
        ("Feed (mm/min)", "1800.0 mm/min"),
    ];
    let spins: Vec<_> = win.get_all_by_role(egui::accesskit::Role::SpinButton).collect();
    let mut last_y = f32::MIN;
    for (label, value) in fields {
        let y = win.get_by_label(label).rect().center().y;
        assert!(y > last_y, "{label:?} must follow the field above it");
        last_y = y;
        assert!(
            spins.iter().any(|s| s.value().as_deref() == Some(value)),
            "the editor must show {label:?} = {value}"
        );
    }
    // Fields on the left half; the right half is the painted profile.
    assert!(
        win.get_by_label("Shank diameter").rect().max.x < wr.center().x,
        "fields sit left of the profile"
    );
    assert!(win.query_by_label("Centre cutting (may plunge into solid)").is_some());
    assert!(win.query_by_label("Apply").is_some() && win.query_by_label("Close").is_some());
    assert!(
        win.get_by_label("Apply").rect().min.y >= last_y,
        "Apply sits at the window's foot"
    );
}

/// #93 slice 3: the Draw cutter view's parameters stand in the editor's
/// third column, right of the fields and the profile, in the order the
/// tool is drawn (cutter, flutes, shank, neck), above Apply cutter.
#[test]
fn tool_cutter_params_structure() {
    let mut harness = shell_harness();
    harness.run();
    crate::actions::open_tool_editor(&mut harness.state_mut().state, "EM-0600-4FL");
    harness.run_steps(2);
    harness.get_by_label("Profile").click();
    harness.run();
    harness.get_by_label("Draw cutter").click();
    harness.run_steps(2);
    let win = harness
        .query_all_by_role(egui::accesskit::Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some("Tool: EM-0600-4FL"))
        .expect("the editor window");
    let fields_right = win.get_by_label("Shank diameter").rect().max.x;
    let mut last_y = f32::MIN;
    for label in ["cutter Ø", "flutes", "shank Ø", "Neck", "Apply cutter"] {
        let r = win.get_by_label(label).rect();
        assert!(r.center().y > last_y, "{label:?} must follow the row above it");
        assert!(r.min.x > fields_right, "{label:?} sits in the third column");
        last_y = r.center().y;
    }
    assert!(win.get_by_label("shank length").rect().min.x > win.get_by_label("shank Ø").rect().max.x, "shank length beside shank Ø");
}

/// #557 / #554 / #555: a setup's own page carries the part-alignment rows in
/// the properties leaf - the plane row (a face), the three-points row (#555) and the axes-in-the-plane
/// rows (an edge or two points), each saying which pick it takes, with nothing armed and Apply hidden until
/// an axis is picked, and every option of the plane rows on screen (#330).
#[test]
fn setup_page_alignment_rows_structure() {
    let _g = snapshot_guard();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::blank());
    {
        let st = &mut h.state_mut().state;
        st.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        st.drain_actions();
        crate::actions::target(st, UiTarget::SetupNode("Setup1".into(), crate::state::SetupPage::Settings));
    }
    h.run_steps(3);
    // #554: each row says which pick it takes - a face for the plane.
    let plane = h.query_by_label_contains("Lay on a plane - pick a face").expect("the plane row shows").rect();
    assert!(h.query_by_label("Pick a face").is_some_and(|n| !n.accesskit_node().is_disabled()), "the plane row picks a face");
    assert!(h.query_by_label("Pick face or edges").is_none(), "#554: the plane row offers no edge pick");
    let axes = h.query_by_label_contains("Axes in the plane - pick an edge or two points").expect("the axes rows show");
    assert!(plane.min.y < axes.rect().min.y, "the axes rows follow the plane row");
    for label in ["+Y by Edge", "+Y by Two points", "Pick +Y edge", "Flip +Y", "+X by Edge", "+X by Two points", "Pick +X edge", "Flip +X"] {
        assert!(h.query_by_label(label).is_some(), "the axes rows must show {label:?}");
        assert!(!disabled(&h, label), "{label:?} is live");
    }
    assert!(h.query_by_label("Apply axes").is_none(), "Apply waits for an axis pick");
    assert!(h.state().state.axis_pick.is_none(), "nothing is armed by drawing the page");
    // #555: the three-points row sits right under the face row, says it
    // takes points, and shows no prompts or Apply until it is armed.
    let points = h.query_by_label_contains("Lay on a plane - pick three points").expect("the points row shows").rect();
    assert!(plane.min.y < points.min.y && points.min.y < axes.rect().min.y, "face row, points row, then the axes rows");
    for label in ["Pick three points", "Points down", "Point 1 to plane level", "3-2-1 frame"] {
        assert!(h.query_by_label(label).is_some(), "the points row must show {label:?}");
        assert!(!disabled(&h, label), "{label:?} is live");
    }
    for label in ["Apply points", "Remove last", "Clear points"] {
        assert!(h.query_by_label(label).is_none(), "{label:?} waits for the row to be armed");
    }
    assert!(h.query_by_label_contains("Point 1:").is_none(), "no prompts until armed");
    // #330 (reopened), #606: short lines, so every option lies inside the
    // narrow properties pane - not merely on screen: one long row ran
    // Flip, the level box and the pick buttons past the pane's right edge,
    // and a control clipped at that edge is out of reach even where the
    // window still has pixels. The pane is the dock leaf's tab body.
    let pane = h
        .state()
        .dock
        .iter_leaves()
        .find(|(_, l)| l.tabs.contains(&Tab::Properties))
        .map(|(_, l)| l.viewport)
        .expect("the properties leaf");
    assert!(pane.width() > 100.0 && pane.width() < 640.0, "the pane is the narrow right leaf: {pane:?}");
    for label in ["Pick a face", "Face down", "Face to plane level", "Pick three points", "Points down", "Point 1 to plane level", "3-2-1 frame", "+Y by Two points", "Pick +Y edge", "Flip +Y", "+X by Two points", "Pick +X edge", "Flip +X"] {
        let r = h.get_by_label(label).rect();
        assert!(pane.contains_rect(r), "{label:?} lies inside the properties pane {pane:?}: {r:?}");
    }
    // #606: the Part alignment row's turn - its angles, its pivot (WCS
    // origin by default, named on the row) and its Apply - too.
    let pivot = h
        .get_by(|n| n.role() == egui::accesskit::Role::ComboBox && n.value().as_deref() == Some("WCS origin"))
        .rect();
    assert!(pane.contains_rect(pivot), "the turn's pivot lies inside the pane: {pivot:?}");
    let turn = h.query_by_label_contains("Turn").expect("the turn row").rect();
    let applies: Vec<egui::Rect> = h.get_all_by_label("Apply").map(|n| n.rect()).collect();
    assert!(applies.iter().any(|r| r.min.y > turn.min.y), "the turn's Apply shows under its angles");
    for r in applies {
        assert!(pane.contains_rect(r), "Apply lies inside the pane: {r:?}");
    }
    assert!(h.state().state.points_pick.is_none(), "nothing is armed by drawing the page");
}

/// #1031: a lathe setup's Settings page carries the "Workholding max rpm"
/// row (a value, Apply, and Clear once a limit is stated); Apply lands one
/// undoable `SetSetupWorkholdingRpm`. A mill setup shows no row.
#[test]
fn setup_page_workholding_rpm_row_structure() {
    let _g = snapshot_guard();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1280.0, 1600.0)) // the row sits at the page's foot
        .build_ui_state(frame, ShellUi::blank());
    {
        let st = &mut h.state_mut().state;
        st.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        st.drain_actions();
        crate::actions::target(st, UiTarget::SetupNode("Setup1".into(), crate::state::SetupPage::Settings));
    }
    h.run_steps(3);
    let row = crate::egui_app::setup_workholding_row::ROW_LABEL.trim();
    assert!(h.query_by_label_contains(row).is_none(), "a mill setup has no chuck rating row");
    {
        let st = &mut h.state_mut().state;
        let lathe = camcraft_app::machine::Machine { kind: Some("lathe".into()), ..Default::default() };
        let load = camcraft_app::commands::LoadMachine::new(&st.world, "lathe.mch".into(), Some(lathe));
        st.history.execute(&st.world, load.expect("a lathe"));
    }
    h.run_steps(3);
    assert!(h.query_by_label_contains(row).is_some(), "a lathe setup shows the row");
    assert!(h.query_by_label_contains("No workholding limit stated").is_some());
    assert!(h.query_by_label("Clear rpm").is_none(), "nothing to clear yet");
    assert!(!disabled(&h, "Apply rpm"), "Apply is live");
    h.get_by_label("Apply rpm").click();
    h.run_steps(3);
    let world = &h.state().state.world;
    assert_eq!(camcraft_app::setup_workholding::of(world, Some("Setup1")), Some(2000.0), "the row's default, applied: {}", h.state().state.status);
    assert!(h.query_by_label_contains("Stated: 2000 rpm").is_some());
    assert!(h.query_by_label("Clear rpm").is_some_and(|n| !n.accesskit_node().is_disabled()), "Clear shows once stated");
    let st = &mut h.state_mut().state;
    assert!(st.history.undo(&st.world), "one undo step");
    assert_eq!(camcraft_app::setup_workholding::of(&st.world, Some("Setup1")), None);
}

/// #948: at the default 1280 px window every ribbon button reads whole
/// (no elided galley, the "Milli..." clash gone: the four section doors
/// say four different things), and the group captions are painted again
/// under the cells they name. Text and coarse placement only.
#[test]
fn ribbon_labels_whole_and_captions_painted() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    h.run_steps(3);
    // (text, top-left, elided) of every painted galley, in paint order.
    fn walk(shape: &egui::Shape, out: &mut Vec<(String, egui::Pos2, bool)>) {
        match shape {
            egui::Shape::Text(t) => {
                out.push((t.galley.text().to_owned(), t.pos, t.galley.elided));
            }
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut all = Vec::new();
    for s in &h.output().shapes {
        walk(&s.shape, &mut all);
    }
    let tab_y = at(&painted(&h), "Browser").y;
    let ribbon: Vec<_> = all.iter().filter(|(_, p, _)| p.y < tab_y).cloned().collect();
    assert!(!ribbon.is_empty(), "the ribbon paints text");
    for (t, _, elided) in &ribbon {
        assert!(!elided, "ribbon text {t:?} is elided at 1280 px");
    }
    // Every button is identifiable: the cell words, the four doors, each
    // once - no two buttons share a label.
    let words = [
        "New", "Model", "Open", "Save", "Save As", "Post", "Setups", "Stock", "Origin", "Tools",
        "Undo", "Redo", "Run", "Delete", "Frame", "Paths", "Simulate", "Deviate", "Panels\u{2026}",
        "Theme\u{2026}", "Mill 2D\u{2026}", "Mill 3D\u{2026}", "Drill\u{2026}", "Multi\u{2026}",
    ];
    for w in words {
        let n = ribbon.iter().filter(|(t, _, _)| t == w).count();
        assert_eq!(n, 1, "ribbon button {w:?} must be painted exactly once, found {n}");
    }
    // The captions are back: one under each multi-cell group, in left to
    // right order, below the cell row and above the dock.
    let caps: Vec<(&str, egui::Pos2)> = ["File", "Setup", "Modify", "Inspect", "View"]
        .into_iter()
        .map(|c| {
            let hit = ribbon.iter().find(|(t, _, _)| t == c);
            match hit {
                Some((_, p, _)) => (c, *p),
                None => panic!("caption {c:?} missing; ribbon: {ribbon:?}"),
            }
        })
        .collect();
    let new_y = ribbon.iter().find(|(t, _, _)| t == "New").map(|(_, p, _)| p.y).unwrap_or(0.0);
    for (c, p) in &caps {
        assert!(p.y > new_y, "caption {c:?} sits under the cell labels");
    }
    for pair in caps.windows(2) {
        assert!(pair[0].1.x < pair[1].1.x, "captions run left to right: {pair:?}");
    }
}

/// #645 re-review: the structural both-states check for the "Show origin
/// names" switch. The seeded document's G54 offset sits at world zero and
/// its triad carries the name row; while the switch is on (the #727
/// default) the painted frame shows that name inside the viewport and
/// the checkbox click drops it. The big `X`/`Y`/`Z` letters and the
/// triad's strokes ignore the switch. `view_names.rs` drives the same
/// checkbox through State and the view file; this pins what the shell
/// paints. The properties pane's WCS row also reads "G54", so the
/// painted probe stays inside the viewport, as the view-cube check does.
/// The re-review also pins the checkbox's AccessKit checked state, in
/// both states: `toggled()` is AccessKit 0.24's name for it.
#[test]
fn origin_names_paint_on_and_quiet_off_letters_and_lines_always() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    // Redirect the at-once persist away from the real user's view.json -
    // the flip would otherwise write it out of this test process.
    let d = std::env::temp_dir().join(format!(
        "camcraft-view-structural-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the test folder");
    h.state_mut().state.view_store = Some(d.join("view.json"));
    // On: the painted frame shows the offset's name row in the viewport,
    // and the shared projection law agrees.
    h.run_steps(2);
    let texts = painted(&h);
    let v = at(&texts, "Viewport").x;
    let p = at(&texts, "Properties").x;
    let in_viewport = |(t, q): &(String, egui::Pos2)| t == "G54" && q.x > v && q.x < p;
    let letters = |labels: &[([f32; 2], [f32; 3], String)]| -> Vec<String> {
        labels
            .iter()
            .filter(|(_, _, t)| interact::is_axis_label(t))
            .map(|(_, _, t)| t.clone())
            .collect()
    };
    let (segs_on, letters_on) = {
        let st = &h.state().state;
        assert!(st.show_gizmo_names, "the #727 default is on");
        assert_eq!(
            h.get_by_label("Show origin names").accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::True),
            "the checkbox reads checked in the default state"
        );
        let labels = interact::overlay_labels(st, 1.0);
        assert!(
            labels.iter().any(|(_, _, t)| t == "G54"),
            "on: the offset's name is in the labels: {labels:?}"
        );
        assert!(
            texts.iter().any(in_viewport),
            "on: the name row is painted in the viewport"
        );
        let segs = interact::gizmo_segments(st, 1.0);
        assert!(!segs.is_empty(), "the triad's strokes draw");
        let letters = letters(&labels);
        assert!(!letters.is_empty(), "the axis letters draw");
        (segs.len(), letters)
    };
    // Off: the checkbox clicks the flag through the real widget and the
    // name row leaves the painted frame; letters and lines stay.
    h.get_by_label("Show origin names").click();
    h.run_steps(2);
    let texts = painted(&h);
    {
        let st = &h.state().state;
        assert!(!st.show_gizmo_names, "the click flipped the flag");
        assert_eq!(
            h.get_by_label("Show origin names").accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::False),
            "the checkbox reads unchecked after the click"
        );
        let labels = interact::overlay_labels(st, 1.0);
        assert!(
            labels.iter().all(|(_, _, t)| t != "G54"),
            "off: no name row anywhere: {labels:?}"
        );
        assert!(
            !texts.iter().any(in_viewport),
            "off: the painted viewport no longer shows the name row"
        );
        // Letters and strokes ignore the switch: the same letters and
        // the same segments draw either way.
        assert_eq!(letters(&labels), letters_on, "the letters survive untouched");
        assert_eq!(
            interact::gizmo_segments(st, 1.0).len(),
            segs_on,
            "the strokes ignore the switch"
        );
    }
    let _ = std::fs::remove_dir_all(&d);
}
