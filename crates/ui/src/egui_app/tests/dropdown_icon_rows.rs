//! #2065 / PR #2457 merge: the operation dropdown's kind icon and its
//! label clearance come from the shared menu metrics. With the icon set
//! loaded, the icon sits at the half-padding inset and the label clears
//! it by one row padding; the defaults keep the 4 / 16 / 28 layout.
//! Provenance: our own structural tests over our own dropdown drawer.

use camcraft_app::operation::Section;
use egui::{Pos2, Rect, TextureId};

use super::*;

struct RestoreDark;

impl Drop for RestoreDark {
    fn drop(&mut self) {
        theme::set_current(&theme::Tokens::DARK);
    }
}

/// The textured rects (icons) and the texts of the last frame.
fn icons_and_texts(h: &Harness<'static, ShellUi>) -> (Vec<Rect>, Vec<(String, Pos2)>) {
    fn walk(shape: &egui::Shape, icons: &mut Vec<Rect>, texts: &mut Vec<(String, Pos2)>) {
        match shape {
            egui::Shape::Rect(r) if r.fill_texture_id() != TextureId::default() => icons.push(r.rect),
            egui::Shape::Mesh(m) if m.texture_id != TextureId::default() => icons.push(m.calc_bounds()),
            egui::Shape::Text(t) => texts.push((t.galley.text().to_owned(), t.pos)),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, icons, texts)),
            _ => {}
        }
    }
    let (mut icons, mut texts) = (Vec::new(), Vec::new());
    for s in &h.output().shapes {
        walk(&s.shape, &mut icons, &mut texts);
    }
    (icons, texts)
}

/// Open the Mill2D section drop under the active theme and measure its
/// first row: the row rect, the icon rect in it, the label x.
fn first_op_row(h: &mut Harness<'static, ShellUi>) -> (Rect, Rect, f32) {
    let pal = theme::current();
    // `Harness::build_ui_state` wraps the frame in an 8 px CentralPanel margin.
    let strip = Rect::from_min_size(Pos2::new(8.0, 8.0), Vec2::new(1280.0 - 16.0, theme::ribbon_h(&pal)));
    let plan = panes::ribbon_plan(camcraft_app::machine::Kind::Mill, &[]);
    let (_, _, _, drops) = panes::ribbon_cells(strip, &pal, &plan);
    let drop = drops.iter().find(|d| d.section == Section::Mill2D).expect("Mill2D drop");
    let (cmd, row) = drop.rows[0];
    let Cmd::Create(i) = cmd else { panic!("the first Mill2D row creates an operation") };
    let label = panes::TYPES[i];
    h.state_mut().state.ribbon_more = Some(Section::Mill2D);
    h.run_steps(2);
    let (icons, texts) = icons_and_texts(h);
    h.state_mut().state.ribbon_more = None;
    h.run_steps(1);
    let icon = icons
        .into_iter()
        .find(|r| row.contains(r.center()))
        .unwrap_or_else(|| panic!("an operation icon is drawn in the row {row:?}"));
    let x = texts
        .iter()
        .find(|(t, p)| t == label && row.contains(*p))
        .map(|(_, p)| p.x)
        .unwrap_or_else(|| panic!("the label {label:?} is drawn in the row {row:?}"));
    (row, icon, x)
}

#[test]
fn op_dropdown_icon_and_label_follow_menu_metrics() {
    let _g = snapshot_guard();
    let _restore = RestoreDark;
    let mut h = shell_harness();
    h.run_steps(2);

    // Default theme: main's 4 / 16 / 28 row, now derived from the metrics.
    let g = theme::current().dropdown();
    assert_eq!((g.clip_inset_x, g.icon_px, g.icon_text_x), (4.0, 16.0, 28.0));
    let (row, icon, x) = first_op_row(&mut h);
    assert!((icon.left() - (row.left() + 4.0)).abs() < 0.5, "default icon left: {icon:?} in {row:?}");
    assert!((icon.width() - 16.0).abs() < 0.5, "default icon size: {icon:?}");
    assert!((x - (row.left() + 28.0)).abs() < 0.5, "default label x: {x} in {row:?}");

    // Custom padding 16: the icon moves to the half inset (8) and the
    // label clears it by the full padding (8 + 16 + 16 = 40).
    let json = r#"{
        "format": "camcraft-theme/1",
        "descriptor": { "name": "Wide Pad" },
        "base": "dark",
        "geometry": { "menu_pad_x": 16.0 }
    }"#;
    let dir = std::env::temp_dir().join(format!("camcraft_drop_icon_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create test theme dir");
    let path = dir.join("wide_pad.json");
    std::fs::write(&path, json).expect("write test theme file");
    theme_set(&h.state_mut().state.world, &ThemeRef::File(path.to_string_lossy().into_owned()));
    h.run_steps(2);
    let g = theme::current().dropdown();
    assert_eq!((g.pad_x, g.clip_inset_x, g.icon_text_x), (16.0, 8.0, 40.0));
    let (row, icon, x) = first_op_row(&mut h);
    assert!((icon.left() - (row.left() + 8.0)).abs() < 0.5, "custom icon left: {icon:?} in {row:?}");
    assert!(x >= icon.right() + 16.0 - 0.5, "the label clears the icon by the padding: {x} vs {icon:?}");
    assert!((x - (row.left() + 40.0)).abs() < 0.5, "custom label x: {x} in {row:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dropdown_icon_clearance_derives_from_padding() {
    // A row without an icon puts its label at `pad_x`; a row with one
    // puts it at `icon_text_x`, which is inset + icon + padding.
    for pad in [0.0_f32, 8.0, 16.0, 32.0] {
        let pal = theme::Tokens { menu_pad_x: pad, ..theme::Tokens::DARK };
        let g = pal.dropdown();
        assert_eq!(g.pad_x, pad);
        assert_eq!(g.clip_inset_x, pad * 0.5);
        assert!(
            (g.icon_text_x - (g.clip_inset_x + g.icon_px + g.pad_x)).abs() < 1e-4,
            "icon clearance is inset + icon + padding at pad {pad}"
        );
        assert!(g.icon_text_x > g.pad_x, "the icon row label is right of the plain label at pad {pad}");
    }
}
