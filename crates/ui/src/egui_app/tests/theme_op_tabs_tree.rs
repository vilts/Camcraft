//! #2066: operation tab strip, browser tree spacing/glyph/eye reserve,
//! ribbon min-label floor and geometry highlight radius read theme tokens.
//! Provenance: our own tests over our own op tabs, browser tree and theme
//! tokens; no HSMWorks material involved.

use super::*;
use crate::egui_app::{browser_pane, op_page, op_tabs, painted, theme, theme_files};
use crate::state::HoverHit;
use camcraft_app::commands::CreateOperation;
use camcraft_app::operation::{OpType, SrcRef};

const CUSTOM_THEME_JSON: &str = r##"{
  "format": "camcraft-theme/1",
  "descriptor": { "name": "Issue2066 Custom" },
  "base": "dark",
  "colors": { "accent": "#2A7F79", "accent_hover": "#3CA099", "selection_stroke": "#11AA55" },
  "geometry": {
    "radius_widget": 9, "font_label": 16.0, "font_label_min": 14.0,
    "font_row": 14.0, "font_row_suffix": 11.0, "row_height": 28.0, "cell_h": 34.0,
    "op_tab_h": 34.0, "op_tab_pad_x": 14.0, "op_tab_gap": 9.0, "op_tab_bottom_gap": 11.0,
    "op_tab_icon_px": 20.0, "op_tab_icon_gap": 8.0, "op_tab_radius": 7, "font_op_tab": 18.0,
    "tree_glyph_px": 22.0, "tree_inset_x": 18.0, "tree_indent": 26.0, "tree_eye_reserve": 36.0,
    "tree_pad_x": 10.0, "tree_pad_y": 5.0, "tree_suffix_gap": 11.0, "tree_header_boost": 3.0,
    "highlight_stroke_px": 5.0
  }
}"##;

const CLAMP_EXTREME_JSON: &str = r#"{
  "format": "camcraft-theme/1",
  "descriptor": { "name": "Issue2066 Clamped" },
  "base": "dark",
  "geometry": {
    "font_label": 16.0, "font_label_min": 999.0, "op_tab_h": 2.0, "op_tab_pad_x": -10.0,
    "op_tab_gap": 999.0, "op_tab_bottom_gap": 999.0, "op_tab_icon_px": 999.0,
    "op_tab_icon_gap": -5.0, "op_tab_radius": 400, "font_op_tab": 2.0, "tree_glyph_px": 1.0,
    "tree_inset_x": -10.0, "tree_indent": 999.0, "tree_eye_reserve": 1.0, "tree_pad_x": 999.0,
    "tree_pad_y": 999.0, "tree_suffix_gap": -5.0, "tree_header_boost": 99.0,
    "highlight_stroke_px": -5.0
  }
}"#;

fn collect_shapes<S>(h: &Harness<'_, S>) -> Vec<egui::Shape> {
    fn walk(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            other => out.push(other.clone()),
        }
    }
    let mut out = Vec::new();
    for c in &h.output().shapes {
        walk(&c.shape, &mut out);
    }
    out
}

fn image_rect(shape: &egui::Shape) -> Option<egui::Rect> {
    match shape {
        egui::Shape::Mesh(m) if m.texture_id != egui::TextureId::default() => Some(m.calc_bounds()),
        egui::Shape::Rect(r) if r.fill_texture_id() != egui::TextureId::default() => Some(r.rect),
        _ => None,
    }
}

fn parse_custom() -> theme::Tokens {
    theme_files::parse_text(CUSTOM_THEME_JSON, "custom-2066")
        .expect("custom #2066 theme tokens must parse")
        .tokens
}

fn parse_clamped() -> theme::Tokens {
    theme_files::parse_text(CLAMP_EXTREME_JSON, "clamped-2066")
        .expect("clamped #2066 theme tokens must parse")
        .tokens
}

fn near(a: f32, b: f32, msg: &str) {
    assert!((a - b).abs() < 0.15, "{msg}: got {a}, expected {b}");
}

fn measure_geo_outline(pal: theme::Tokens) -> egui::epaint::RectShape {
    let mut seeded = ShellUi::seeded();
    let face = seeded.state.geo_chips()[0].sel.clone();
    seeded.state.hover = Some(HoverHit {
        entity: face.entity,
        facet: 0,
        feature: Some(face.feature),
    });
    let mut h = Harness::builder()
        .with_size(Vec2::new(600.0, 500.0))
        .build_ui_state(
            move |ui, st: &mut State| {
                theme::apply_to(ui.ctx(), &pal);
                op_page::operation_page(ui, st, "Contour1", &pal);
            },
            seeded.state,
        );
    h.run_steps(2);
    collect_shapes(&h)
        .into_iter()
        .find_map(|s| match s {
            egui::Shape::Rect(r) if r.stroke.color == pal.selection_stroke && r.fill == egui::Color32::TRANSPARENT => Some(r),
            _ => None,
        })
        .expect("hovered geometry row paints selection stroke outline")
}

#[test]
fn geo_row_highlight_reads_widget_radius_and_stroke() {
    // Mutate only the already-existing `radius_widget` and `selection_stroke`.
    let mut pal = theme::Tokens::DARK;
    pal.radius_widget = 9;
    pal.selection_stroke = egui::Color32::from_rgb(0x11, 0xAA, 0x55);
    let outline = measure_geo_outline(pal);
    assert_eq!(outline.corner_radius, egui::CornerRadius::same(9), "highlight radius must come from pal.radius_widget");
    near(outline.stroke.width, 2.0, "default highlight stroke width");

    // Custom highlight_stroke_px must resize the stroke.
    let outline2 = measure_geo_outline(parse_custom());
    assert_eq!(outline2.corner_radius, egui::CornerRadius::same(9));
    near(outline2.stroke.width, 5.0, "custom highlight stroke width");
}

fn measure_op_tabs(pal: theme::Tokens) -> (Vec<egui::Shape>, f32, egui::Rect, egui::Rect) {
    use egui_kittest::kittest::Queryable;
    let mut h = Harness::builder()
        .with_size(Vec2::new(700.0, 200.0))
        .build_ui_state(
            move |ui, (open, after_y): &mut (usize, f32)| {
                if IconSet::from_ctx(ui.ctx()).is_none() {
                    let _ = IconSet::load(ui.ctx());
                }
                op_tabs::tab_bar(ui, OpType::Contour, open, &pal);
                *after_y = ui.cursor().min.y;
            },
            (0_usize, 0.0_f32),
        );
    h.run_steps(2);
    let after_y = h.state().1;
    let geo = h.get_by_label("Geometry").rect();
    let heights = h.get_by_label("Heights").rect();
    (collect_shapes(&h), after_y, geo, heights)
}

struct OpTabExpect {
    h: f32,
    gap: f32,
    radius: u8,
    icon_px: f32,
    rule_w: f32,
    bottom_gap: f32,
    details: Option<(f32, f32, f32)>,
}

fn check_op_tabs(tag: &str, pal: theme::Tokens, exp: OpTabExpect) {
    let (shapes, after_y, geo, heights) = measure_op_tabs(pal);
    near(geo.height(), exp.h, &format!("{tag} op tab height"));
    near(heights.left() - geo.right(), exp.gap, &format!("{tag} op tab gap"));
    let sel = shapes
        .iter()
        .find_map(|s| match s {
            egui::Shape::Rect(r) if r.fill == pal.accent => Some(r.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{tag} selected tab fill"));
    assert_eq!(sel.corner_radius, egui::CornerRadius { nw: exp.radius, ne: exp.radius, sw: 0, se: 0 });
    let icon = shapes.iter().find_map(image_rect).unwrap_or_else(|| panic!("{tag} tab icon image"));
    near(icon.width(), exp.icon_px, &format!("{tag} tab icon size"));
    let rule = shapes
        .iter()
        .find_map(|s| match s {
            egui::Shape::LineSegment { points, stroke } if stroke.color == pal.accent => Some((points[0].y, stroke.width)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{tag} tab bottom rule"));
    near(rule.1, exp.rule_w, &format!("{tag} tab rule width"));
    near(after_y - rule.0, exp.bottom_gap, &format!("{tag} tab bottom gap"));
    if let Some((outline_w, label_dx, font_px)) = exp.details {
        let outline = shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Rect(r) if r.stroke.color == pal.accent_hover => Some(r.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{tag} selected tab outline"));
        near(outline.stroke.width, outline_w, &format!("{tag} tab outline width"));
        let label = shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Text(t) if t.galley.text() == "Geometry" => Some(t.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{tag} Geometry tab text"));
        near(label.pos.x - geo.left(), label_dx, &format!("{tag} tab pad + icon + icon_gap"));
        near(label.galley.job.sections[0].format.font_id.size, font_px, &format!("{tag} tab font"));
    }
}

#[test]
fn op_tabs_respect_theme_tokens_and_clamps() {
    check_op_tabs("dark", theme::Tokens::DARK, OpTabExpect {
        h: 22.0,
        gap: 1.0,
        radius: 4,
        icon_px: 12.0,
        rule_w: 2.0,
        bottom_gap: 4.0,
        details: Some((1.0, 6.0 + 12.0 + 3.0, 12.0)),
    });
    check_op_tabs("custom", parse_custom(), OpTabExpect {
        h: 34.0,
        gap: 9.0,
        radius: 7,
        icon_px: 20.0,
        rule_w: 5.0,
        bottom_gap: 11.0,
        details: Some((2.5, 14.0 + 20.0 + 8.0, 18.0)),
    });
    check_op_tabs("clamped", parse_clamped(), OpTabExpect {
        h: 12.0,
        gap: 32.0,
        radius: 12,
        icon_px: 64.0,
        rule_w: 0.5,
        bottom_gap: 24.0,
        details: None,
    });
}

#[test]
fn ribbon_min_label_floor_comes_from_theme() {
    let measure = |pal: theme::Tokens| -> f32 {
        let mut h = Harness::builder()
            .with_size(Vec2::new(120.0, 80.0))
            .build_ui(move |ui| {
                let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(40.0, pal.cell_h));
                painted::cell(ui, egui::Id::new("min_floor"), rect, "Operation", &pal, true, false, None);
            });
        h.run_steps(2);
        collect_shapes(&h)
            .into_iter()
            .find_map(|s| match s {
                egui::Shape::Text(t) if t.galley.text() == "Operation" => Some(t.galley.job.sections[0].format.font_id.size),
                _ => None,
            })
            .expect("cell label text")
    };
    near(measure(theme::Tokens::DARK), 10.0, "dark min label floor");
    near(measure(parse_custom()), 14.0, "custom min label floor");
    near(measure(parse_clamped()), 16.0, "clamped floor capped by start font_label");
}

#[test]
fn browser_tree_and_pane_follow_theme_tokens() {
    let make_row = |label: &str, suffix: &str, kind, icon, depth| painted::Row {
        label: label.into(),
        suffix: suffix.into(),
        kind,
        icon,
        badge: None,
        selected: false,
        hidden: false,
        depth,
    };
    let make_rows = || {
        vec![
            make_row("Setups", "", painted::Kind::Section, Some(Icon::Setup), 0),
            make_row("Setup1", "", painted::Kind::Setup(true), None, 1),
            painted::Row {
                badge: Some(Icon::BadgeOk),
                ..make_row("LongContourOperationName", "ok", painted::Kind::Op, Some(Icon::Toolpaths), 2)
            },
        ]
    };
    let tree_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(260.0, 200.0));
    let run_tree = |pal: theme::Tokens, with_icons: bool| -> Vec<egui::Shape> {
        let rows = make_rows();
        let mut h = Harness::builder()
            .with_size(Vec2::new(300.0, 240.0))
            .build_ui(move |ui| {
                let icons = with_icons.then(|| IconSet::load(ui.ctx()));
                painted::tree(ui, tree_rect, &rows, &pal, icons.as_ref());
            });
        h.run_steps(2);
        collect_shapes(&h)
    };
    let find_txt = |shapes: &[egui::Shape], prefix: &str| {
        shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Text(t) if t.galley.text().starts_with(prefix) => Some(t.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing tree text {prefix:?}"))
    };
    let check_tree = |tag: &str, pal: theme::Tokens, inset: f32, indent: f32, glyph: f32, eye_w: f32, hdr_font: f32, sfx_gap: f32, pad_x: f32, eye_res: f32| {
        let shapes = run_tree(pal, false);
        let (sec, setup, op) = (find_txt(&shapes, "Setups"), find_txt(&shapes, "Setup1"), find_txt(&shapes, "LongContour"));
        let label_dx = glyph * 0.5 + 4.0;
        near(sec.pos.x, inset + label_dx, &format!("{tag} depth 0 label x"));
        near(setup.pos.x - sec.pos.x, indent, &format!("{tag} depth 1 indent"));
        near(op.pos.x - setup.pos.x, indent, &format!("{tag} depth 2 indent"));
        near(sec.galley.job.sections[0].format.font_id.size, hdr_font, &format!("{tag} header font + boost"));
        near(op.galley.job.sections[1].leading_space, sfx_gap, &format!("{tag} suffix gap"));
        near(sec.galley.job.wrap.max_width, 260.0 - pad_x - (inset + label_dx), &format!("{tag} non-eye max_w"));
        near(op.galley.job.wrap.max_width, 260.0 - eye_res - (inset + 2.0 * indent + label_dx), &format!("{tag} eye max_w"));
        let eye = shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Rect(r) if r.rect.right() > 215.0 && r.corner_radius == egui::CornerRadius::same(4) => Some(r.rect),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{tag} eye vector rect"));
        near(eye.center().x, 260.0 - inset, &format!("{tag} eye center inset"));
        near(eye.width(), eye_w, &format!("{tag} eye hit width"));
        near(eye.height(), glyph, &format!("{tag} eye hit height"));

        let tex = run_tree(pal, true);
        let img = tex
            .iter()
            .find_map(|s| image_rect(s).filter(|r| r.left() < 40.0 && r.width() > 9.5))
            .unwrap_or_else(|| panic!("{tag} textured tree icon"));
        near(img.width(), glyph, &format!("{tag} tree glyph edge"));
    };

    check_tree("dark", theme::Tokens::DARK, 12.0, 16.0, 16.0, 18.0, 14.0, 6.0, 6.0, 24.0);
    let custom = parse_custom();
    check_tree("custom", custom, 18.0, 26.0, 22.0, 26.0, 17.0, 11.0, 10.0, 36.0);

    // 3. browser_pane header margin + drag insertion line stroke
    let mut seeded = ShellUi::seeded();
    let entity = model::find_by_name(&seeded.state.world, "Contour1").expect("Contour1");
    let op = model::operation_of(&seeded.state.world, &entity).expect("op").clone();
    seeded.state.history.execute(
        &seeded.state.world,
        CreateOperation::new(&seeded.state.world, "Contour2", &op.tool, SrcRef::Stock, op.params)
            .expect("second op"),
    );
    let mut bh = Harness::builder()
        .with_size(Vec2::new(320.0, 500.0))
        .build_ui_state(
            move |ui, st: &mut State| {
                browser_pane::browser(ui, st, &custom, None);
            },
            seeded.state,
        );
    bh.run_steps(2);
    use egui_kittest::kittest::Queryable;
    let view_btn = bh.get_by_label("View").rect();
    let b_shapes = collect_shapes(&bh);
    let strip = b_shapes
        .iter()
        .find_map(|s| match s {
            egui::Shape::Rect(r) if r.fill == custom.strip_fill() && r.rect.intersects(view_btn) => Some(r.rect),
            _ => None,
        })
        .expect("browser strip background");
    near(view_btn.left() - strip.left(), 10.0, "custom browser bar pad x");
    near(view_btn.top() - strip.top(), 5.0, "custom browser bar pad y");

    // Drag Contour1 onto Contour2 and hold over Contour2 so the insertion rule stays painted.
    let find_row_y = |prefix: &str| {
        b_shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Text(t) if t.galley.text().starts_with(prefix) => Some(t.pos.y + 4.0),
                _ => None,
            })
            .unwrap_or_else(|| panic!("painted row {prefix:?}"))
    };
    let from = egui::pos2(80.0, find_row_y("Contour1"));
    let to = egui::pos2(80.0, find_row_y("Contour2"));
    bh.input_mut().events.push(egui::Event::PointerMoved(from));
    bh.run();
    bh.input_mut().events.push(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    bh.run();
    bh.input_mut().events.push(egui::Event::PointerMoved(to));
    bh.run();
    let drag_line = collect_shapes(&bh)
        .into_iter()
        .find_map(|s| match s {
            egui::Shape::LineSegment { stroke, .. } if stroke.color == custom.accent => Some(stroke.width),
            _ => None,
        })
        .expect("drag insertion rule painted");
    near(drag_line, 5.0, "custom drag insertion rule stroke width");
}
