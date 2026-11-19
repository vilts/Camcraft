//! #2065: shared dropdown menu widths, recent width, trigger gap, and
//! row horizontal padding come from the theme, clamp inside narrow
//! windows, and keep `machine_menu_width` expansion intact.
//! Provenance: our own structural tests over our own dropdown drawers.

use camcraft_app::machine::Kind as MachineKind;
use camcraft_app::operation::Section;
use camcraft_app::recent::RecentKind;
use egui::{CornerRadius, Pos2, Rect, Stroke, TextureId, Vec2};

use super::super::{recent_drop, theme_drop};
use super::*;
use crate::state::TreeMenuTarget;

#[test]
fn hostile_direct_dropdown_tokens_and_tiny_windows_stay_usable() {
    let mut pal = theme::Tokens::DARK;
    pal.menu_width = f32::NAN;
    pal.recent_width = f32::INFINITY;
    pal.menu_gap = f32::NEG_INFINITY;
    pal.menu_pad_x = -900.0;
    pal.row_height = f32::NAN;
    let g = pal.dropdown();
    assert!(g.menu_width.is_finite() && g.menu_width > 0.0);
    assert!(g.recent_width.is_finite() && g.recent_width > 0.0);
    assert!(g.gap.is_finite() && g.gap >= 0.0);
    assert!(g.pad_x.is_finite() && g.pad_x >= 0.0);
    assert!(g.icon_text_x.is_finite() && g.box_px.is_finite());
    for available in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(g.fit_width(g.menu_width, available), g.menu_width);
    }
    assert_eq!(g.fit_width(g.menu_width, -20.0), 0.0);
    for width in [0.0, 20.0, 39.0] {
        let bounds = Rect::from_min_size(Pos2::new(300.0, 0.0), Vec2::new(width, 200.0));
        let fitted = g.fit_width(g.menu_width, width);
        let left = g.fit_left(600.0, bounds, fitted);
        assert_eq!(fitted, width, "the popup fits even below normal minimum widths");
        assert!(left >= bounds.left() && left + fitted <= bounds.right());
        assert_eq!(g.fit_left(100.0, bounds, fitted), bounds.left());
    }
    for row_height in [0.0, 1000.0] {
        let pal = theme::Tokens { row_height, ..theme::Tokens::DARK };
        let g = pal.dropdown();
        assert!((6.0..=16.0).contains(&g.box_px));
        assert!(g.icon_text_x <= g.pad_x * 1.5 + 24.0);
    }
}

struct RestoreDark;

impl Drop for RestoreDark {
    fn drop(&mut self) {
        theme::set_current(&theme::Tokens::DARK);
    }
}

struct Drawn {
    rects: Vec<egui::epaint::RectShape>,
    texts: Vec<(String, Pos2, Rect)>,
}

fn collect_drawn(h: &Harness<'static, ShellUi>) -> Drawn {
    fn walk(shape: &egui::Shape, clip: Rect, out: &mut Drawn) {
        match shape {
            egui::Shape::Rect(r) if r.fill_texture_id() == TextureId::default() => {
                out.rects.push(r.clone());
            }
            egui::Shape::Text(t) => out.texts.push((t.galley.text().to_owned(), t.pos, clip)),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, out)),
            _ => {}
        }
    }
    let mut out = Drawn { rects: Vec::new(), texts: Vec::new() };
    for s in &h.output().shapes {
        walk(&s.shape, s.clip_rect, &mut out);
    }
    out
}

fn popup_panel(drawn: &Drawn, pal: &theme::Tokens, contains_text: &str) -> Rect {
    let menu_rad = CornerRadius::same(pal.radius_menu);
    drawn
        .rects
        .iter()
        .filter(|r| {
            r.fill == pal.ink
                && r.stroke == Stroke::NONE
                && r.corner_radius == menu_rad
                && r.rect.width() < 700.0
                && drawn
                    .texts
                    .iter()
                    .any(|(t, p, _)| t.contains(contains_text) && r.rect.contains(*p))
        })
        .map(|r| r.rect)
        .min_by(|a, b| a.width().total_cmp(&b.width()))
        .unwrap_or_else(|| {
            let matching_texts: Vec<_> = drawn
                .texts
                .iter()
                .filter(|(t, _, _)| t.contains(contains_text))
                .map(|(t, p, _)| (t.as_str(), *p))
                .collect();
            panic!(
                "expected popup panel containing {contains_text:?}; matching_texts={matching_texts:?}; radius={:?}",
                drawn.rects.iter().filter(|r| r.fill == pal.ink && r.rect.width() < 700.0).map(|r| (r.rect, r.corner_radius, r.stroke)).collect::<Vec<_>>()
            );
        })
}

fn text_entry(drawn: &Drawn, exact_or_prefix: &str, panel: Rect) -> (Pos2, Rect) {
    drawn
        .texts
        .iter()
        .find(|(t, p, _)| t.starts_with(exact_or_prefix) && panel.contains(*p))
        .map(|(_, p, c)| (*p, *c))
        .unwrap_or_else(|| panic!("expected row text {exact_or_prefix:?} inside {panel:?}"))
}

fn custom_theme_file(tag: &str, body: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("camcraft_drop_theme_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create test theme dir");
    let path = dir.join("custom.json");
    std::fs::write(&path, body).expect("write test theme file");
    path
}

#[test]
fn narrow_window_clamps_dropdown_widths_to_stay_on_screen() {
    let _g = snapshot_guard();
    let pal = theme::Tokens::DARK;
    let state = State::new();
    let narrow = Rect::from_min_size(Pos2::ZERO, Vec2::new(160.0, super::super::shell::RIBBON_H));
    let near_right = Rect::from_min_size(Pos2::new(100.0, 24.0), Vec2::new(50.0, 30.0));

    let view_rows = panes::view_rows_of(near_right, narrow, &pal);
    assert!(view_rows.iter().all(|(_, r)| r.width() <= narrow.width() && r.right() <= narrow.right()));
    assert!(view_rows[0].1.width() >= 80.0);

    let theme_rows = theme_drop::rows_of(&state, near_right, narrow, &pal);
    assert!(theme_rows.iter().all(|(_, r)| r.width() <= narrow.width() && r.right() <= narrow.right()));
    assert!(theme_rows[0].1.width() >= 80.0);

    let plan = panes::ribbon_plan(MachineKind::Mill, &[]);
    let (_, _, _, drops) = panes::ribbon_cells(narrow, &pal, &plan);
    assert!(!drops.is_empty(), "section drops exist on the narrow ribbon");
    for d in drops {
        for (_, r) in d.rows {
            assert!(r.width() <= narrow.width() && r.right() <= narrow.right(), "section row overflowed {narrow:?}: {r:?}");
        }
    }

    let recent_narrow = Rect::from_min_size(Pos2::ZERO, Vec2::new(220.0, super::super::shell::RIBBON_H));
    let recent_rows = recent_drop::rows_of(&state, RecentKind::Project, near_right, recent_narrow, &pal);
    for (_, r) in recent_rows {
        assert!(r.width() <= recent_narrow.width() && r.right() <= recent_narrow.right(), "recent row overflowed {recent_narrow:?}: {r:?}");
        assert!(r.width() >= 80.0);
    }

    let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(170.0, 480.0));

    // machine_rows stays inside narrow bounds too.
    let g = pal.dropdown();
    let m_w = g.fit_width(panes::machine_menu_width(220.0, &pal), screen.width());
    let m_rows = panes::machine_rows(near_right, screen, pal.row_height, m_w, &g);
    assert!(m_rows.iter().all(|(_, r)| r.width() <= screen.width() && r.right() <= screen.right()));
    assert!(m_rows[0].1.width() >= 80.0);

    // Narrow rendered shell: tree_menu and fixture_menu stay inside content_rect.
    let check_narrow_menu = |setup: fn(&mut State), needle: &str| {
        let mut h = Harness::builder().with_size(screen.size()).build_ui_state(frame, ShellUi::seeded());
        setup(&mut h.state_mut().state);
        h.run_steps(2);
        let drawn = collect_drawn(&h);
        let panel = popup_panel(&drawn, &pal, needle);
        assert!(panel.right() <= screen.right() + 0.5, "{needle} overflowed {screen:?}: {panel:?}");
        assert!(panel.left() >= screen.left() - 0.5, "{needle} left-overflowed {screen:?}: {panel:?}");
        assert!(panel.width() >= 80.0, "{needle} collapsed: {panel:?}");
    };
    check_narrow_menu(|st| st.tree_menu = Some(TreeMenuTarget::Operation("Contour1".into())), "Delete");
    check_narrow_menu(|st| st.tree_menu = Some(TreeMenuTarget::Model { name: "Block".into(), row: 0 }), "Use as fixture");
}

#[test]
fn all_dropdown_drawers_follow_theme_tokens() {
    let _g = snapshot_guard();
    let _restore = RestoreDark;
    let json = r#"{
        "format": "camcraft-theme/1",
        "descriptor": { "name": "Wide Drops" },
        "base": "dark",
        "geometry": {
            "menu_width": 260.0,
            "recent_width": 360.0,
            "menu_gap": 10.0,
            "menu_pad_x": 16.0
        }
    }"#;
    let parsed = super::super::theme_files::parse_text(json, "wide.json")
        .expect("dropdown theme tokens must parse");
    let custom_pal = parsed.tokens;
    let path = custom_theme_file("all_drawers", json);

    let mut h = shell_harness();
    theme_set(&h.state_mut().state.world, &ThemeRef::File(path.to_string_lossy().into_owned()));
    h.run_steps(2);
    let pal = theme::current();
    assert_eq!(pal, custom_pal, "custom dropdown theme is active");

    // `Harness::build_ui_state` wraps the frame in an 8 px CentralPanel margin.
    let strip = Rect::from_min_size(Pos2::new(8.0, 8.0), Vec2::new(1280.0 - 16.0, theme::ribbon_h(&pal)));
    let plan = panes::ribbon_plan(MachineKind::Mill, &[]);
    let (cells, _, _, drops) = panes::ribbon_cells(strip, &pal, &plan);
    let btn = |cmd: Cmd| cells.iter().find(|c| c.cmd == cmd).expect("ribbon button").rect;

    // 1. panes section dropdown
    let sec_drop = drops.iter().find(|d| d.section == Section::Mill2D).expect("Mill2D drop");
    assert!((sec_drop.rows[0].1.width() - 260.0).abs() < 0.01);
    assert!((sec_drop.rows[0].1.top() - sec_drop.button.bottom() - 10.0).abs() < 0.01);
    let first_sec_label = match sec_drop.rows[0].0 {
        Cmd::Create(i) => panes::TYPES[i],
        _ => "",
    };
    h.state_mut().state.ribbon_more = Some(Section::Mill2D);
    h.run_steps(2);
    let drawn = collect_drawn(&h);
    let panel = popup_panel(&drawn, &pal, first_sec_label);
    assert!((panel.width() - 260.0).abs() < 0.01, "section panel: {panel:?}");
    assert!(
        (panel.top() - sec_drop.button.bottom() - 10.0).abs() < 0.01,
        "section top {} vs btn bottom {} + 10",
        panel.top(),
        sec_drop.button.bottom(),
    );
    // The shell harness loads the icon set, so the operation row wears
    // its kind icon: the label clears it by the padding (8 + 16 + 16).
    let (txt_pos, _) = text_entry(&drawn, first_sec_label, panel);
    assert!((txt_pos.x - (panel.left() + 40.0)).abs() < 0.5, "section text x: {txt_pos:?} vs panel {panel:?}");
    h.state_mut().state.ribbon_more = None;

    // 2. panes machine dropdown math (uses menu_width, gap, and preserves machine_menu_width expansion)
    let g = pal.dropdown();
    assert!((panes::machine_menu_width(10.0, &pal) - 260.0).abs() < 0.01);
    assert!((panes::machine_menu_width(400.0, &pal) - 432.0).abs() < 0.01);
    let m_rows = panes::machine_rows(sec_drop.button, strip, pal.row_height, 260.0, &g);
    assert!((m_rows[0].1.top() - sec_drop.button.bottom() - 10.0).abs() < 0.01);

    // 3. layout::view_rows / view_drop (width, gap, checkbox inset, text inset, expand hit zone)
    let view_btn = btn(Cmd::View);
    let vrows = panes::view_rows_of(view_btn, strip, &pal);
    assert!((vrows[0].1.width() - 260.0).abs() < 0.01);
    assert!((vrows[0].1.top() - view_btn.bottom() - 10.0).abs() < 0.01);
    h.state_mut().state.ribbon_view_open = true;
    h.run_steps(2);
    let drawn = collect_drawn(&h);
    let panel = popup_panel(&drawn, &pal, "Viewport");
    assert!((panel.width() - 260.0).abs() < 0.01);
    assert!((panel.top() - view_btn.bottom() - 10.0).abs() < 0.01);
    let (txt_pos, _) = text_entry(&drawn, "Viewport", panel);
    assert!((txt_pos.x - (panel.left() + 40.0)).abs() < 0.5, "view text x: {txt_pos:?}");
    let vp_row = vrows[0].1;
    let cbox = drawn
        .rects
        .iter()
        .find(|r| r.fill == pal.accent && vp_row.contains(r.rect.center()))
        .expect("Viewport checkbox fill drawn")
        .rect;
    assert!((cbox.center().x - (panel.left() + 20.0)).abs() < 0.5, "checkbox center x: {cbox:?}");
    // Pressing inside the 10px menu_gap (8px below button bottom) must NOT close the View door.
    let gap_click = Pos2::new(view_btn.center().x, view_btn.bottom() + 8.0);
    set_pos(&mut h, gap_click);
    button_at(&mut h, egui::PointerButton::Primary, gap_click, true);
    button_at(&mut h, egui::PointerButton::Primary, gap_click, false);
    assert!(h.state().state.ribbon_view_open, "button.expand(gap) keeps View door open in 10px gap");
    h.state_mut().state.ribbon_view_open = false;

    // 4. theme_drop
    let theme_btn = btn(Cmd::Theme);
    let trows = theme_drop::rows_of(&h.state().state, theme_btn, strip, &pal);
    assert!((trows[0].1.width() - 260.0).abs() < 0.01);
    assert!((trows[0].1.top() - theme_btn.bottom() - 10.0).abs() < 0.01);
    h.state_mut().state.ribbon_theme_open = true;
    h.run_steps(2);
    let drawn = collect_drawn(&h);
    let panel = popup_panel(&drawn, &pal, "Colour vision");
    assert!((panel.width() - 260.0).abs() < 0.01);
    let (txt_pos, _) = text_entry(&drawn, "Colour vision", panel);
    assert!((txt_pos.x - (panel.left() + 16.0)).abs() < 0.5, "theme text x: {txt_pos:?}");
    h.state_mut().state.ribbon_theme_open = false;

    // 5. recent_drop
    let dummy = path.with_file_name("sample.camproj");
    std::fs::write(&dummy, b"{}").ok();
    h.state_mut().state.recent.add(RecentKind::Project, &dummy);
    let open_btn = btn(Cmd::Project);
    let rrows = recent_drop::rows_of(&h.state().state, RecentKind::Project, open_btn, strip, &pal);
    assert!((rrows[0].1.width() - 360.0).abs() < 0.01, "recent_width must be 360: {:?}", rrows[0].1);
    assert!((rrows[0].1.top() - open_btn.bottom() - 10.0).abs() < 0.01);
    h.state_mut().state.ribbon_recent_open = Some(RecentKind::Project);
    h.run_steps(2);
    let drawn = collect_drawn(&h);
    let panel = popup_panel(&drawn, &pal, recent_drop::CLEAR);
    assert!((panel.width() - 360.0).abs() < 0.01, "recent panel: {panel:?}");
    let (txt_pos, clip) = text_entry(&drawn, recent_drop::CLEAR, panel);
    assert!((txt_pos.x - (panel.left() + 16.0)).abs() < 0.5, "recent text x: {txt_pos:?}");
    assert!((clip.left() - (panel.left() + 8.0)).abs() < 0.5, "recent clip inset: {clip:?}");
    h.state_mut().state.ribbon_recent_open = None;

    // 6. tree_menu & 7. fixture_menu: measure delta from default theme so dock chrome height cancels.
    h.state_mut().state.tree_menu = Some(TreeMenuTarget::Operation("Contour1".into()));
    h.run_steps(2);
    let drawn = collect_drawn(&h);
    let panel = popup_panel(&drawn, &pal, "Delete");
    assert!((panel.width() - 260.0).abs() < 0.01, "tree panel: {panel:?}");
    let (txt_pos, _) = text_entry(&drawn, "Delete", panel);
    assert!((txt_pos.x - (panel.left() + 16.0)).abs() < 0.5, "tree text x: {txt_pos:?}");
    let custom_tree_top = panel.top();
    h.state_mut().state.tree_menu = None;

    let (_, leaves) = panes::browser_rows(&h.state().state);
    let mod_ix = leaves.iter().position(|l| matches!(l, panes::Leaf::Model(n) if n == "Block")).expect("Block");
    h.state_mut().state.tree_menu = Some(TreeMenuTarget::Model { name: "Block".into(), row: mod_ix });
    h.run_steps(2);
    let drawn = collect_drawn(&h);
    let panel = popup_panel(&drawn, &pal, "Use as fixture");
    assert!((panel.width() - 260.0).abs() < 0.01, "fixture panel: {panel:?}");
    let (txt_pos, _) = text_entry(&drawn, "Use as fixture", panel);
    assert!((txt_pos.x - (panel.left() + 16.0)).abs() < 0.5, "fixture text x: {txt_pos:?}");
    let custom_fix_top = panel.top();
    h.state_mut().state.tree_menu = None;

    // Switch back to Dark (menu_gap = 2.0) and confirm tree/fixture popup tops shift by 10.0 - 2.0 = 8.0 px.
    theme_set(&h.state_mut().state.world, &ThemeRef::Dark);
    h.run_steps(2);
    let dark = theme::current();
    h.state_mut().state.tree_menu = Some(TreeMenuTarget::Operation("Contour1".into()));
    h.run_steps(2);
    let dark_tree = popup_panel(&collect_drawn(&h), &dark, "Delete");
    assert!((custom_tree_top - dark_tree.top() - 8.0).abs() < 0.5, "tree gap shift: {} -> {}", dark_tree.top(), custom_tree_top);
    h.state_mut().state.tree_menu = Some(TreeMenuTarget::Model { name: "Block".into(), row: mod_ix });
    h.run_steps(2);
    let dark_fix = popup_panel(&collect_drawn(&h), &dark, "Use as fixture");
    assert!((custom_fix_top - dark_fix.top() - 8.0).abs() < 0.5, "fixture gap shift: {} -> {}", dark_fix.top(), custom_fix_top);

    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
