//! Large theme glyphs must not let a tree control receive another row's click.

use super::*;

fn click_large_tree_glyph(kind: painted::Kind, pos: egui::Pos2) -> painted::TreeHit {
    let mut pal = theme::Tokens::DARK;
    pal.tree_glyph_px = 64.0;
    assert_eq!(pal.row_height, 20.0);
    let rows: Vec<_> = (0..3)
        .map(|ix| painted::Row {
            label: format!("Row {ix}"),
            suffix: String::new(),
            kind,
            icon: None,
            badge: None,
            selected: false,
            hidden: false,
            depth: 1,
        })
        .collect();
    let mut h = Harness::builder()
        .with_size(Vec2::new(320.0, 200.0))
        .build_ui_state(
            move |ui, clicks: &mut Option<painted::TreeHit>| {
                let rect = egui::Rect::from_min_size(egui::pos2(20.0, 40.0), Vec2::new(260.0, 60.0));
                let hit = painted::tree(ui, rect, &rows, &pal, None);
                if hit.eye.is_some() || hit.radio.is_some() || hit.row.is_some() {
                    *clicks = Some(hit);
                }
            },
            None,
        );
    h.run_steps(2);
    h.input_mut().events.push(Event::PointerMoved(pos));
    h.run();
    for pressed in [true, false] {
        h.input_mut().events.push(Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run();
    }
    h.state_mut().take().expect("tree receives pointer click")
}

#[test]
fn large_tree_eye_clicks_stay_in_their_rows() {
    // The centres of all three adjacent rows were inside multiple 64 px targets.
    for ix in 0..3 {
        let hit = click_large_tree_glyph(painted::Kind::Op, egui::pos2(268.0, 50.0 + ix as f32 * 20.0));
        assert_eq!(hit.eye, Some(ix), "eye click belongs to row {ix}");
        assert_eq!(hit.radio, None);
        assert_eq!(hit.row, None, "eye does not also select the row");
    }
}

#[test]
fn large_tree_setup_radio_clicks_stay_in_their_rows() {
    for ix in 0..3 {
        let hit = click_large_tree_glyph(painted::Kind::Setup(false), egui::pos2(48.0, 50.0 + ix as f32 * 20.0));
        assert_eq!(hit.radio, Some(ix), "setup radio click belongs to row {ix}");
        assert_eq!(hit.eye, None);
    }
}
